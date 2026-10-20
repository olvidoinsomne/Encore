//! Interchange exporters written from the public MusicXML and Standard MIDI formats.
use crate::model::{Clef, Duration, Note, PPQ, Part, Pitch, Score};
use std::collections::HashMap;
use std::fmt::Write as _;

fn xml(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            c == '\t'
                || c == '\n'
                || c == '\r'
                || ('\u{20}'..='\u{d7ff}').contains(&c)
                || ('\u{e000}'..='\u{fffd}').contains(&c)
                || c >= '\u{10000}'
        })
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn channel(index: usize) -> u8 {
    let n = (index % 15) as u8;
    if n >= 9 { n + 1 } else { n }
}
fn flattened(part: &Part, measure_ticks: u32) -> Vec<(u32, &Note)> {
    part.measures
        .iter()
        .enumerate()
        .flat_map(|(bar, measure)| {
            measure
                .notes
                .iter()
                .map(move |note| (bar as u32 * measure_ticks + note.start, note))
        })
        .collect()
}
struct Timeline<'a> {
    notes: Vec<(u32, &'a Note)>,
    starts: HashMap<u32, usize>,
    ends: HashMap<u32, usize>,
}
impl<'a> Timeline<'a> {
    fn new(part: &'a Part, measure_ticks: u32) -> Self {
        let notes = flattened(part, measure_ticks);
        let starts = notes
            .iter()
            .enumerate()
            .map(|(i, (t, _))| (*t, i))
            .collect();
        let ends = notes
            .iter()
            .enumerate()
            .map(|(i, (t, n))| (*t + n.ticks(), i))
            .collect();
        Self {
            notes,
            starts,
            ends,
        }
    }
    fn tie_in(&self, start: u32, midi: u8) -> bool {
        self.ends.get(&start).is_some_and(|i| {
            self.notes[*i].1.tied && self.notes[*i].1.pitches.iter().any(|p| p.midi() == midi)
        })
    }
    fn tie_out(&self, start: u32, note: &Note, midi: u8) -> bool {
        note.tied
            && self
                .starts
                .get(&(start + note.ticks()))
                .is_some_and(|i| self.notes[*i].1.pitches.iter().any(|p| p.midi() == midi))
    }
}

