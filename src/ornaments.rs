//! Voice-local turns, mordents and chord rolls over unchanged written rhythm.
use crate::{
    document::{Anchor, note_at},
    model::{Note, Pitch, Score, Step, key_alter},
    slurs::SlurDirection,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_ORNAMENTS: usize = 20_000;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrnamentKind {
    #[default]
    Turn,
    InvertedTurn,
    Mordent,
    UpperMordent,
    ArpeggioUp,
    ArpeggioDown,
    NonArpeggiate,
}
impl OrnamentKind {
    pub const ALL: [Self; 7] = [
        Self::Turn,
        Self::InvertedTurn,
        Self::Mordent,
        Self::UpperMordent,
        Self::ArpeggioUp,
        Self::ArpeggioDown,
        Self::NonArpeggiate,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Turn => "Turn",
            Self::InvertedTurn => "Inverted turn",
            Self::Mordent => "Lower mordent",
            Self::UpperMordent => "Upper mordent",
            Self::ArpeggioUp => "Arpeggio up",
            Self::ArpeggioDown => "Arpeggio down",
            Self::NonArpeggiate => "Non-arpeggiate",
        }
    }
    pub fn chord(self) -> bool {
        matches!(
            self,
            Self::ArpeggioUp | Self::ArpeggioDown | Self::NonArpeggiate
        )
    }
}
fn default_window() -> u16 {
    120
}
fn playback() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ornament {
    pub anchor: Anchor,
    pub kind: OrnamentKind,
    #[serde(default)]
    pub upper: Option<i8>,
    #[serde(default)]
    pub lower: Option<i8>,
    #[serde(default = "default_window")]
    pub window_ticks: u16,
    #[serde(default)]
    pub delay_percent: f32,
    #[serde(default = "playback")]
    pub playback: bool,
    #[serde(default)]
    pub placement: SlurDirection,
    #[serde(default)]
    pub offset: [f32; 2],
}
impl Ornament {
    pub fn new(anchor: Anchor, kind: OrnamentKind) -> Self {
        Self {
            anchor,
            kind,
            upper: None,
            lower: None,
            window_ticks: default_window(),
            delay_percent: 0.0,
            playback: true,
            placement: SlurDirection::Auto,
            offset: [0.; 2],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.anchor.layer > 3
            || self.anchor.voice > 1
            || !(1..=960).contains(&self.window_ticks)
            || !self.delay_percent.is_finite()
            || !(0.0..=75.0).contains(&self.delay_percent)
            || self.upper.is_some_and(|v| !(1..=4).contains(&v))
            || self.lower.is_some_and(|v| !(-4..=-1).contains(&v))
            || self.offset.iter().any(|v| !v.is_finite() || v.abs() > 160.)
        {
            return Err("Ornaments need a valid voice, 1–960 tick window, 0–75% delay, bounded neighbor intervals and offsets.".into());
        }
        Ok(())
    }
}
pub fn compatible(kind: OrnamentKind, note: &Note) -> bool {
    if kind.chord() {
        (2..=16).contains(&note.pitches.len())
    } else {
        note.pitches.len() == 1
    }
}
pub fn pitch_number(pitch: Pitch) -> i32 {
    (i32::from(pitch.octave) + 1) * 12 + pitch.step.semitone() + i32::from(pitch.alter)
}
fn neighbor(score: &Score, pitch: Pitch, up: bool, override_interval: Option<i8>) -> Pitch {
    let position = i32::from(pitch.octave) * 7 + pitch.step.index() + if up { 1 } else { -1 };
    let step = Step::from_index(position);
    let octave = position.div_euclid(7) as i8;
    let alter = override_interval.map_or_else(
        || key_alter(step, score.key_fifths),
        |interval| {
            (pitch_number(pitch) + i32::from(interval)
                - ((i32::from(octave) + 1) * 12 + step.semitone())) as i8
        },
    );
    Pitch {
        step,
        octave,
        alter,
    }
}
pub fn auxiliary_pitches_for_note(
    score: &Score,
    ornament: &Ornament,
    note: &Note,
) -> Result<(Pitch, Pitch), String> {
    let pitch = *note
        .pitches
        .first()
        .ok_or("An ornament needs a pitched principal.")?;
    Ok((
        neighbor(score, pitch, true, ornament.upper),
        neighbor(score, pitch, false, ornament.lower),
    ))
}
pub fn auxiliary_pitches(score: &Score, ornament: &Ornament) -> Result<(Pitch, Pitch), String> {
    auxiliary_pitches_for_note(
        score,
        ornament,
        note_at(score, &ornament.anchor)
            .ok_or("An ornament references a missing principal entry.")?,
    )
}
pub(crate) fn principal_index(score: &Score) -> BTreeMap<Anchor, &Note> {
    score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(part, p)| {
            p.measures.iter().enumerate().flat_map(move |(measure, m)| {
                m.notes.iter().map(move |note| {
                    (
                        Anchor {
                            part,
                            measure,
                            tick: note.start,
                            layer: note.layer,
                            voice: note.voice,
                        },
                        note,
                    )
                })
            })
        })
        .collect()
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.ornaments.len() > MAX_ORNAMENTS {
        return Err("Score exceeds the 20000 ornament limit.".into());
    }
    let notes = principal_index(score);
    let mut lines = BTreeMap::<(usize, u8), Vec<(u64, u64)>>::new();
    for line in &score.notation.lines {
        if matches!(
            line.kind,
            crate::lines::LineKind::Trill { .. } | crate::lines::LineKind::Glissando
        ) {
            lines
                .entry((line.start.part, line.start.lane()))
                .or_default()
                .push((
                    crate::expressions::tick(score, line.start.into()),
                    crate::expressions::tick(score, line.end.into()),
                ));
        }
    }
    for lane in lines.values_mut() {
        lane.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(lane.len());
        for &(a, b) in lane.iter() {
            if let Some(last) = merged.last_mut().filter(|last| last.1 >= a) {
                last.1 = last.1.max(b);
            } else {
                merged.push((a, b));
            }
        }
        *lane = merged;
    }
    let mut seen = BTreeSet::new();
    for mark in &score.notation.ornaments {
        mark.validate()?;
        let note = notes
            .get(&mark.anchor)
            .ok_or("An ornament references a missing principal entry.")?;
        if !compatible(mark.kind, note) {
            return Err(
                "Turns and mordents need a single pitch; chord roll signs need 2–16 pitches."
                    .into(),
            );
        }
        if !seen.insert(mark.anchor) {
            return Err("Each voice entry supports one ornament or roll sign.".into());
        }
        if !mark.kind.chord() {
            let (upper, lower) = auxiliary_pitches_for_note(score, mark, note)?;
            for pitch in match mark.kind {
                OrnamentKind::Mordent => vec![lower],
                OrnamentKind::UpperMordent => vec![upper],
                _ => vec![upper, lower],
            } {
                if !(-2..=2).contains(&pitch.alter)
                    || !(0..=127).contains(&pitch_number(pitch))
                    || pitch_number(pitch) == pitch_number(note.pitches[0])
                {
                    return Err("An ornament neighbor must be a distinct MIDI pitch; set an explicit upper/lower interval.".into());
                }
            }
        }
        if mark.playback
            && mark.kind != OrnamentKind::NonArpeggiate
            && let Some(lane) = lines.get(&(mark.anchor.part, mark.anchor.lane()))
        {
            let now = crate::expressions::tick(score, mark.anchor.into());
            let end = now + u64::from(note.ticks());
            let i = lane.partition_point(|(_, end)| *end <= now);
            if lane.get(i).is_some_and(|(start, _)| *start < end) {
                return Err(
                    "A played ornament principal cannot overlap a trill or glissando line.".into(),
                );
            }
        }
    }
    Ok(())
}
pub fn assign(score: &mut Score, mark: Ornament) -> Result<(), String> {
    mark.validate()?;
    let mut candidate = score.clone();
    candidate
        .notation
        .ornaments
        .retain(|prior| prior.anchor != mark.anchor);
    candidate.notation.ornaments.push(mark);
    candidate.validate()?;
    *score = candidate;
    Ok(())
}
pub fn remove_anchor(score: &mut Score, at: &Anchor) {
    score.notation.ornaments.retain(|o| o.anchor != *at);
}
pub fn remove_part(score: &mut Score, part: usize) {
    score.notation.ornaments.retain(|o| o.anchor.part != part);
}
pub fn reindex_parts(score: &mut Score, removed: usize) {
    for o in &mut score.notation.ornaments {
        if o.anchor.part > removed {
            o.anchor.part -= 1;
        }
    }
}
pub fn move_anchor(score: &mut Score, from: Anchor, to: Anchor) {
    for o in &mut score.notation.ornaments {
        if o.anchor == from {
            o.anchor = to;
        }
    }
}
pub fn prune(score: &mut Score) {
    let notes = principal_index(score);
    let keep: BTreeSet<_> = score
        .notation
        .ornaments
        .iter()
        .filter(|o| notes.get(&o.anchor).is_some_and(|n| compatible(o.kind, n)))
        .map(|o| o.anchor)
        .collect();
    score
        .notation
        .ornaments
        .retain(|o| keep.contains(&o.anchor));
}
#[derive(Default)]
pub struct MusicXmlIndex {
    marks: BTreeMap<Anchor, (String, String)>,
    brackets: BTreeMap<Anchor, (u8, u8, [f32; 2])>,
}
impl MusicXmlIndex {
    pub fn new(score: &Score) -> Self {
        let notes = principal_index(score);
        let mut index = Self::default();
        for mark in &score.notation.ornaments {
            let Some(note) = notes.get(&mark.anchor) else {
                continue;
            };
            let placement = match mark.placement {
                SlurDirection::Auto => "",
                SlurDirection::Above => " placement=\"above\"",
                SlurDirection::Below => " placement=\"below\"",
            };
            let standard = match mark.kind {
                OrnamentKind::ArpeggioUp | OrnamentKind::ArpeggioDown => format!(
                    "<arpeggiate direction=\"{}\" relative-x=\"{}\" relative-y=\"{}\"/>",
                    if mark.kind == OrnamentKind::ArpeggioUp {
                        "up"
                    } else {
                        "down"
                    },
                    mark.offset[0],
                    -mark.offset[1]
                ),
                OrnamentKind::NonArpeggiate => {
                    let low = note.pitches.iter().map(|p| p.midi()).min().unwrap_or(0);
                    let high = note.pitches.iter().map(|p| p.midi()).max().unwrap_or(0);
                    index.brackets.insert(mark.anchor, (low, high, mark.offset));
                    String::new()
                }
                kind => {
                    let tag = match kind {
                        OrnamentKind::Turn => {
                            if mark.delay_percent > 0. {
                                "delayed-turn"
                            } else {
                                "turn"
                            }
                        }
                        OrnamentKind::InvertedTurn => {
                            if mark.delay_percent > 0. {
                                "delayed-inverted-turn"
                            } else {
                                "inverted-turn"
                            }
                        }
                        OrnamentKind::Mordent => "mordent",
                        _ => "inverted-mordent",
                    };
                    let mut out = format!(
                        "<ornaments><{tag}{placement} relative-x=\"{}\" relative-y=\"{}\"/>",
                        mark.offset[0], -mark.offset[1]
                    );
                    if let Ok((upper, lower)) = auxiliary_pitches_for_note(score, mark, note) {
                        for (pitch, at, needed) in [
                            (
                                upper,
                                "above",
                                mark.upper.is_some() && !matches!(kind, OrnamentKind::Mordent),
                            ),
                            (
                                lower,
                                "below",
                                mark.lower.is_some() && !matches!(kind, OrnamentKind::UpperMordent),
                            ),
                        ] {
                            if needed {
                                let accidental = match pitch.alter {
                                    -2 => "flat-flat",
                                    -1 => "flat",
                                    0 => "natural",
                                    1 => "sharp",
                                    _ => "double-sharp",
                                };
                                out.push_str(&format!("<accidental-mark placement=\"{at}\">{accidental}</accidental-mark>"));
                            }
                        }
                    }
                    out.push_str("</ornaments>");
                    out
                }
            };
            let native = serde_json::to_string(mark)
                .unwrap_or_default()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let first = format!(
                "{standard}<other-notation type=\"single\" print-object=\"no\">encore-ornament:{native}</other-notation>"
            );
            let chord = if mark.kind.chord() {
                standard
            } else {
                String::new()
            };
            index.marks.insert(mark.anchor, (first, chord));
        }
        index
    }
    pub fn notations(&self, at: &Anchor, chord: bool) -> &str {
        self.marks.get(at).map_or("", |mark| {
            if chord {
                mark.1.as_str()
            } else {
                mark.0.as_str()
            }
        })
    }
    pub fn notations_for_pitch(&self, at: &Anchor, midi: Option<u8>, chord: bool) -> String {
        let mut out = self.notations(at, chord).to_owned();
        if let (Some(&(low, high, offset)), Some(midi)) = (self.brackets.get(at), midi) {
            for (value, kind) in [(low, "bottom"), (high, "top")] {
                if midi == value {
                    out.push_str(&format!(
                        "<non-arpeggiate type=\"{kind}\" relative-x=\"{}\" relative-y=\"{}\"/>",
                        offset[0], -offset[1]
                    ));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Score, Anchor) {
        let mut s = Score::blank();
        s.parts.truncate(1);
        s.parts[0].measures.truncate(1);
        s.parts[0].measures[0].notes.clear();
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        let n = Note {
            layer: 0,
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: crate::model::Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: false,
        };
        s.parts[0].measures[0].notes.push(n);
        (s, a)
    }
    #[test]
    fn key_neighbors_and_explicit_chromatic_neighbors_are_spelled() {
        let (mut s, a) = fixture();
        s.key_fifths = 1;
        s.parts[0].measures[0].notes[0].pitches[0] = Pitch {
            step: Step::E,
            alter: 0,
            octave: 4,
        };
        let mut o = Ornament::new(a, OrnamentKind::Turn);
        let (u, l) = auxiliary_pitches(&s, &o).unwrap();
        assert_eq!(
            u,
            Pitch {
                step: Step::F,
                alter: 1,
                octave: 4
            }
        );
        assert_eq!(l.step, Step::D);
        o.upper = Some(1);
        assert_eq!(auxiliary_pitches(&s, &o).unwrap().0.alter, 0);
        s.notation.ornaments.push(o);
        s.validate().unwrap();
    }
    #[test]
    fn invalid_assignment_is_atomic_and_voice_local() {
        let (mut s, a) = fixture();
        let original = s.clone();
        assert!(assign(&mut s, Ornament::new(a, OrnamentKind::ArpeggioUp)).is_err());
        assert_eq!(s, original);
        let mut n = s.parts[0].measures[0].notes[0].clone();
        n.voice = 1;
        s.parts[0].measures[0].notes.push(n);
        assign(&mut s, Ornament::new(a, OrnamentKind::Turn)).unwrap();
        assign(
            &mut s,
            Ornament::new(Anchor { voice: 1, ..a }, OrnamentKind::Mordent),
        )
        .unwrap();
        assert_eq!(s.notation.ornaments.len(), 2);
    }
    #[test]
    fn non_arpeggiate_only_brackets_outer_chord_members() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes[0].pitches = vec![
            Pitch::from_midi(64),
            Pitch::from_midi(67),
            Pitch::from_midi(60),
        ];
        s.notation
            .ornaments
            .push(Ornament::new(a, OrnamentKind::NonArpeggiate));
        let x = MusicXmlIndex::new(&s);
        assert!(
            !x.notations_for_pitch(&a, Some(64), false)
                .contains("<non-arpeggiate")
        );
        assert!(
            x.notations_for_pitch(&a, Some(60), true)
                .contains("type=\"bottom\"")
        );
        assert!(
            x.notations_for_pitch(&a, Some(67), true)
                .contains("type=\"top\"")
        );
    }
}
