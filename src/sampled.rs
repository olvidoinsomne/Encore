//! Recorded SF2 playback for the complete 128-program General MIDI catalog.
//!
//! Each sampled part owns an independent synthesizer sharing one immutable bank, so program changes,
//! identical pitches and release tails cannot interfere across staves. There are
//! no MIDI-channel limits on the number of staves.
//!
//! Commands use rounded sample positions. OxiSynth buffers DSP internally in
//! fixed 64-frame blocks, bounding additional command latency to 0–63 samples,
//! under 1.43 ms at 44.1 kHz. Rendering up to each command avoids adding larger
//! application-level timing quantization. SF2 modulators are honored by OxiSynth.

use crate::{
    model::{Instrument, Note, PPQ, Part, Score},
    synth::{self, MAX_RENDER_SECONDS, SAMPLE_RATE},
};
use oxisynth::{MidiEvent, SoundFont, SoundFontId, Synth, SynthDescriptor};
use std::{
    collections::HashSet,
    f32::consts::{FRAC_PI_2, SQRT_2},
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

pub const MAX_SOUNDFONT_BYTES: u64 = 256 * 1024 * 1024;
const RELEASE_TAIL: f32 = 3.0;
const MIX_BLOCK: usize = 512;
/// The bank must provide every melodic General MIDI program. Electronic patches
/// and sound effects also come from its SF2 sample regions, never DSP substitutes.
pub fn supports(instrument: Instrument) -> bool {
    instrument.gm_program() < 128
}

#[derive(Clone)]
pub struct SampleBank {
    font: Arc<Mutex<Option<SoundFont>>>,
    path: PathBuf,
    name: String,
    presets: Vec<(u8, String)>,
    calibrated_general_user: bool,
}

impl SampleBank {
    /// Load and validate all 128 bank-0 General MIDI presets.
    /// Missing patches are errors; an implicit piano fallback is never
    /// used for a requested sampled instrument.
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("Could not read SoundFont {}: {e}", path.display()))?;
        if !metadata.is_file() {
            return Err("Choose an SF2 file, not a directory or device.".into());
        }
        if metadata.len() > MAX_SOUNDFONT_BYTES {
            return Err("SoundFont exceeds the 256 MiB loading limit.".into());
        }
        let file = File::open(path).map_err(|e| format!("Could not open SoundFont: {e}"))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(metadata.len() as usize)
            .map_err(|_| "Not enough memory to load this SoundFont.".to_string())?;
        file.take(MAX_SOUNDFONT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Could not read SoundFont: {e}"))?;
        if bytes.len() as u64 > MAX_SOUNDFONT_BYTES {
            return Err("SoundFont exceeds the 256 MiB loading limit.".into());
        }
        validate_riff(&bytes)?;
        // A corrupt third-party bank must remain a recoverable load error even
        // if an invalid internal table triggers a parser assertion.
        let font = std::panic::catch_unwind(|| SoundFont::load(&mut Cursor::new(&bytes)))
            .map_err(|_| {
                "Malformed SoundFont: an internal table could not be decoded.".to_string()
            })?
            .map_err(|e| format!("Could not decode SF2 SoundFont: {e}"))?;
        let presets = required_presets(&bytes)?;
        let supplied_name = list(&bytes[12..], b"INFO")
            .and_then(|data| chunk(data, b"INAM"))
            .map(decode_name)
            .unwrap_or_default();
        let name = if supplied_name.is_empty() {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            supplied_name
        };
        // These trims were measured against the shipped 2.0.3 bank. Other SF2
        // libraries keep their native relative preset balance and gain.
        let calibrated_general_user = name.starts_with("GeneralUser GS");
        // Verify OxiSynth agrees with the raw catalog before accepting the bank.
        let mut probe = Synth::new(descriptor())
            .map_err(|e| format!("Could not initialize recorded instruments: {e}"))?;
        let id = probe.add_font(font, false);
        for instrument in Instrument::ALL {
            probe
                .select_program(0, id, 0, instrument.gm_program())
                .map_err(|e| format!("SoundFont cannot select {}: {e}", instrument.name()))?;
        }
        let font = probe
            .remove_font(id, false)
            .ok_or("Could not retain decoded SoundFont.")?;
        Ok(Self {
            font: Arc::new(Mutex::new(Some(font))),
            path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
            name,
            presets,
            calibrated_general_user,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn preset_name(&self, instrument: Instrument) -> Option<&str> {
        self.presets
            .iter()
            .find(|(program, _)| *program == instrument.gm_program())
            .map(|(_, name)| name.as_str())
    }

    pub fn render(&self, score: &Score) -> Result<Vec<f32>, String> {
        score.validate()?;
        let duration = synth::duration_seconds(score);
        if !duration.is_finite() || duration > MAX_RENDER_SECONDS {
            return Err("Audio rendering supports scores up to 10 minutes.".into());
        }
        let frames = ((duration + RELEASE_TAIL) * SAMPLE_RATE as f32).ceil() as usize;
        let mut output = vec![0.0; frames * 2];
        let any_solo = score.parts.iter().any(|part| part.solo && !part.muted);
        for part in &score.parts {
            if part.muted || part.volume <= 0.0 || (any_solo && !part.solo) {
                continue;
            }
            let events = timeline(part, score.measure_ticks(), score.tempo);
            if events.is_empty() {
                continue;
            }
            let mut engine = self.engine(part.instrument)?;
            guarded_mix(
                &mut engine.engine,
                &events,
                &mut output,
                part.volume * self.instrument_trim(part.instrument),
                part.pan,
            )?;
        }
        synth::finish(&mut output);
        Ok(output)
    }

    pub fn render_note(
        &self,
        instrument: Instrument,
        midi: u8,
        velocity: u8,
        duration_secs: f32,
    ) -> Result<Vec<f32>, String> {
        if midi > 127 || velocity > 127 {
            return Err("MIDI pitch and velocity must be 0–127.".into());
        }
        if !duration_secs.is_finite() || !(0.01..=120.0).contains(&duration_secs) {
            return Err("Audition duration must be between 0.01 and 120 seconds.".into());
        }
        if self.preset_name(instrument).is_none() {
            return Err(format!("No sampled preset for {}.", instrument.name()));
        }
        let frames = ((duration_secs + RELEASE_TAIL) * SAMPLE_RATE as f32).ceil() as usize;
        let mut output = vec![0.0; frames * 2];
        if velocity > 0 {
            let events = [
                Command {
                    frame: 0,
                    midi,
                    kind: Kind::On(velocity),
                },
                Command {
                    frame: (duration_secs * SAMPLE_RATE as f32).round() as usize,
                    midi,
                    kind: Kind::Off,
                },
            ];
            let mut engine = self.engine(instrument)?;
            guarded_mix(
                &mut engine.engine,
                &events,
                &mut output,
                self.instrument_trim(instrument),
                0.0,
            )?;
        }
        synth::finish(&mut output);
        Ok(output)
    }

    fn engine(&self, instrument: Instrument) -> Result<EngineLease<'_>, String> {
        if self.preset_name(instrument).is_none() {
            return Err(format!("No sampled preset for {}.", instrument.name()));
        }
        let mut engine = Synth::new(descriptor())
            .map_err(|e| format!("Could not initialize recorded instruments: {e}"))?;
        engine.set_gain(if self.calibrated_general_user {
            0.8
        } else {
            0.25
        });
        // OxiSynth consumes a SoundFont rather than an Arc. Loan the immutable
        // decoded font to one fresh engine at a time. Releasing each part's loan
        // lets an audition run between score parts without decoding samples twice.
        let mut slot = self
            .font
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let font = slot
            .take()
            .ok_or("Recorded SoundFont is unavailable; reload the bank.")?;
        let id = engine.add_font(font, false);
        let mut lease = EngineLease { engine, id, slot };
        lease
            .engine
            .select_program(0, id, 0, instrument.gm_program())
            .map_err(|e| format!("Could not select {}: {e}", instrument.name()))?;
        // Mixer volume is applied once, linearly, after synthesis. MIDI velocity
        // is passed untouched to the bank's recorded layers and SF2 modulators.
        for (ctrl, value) in [(7, 127), (11, 127), (10, 64)] {
            lease
                .engine
                .send_event(MidiEvent::ControlChange {
                    channel: 0,
                    ctrl,
                    value,
                })
                .map_err(|e| format!("Could not initialize sampled mixer: {e}"))?;
        }
        Ok(lease)
    }

    fn instrument_trim(&self, instrument: Instrument) -> f32 {
        if !self.calibrated_general_user {
            return 1.0;
        }
        // Register-appropriate, velocity-100 body RMS measurements set these
        // conservative orchestral trims. The seven added woodwind/harp voices
        // were matched to the existing sampled section's relative body level;
        // all other GM patches retain native bank balance. These trims preserve
        // recorded envelopes and velocity response without peak normalization.
        let instrument = Instrument::from_gm_program(instrument.gm_program()).unwrap_or(instrument);
        match instrument {
            Instrument::Piano => 2.0,
            Instrument::Violin => 0.85,
            Instrument::Viola => 1.1,
            Instrument::Cello => 0.9,
            Instrument::DoubleBass => 2.2,
            Instrument::Flute => 0.8,
            Instrument::Piccolo => 0.5,
            Instrument::EnglishHorn => 1.3,
            Instrument::Harp => 1.2,
            Instrument::FrenchHorn => 1.0,
            Instrument::Trumpet => 1.4,
            Instrument::Trombone => 1.35,
            Instrument::Tuba => 2.8,
            Instrument::Timpani => 2.3,
            Instrument::StringEnsemble => 1.6,
            _ => 1.0,
        }
    }
}

impl std::fmt::Debug for SampleBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SampleBank")
            .field("path", &self.path)
            .field("name", &self.name)
            .field("presets", &self.presets)
            .finish_non_exhaustive()
    }
}