/// A concert-pitch MusicXML 4.0 score. Gaps are filled by explicit rests, so
/// every part has an identical measured timeline in other notation programs.
pub fn musicxml(score: &Score) -> String {
    if score.validate().is_err() {
        return String::new();
    }
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n<score-partwise version=\"4.0\">\n",
    );
    let _ = writeln!(
        out,
        "  <work><work-title>{}</work-title></work>\n  <movement-title>{}</movement-title>\n  <identification><creator type=\"composer\">{}</creator><encoding><software>Encore</software></encoding></identification>",
        xml(&score.title),
        xml(&score.title),
        xml(&score.composer)
    );
    out.push_str("  <defaults><scaling><millimeters>7</millimeters><tenths>40</tenths></scaling></defaults>\n  <part-list>\n");
    for (pi, part) in score.parts.iter().enumerate() {
        let id = pi + 1;
        let volume = if part.muted || (score.parts.iter().any(|p| p.solo) && !part.solo) {
            0.0
        } else {
            part.volume * 100.0
        };
        let _ = writeln!(
            out,
            "    <score-part id=\"P{id}\"><part-name>{}</part-name><part-abbreviation>{}</part-abbreviation><score-instrument id=\"I{id}\"><instrument-name>{}</instrument-name></score-instrument><midi-device port=\"{}\"/><midi-instrument id=\"I{id}\"><midi-channel>{}</midi-channel><midi-program>{}</midi-program><volume>{volume:.2}</volume><pan>{:.2}</pan></midi-instrument></score-part>",
            xml(&part.name),
            xml(part.instrument.name()),
            xml(part.instrument.name()),
            pi / 15 + 1,
            channel(pi) + 1,
            part.instrument.gm_program() + 1,
            part.pan * 90.0
        );
    }
    out.push_str("  </part-list>\n");
    let measure_ticks = score.measure_ticks();
    for (pi, part) in score.parts.iter().enumerate() {
        let _ = writeln!(out, "  <part id=\"P{}\">", pi + 1);
        let timeline = Timeline::new(part, measure_ticks);
        let mut previous_dynamic = None;
        for (mi, measure) in part.measures.iter().enumerate() {
            let _ = writeln!(out, "    <measure number=\"{}\">", mi + 1);
            if mi == 0 {
                let (sign, line) = match part.clef {
                    Clef::Treble => ("G", 2),
                    Clef::Bass => ("F", 4),
                    Clef::Alto => ("C", 3),
                };
                let _ = writeln!(
                    out,
                    "      <attributes><divisions>{PPQ}</divisions><key><fifths>{}</fifths></key><time><beats>{}</beats><beat-type>{}</beat-type></time><clef><sign>{sign}</sign><line>{line}</line></clef></attributes>",
                    score.key_fifths, score.time_numerator, score.time_denominator
                );
                let _ = writeln!(
                    out,
                    "      <direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome></direction-type><sound tempo=\"{}\"/></direction>",
                    score.tempo, score.tempo
                );
            }
            if measure.notes.is_empty() {
                let _ = writeln!(
                    out,
                    "      <note><rest measure=\"yes\"/><duration>{measure_ticks}</duration></note>"
                );
            } else {
                let mut cursor = 0;
                for note in &measure.notes {
                    if note.start > cursor {
                        write_gap(&mut out, note.start - cursor);
                    }
                    let start = mi as u32 * measure_ticks + note.start;
                    if note.pitches.is_empty() {
                        write_xml_note(&mut out, note, None, false, false, false);
                    } else {
                        let dynamic = crate::model::dynamic_name(note.velocity);
                        if note.velocity > 0 && previous_dynamic != Some(dynamic) {
                            let _ = writeln!(
                                out,
                                "      <direction placement=\"below\"><direction-type><dynamics><{dynamic}/></dynamics></direction-type></direction>"
                            );
                            previous_dynamic = Some(dynamic);
                        }
                        for (chord_index, pitch) in note.pitches.iter().enumerate() {
                            write_xml_note(
                                &mut out,
                                note,
                                Some(*pitch),
                                chord_index > 0,
                                timeline.tie_in(start, pitch.midi()),
                                timeline.tie_out(start, note, pitch.midi()),
                            );
                        }
                    }
                    cursor = note.start + note.ticks();
                }
                if cursor < measure_ticks {
                    write_gap(&mut out, measure_ticks - cursor);
                }
            }
            if mi + 1 == part.measures.len() {
                out.push_str("      <barline location=\"right\"><bar-style>light-heavy</bar-style></barline>\n");
            }
            out.push_str("    </measure>\n");
        }
        out.push_str("  </part>\n");
    }
    out.push_str("</score-partwise>\n");
    out
}

