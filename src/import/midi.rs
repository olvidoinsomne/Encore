//! Original bounded Standard MIDI File reader (SMF 0/1, PPQ timing).
//! Format reference: https://midi.org/standard-midi-files-specification
//! MIDI performance timing is preserved exactly; notation is inferred and
//! unrepresentable timing is rejected rather than quantized silently.
use super::{ImportResult, Warnings, finish_parts, initial_score};
use crate::model::{
    Duration, Instrument, MAX_MEASURES, MAX_PARTS, Measure, Note, PPQ, Part, Pitch, Tuplet,
};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub(super) const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_TRACKS: usize = 256;
const MAX_EVENTS: usize = 500_000;
const MAX_NOTES: usize = 100_000;
const MAX_SCORE_EVENTS: usize = 200_000;
const MAX_TIMELINE: u64 = PPQ as u64 * 4 * 32 * MAX_MEASURES as u64;
const QUANTIZATION_GRIDS: [u32; 11] = [480, 240, 160, 120, 80, 60, 40, 30, 20, 15, 10];

pub(super) fn validate_quantization_grid(grid: u32) -> Result<(), String> {
    if QUANTIZATION_GRIDS.contains(&grid) {
        Ok(())
    } else {
        Err("MIDI quantization grid must be 480, 240, 160, 120, 80, 60, 40, 30, 20, 15, or 10 ticks per note.".into())
    }
}

struct Quantization {
    grid: u32,
    adjusted: usize,
    max_error_numerator: u64,
}

impl Quantization {
    fn nearest(&self, source_tick: u64, division: u16) -> Result<u32, String> {
        let numerator = source_tick
            .checked_mul(u64::from(PPQ))
            .ok_or("MIDI timestamp overflow.")?;
        let quantum = u64::from(division) * u64::from(self.grid);
        let units = numerator
            .checked_add(quantum / 2)
            .ok_or("MIDI quantization overflow.")?
            / quantum;
        let tick = units
            .checked_mul(u64::from(self.grid))
            .ok_or("MIDI quantization overflow.")?;
        u32::try_from(tick)
            .map_err(|_| "Quantized MIDI timestamp exceeds the score timeline.".into())
    }

    fn note(
        &mut self,
        source_start: u64,
        source_end: u64,
        division: u16,
    ) -> Result<(u32, u32), String> {
        let start = self.nearest(source_start, division)?;
        let end = self.nearest(source_end, division)?.max(
            start
                .checked_add(self.grid)
                .ok_or("Quantized MIDI note position overflow.")?,
        );
        if u64::from(end) > MAX_TIMELINE {
            return Err(
                "Quantization extends a MIDI note beyond Encore's largest supported score.".into(),
            );
        }
        let start_error =
            (u64::from(start) * u64::from(division)).abs_diff(source_start * u64::from(PPQ));
        let end_error =
            (u64::from(end) * u64::from(division)).abs_diff(source_end * u64::from(PPQ));
        if start_error != 0 || end_error != 0 {
            self.adjusted += 1;
            self.max_error_numerator = self.max_error_numerator.max(start_error).max(end_error);
        }
        Ok((start, end))
    }