struct EngineLease<'a> {
    // Fields drop in this order, keeping the font locked until the old engine's
    // remaining voice references have also been released.
    engine: Synth,
    id: SoundFontId,
    slot: MutexGuard<'a, Option<SoundFont>>,
}

impl Drop for EngineLease<'_> {
    fn drop(&mut self) {
        *self.slot = self.engine.remove_font(self.id, false);
    }
}

fn descriptor() -> SynthDescriptor {
    SynthDescriptor {
        reverb_active: false,
        chorus_active: false,
        drums_channel_active: false,
        sample_rate: SAMPLE_RATE as f32,
        polyphony: 512,
        gain: 0.25,
        min_note_length: 0,
        ..SynthDescriptor::default()
    }
}

fn decode_name(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end])
        .trim()
        .chars()
        .take(256)
        .collect()
}

// Called only after validate_riff has established all chunk bounds.
fn chunk<'a>(bytes: &'a [u8], id: &[u8; 4]) -> Option<&'a [u8]> {
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let start = offset + 8;
        let end = start.checked_add(size)?;
        let data = bytes.get(start..end)?;
        if &bytes[offset..offset + 4] == id {
            return Some(data);
        }
        offset = end + (size & 1);
    }
    None
}

fn list<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    let mut offset = 0;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let start = offset + 8;
        let end = start.checked_add(size)?;
        let data = bytes.get(start..end)?;
        if &bytes[offset..offset + 4] == b"LIST" && data.get(..4)? == kind {
            return data.get(4..);
        }
        offset = end + (size & 1);
    }
    None
}