fn write_gap(out: &mut String, mut ticks: u32) {
    for duration in Duration::ALL {
        for dotted in [true, false] {
            let size = duration.ticks() + if dotted { duration.ticks() / 2 } else { 0 };
            while ticks >= size {
                let note = Note {
                    start: 0,
                    duration: *duration,
                    dotted,
                    pitches: Vec::new(),
                    velocity: 64,
                    staccato: false,
                    tied: false,
                };
                write_xml_note(out, &note, None, false, false, false);
                ticks -= size;
            }
        }
    }
    // Unquantized imported/edit positions remain exact even without a conventional glyph.
    if ticks > 0 {
        let _ = writeln!(
            out,
            "      <note><rest/><duration>{ticks}</duration></note>"
        );
    }
}
fn write_xml_note(
    out: &mut String,
    note: &Note,
    pitch: Option<Pitch>,
    chord: bool,
    tie_in: bool,
    tie_out: bool,
) {
    if pitch.is_some() {
        let _ = write!(
            out,
            "      <note dynamics=\"{:.3}\">",
            f32::from(note.velocity) * 100.0 / 90.0
        );
    } else {
        out.push_str("      <note>");
    }
    if chord {
        out.push_str("<chord/>");
    }
    if let Some(pitch) = pitch {
        let _ = write!(
            out,
            "<pitch><step>{}</step><alter>{}</alter><octave>{}</octave></pitch>",
            pitch.step.name(),
            pitch.alter,
            pitch.octave
        );
    } else {
        out.push_str("<rest/>");
    }
    let _ = write!(out, "<duration>{}</duration>", note.ticks());
    if tie_in {
        out.push_str("<tie type=\"stop\"/>");
    }
    if tie_out {
        out.push_str("<tie type=\"start\"/>");
    }
    let _ = write!(
        out,
        "<type>{}</type>",
        match note.duration {
            Duration::Whole => "whole",
            Duration::Half => "half",
            Duration::Quarter => "quarter",
            Duration::Eighth => "eighth",
            Duration::Sixteenth => "16th",
        }
    );
    if note.dotted {
        out.push_str("<dot/>");
    }
    if pitch.is_some() {
        // Pitch alter is authoritative; accidental display is left to the importer
        // so it can apply key signatures and same-measure accidental carry correctly.
        if tie_in || tie_out || note.staccato {
            out.push_str("<notations>");
            if tie_in {
                out.push_str("<tied type=\"stop\"/>");
            }
            if tie_out {
                out.push_str("<tied type=\"start\"/>");
            }
            if note.staccato {
                out.push_str("<articulations><staccato/></articulations>");
            }
            out.push_str("</notations>");
        }
    }
    out.push_str("</note>\n");
}

#[derive(Debug)]
struct Event {
    tick: u32,
    priority: u8,
    bytes: Vec<u8>,
}
fn meta(kind: u8, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0xff, kind];
    variable_length(bytes.len() as u32, &mut out);
    out.extend_from_slice(bytes);
    out
}
fn variable_length(mut value: u32, out: &mut Vec<u8>) {
    let mut buffer = [0u8; 5];
    let mut i = 4;
    buffer[i] = (value & 0x7f) as u8;
    value >>= 7;
    while value > 0 {
        i -= 1;
        buffer[i] = (value & 0x7f) as u8 | 0x80;
        value >>= 7;
    }
    out.extend_from_slice(&buffer[i..]);
}
fn track(mut events: Vec<Event>, end: u32) -> Vec<u8> {
    events.sort_by_key(|event| (event.tick, event.priority));
    let mut data = Vec::new();
    let mut previous = 0;
    for event in events {
        variable_length(event.tick.saturating_sub(previous), &mut data);
        data.extend_from_slice(&event.bytes);
        previous = event.tick;
    }
    variable_length(end.saturating_sub(previous), &mut data);
    data.extend_from_slice(&[0xff, 0x2f, 0]);
    let mut out = b"MTrk".to_vec();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend(data);
    out
}