    fn warning(&self, count: usize, division: u16) -> String {
        let numerator = self.max_error_numerator;
        let denominator = u64::from(division);
        let error = if numerator.is_multiple_of(denominator) {
            (numerator / denominator).to_string()
        } else {
            let (mut a, mut b) = (numerator, denominator);
            while b != 0 {
                (a, b) = (b, a % b);
            }
            format!("{}/{}", numerator / a, denominator / a)
        };
        format!(
            "Explicit MIDI quantization: {} of {count} notes adjusted to a {}-tick grid; maximum endpoint change {error} ticks. Every note lasts at least one grid unit. Original performance timing may change; articulation marks are not recovered from MIDI.",
            self.adjusted, self.grid
        )
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        if count > self.remaining() {
            return Err("Truncated MIDI chunk or event.".into());
        }
        let start = self.position;
        self.position += count;
        Ok(&self.bytes[start..self.position])
    }
    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn data_byte(&mut self) -> Result<u8, String> {
        let byte = self.byte()?;
        if byte >= 128 {
            return Err("A MIDI data byte contains an unexpected status bit.".into());
        }
        Ok(byte)
    }
    fn be16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn be32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn variable(&mut self) -> Result<u32, String> {
        let mut value = 0_u32;
        for _ in 0..4 {
            let byte = self.byte()?;
            value = (value << 7) | u32::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("MIDI variable-length quantities must use at most four bytes.".into())
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Note {
        port: u8,
        channel: u8,
        pitch: u8,
        velocity: u8,
        on: bool,
    },
    Program {
        port: u8,
        channel: u8,
        program: u8,
    },
    Control {
        port: u8,
        channel: u8,
        controller: u8,
        value: u8,
    },
    Tempo(u32),
    Meter(u8, u8),
    Key(i8, u8),
}

struct Event {
    tick: u64,
    track: usize,
    kind: Kind,
}

#[derive(Default)]
struct TrackData {
    events: Vec<Event>,
    names: Vec<String>,
    count: usize,
    last_tick: u64,
}

#[derive(Clone, Copy)]
struct Channel {
    program: u8,
    volume: f32,
    pan: f32,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            program: 0,
            volume: 100.0 / 127.0,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Group {
    track: usize,
    port: u8,
    channel: u8,
    program: u8,
}

struct Active {
    start: u64,
    velocity: u8,
    group: Group,
    channel: Channel,
}
struct Completed {
    start: u32,
    end: u32,
    pitch: u8,
    velocity: u8,
    group: Group,
    channel: Channel,
}
struct Chord {
    start: u32,
    end: u32,
    pitches: Vec<Pitch>,
    velocity: u8,
}

fn scaled(tick: u64, division: u16) -> Result<u32, String> {
    let numerator = tick
        .checked_mul(u64::from(PPQ))
        .ok_or("MIDI timestamp overflow.")?;
    if !numerator.is_multiple_of(u64::from(division)) {
        return Err("MIDI timing cannot be represented exactly on Encore's 480-tick grid. Quantize the performance before importing; Encore does not round note timing silently.".into());
    }
    u32::try_from(numerator / u64::from(division)).map_err(|_| "MIDI timeline is too long.".into())
}

fn bounded_text(bytes: &[u8], warnings: &mut Warnings) -> String {
    if std::str::from_utf8(bytes).is_err() {
        warnings.add("Non-UTF-8 MIDI text uses replacement characters.");
    }
    if bytes.len() > 900 {
        warnings.add("Long MIDI track names were shortened to fit Encore's staff labels.");
    }
    let mut result = String::new();
    // Inspect only a bounded prefix even for a very large text meta-event.
    for ch in String::from_utf8_lossy(&bytes[..bytes.len().min(904)]).chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if result.len() + ch.len_utf8() > 900 {
            break;
        }
        result.push(ch);
    }
    result.trim().into()
}

fn parse_track(
    bytes: &[u8],
    track: usize,
    division: u16,
    output: &mut TrackData,
    warnings: &mut Warnings,
) -> Result<(), String> {
    let TrackData {
        events,
        names,
        count: counter,
        last_tick,
    } = output;
    let mut reader = Reader::new(bytes);
    let mut tick = 0_u64;
    let mut running = None;
    let mut port = 0_u8;
    let mut ended = false;
    while reader.remaining() > 0 {
        *counter += 1;
        if *counter > MAX_EVENTS {
            return Err("MIDI exceeds the 500,000-event limit.".into());
        }
        tick = tick
            .checked_add(u64::from(reader.variable()?))
            .ok_or("MIDI timeline overflow.")?;
        if tick * u64::from(PPQ) > MAX_TIMELINE * u64::from(division) {
            return Err("MIDI timeline exceeds Encore's largest supported score.".into());
        }
        *last_tick = (*last_tick).max(tick);
        let first = reader.byte()?;
        let (status, first_data) = if first < 128 {
            (
                running.ok_or("MIDI running status has no preceding channel message.")?,
                Some(first),
            )
        } else {
            (first, None)
        };
        let kind = if (0x80..=0xef).contains(&status) {
            running = Some(status);
            let channel = status & 0x0f;
            let a = match first_data {
                Some(value) => value,
                None => reader.data_byte()?,
            };
            let b = if matches!(status & 0xf0, 0xc0 | 0xd0) {
                0
            } else {
                reader.data_byte()?
            };
            match status & 0xf0 {
                0x80 | 0x90 => Some(Kind::Note {
                    port,
                    channel,
                    pitch: a,
                    velocity: b,
                    on: status & 0xf0 == 0x90 && b != 0,
                }),
                0xc0 => Some(Kind::Program {
                    port,
                    channel,
                    program: a,
                }),
                0xb0 => Some(Kind::Control {
                    port,
                    channel,
                    controller: a,
                    value: b,
                }),
                _ => {
                    warnings.add("MIDI pitch bend and aftertouch are not retained.");
                    None
                }
            }
        } else {
            // Per SMF, meta and SysEx messages cancel channel running status.
            running = None;
            match status {
                0xff => {
                    let meta = reader.data_byte()?;
                    let length = reader.variable()? as usize;
                    let data = reader.take(length)?;
                    match meta {
                        0x2f => {
                            if reader.remaining() != 0 {
                                return Err(
                                    "MIDI End-of-Track must be the last event in its track.".into(),
                                );
                            }
                            ended = true;
                            break;
                        }
                        0x03 if names[track].is_empty() => {
                            names[track] = bounded_text(data, warnings);
                            None
                        }
                        0x21 => {
                            if data.is_empty() || data[0] >= 128 {
                                return Err("Invalid MIDI port meta-event.".into());
                            }
                            port = data[0];
                            None
                        }
                        0x51 => {
                            if data.len() < 3 {
                                return Err("Truncated MIDI tempo meta-event.".into());
                            }
                            let micros = u32::from_be_bytes([0, data[0], data[1], data[2]]);
                            if micros == 0 {
                                return Err("MIDI tempo cannot be zero.".into());
                            }
                            Some(Kind::Tempo(micros))
                        }
                        0x58 => {
                            if data.len() < 4 || data[0] == 0 || data[0] > 32 || data[1] > 4 {
                                return Err("MIDI meter requires 1–32 beats and a denominator from 1 to 16.".into());
                            }
                            Some(Kind::Meter(data[0], 1_u8 << data[1]))
                        }
                        0x59 => {
                            if data.len() < 2 || !(-7..=7).contains(&(data[0] as i8)) || data[1] > 1
                            {
                                return Err("Invalid MIDI key signature.".into());
                            }
                            Some(Kind::Key(data[0] as i8, data[1]))
                        }
                        0x00 | 0x20 => None,
                        _ => {
                            warnings.add("MIDI text, lyrics, markers, and sequencer-specific metadata are not retained, apart from track names.");
                            None
                        }
                    }
                }
                0xf0 | 0xf7 => {
                    let length = reader.variable()? as usize;
                    reader.take(length)?;
                    warnings
                        .add("MIDI system-exclusive and escaped device messages are not retained.");
                    None
                }
                _ => return Err(format!("Unsupported MIDI file status byte 0x{status:02x}.")),
            }
        };
        if let Some(kind) = kind {
            events.push(Event { tick, track, kind });
        }
    }
    if !ended {
        return Err("MIDI track has no End-of-Track event.".into());
    }
    Ok(())
}

pub(super) fn parse(data: &[u8]) -> Result<ImportResult, String> {
    parse_with_quantization(data, None)
}

pub(super) fn parse_with_quantization(
    data: &[u8],
    grid_ticks: Option<u32>,
) -> Result<ImportResult, String> {
    if let Some(grid) = grid_ticks {
        validate_quantization_grid(grid)?;
    }
    let mut quantization = grid_ticks.map(|grid| Quantization {
        grid,
        adjusted: 0,
        max_error_numerator: 0,
    });
    if data.len() > MAX_BYTES {
        return Err("MIDI file exceeds the 16 MiB limit.".into());
    }
    let mut reader = Reader::new(data);
    if reader.take(4)? != b"MThd" {
        return Err("Not a Standard MIDI File (missing MThd header).".into());
    }
    let header_length = reader.be32()? as usize;
    if header_length < 6 {
        return Err("MIDI header is shorter than six bytes.".into());
    }
    let mut header = Reader::new(reader.take(header_length)?);
    let format = header.be16()?;
    let tracks = usize::from(header.be16()?);
    let division = header.be16()?;
    if format > 1 {
        return Err("Only Standard MIDI File formats 0 and 1 are supported; format 2 contains independent sequences.".into());
    }
    if tracks == 0 || tracks > MAX_TRACKS || (format == 0 && tracks != 1) {
        return Err("MIDI requires 1–256 tracks; format 0 must contain exactly one track.".into());
    }
    if division & 0x8000 != 0 {
        return Err(
            "SMPTE-timed MIDI is not supported. Export MIDI with pulses-per-quarter-note timing."
                .into(),
        );
    }
    if division == 0 {
        return Err("MIDI pulses per quarter note cannot be zero.".into());
    }
    let mut warnings = Warnings::default();
    warnings.add("MIDI contains performance data rather than engraving: note spelling, rests, ties, and voice allocation are inferred. Original page layout and notation marks are not retained.");
    let mut output = TrackData {
        names: vec![String::new(); tracks],
        ..TrackData::default()
    };
    let mut parsed_tracks = 0;
    while reader.remaining() > 0 {
        let chunk_type = reader.take(4)?;
        let length = reader.be32()? as usize;
        let chunk = reader.take(length)?;
        if chunk_type == b"MTrk" {
            if parsed_tracks >= tracks {
                return Err("MIDI contains more tracks than its header declares.".into());
            }
            parse_track(chunk, parsed_tracks, division, &mut output, &mut warnings)?;
            parsed_tracks += 1;
        } else {
            warnings.add("Unknown MIDI chunks were skipped.");
        }
    }
    if parsed_tracks != tracks {
        return Err("MIDI is missing one or more declared track chunks.".into());
    }
    let TrackData {
        mut events,
        names,
        last_tick,
        ..
    } = output;
    // Stable ordering preserves each track's original order at equal timestamps.
    events.sort_by_key(|event| (event.tick, event.track));
    let title = names
        .first()
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| "Imported MIDI score".into());
    let mut score = initial_score(title, String::new());
    let mut channels = BTreeMap::<(u8, u8), Channel>::new();
    let mut active = BTreeMap::<(u8, u8, u8), Active>::new();
    let mut notes = Vec::new();
    let mut tempo_seen = None;
    let mut tempo_changes = BTreeMap::new();
    let mut meter_seen = None;
    let mut key_seen = None;
    for event in events {
        match event.kind {
            Kind::Tempo(micros) => {
                let bpm = 60_000_000.0 / f64::from(micros);
                if !(20.0..=400.0).contains(&bpm) {
                    return Err("MIDI tempo must be 20–400 BPM.".into());
                }
                let bpm = bpm.round() as u16;
                if event.tick == 0 && tempo_seen.is_none() {
                    score.tempo = bpm;
                    tempo_seen = Some(micros);
                }
                if event.tick > 0 {
                    let tick = scaled(event.tick, division)?;
                    if tempo_changes
                        .insert(tick, bpm)
                        .is_some_and(|prior| prior != bpm)
                    {
                        return Err("MIDI tracks disagree on simultaneous tempo changes.".into());
                    }
                }
                if (60_000_000.0 / f64::from(micros) - f64::from(bpm)).abs() > 0.0001 {
                    warnings.add("Fractional MIDI tempos were rounded to whole BPM.");
                }
            }
            Kind::Meter(numerator, denominator) => {
                if event.tick == 0 && meter_seen.is_none() {
                    score.time_numerator = numerator;
                    score.time_denominator = denominator;
                    meter_seen = Some((numerator, denominator));
                } else if meter_seen != Some((numerator, denominator)) {
                    warnings.add("MIDI meter changes are not retained; the initial meter defines the imported barlines.");
                }
            }
            Kind::Key(fifths, mode) => {
                if event.tick == 0 && key_seen.is_none() {
                    score.key_fifths = fifths;
                    key_seen = Some((fifths, mode));
                    if mode == 1 {
                        warnings.add("MIDI minor-key mode is not stored separately; its key signature and sounding pitches are preserved.");
                    }
                } else if key_seen != Some((fifths, mode)) {
                    warnings.add("MIDI key-signature changes are not retained; all sounding pitches are preserved.");
                }
            }
            Kind::Program {
                port,
                channel,
                program,
            } => {
                channels.entry((port, channel)).or_default().program = program;
                if event.tick != 0 {
                    warnings.add("MIDI program changes create separate instrument staves rather than instrument changes within a staff.");
                }
            }
            Kind::Control {
                port,
                channel,
                controller,
                value,
            } => {
                let state = channels.entry((port, channel)).or_default();
                match (controller, event.tick) {
                    (7, 0) => state.volume = f32::from(value) / 127.0,
                    (10, 0) => state.pan = f32::from(value) / 127.0 * 2.0 - 1.0,
                    (0 | 32, _) if value == 0 => (),
                    (0 | 32, _) => {
                        warnings.add("MIDI bank selection is not retained; instrument programs use General MIDI melodic bank 0.");
                    }
                    _ => {
                        if quantization.is_some() {
                            warnings.add("MIDI sustain, expression, controller automation, and later mixer changes are not retained; key-release lengths are quantized to the requested grid without pedal sustain.");
                        } else {
                            warnings.add("MIDI sustain, expression, controller automation, and later mixer changes are not retained; note lengths use the original key releases.");
                        }
                    }
                }
            }
            Kind::Note {
                port,
                channel,
                pitch,
                velocity,
                on,
            } => {
                if channel == 9 {
                    return Err("MIDI channel 10 percussion kits are not supported yet. Pitched percussion such as timpani on a melodic channel can be imported.".into());
                }
                let key = (port, channel, pitch);
                if on {
                    let state = *channels.entry((port, channel)).or_default();
                    let group = Group {
                        track: event.track,
                        port,
                        channel,
                        program: state.program,
                    };
                    if active
                        .insert(
                            key,
                            Active {
                                start: event.tick,
                                velocity,
                                group,
                                channel: state,
                            },
                        )
                        .is_some()
                    {
                        return Err("Overlapping MIDI note-ons for the same pitch and channel are ambiguous; separate them before importing.".into());
                    }
                } else if let Some(note) = active.remove(&key) {
                    if event.tick <= note.start {
                        return Err("MIDI notes must have positive duration.".into());
                    }
                    if notes.len() >= MAX_NOTES {
                        return Err("MIDI exceeds the 100,000-note import limit.".into());
                    }
                    let (start, end) = match &mut quantization {
                        Some(options) => options.note(note.start, event.tick, division)?,
                        None => (scaled(note.start, division)?, scaled(event.tick, division)?),
                    };
                    notes.push(Completed {
                        start,
                        end,
                        pitch,
                        velocity: note.velocity,
                        group: note.group,
                        channel: note.channel,
                    });
                } else {
                    warnings.add("Unmatched MIDI note-off messages were ignored.");
                }
            }
        }
    }
    if !active.is_empty() {
        return Err(
            "MIDI has notes without matching note-offs. Close those notes before importing.".into(),
        );
    }
    if tempo_seen.is_none() {
        warnings.add("MIDI has no initial tempo; the Standard MIDI default of 120 BPM is used.");
    }
    if meter_seen.is_none() {
        warnings.add("MIDI has no initial meter; 4/4 is used.");
    }
    if notes.is_empty() {
        return Err("MIDI contains no supported pitched notes.".into());
    }
    if let Some(options) = &quantization {
        warnings.add(options.warning(notes.len(), division));
    }
    let bar_ticks = score.measure_ticks();
    // Track tails need not lie on the note grid, but their silence is retained.
    let end_numerator = last_tick * u64::from(PPQ);
    let end = end_numerator.div_ceil(u64::from(division)).max(
        notes
            .iter()
            .map(|note| u64::from(note.end))
            .max()
            .unwrap_or(0),
    );
    let measures = end.div_ceil(u64::from(bar_ticks)).max(1) as usize;
    if measures > MAX_MEASURES {
        return Err(format!(
            "MIDI needs {measures} measures; Encore supports at most {MAX_MEASURES}."
        ));
    }
    let mut groups = BTreeMap::<Group, (Channel, BTreeMap<(u32, u32, u8), Vec<Pitch>>)>::new();
    for note in notes {
        if !groups.contains_key(&note.group) && groups.len() >= MAX_PARTS {
            return Err("MIDI exceeds Encore's 64-staff limit before voice separation.".into());
        }
        groups
            .entry(note.group)
            .or_insert_with(|| (note.channel, BTreeMap::new()))
            .1
            .entry((note.start, note.end, note.velocity))
            .or_default()
            .push(Pitch::from_midi(note.pitch));
    }
    let mut parts = Vec::new();
    let mut generated = 0;
    for (group, (channel, chords)) in groups {
        let mut lanes: Vec<Vec<Chord>> = Vec::new();
        let mut lane_ends = Vec::new();
        for ((start, end, velocity), pitches) in chords {
            // Quantization can move repeated notes of the same pitch onto
            // identical positions. Retain every event as an independent voice
            // instead of creating an invalid chord or dropping a repetition.
            let mut simultaneous: Vec<Vec<Pitch>> = Vec::new();
            for pitch in pitches {
                if let Some(chord) = simultaneous.iter_mut().find(|chord| {
                    chord.len() < 16 && !chord.iter().any(|other| other.midi() == pitch.midi())
                }) {
                    chord.push(pitch);
                } else {
                    if parts.len() + simultaneous.len() >= MAX_PARTS {
                        return Err("MIDI polyphony exceeds Encore's 64-staff limit after voice separation.".into());
                    }
                    simultaneous.push(vec![pitch]);
                }
            }
            for pitches in simultaneous {
                let lane = if let Some(index) = lane_ends.iter().position(|last| *last <= start) {
                    index
                } else {
                    if parts.len() + lanes.len() >= MAX_PARTS {
                        return Err("MIDI polyphony exceeds Encore's 64-staff limit after voice separation.".into());
                    }
                    lanes.push(Vec::new());
                    lane_ends.push(0);
                    lanes.len() - 1
                };
                lane_ends[lane] = end;
                lanes[lane].push(Chord {
                    start,
                    end,
                    pitches,
                    velocity,
                });
            }
        }
        let instrument =
            Instrument::from_gm_program(group.program).ok_or("Invalid MIDI program.")?;
        let voice_count = lanes.len();
        if voice_count > 1 {
            warnings.add("Overlapping MIDI voices and unequal chord velocities were separated into editable staves.");
        }
        for (voice, lane) in lanes.into_iter().enumerate() {
            let mut part = Part {
                name: if names[group.track].is_empty() {
                    instrument.name().into()
                } else {
                    names[group.track].clone()
                },
                instrument,
                clef: instrument.default_clef(),
                measures: vec![Measure::default(); measures],
                volume: channel.volume,
                pan: channel.pan,
                muted: false,
                solo: false,
            };
            part.name.push_str(&format!(" · ch {}", group.channel + 1));
            if group.port != 0 {
                part.name.push_str(&format!(" · port {}", group.port + 1));
            }
            if voice_count > 1 {
                part.name.push_str(&format!(" · voice {}", voice + 1));
            }
            for chord in lane {
                let mut position = chord.start;
                while position < chord.end {
                    let bar = (position / bar_ticks) as usize;
                    let start = position % bar_ticks;
                    let size = (chord.end - position).min(bar_ticks - start);
                    let mut pieces = rhythmic_notes(
                        start,
                        size,
                        &chord.pitches,
                        chord.velocity,
                        position + size < chord.end,
                        &mut warnings,
                    )?;
                    generated += pieces.len();
                    if generated > MAX_SCORE_EVENTS {
                        return Err("MIDI expands beyond Encore's 200,000-event limit.".into());
                    }
                    part.measures[bar].notes.append(&mut pieces);
                    position += size;
                }
            }
            for measure in &mut part.measures {
                if measure.notes.is_empty() {
                    continue;
                }
                let mut filled = Vec::new();
                let mut position = 0;
                for note in std::mem::take(&mut measure.notes) {
                    if note.start > position {
                        let mut rests = rhythmic_notes(
                            position,
                            note.start - position,
                            &[],
                            0,
                            false,
                            &mut warnings,
                        )?;
                        generated += rests.len();
                        if generated > MAX_SCORE_EVENTS {
                            return Err("MIDI expands beyond Encore's 200,000-event limit.".into());
                        }
                        filled.append(&mut rests);
                    }
                    position = note.start + note.ticks();
                    filled.push(note);
                }
                if position < bar_ticks {
                    let mut rests = rhythmic_notes(
                        position,
                        bar_ticks - position,
                        &[],
                        0,
                        false,
                        &mut warnings,
                    )?;
                    generated += rests.len();
                    if generated > MAX_SCORE_EVENTS {
                        return Err("MIDI expands beyond Encore's 200,000-event limit.".into());
                    }
                    filled.append(&mut rests);
                }
                measure.notes = filled;
            }
            parts.push(part);
        }
    }
    for (tick, bpm) in tempo_changes {
        if ((tick / bar_ticks) as usize) < measures {
            score
                .notation
                .tempo
                .push(crate::expressions::TempoChange::new(
                    crate::expressions::TimePoint {
                        measure: (tick / bar_ticks) as usize,
                        tick: tick % bar_ticks,
                    },
                    bpm,
                ));
        }
    }
    finish_parts(score, parts, warnings)
}

fn rhythmic_notes(
    start: u32,
    ticks: u32,
    pitches: &[Pitch],
    velocity: u8,
    tied: bool,
    warnings: &mut Warnings,
) -> Result<Vec<Note>, String> {
    type Rhythm = (u32, Duration, bool, Option<Tuplet>);
    const MAX_BAR_TICKS: usize = PPQ as usize * 4 * 32;
    // MIDI has no written tuplet metadata. Consider only this small set of
    // conventional ratios, with ordinary notation and triplets preferred.
    const RATIOS: [Option<Tuplet>; 8] = [
        None,
        Some(Tuplet::TRIPLET),
        Some(Tuplet {
            actual: 2,
            normal: 3,
        }),
        Some(Tuplet {
            actual: 4,
            normal: 3,
        }),
        Some(Tuplet {
            actual: 5,
            normal: 4,
        }),
        Some(Tuplet {
            actual: 6,
            normal: 4,
        }),
        Some(Tuplet {
            actual: 7,
            normal: 4,
        }),
        Some(Tuplet {
            actual: 9,
            normal: 8,
        }),
    ];
    static OPTIONS: OnceLock<(Vec<Rhythm>, Vec<bool>)> = OnceLock::new();
    let (candidates, reachable) = OPTIONS.get_or_init(|| {
        let mut candidates = Vec::new();
        for duration in Duration::ALL {
            for tuplet in RATIOS {
                for dotted in [false, true] {
                    let note = Note {
                        voice: 0,
                        voice_launch: None,
                        articulations: Vec::new(),
                        start: 0,
                        layer: 0,
                        duration: *duration,
                        dotted,
                        tuplet,
                        pitches: Vec::new(),
                        velocity: 0,
                        staccato: false,
                        tied: false,
                    };
                    if let Ok(size) = note.checked_ticks() {
                        candidates.push((size, *duration, dotted, tuplet));
                    }
                }
            }
        }
        // Prefer ordinary notation for equal durations; e.g. a dotted triplet
        // eighth is displayed as an ordinary eighth.
        candidates.sort_by_key(|(size, _, dotted, tuplet)| {
            (
                std::cmp::Reverse(*size),
                RATIOS.iter().position(|ratio| ratio == tuplet).unwrap(),
                *dotted,
            )
        });
        // Quintuplets introduce 12- and 18-tick values, so divisibility by five
        // alone no longer decides whether a tied remainder is representable.
        // This bounded table considers only the finite values above; it never
        // invents arbitrary tuplet ratios or changes the imported timing.
        let mut sizes: Vec<_> = candidates.iter().map(|(size, ..)| *size as usize).collect();
        sizes.sort_unstable();
        sizes.dedup();
        let mut reachable = vec![false; MAX_BAR_TICKS + 1];
        reachable[0] = true;
        for size in sizes {
            for value in size..=MAX_BAR_TICKS {
                reachable[value] |= reachable[value - size];
            }
        }
        (candidates, reachable)
    });
    let mut remaining = ticks;
    let mut position = start;
    let mut result = Vec::new();
    while remaining != 0 {
        let Some((size, duration, dotted, tuplet)) = candidates.iter().find(|(size, ..)| {
            let remainder = remaining.saturating_sub(*size);
            *size <= remaining && reachable.get(remainder as usize).copied().unwrap_or(false)
        }) else {
            return Err(format!(
                "MIDI has a {ticks}-tick note or rest that cannot be notated exactly with ordinary values or conventional tuplets down to 128ths. Quantize the performance before importing; no note timing was rounded."
            ));
        };
        let note = Note {
            voice: 0,
            voice_launch: None,
            articulations: Vec::new(),
            start: position,
            layer: 0,
            duration: *duration,
            dotted: *dotted,
            tuplet: *tuplet,
            pitches: pitches.to_vec(),
            velocity,
            staccato: false,
            tied: !pitches.is_empty() && (remaining > *size || tied),
        };
        if note.tuplet == Some(Tuplet::TRIPLET) {
            warnings.add("Triplet notation was inferred from exact MIDI timing; confirm the displayed rhythm.");
        } else if note.tuplet.is_some() {
            warnings.add("Tuplet notation was inferred from exact MIDI timing; confirm the displayed rhythm.");
        }
        result.push(note);
        position += *size;
        remaining -= *size;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variable(mut value: u32) -> Vec<u8> {
        let mut bytes = vec![(value & 0x7f) as u8];
        value >>= 7;
        while value != 0 {
            bytes.push((value & 0x7f) as u8 | 0x80);
            value >>= 7;
        }
        bytes.reverse();
        bytes
    }

    fn track(events: &[(u32, Vec<u8>)], end: u32) -> Vec<u8> {
        let mut result = Vec::new();
        let mut previous = 0;
        for (tick, event) in events {
            result.extend(variable(*tick - previous));
            result.extend(event);
            previous = *tick;
        }
        result.extend(variable(end - previous));
        result.extend([0xff, 0x2f, 0]);
        result
    }

    fn smf(format: u16, division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut result = b"MThd".to_vec();
        result.extend(6_u32.to_be_bytes());
        result.extend(format.to_be_bytes());
        result.extend((tracks.len() as u16).to_be_bytes());
        result.extend(division.to_be_bytes());
        for track in tracks {
            result.extend(b"MTrk");
            result.extend((track.len() as u32).to_be_bytes());
            result.extend(track);
        }
        result
    }

    fn pitched(score: &crate::model::Score) -> Vec<&Note> {
        score
            .parts
            .iter()
            .flat_map(|part| &part.measures)
            .flat_map(|measure| &measure.notes)
            .filter(|note| !note.pitches.is_empty())
            .collect()
    }

    #[test]
    fn format_zero_preserves_channels_chords_initial_metadata_and_mixer() {
        let events = track(
            &[
                (
                    0,
                    vec![0xff, 0x03, 7, b'F', b'i', b'x', b't', b'u', b'r', b'e'],
                ),
                (0, vec![0xff, 0x51, 3, 0x09, 0x89, 0x68]), // 96 BPM.
                (0, vec![0xff, 0x58, 4, 3, 2, 24, 8]),
                (0, vec![0xff, 0x59, 2, 0xfd, 0]),
                (0, vec![0xc0, 40]),
                (0, vec![0xb0, 7, 90]),
                (0, vec![0xb0, 10, 32]),
                (0, vec![0xc1, 43]),
                (0, vec![0x90, 64, 88]),
                (0, vec![0x90, 67, 88]),
                (0, vec![0x91, 36, 102]),
                (480, vec![0x80, 64, 0]),
                (480, vec![0x80, 67, 0]),
                (1440, vec![0x81, 36, 0]),
            ],
            1440,
        );
        let result = parse(&smf(0, 480, &[events])).unwrap();
        assert_eq!(result.score.title, "Fixture");
        assert_eq!((result.score.tempo, result.score.key_fifths), (96, -3));
        assert_eq!(
            (result.score.time_numerator, result.score.time_denominator),
            (3, 4)
        );
        assert_eq!(result.score.parts.len(), 2);
        assert_eq!(result.score.parts[0].instrument, Instrument::Violin);
        assert_eq!(result.score.parts[1].instrument, Instrument::DoubleBass);
        assert!((result.score.parts[0].volume - 90.0 / 127.0).abs() < 0.0001);
        assert!((result.score.parts[0].pan - (32.0 / 127.0 * 2.0 - 1.0)).abs() < 0.0001);
        let notes = pitched(&result.score);
        assert_eq!(notes.len(), 2);
        assert_eq!(
            notes[0]
                .pitches
                .iter()
                .map(|pitch| pitch.midi())
                .collect::<Vec<_>>(),
            vec![64, 67]
        );
        assert_eq!(notes[0].ticks(), 480);
        assert_eq!(notes[1].ticks(), 1440);
        result.score.validate().unwrap();
    }

    #[test]
    fn format_one_conductor_tempo_and_midi_ports_preserve_distinct_instruments() {
        let conductor = track(&[(0, vec![0xff, 0x51, 3, 0x07, 0xa1, 0x20])], 960);
        let music = |port, program, pitch| {
            track(
                &[
                    (0, vec![0xff, 0x21, 1, port]),
                    (0, vec![0xc0, program]),
                    (0, vec![0x90, pitch, 80]),
                    (960, vec![0x80, pitch, 0]),
                ],
                960,
            )
        };
        let result = parse(&smf(
            1,
            960,
            &[conductor, music(0, 47, 41), music(1, 43, 28)],
        ))
        .unwrap();
        assert_eq!(result.score.parts.len(), 2);
        assert_eq!(result.score.parts[0].instrument, Instrument::Timpani);
        assert_eq!(result.score.parts[1].instrument, Instrument::DoubleBass);
        assert!(result.score.parts[1].name.contains("port 2"));
        assert!(
            pitched(&result.score)
                .iter()
                .all(|note| note.ticks() == 480)
        );
    }

    #[test]
    fn channel_running_status_and_zero_velocity_note_offs_are_supported() {
        let events = track(
            &[
                (0, vec![0x90, 60, 80]),
                (0, vec![64, 80]),
                (480, vec![60, 0]),
                (480, vec![64, 0]),
            ],
            480,
        );
        let result = parse(&smf(0, 480, &[events])).unwrap();
        assert_eq!(pitched(&result.score).len(), 1);
        assert_eq!(pitched(&result.score)[0].pitches.len(), 2);
        assert_eq!(pitched(&result.score)[0].ticks(), 480);
    }

    #[test]
    fn overlapping_durations_and_distinct_velocities_become_independent_staves() {
        let events = track(
            &[
                (0, vec![0x90, 60, 80]),
                (240, vec![0x90, 64, 90]),
                (240, vec![0x90, 67, 100]),
                (480, vec![0x80, 64, 0]),
                (960, vec![0x80, 60, 0]),
                (960, vec![0x80, 67, 0]),
            ],
            960,
        );
        let result = parse(&smf(0, 480, &[events])).unwrap();
        assert_eq!(result.score.parts.len(), 3);
        let notes = pitched(&result.score);
        assert_eq!(
            notes
                .iter()
                .map(|note| (note.start, note.ticks(), note.velocity))
                .collect::<Vec<_>>(),
            vec![(0, 960, 80), (240, 240, 90), (240, 720, 100)]
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("separated into editable staves"))
        );
    }

    #[test]
    fn notes_crossing_barlines_are_split_with_ties_and_track_silence_is_preserved() {
        let events = track(&[(0, vec![0x90, 48, 80]), (2400, vec![0x80, 48, 0])], 7680);
        let result = parse(&smf(0, 480, &[events])).unwrap();
        assert_eq!(result.score.measure_count(), 4);
        let notes = pitched(&result.score);
        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].ticks(), notes[0].tied), (1920, true));
        assert_eq!((notes[1].ticks(), notes[1].tied), (480, false));
    }

    #[test]
    fn exact_triplets_are_inferred_without_rounding_their_onsets_or_durations() {
        let events = track(
            &[
                (0, vec![0x90, 60, 80]),
                (160, vec![0x80, 60, 0]),
                (160, vec![0x90, 62, 80]),
                (320, vec![0x80, 62, 0]),
                (320, vec![0x90, 64, 80]),
                (480, vec![0x80, 64, 0]),
            ],
            480,
        );
        let result = parse(&smf(0, 480, &[events])).unwrap();
        let notes = pitched(&result.score);
        assert_eq!(
            notes.iter().map(|note| note.start).collect::<Vec<_>>(),
            vec![0, 160, 320]
        );
        assert!(
            notes
                .iter()
                .all(|note| note.ticks() == 160 && note.tuplet == Some(Tuplet::TRIPLET))
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("Triplet notation was inferred"))
        );
    }

    #[test]
    fn quintuplet_eighths_and_dotted_short_notes_survive_midi_export_and_import() {
        let quintuplet = Tuplet {
            actual: 5,
            normal: 4,
        };
        for (duration, dotted, ticks) in [
            (Duration::Eighth, false, 192),
            (Duration::SixtyFourth, true, 36),
            (Duration::OneTwentyEighth, true, 18),
        ] {
            let mut source = crate::model::Score::blank();
            source.parts.truncate(1);
            source.parts[0].measures.truncate(1);
            source.parts[0].measures[0].notes = (0..5)
                .map(|index| Note {
                    voice: 0,
                    voice_launch: None,
                    articulations: Vec::new(),
                    start: index * ticks,
                    layer: 0,
                    duration,
                    dotted,
                    tuplet: Some(quintuplet),
                    pitches: vec![Pitch::from_midi(60 + index as u8)],
                    velocity: 88,
                    staccato: false,
                    tied: false,
                })
                .collect();
            source.validate().unwrap();
            let imported = parse(&crate::export::midi(&source)).unwrap();
            imported.score.validate().unwrap();
            let notes = pitched(&imported.score);
            assert_eq!(notes.len(), 5);
            for (actual, expected) in notes.iter().zip(&source.parts[0].measures[0].notes) {
                assert_eq!(actual.start, expected.start);
                assert_eq!(actual.ticks(), ticks);
                assert_eq!(actual.duration, duration);
                assert_eq!(actual.dotted, dotted);
                assert_eq!(actual.tuplet, Some(quintuplet));
                assert_eq!(actual.pitches, expected.pitches);
                assert!(!actual.tied);
            }
            assert_eq!(
                notes.last().unwrap().start + notes.last().unwrap().ticks(),
                ticks * 5
            );
            assert!(
                imported
                    .warnings
                    .iter()
                    .any(|warning| warning.starts_with("Tuplet notation was inferred"))
            );
        }
    }

    #[test]
    fn mixed_conventional_tuplet_remainders_preserve_exact_note_and_rest_timing() {
        let mut warnings = Warnings::default();
        let pitches = vec![Pitch::from_midi(60)];
        // 1848 ticks is 1440 ordinary + 384 quintuplet + 24 quintuplet ticks.
        // A five-divisibility heuristic incorrectly rejects this exact gap.
        for pitches in [pitches.as_slice(), &[]] {
            let notes = rhythmic_notes(0, 1848, pitches, 88, false, &mut warnings).unwrap();
            assert_eq!(notes.iter().map(Note::ticks).sum::<u32>(), 1848);
            let mut next = 0;
            for (index, note) in notes.iter().enumerate() {
                assert_eq!(note.start, next);
                assert_eq!(note.tied, !pitches.is_empty() && index + 1 < notes.len());
                next += note.ticks();
            }
        }
        let ordinary = rhythmic_notes(0, 240, &[], 0, false, &mut warnings).unwrap();
        assert_eq!(
            (ordinary[0].duration, ordinary[0].tuplet),
            (Duration::Eighth, None)
        );
        let triplet = rhythmic_notes(0, 160, &[], 0, false, &mut warnings).unwrap();
        assert_eq!(
            (triplet[0].duration, triplet[0].tuplet),
            (Duration::Eighth, Some(Tuplet::TRIPLET))
        );
    }

    #[test]
    fn unsupported_fractional_grid_notes_and_gaps_fail_without_quantization() {
        for (division, start, end) in [(960, 0, 1), (480, 0, 17), (480, 1, 481)] {
            let events = track(
                &[(start, vec![0x90, 60, 80]), (end, vec![0x80, 60, 0])],
                end,
            );
            let error = parse(&smf(0, division, &[events])).unwrap_err();
            assert!(error.contains("Quantize"), "{error}");
        }
    }

    #[test]
    fn later_global_changes_and_unretained_performance_controls_are_reported() {
        let events = track(
            &[
                (0, vec![0xff, 0x51, 3, 0x07, 0xa1, 0x20]),
                (0, vec![0xff, 0x58, 4, 4, 2, 24, 8]),
                (0, vec![0xff, 0x59, 2, 0, 0]),
                (0, vec![0x90, 60, 80]),
                (240, vec![0xb0, 64, 127]),
                (240, vec![0xe0, 0, 64]),
                (480, vec![0xff, 0x51, 3, 0x09, 0x89, 0x68]),
                (480, vec![0xff, 0x58, 4, 3, 2, 24, 8]),
                (480, vec![0xff, 0x59, 2, 2, 0]),
                (480, vec![0x80, 60, 0]),
            ],
            480,
        );
        let result = parse(&smf(0, 480, &[events])).unwrap();
        assert_eq!(
            (
                result.score.tempo,
                result.score.time_numerator,
                result.score.key_fifths
            ),
            (120, 4, 0)
        );
        for loss in [
            "meter changes",
            "key-signature changes",
            "sustain",
            "pitch bend",
        ] {
            assert!(
                result.warnings.iter().any(|warning| warning.contains(loss)),
                "missing warning: {loss}"
            );
        }
        assert_eq!(pitched(&result.score)[0].ticks(), 480);
    }

    #[test]
    fn invalid_timing_formats_notes_and_running_status_are_rejected() {
        let ordinary = track(&[(0, vec![0x90, 60, 80]), (480, vec![0x80, 60, 0])], 480);
        assert!(
            parse(&smf(2, 480, std::slice::from_ref(&ordinary)))
                .unwrap_err()
                .contains("formats 0 and 1")
        );
        assert!(
            parse(&smf(0, 0xe728, std::slice::from_ref(&ordinary)))
                .unwrap_err()
                .contains("SMPTE")
        );
        assert!(
            parse(&smf(0, 0, &[ordinary]))
                .unwrap_err()
                .contains("cannot be zero")
        );
        for events in [
            vec![(0, vec![0x90, 60, 80])],
            vec![(0, vec![0x90, 60, 80]), (0, vec![0x90, 60, 80])],
            vec![(0, vec![0x90, 60, 80]), (0, vec![0x80, 60, 0])],
            vec![(0, vec![0x99, 60, 80]), (480, vec![0x89, 60, 0])],
            vec![
                (0, vec![0x90, 60, 80]),
                (240, vec![0xff, 1, 0]),
                (480, vec![60, 0]),
            ],
        ] {
            assert!(parse(&smf(0, 480, &[track(&events, 480)])).is_err());
        }
    }

    #[test]
    fn all_truncated_prefixes_and_hostile_lengths_return_errors() {
        let valid = smf(
            0,
            480,
            &[track(
                &[(0, vec![0x90, 60, 80]), (480, vec![0x80, 60, 0])],
                480,
            )],
        );
        for length in 0..valid.len() {
            assert!(parse(&valid[..length]).is_err(), "prefix {length}");
        }
        let bad_variable = smf(0, 480, &[vec![0xff; 6]]);
        assert!(
            parse(&bad_variable)
                .unwrap_err()
                .contains("at most four bytes")
        );
        let mut bad_length = valid.clone();
        bad_length[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse(&bad_length).unwrap_err().contains("Truncated"));
        assert!(
            parse(&vec![0; MAX_BYTES + 1])
                .unwrap_err()
                .contains("16 MiB")
        );
    }

    #[test]
    fn score_limits_are_checked_before_allocating_staves_or_long_measure_grids() {
        let tracks: Vec<_> = (0..65)
            .map(|port| {
                track(
                    &[
                        (0, vec![0xff, 0x21, 1, port]),
                        (0, vec![0x90, 60, 80]),
                        (480, vec![0x80, 60, 0]),
                    ],
                    480,
                )
            })
            .collect();
        assert!(
            parse(&smf(1, 480, &tracks))
                .unwrap_err()
                .contains("64-staff")
        );
        let too_long = track(
            &[(0, vec![0x90, 60, 80]), (480, vec![0x80, 60, 0])],
            1920 * 1025,
        );
        assert!(
            parse(&smf(0, 480, &[too_long]))
                .unwrap_err()
                .contains("1024")
        );
        assert!(
            parse(&smf(1, 480, &vec![vec![0, 0xff, 0x2f, 0]; 257]))
                .unwrap_err()
                .contains("1–256")
        );
    }

    #[test]
    fn encore_orchestral_midi_export_can_be_reopened_including_the_second_port() {
        let source = crate::model::Score::carol_demo();
        let data = crate::export::midi(&source);
        let result = parse(&data).unwrap();
        result.score.validate().unwrap();
        assert_eq!(result.score.tempo, source.tempo);
        assert_eq!(result.score.measure_count(), source.measure_count());
        assert!(
            result
                .score
                .parts
                .iter()
                .any(|part| part.name.contains("port 2"))
        );
        assert!(
            result
                .score
                .parts
                .iter()
                .any(|part| part.instrument == Instrument::Timpani)
        );
        assert!(
            result
                .score
                .parts
                .iter()
                .any(|part| part.instrument == Instrument::DoubleBass)
        );
    }

    #[test]
    fn short_performance_gates_require_an_explicit_quantization_choice() {
        let data = smf(
            0,
            480,
            &[track(
                &[
                    (0, vec![0x90, 60, 88]),
                    (463, vec![0x80, 60, 0]),
                    (480, vec![0x90, 62, 88]),
                    (943, vec![0x80, 62, 0]),
                ],
                960,
            )],
        );
        assert!(
            parse(&data)
                .unwrap_err()
                .contains("cannot be notated exactly")
        );
        assert!(parse_with_quantization(&data, None).is_err());
        let result = parse_with_quantization(&data, Some(120)).unwrap();
        result.score.validate().unwrap();
        let notes = pitched(&result.score);
        assert_eq!(
            notes
                .iter()
                .map(|note| (note.start, note.ticks()))
                .collect::<Vec<_>>(),
            vec![(0, 480), (480, 480)]
        );
        assert!(result.warnings.iter().any(|warning| warning.contains(
            "2 of 2 notes adjusted to a 120-tick grid; maximum endpoint change 17 ticks"
        )));
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("articulation marks are not recovered"))
        );
    }

    #[test]
    fn fractional_source_ticks_and_unnotatable_gaps_can_be_explicitly_quantized() {
        let data = smf(
            0,
            960,
            &[track(
                &[(1, vec![0x90, 60, 80]), (477, vec![0x80, 60, 0])],
                480,
            )],
        );
        assert!(parse(&data).unwrap_err().contains("480-tick grid"));
        let result = parse_with_quantization(&data, Some(240)).unwrap();
        assert_eq!(
            (
                pitched(&result.score)[0].start,
                pitched(&result.score)[0].ticks()
            ),
            (0, 240)
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("1 of 1 notes adjusted")
                    && warning.contains("maximum endpoint change 3/2 ticks"))
        );
        let odd_gap = smf(
            0,
            480,
            &[track(
                &[(17, vec![0x90, 60, 80]), (497, vec![0x80, 60, 0])],
                500,
            )],
        );
        assert!(parse(&odd_gap).is_err());
        let result = parse_with_quantization(&odd_gap, Some(30)).unwrap();
        assert_eq!(
            (
                pitched(&result.score)[0].start,
                pitched(&result.score)[0].ticks()
            ),
            (30, 480)
        );
    }

    #[test]
    fn rounding_ties_and_minimum_lengths_keep_collapsed_repetitions_as_distinct_notes() {
        let data = smf(
            0,
            480,
            &[track(
                &[
                    (0, vec![0x90, 60, 80]),
                    (1, vec![0x80, 60, 0]),
                    (1, vec![0x90, 60, 80]),
                    (2, vec![0x80, 60, 0]),
                ],
                2,
            )],
        );
        let result = parse_with_quantization(&data, Some(120)).unwrap();
        result.score.validate().unwrap();
        let notes = pitched(&result.score);
        assert_eq!(notes.len(), 2);
        assert_eq!(result.score.parts.len(), 2);
        assert!(
            notes
                .iter()
                .all(|note| note.start == 0 && note.ticks() == 120 && note.pitches[0].midi() == 60)
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("maximum endpoint change 119 ticks"))
        );
        let midpoint = smf(
            0,
            480,
            &[track(
                &[(60, vec![0x90, 64, 80]), (61, vec![0x80, 64, 0])],
                61,
            )],
        );
        let result = parse_with_quantization(&midpoint, Some(120)).unwrap();
        assert_eq!(
            (
                pitched(&result.score)[0].start,
                pitched(&result.score)[0].ticks()
            ),
            (120, 120)
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("maximum endpoint change 179 ticks"))
        );
        let invalid = smf(
            0,
            480,
            &[track(&[(0, vec![0x90, 60, 80]), (0, vec![0x80, 60, 0])], 0)],
        );
        assert!(
            parse_with_quantization(&invalid, Some(120))
                .unwrap_err()
                .contains("positive duration")
        );
    }

    #[test]
    fn supported_grids_are_bounded_and_exact_default_import_does_not_change() {
        let short = smf(
            0,
            480,
            &[track(&[(0, vec![0x90, 60, 80]), (1, vec![0x80, 60, 0])], 1)],
        );
        for grid in QUANTIZATION_GRIDS {
            let result = parse_with_quantization(&short, Some(grid)).unwrap();
            let notes = pitched(&result.score);
            assert_eq!(notes[0].start, 0);
            assert_eq!(notes.iter().map(|note| note.ticks()).sum::<u32>(), grid);
        }
        for grid in [0, 1, 5, 12, 14, 16, 32, 960, u32::MAX] {
            assert!(
                parse_with_quantization(&[], Some(grid))
                    .unwrap_err()
                    .contains("quantization grid")
            );
        }
        let exact = smf(
            0,
            480,
            &[track(
                &[(0, vec![0x90, 60, 80]), (480, vec![0x80, 60, 0])],
                480,
            )],
        );
        let normal = parse(&exact).unwrap();
        let strict = parse_with_quantization(&exact, None).unwrap();
        assert_eq!(normal.score, strict.score);
        assert_eq!(normal.warnings, strict.warnings);
        let chosen = parse_with_quantization(&exact, Some(120)).unwrap();
        assert_eq!(normal.score, chosen.score);
        assert!(
            normal
                .warnings
                .iter()
                .all(|warning| chosen.warnings.contains(warning))
        );
        assert!(
            chosen
                .warnings
                .iter()
                .any(|warning| warning.contains("0 of 1 notes adjusted")
                    && warning.contains("maximum endpoint change 0 ticks"))
        );
    }

    #[test]
    fn minimum_note_extension_preserves_timeline_and_measure_resource_caps() {
        let boundary = smf(
            0,
            480,
            &[track(
                &[(1919, vec![0x90, 60, 80]), (1920, vec![0x80, 60, 0])],
                1920,
            )],
        );
        let result = parse_with_quantization(&boundary, Some(120)).unwrap();
        assert_eq!(result.score.measure_count(), 2);
        assert_eq!(result.score.parts[0].measures[1].notes[0].start, 0);
        assert_eq!(result.score.parts[0].measures[1].notes[0].ticks(), 120);
        let score_end = 1920 * MAX_MEASURES as u32;
        let too_many_measures = smf(
            0,
            480,
            &[track(
                &[
                    (score_end - 1, vec![0x90, 60, 80]),
                    (score_end, vec![0x80, 60, 0]),
                ],
                score_end,
            )],
        );
        assert!(
            parse_with_quantization(&too_many_measures, Some(120))
                .unwrap_err()
                .contains("1024")
        );
        let end = MAX_TIMELINE as u32;
        let beyond_timeline = smf(
            0,
            480,
            &[track(
                &[
                    (0, vec![0xff, 0x58, 4, 32, 0, 24, 8]),
                    (end - 1, vec![0x90, 60, 80]),
                    (end, vec![0x80, 60, 0]),
                ],
                end,
            )],
        );
        assert!(
            parse_with_quantization(&beyond_timeline, Some(120))
                .unwrap_err()
                .contains("beyond Encore")
        );
    }

    #[test]
    fn public_quantized_loader_is_opt_in_bounded_and_preserves_the_source_file() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("encore-midi-options-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        let path = directory.0.join("Performance.MIDI");
        let source = smf(
            0,
            480,
            &[track(
                &[
                    (0, vec![0x90, 60, 80]),
                    (463, vec![0x80, 60, 0]),
                    (480, vec![0x90, 62, 80]),
                    (943, vec![0x80, 62, 0]),
                ],
                960,
            )],
        );
        std::fs::write(&path, &source).unwrap();
        assert!(crate::import::load(&path).is_err());
        let imported = crate::import::load_midi_quantized(&path, 120).unwrap();
        imported.score.validate().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), source);
        assert!(crate::import::load(&path).is_err());
        assert!(
            crate::import::load_midi_quantized(&directory.0.join("missing.mid"), 0)
                .unwrap_err()
                .contains("quantization grid")
        );
        assert!(
            crate::import::load_midi_quantized(&directory.0.join("Score.musicxml"), 120)
                .unwrap_err()
                .contains(".mid or .midi")
        );
        let oversized = directory.0.join("Oversized.mid");
        std::fs::File::create(&oversized)
            .unwrap()
            .set_len(MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(
            crate::import::load_midi_quantized(&oversized, 120)
                .unwrap_err()
                .contains("16 MiB")
        );
    }

    #[test]
    fn quantization_keeps_controller_loss_warnings_and_does_not_recover_articulation_marks() {
        let data = smf(
            0,
            480,
            &[track(
                &[
                    (0, vec![0x90, 60, 80]),
                    (60, vec![0xb0, 64, 127]),
                    (60, vec![0xe0, 0, 64]),
                    (463, vec![0x80, 60, 0]),
                ],
                480,
            )],
        );
        let result = parse_with_quantization(&data, Some(120)).unwrap();
        for loss in [
            "sustain",
            "pitch bend",
            "articulation marks are not recovered",
        ] {
            assert!(result.warnings.iter().any(|warning| warning.contains(loss)));
        }
        assert!(
            pitched(&result.score)
                .iter()
                .all(|note| note.articulations.is_empty())
        );
    }

    #[test]
    fn optional_articulated_midi_qa_source_uses_explicit_quantization_only() {
        let Some(path) = std::env::var_os("ENCORE_TEST_MIDI") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let before = std::fs::read(&path).unwrap();
        assert!(crate::import::load(&path).is_err());
        let result = crate::import::load_midi_quantized(&path, 30).unwrap();
        result.score.validate().unwrap();
        assert!(!pitched(&result.score).is_empty());
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.starts_with("Explicit MIDI quantization:")
                    && !warning.contains(": 0 of"))
        );
        for warning in &result.warnings {
            if warning.starts_with("Explicit MIDI quantization:") {
                eprintln!("Quantized QA source: {warning}");
            }
        }
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}