fn required_presets(bytes: &[u8]) -> Result<Vec<(u8, String)>, String> {
    let headers = list(&bytes[12..], b"pdta")
        .and_then(|data| chunk(data, b"phdr"))
        .ok_or("Malformed SoundFont: preset headers are missing.")?;
    let (records, remainder) = headers.as_chunks::<38>();
    if !remainder.is_empty() || records.len() < 2 {
        return Err("Malformed SoundFont: incomplete preset header table.".into());
    }
    let mut names = Vec::new();
    let mut missing = Vec::new();
    for instrument in Instrument::ALL {
        let found = records.windows(2).find(|pair| {
            u16::from_le_bytes([pair[0][22], pair[0][23]]) == 0
                && u16::from_le_bytes([pair[0][20], pair[0][21]]) == instrument.gm_program() as u16
                && u16::from_le_bytes([pair[1][24], pair[1][25]])
                    > u16::from_le_bytes([pair[0][24], pair[0][25]])
        });
        match found {
            Some(pair) => names.push((instrument.gm_program(), decode_name(&pair[0][..20]))),
            None => missing.push(format!(
                "{} (GM {})",
                instrument.name(),
                instrument.gm_program() as u16 + 1
            )),
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "SoundFont is missing required bank-0 presets: {}.",
            missing.join(", ")
        ));
    }
    Ok(names)
}