/// Standard MIDI format 1, 480 ticks/quarter. Each staff has its own track;
/// ports extend beyond the 15 melodic channels without using percussion channel 10.
pub fn midi(score: &Score) -> Vec<u8> {
    // The normal UI always validates first. Keep this free-function safe for callers.
    if score.validate().is_err() {
        return Vec::new();
    }
    let measure_ticks = score.measure_ticks();
    let end = measure_ticks * score.measure_count() as u32;
    let tempo = 60_000_000u32 / u32::from(score.tempo);
    let denominator_power = score.time_denominator.trailing_zeros() as u8;
    let mut tracks = vec![track(
        vec![
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x03, score.title.as_bytes()),
            },
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x01, score.composer.as_bytes()),
            },
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x51, &tempo.to_be_bytes()[1..]),
            },
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x58, &[score.time_numerator, denominator_power, 24, 8]),
            },
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x59, &[score.key_fifths as u8, 0]),
            },
        ],
        end,
    )];
    let has_solo = score.parts.iter().any(|part| part.solo);
    for (pi, part) in score.parts.iter().enumerate() {
        let channel = channel(pi);
        let mut events = vec![
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x03, part.name.as_bytes()),
            },
            Event {
                tick: 0,
                priority: 0,
                bytes: meta(0x21, &[(pi / 15) as u8]),
            },
            Event {
                tick: 0,
                priority: 1,
                bytes: vec![0xc0 | channel, part.instrument.gm_program()],
            },
            Event {
                tick: 0,
                priority: 1,
                bytes: vec![0xb0 | channel, 7, (part.volume * 127.0).round() as u8],
            },
            Event {
                tick: 0,
                priority: 1,
                bytes: vec![0xb0 | channel, 10, ((part.pan + 1.0) * 63.5).round() as u8],
            },
        ];
        if !part.muted && (!has_solo || part.solo) {
            let timeline = Timeline::new(part, measure_ticks);
            for (start, note) in &timeline.notes {
                for pitch in &note.pitches {
                    let midi = pitch.midi();
                    if !timeline.tie_in(*start, midi) {
                        events.push(Event {
                            tick: *start,
                            priority: 3,
                            bytes: vec![0x90 | channel, midi, note.velocity],
                        });
                    }
                    if !timeline.tie_out(*start, note, midi) {
                        let ticks = if note.staccato {
                            note.ticks() / 2
                        } else {
                            note.ticks()
                        };
                        events.push(Event {
                            tick: *start + ticks.max(1),
                            priority: 2,
                            bytes: vec![0x80 | channel, midi, 0],
                        });
                    }
                }
            }
        }
        tracks.push(track(events, end));
    }
    let mut out = b"MThd".to_vec();
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&(PPQ as u16).to_be_bytes());
    for track in tracks {
        out.extend(track);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instrument;
    fn note(start: u32, pitches: &[u8]) -> Note {
        Note {
            start,
            duration: Duration::Quarter,
            dotted: false,
            pitches: pitches.iter().map(|p| Pitch::from_midi(*p)).collect(),
            velocity: 80,
            staccato: false,
            tied: false,
        }
    }
    #[test]
    fn xml_contains_instruments_tempo_and_escaped_metadata() {
        let mut score = Score::demo();
        score.title = "A < B & C".into();
        let out = musicxml(&score);
        assert!(out.contains("A &lt; B &amp; C"));
        assert!(out.contains("<fifths>2</fifths>"));
        assert!(out.contains("<midi-program>74</midi-program>"));
        assert!(out.contains("tempo=\"108\""));
        assert!(out.contains("<tied type=\"start\"/>"));
        assert!(out.contains("<tied type=\"stop\"/>"));
        assert!(out.contains("<staccato/>"));
    }
    #[test]
    fn xml_fills_gaps_and_writes_chords() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(480, &[60, 64, 67])];
        let out = musicxml(&score);
        assert_eq!(out.matches("<chord/>").count(), 2);
        assert!(out.contains("<rest/><duration>480</duration><type>quarter</type>"));
        assert!(out.contains("<rest/><duration>960</duration><type>half</type>"));
    }

    #[test]
    fn xml_prints_dynamic_changes_without_repeating_each_note() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, &[60]), note(480, &[64])];
        let mut louder = note(0, &[67]);
        louder.velocity = 116;
        score.parts[0].measures[1].notes = vec![louder];
        let xml = musicxml(&score);
        assert_eq!(xml.matches("<mf/>").count(), 1);
        assert_eq!(xml.matches("<ff/>").count(), 1);
    }
    fn read_variable(bytes: &[u8], pos: &mut usize) -> u32 {
        let mut n = 0;
        loop {
            let b = bytes[*pos];
            *pos += 1;
            n = (n << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return n;
            }
        }
    }
    fn midi_notes(bytes: &[u8]) -> Vec<(u32, u8, u8)> {
        let mut out = Vec::new();
        let mut p = 14;
        while p < bytes.len() {
            assert_eq!(&bytes[p..p + 4], b"MTrk");
            let n = u32::from_be_bytes(bytes[p + 4..p + 8].try_into().unwrap()) as usize;
            p += 8;
            let finish = p + n;
            let mut tick = 0;
            while p < finish {
                tick += read_variable(bytes, &mut p);
                let status = bytes[p];
                p += 1;
                if status == 0xff {
                    p += 1;
                    let size = read_variable(bytes, &mut p) as usize;
                    p += size;
                } else if status & 0xf0 == 0xc0 {
                    p += 1;
                } else {
                    let a = bytes[p];
                    let b = bytes[p + 1];
                    p += 2;
                    if status & 0xf0 == 0x90 || status & 0xf0 == 0x80 {
                        out.push((tick, status & 0xf0, a));
                    }
                    let _ = b;
                }
            }
        }
        out
    }
    #[test]
    fn midi_honors_gap_chord_tie_staccato_and_program() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].instrument = Instrument::Violin;
        let mut first = note(480, &[60, 64]);
        first.tied = true;
        let mut next = note(960, &[60, 64]);
        next.staccato = true;
        score.parts[0].measures[0].notes = vec![first, next];
        let bytes = midi(&score);
        assert_eq!(&bytes[..4], b"MThd");
        assert!(bytes.windows(2).any(|pair| pair == [0xc0, 40]));
        assert_eq!(
            midi_notes(&bytes),
            vec![
                (480, 0x90, 60),
                (480, 0x90, 64),
                (1200, 0x80, 60),
                (1200, 0x80, 64)
            ]
        );
        score.parts[0].muted = true;
        assert!(midi_notes(&midi(&score)).is_empty());
    }
    #[test]
    fn every_catalog_program_survives_midi_and_musicxml_export() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, &[60])];
        for &instrument in Instrument::ALL {
            score.parts[0].instrument = instrument;
            let program = instrument.gm_program();
            assert!(
                midi(&score).windows(2).any(|pair| pair == [0xc0, program]),
                "{}",
                instrument.name()
            );
            assert!(
                musicxml(&score).contains(&format!(
                    "<midi-program>{}</midi-program>",
                    u16::from(program) + 1
                )),
                "{}",
                instrument.name()
            );
        }
    }

    #[test]
    fn midi_honors_cross_bar_tie_and_solo() {
        let mut score = Score::blank();
        let mut first = note(1440, &[60]);
        first.tied = true;
        score.parts[0].measures[0].notes = vec![first];
        score.parts[0].measures[1].notes = vec![note(0, &[60])];
        score.parts[1].measures[0].notes = vec![note(0, &[48])];
        score.parts[0].solo = true;
        assert_eq!(
            midi_notes(&midi(&score)),
            vec![(1440, 0x90, 60), (2400, 0x80, 60)]
        );
    }
    #[test]
    fn midi_partial_chord_ties_do_not_retrigger_or_stick() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        let mut first = note(480, &[60, 64]);
        first.tied = true;
        score.parts[0].measures[0].notes = vec![first, note(960, &[60, 67])];
        assert_eq!(
            midi_notes(&midi(&score)),
            vec![
                (480, 0x90, 60),
                (480, 0x90, 64),
                (960, 0x80, 64),
                (960, 0x90, 67),
                (1440, 0x80, 60),
                (1440, 0x80, 67),
            ]
        );
    }
    #[test]
    fn xml_sixteenth_type_and_velocity_follow_musicxml() {
        let mut score = Score::blank();
        let mut sixteenth = note(0, &[60]);
        sixteenth.duration = Duration::Sixteenth;
        sixteenth.velocity = 90;
        score.parts[0].measures[0].notes = vec![sixteenth];
        let out = musicxml(&score);
        assert!(out.contains("<type>16th</type>"));
        assert!(out.contains("dynamics=\"100.000\""));
        assert!(!out.contains("sixteenth"));
    }
    #[test]
    fn malformed_midi_input_is_safe() {
        let mut score = Score::blank();
        score.tempo = 0;
        assert!(midi(&score).is_empty());
    }
}