// Bound every chunk before handing data to the SF2 parser. The file-size cap by
// itself cannot protect against a corrupt small file claiming a huge inner chunk.
fn validate_riff(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"sfbk" {
        return Err("Choose an uncompressed SF2 SoundFont (RIFF/sfbk).".into());
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if length.checked_add(8) != Some(bytes.len()) {
        return Err("Malformed SoundFont: RIFF length does not match the file.".into());
    }
    fn chunks(bytes: &[u8], depth: usize) -> Result<(), String> {
        if depth > 4 {
            return Err("Malformed SoundFont: excessive chunk nesting.".into());
        }
        let mut offset = 0;
        while offset < bytes.len() {
            if bytes.len() - offset < 8 {
                return Err("Malformed SoundFont: incomplete chunk header.".into());
            }
            let size =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let start = offset + 8;
            let end = start
                .checked_add(size)
                .filter(|end| *end <= bytes.len())
                .ok_or_else(|| {
                    "Malformed SoundFont: a chunk extends beyond its container.".to_string()
                })?;
            if &bytes[offset..offset + 4] == b"LIST" {
                if size < 4 {
                    return Err("Malformed SoundFont: incomplete LIST chunk.".into());
                }
                chunks(&bytes[start + 4..end], depth + 1)?;
            }
            offset = end + (size & 1);
            if offset > bytes.len() {
                return Err("Malformed SoundFont: missing chunk padding.".into());
            }
        }
        Ok(())
    }
    chunks(&bytes[12..], 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Off,
    On(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Command {
    frame: usize,
    midi: u8,
    kind: Kind,
}

#[derive(Clone, Copy)]
struct Event<'a> {
    tick: u64,
    note: &'a Note,
}

fn timeline(part: &Part, measure_ticks: u32, tempo: u16) -> Vec<Command> {
    let mut notes = Vec::new();
    for (measure_index, measure) in part.measures.iter().enumerate() {
        for note in &measure.notes {
            notes.push(Event {
                tick: measure_index as u64 * measure_ticks as u64 + note.start as u64,
                note,
            });
        }
    }
    notes.sort_by_key(|note| note.tick);
    let frames_per_tick = 60.0_f64 * SAMPLE_RATE as f64 / (tempo.max(1) as f64 * PPQ as f64);
    let mut consumed = HashSet::new();
    let mut commands = Vec::new();
    for (index, event) in notes.iter().enumerate() {
        if event.note.velocity == 0 {
            continue;
        }
        for pitch in &event.note.pitches {
            let midi = pitch.midi();
            if consumed.contains(&(index, midi)) {
                continue;
            }
            let mut end = event.tick + event.note.ticks() as u64;
            let mut last = event.note;
            let mut current = index;
            while last.tied {
                let next = notes
                    .iter()
                    .enumerate()
                    .skip(current + 1)
                    .take_while(|(_, note)| note.tick <= end)
                    .find(|(_, note)| {
                        note.tick == end && note.note.pitches.iter().any(|p| p.midi() == midi)
                    });
                let Some((next_index, next_note)) = next else {
                    break;
                };
                if !consumed.insert((next_index, midi)) {
                    break;
                }
                end += next_note.note.ticks() as u64;
                last = next_note.note;
                current = next_index;
            }
            let shortening = if last.staccato {
                last.ticks() as f64 * 0.52
            } else {
                0.0
            };
            let gate_end = end as f64 - shortening;
            let start_frame = (event.tick as f64 * frames_per_tick).round() as usize;
            let end_frame = ((gate_end * frames_per_tick).round() as usize).max(start_frame + 1);
            commands.push(Command {
                frame: start_frame,
                midi,
                kind: Kind::On(event.note.velocity),
            });
            commands.push(Command {
                frame: end_frame,
                midi,
                kind: Kind::Off,
            });
        }
    }
    commands.sort_by_key(|command| {
        (
            command.frame,
            matches!(command.kind, Kind::On(_)),
            command.midi,
        )
    });
    commands
}

fn mix_part(
    engine: &mut Synth,
    events: &[Command],
    output: &mut [f32],
    volume: f32,
    pan: f32,
) -> Result<(), String> {
    let frames = output.len() / 2;
    // This internal gain includes the bank-specific trim after validated mixer
    // volume, so a quiet bass/tuba patch may legitimately exceed unity here.
    let volume = volume.clamp(0.0, 8.0);
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_2 * 0.5;
    // The sampler has already panned mono sample regions to stereo. Multiplying
    // by sqrt(2) gives unity at center and the same equal-power law as synthesis.
    let left_gain = angle.cos() * SQRT_2 * volume;
    let right_gain = angle.sin() * SQRT_2 * volume;
    let mut left = [0.0; MIX_BLOCK];
    let mut right = [0.0; MIX_BLOCK];
    let mut cursor = 0;
    let mut event_index = 0;
    while cursor < frames {
        while event_index < events.len() && events[event_index].frame <= cursor {
            let event = events[event_index];
            match event.kind {
                Kind::On(vel) => engine.send_event(MidiEvent::NoteOn {
                    channel: 0,
                    key: event.midi,
                    vel,
                }),
                Kind::Off => engine.send_event(MidiEvent::NoteOff {
                    channel: 0,
                    key: event.midi,
                }),
            }
            .map_err(|e| format!("Recorded instrument event failed: {e}"))?;
            event_index += 1;
        }
        let next = events
            .get(event_index)
            .map_or(frames, |event| event.frame.min(frames));
        let count = (next - cursor).min(MIX_BLOCK);
        if count == 0 {
            break;
        }
        engine.write((&mut left[..count], &mut right[..count]));
        for index in 0..count {
            let offset = (cursor + index) * 2;
            output[offset] += left[index] * left_gain;
            output[offset + 1] += right[index] * right_gain;
        }
        cursor += count;
    }
    Ok(())
}

fn guarded_mix(
    engine: &mut Synth,
    events: &[Command],
    output: &mut [f32],
    volume: f32,
    pan: f32,
) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mix_part(engine, events, output, volume, pan)
    }))
    .map_err(|_| "Recorded renderer could not process this SoundFont safely.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Pitch};

    fn score() -> Score {
        let mut score = Score::blank();
        score.tempo = 120;
        score.parts.truncate(1);
        score.parts[0].measures.truncate(2);
        score
    }

    fn note(start: u32, pitches: &[u8]) -> Note {
        Note {
            start,
            duration: Duration::Quarter,
            dotted: false,
            pitches: pitches.iter().map(|m| Pitch::from_midi(*m)).collect(),
            velocity: 100,
            staccato: false,
            tied: false,
        }
    }

    #[test]
    fn complete_catalog_and_contrabass_program_are_explicit() {
        assert_eq!(
            Instrument::ALL
                .iter()
                .filter(|kind| supports(**kind))
                .count(),
            128
        );
        assert!(supports(Instrument::DoubleBass));
        assert_eq!(Instrument::DoubleBass.gm_program(), 43);
        for kind in [
            Instrument::Flute,
            Instrument::Clarinet,
            Instrument::Oboe,
            Instrument::Harp,
        ] {
            assert!(supports(kind));
        }
        assert!(!supports(Instrument::GeneralMidi(128)));
    }

    #[test]
    fn timeline_respects_gaps_rests_and_repeated_note_order() {
        let mut score = score();
        score.parts[0].measures[0].notes =
            vec![note(0, &[]), note(PPQ, &[60]), note(PPQ * 2, &[60])];
        let events = timeline(&score.parts[0], score.measure_ticks(), score.tempo);
        assert_eq!(
            events,
            vec![
                Command {
                    frame: 22_050,
                    midi: 60,
                    kind: Kind::On(100)
                },
                Command {
                    frame: 44_100,
                    midi: 60,
                    kind: Kind::Off
                },
                Command {
                    frame: 44_100,
                    midi: 60,
                    kind: Kind::On(100)
                },
                Command {
                    frame: 66_150,
                    midi: 60,
                    kind: Kind::Off
                },
            ]
        );
    }

    #[test]
    fn timeline_merges_partial_chord_ties_across_barlines() {
        let mut score = score();
        let mut first = note(PPQ * 3, &[60, 64]);
        first.tied = true;
        score.parts[0].measures[0].notes = vec![first];
        score.parts[0].measures[1].notes = vec![note(0, &[60, 67])];
        let events = timeline(&score.parts[0], score.measure_ticks(), score.tempo);
        let ons: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, Kind::On(_)))
            .map(|e| (e.frame, e.midi))
            .collect();
        assert_eq!(ons, vec![(66_150, 60), (66_150, 64), (88_200, 67)]);
        assert!(events.contains(&Command {
            frame: 88_200,
            midi: 64,
            kind: Kind::Off
        }));
        assert!(events.contains(&Command {
            frame: 110_250,
            midi: 60,
            kind: Kind::Off
        }));
    }

    #[test]
    fn staccato_and_tempo_use_rounded_sample_positions() {
        let mut score = score();
        score.tempo = 137;
        let mut event = note(120, &[67]);
        event.staccato = true;
        score.parts[0].measures[0].notes = vec![event];
        let events = timeline(&score.parts[0], score.measure_ticks(), score.tempo);
        let frames_per_tick = 60.0_f64 * SAMPLE_RATE as f64 / (137.0 * PPQ as f64);
        assert_eq!(events[0].frame, (120.0 * frames_per_tick).round() as usize);
        assert_eq!(
            events[1].frame,
            ((120.0 + 480.0 * 0.48) * frames_per_tick).round() as usize
        );
    }

    #[test]
    fn malformed_and_oversized_files_are_recoverable_errors() {
        let directory =
            std::env::temp_dir().join(format!("encore-sf2-errors-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("invalid.sf2");
        std::fs::write(&path, b"not a SoundFont").unwrap();
        assert!(SampleBank::load(&path).unwrap_err().contains("SF2"));
        assert!(
            SampleBank::load(&directory)
                .unwrap_err()
                .contains("directory")
        );
        assert!(SampleBank::load(&directory.join("missing.sf2")).is_err());
        let sparse = directory.join("huge.sf2");
        File::create(&sparse)
            .unwrap()
            .set_len(MAX_SOUNDFONT_BYTES + 1)
            .unwrap();
        assert!(SampleBank::load(&sparse).unwrap_err().contains("256 MiB"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_inner_chunk_lengths_are_rejected_before_decoding() {
        let mut bytes = b"RIFF\x10\0\0\0sfbkLIST\xff\xff\xff\x7fINFO".to_vec();
        let length = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        assert!(
            validate_riff(&bytes)
                .unwrap_err()
                .contains("extends beyond")
        );
    }

    #[test]
    fn catalog_rejects_plucked_bass_as_a_substitute_for_contrabass() {
        // A metadata-only RIFF fixture validates programs without substituting
        // generated waveforms for the recorded integration bank.
        let mut headers = Vec::new();
        for (index, instrument) in Instrument::ALL.iter().enumerate() {
            let mut record = [0_u8; 38];
            record[..4].copy_from_slice(b"Test");
            record[20..22].copy_from_slice(&(instrument.gm_program() as u16).to_le_bytes());
            record[24..26].copy_from_slice(&(index as u16).to_le_bytes());
            headers.extend_from_slice(&record);
        }
        let mut terminal = [0_u8; 38];
        terminal[..3].copy_from_slice(b"EOP");
        terminal[24..26].copy_from_slice(&(Instrument::ALL.len() as u16).to_le_bytes());
        headers.extend_from_slice(&terminal);
        let mut pdta = b"pdtaphdr".to_vec();
        pdta.extend_from_slice(&(headers.len() as u32).to_le_bytes());
        pdta.extend_from_slice(&headers);
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&((pdta.len() + 12) as u32).to_le_bytes());
        bytes.extend_from_slice(b"sfbkLIST");
        bytes.extend_from_slice(&(pdta.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&pdta);
        validate_riff(&bytes).unwrap();
        assert_eq!(required_presets(&bytes).unwrap().len(), 128);
        // Bowed Contrabass43 cannot be replaced by plucked Acoustic Bass32.
        let program_offset = 32 + 43 * 38 + 20;
        bytes[program_offset..program_offset + 2].copy_from_slice(&32_u16.to_le_bytes());
        let error = required_presets(&bytes).unwrap_err();
        assert!(error.contains("bass") || error.contains("Bass"), "{error}");
    }

    fn integration_bank() -> Option<SampleBank> {
        let configured = std::env::var_os("ENCORE_TEST_SOUNDFONT");
        let path = configured.as_ref().map(PathBuf::from).unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/soundfonts/GeneralUser-GS.sf2")
        });
        if !path.exists() && configured.is_none() {
            eprintln!(
                "Skipping recorded-bank integration: set ENCORE_TEST_SOUNDFONT to a complete SF2 bank."
            );
            return None;
        }
        Some(
            SampleBank::load(&path)
                .expect("integration SoundFont must have all 128 General MIDI presets"),
        )
    }

    #[test]
    fn recorded_bank_renders_every_preset_and_preserves_solo() {
        let Some(bank) = integration_bank() else {
            return;
        };
        for kind in Instrument::ALL {
            assert!(bank.preset_name(*kind).is_some());
            let profile = crate::instruments::profile(*kind);
            let samples = bank.render_note(*kind, profile.audition, 100, 0.6).unwrap();
            assert!(
                samples
                    .iter()
                    .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
            );
            assert!(
                samples.iter().any(|sample| sample.abs() > 0.0005),
                "{kind:?} / {:?} must contain audible sampled audio",
                bank.preset_name(*kind)
            );
        }
        let mut score = score();
        score.parts[0].instrument = Instrument::Violin;
        score.parts[0].solo = true;
        let mut flute = score.parts[0].clone();
        flute.instrument = Instrument::Flute;
        flute.solo = false;
        flute.measures[0].notes = vec![note(0, &[72])];
        score.parts.push(flute);
        assert!(
            bank.render(&score)
                .unwrap()
                .iter()
                .all(|sample| *sample == 0.0)
        );
        score.parts[0].solo = false;
        assert!(
            bank.render(&score)
                .unwrap()
                .iter()
                .any(|sample| sample.abs() > 0.01)
        );
        score.parts[1].muted = true;
        assert!(
            bank.render(&score)
                .unwrap()
                .iter()
                .all(|sample| *sample == 0.0)
        );
        assert_ne!(
            bank.render_note(Instrument::Harp, 60, 90, 0.3).unwrap(),
            synth::render_note(Instrument::Harp, 60, 90, 0.3)
        );
    }

    fn stereo_rms(samples: &[f32], start: f32, end: f32) -> f32 {
        let start = (start * SAMPLE_RATE as f32) as usize * 2;
        let end = (end * SAMPLE_RATE as f32) as usize * 2;
        let segment = &samples[start..end];
        (segment.iter().map(|sample| sample * sample).sum::<f32>() / segment.len() as f32).sqrt()
    }

    #[test]
    fn recorded_bank_preserves_sustained_low_bass_and_recorded_dynamics() {
        let Some(bank) = integration_bank() else {
            return;
        };
        let bass = bank
            .render_note(Instrument::DoubleBass, 28, 100, 4.0)
            .unwrap();
        let attack = stereo_rms(&bass, 0.08, 0.40);
        let early = stereo_rms(&bass, 0.70, 1.50);
        let late = stereo_rms(&bass, 3.00, 3.80);
        let released = stereo_rms(&bass, 6.40, 6.90);
        assert!(
            late > 0.001,
            "Bowed E1 must remain audible throughout a four-second hold."
        );
        assert!(
            late > early * 0.35 && late > attack * 0.25,
            "Bowed bass must sustain rather than decay like a plucked bass."
        );
        assert!(
            released < late * 0.30,
            "Bowed bass must stop after note release."
        );

        let timpani = bank
            .render_note(Instrument::Timpani, 40, 108, 0.25)
            .unwrap();
        let strike = stereo_rms(&timpani, 0.04, 0.22);
        let ring = stereo_rms(&timpani, 0.30, 0.65);
        assert!(
            strike > 0.001 && ring > 0.0005 && ring > strike * 0.04,
            "Low E2 timpani must sound and retain a short resonant release."
        );

        for (instrument, midi) in [
            (Instrument::Piano, 60),
            (Instrument::FrenchHorn, 55),
            (Instrument::Trumpet, 60),
        ] {
            let soft = bank.render_note(instrument, midi, 32, 1.2).unwrap();
            let loud = bank.render_note(instrument, midi, 112, 1.2).unwrap();
            let soft_level = stereo_rms(&soft, 0.12, 0.90);
            let loud_level = stereo_rms(&loud, 0.12, 0.90);
            assert!(
                loud_level > 0.002 && loud_level > soft_level * 1.5,
                "{instrument:?} must retain recorded velocity response."
            );
        }
    }

    #[test]
    fn program_aliases_select_the_same_samples_and_calibration() {
        let Some(bank) = integration_bank() else {
            return;
        };
        assert_eq!(
            bank.preset_name(Instrument::GeneralMidi(0)),
            bank.preset_name(Instrument::Piano)
        );
        assert_eq!(
            bank.render_note(Instrument::GeneralMidi(0), 60, 95, 0.4)
                .unwrap(),
            bank.render_note(Instrument::Piano, 60, 95, 0.4).unwrap()
        );
        assert!(
            bank.render_note(Instrument::GeneralMidi(128), 60, 0, 0.4)
                .unwrap_err()
                .contains("No sampled preset")
        );
    }

    #[test]
    fn font_lease_restores_the_bank_after_unwinding() {
        let Some(bank) = integration_bank() else {
            return;
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lease = bank.engine(Instrument::Piano).unwrap();
            panic!("exercise recorded-bank lease cleanup");
        }));
        assert!(result.is_err());
        let samples = bank.render_note(Instrument::Piano, 60, 100, 0.25).unwrap();
        assert!(samples.iter().any(|sample| sample.abs() > 0.005));
    }
}
