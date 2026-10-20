//! Anchored engraving overrides and per-pitch ties, separate from sounding notes.
use crate::{
    document::{Anchor, note_at},
    model::{Note, Score},
    slurs::SlurDirection,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StemDirection {
    #[default]
    Auto,
    Up,
    Down,
    None,
}
impl StemDirection {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Up, Self::Down, Self::None];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::None => "Hidden",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeamStyle {
    #[default]
    Auto,
    Break,
    Join,
    Flat,
}
impl BeamStyle {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Break, Self::Join, Self::Flat];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Break => "Break before",
            Self::Join => "Join previous",
            Self::Flat => "Flat beam",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Notehead {
    #[default]
    Normal,
    Cross,
    Diamond,
    Triangle,
    Slash,
    Square,
}
impl Notehead {
    pub const ALL: [Self; 6] = [
        Self::Normal,
        Self::Cross,
        Self::Diamond,
        Self::Triangle,
        Self::Slash,
        Self::Square,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Cross => "Cross",
            Self::Diamond => "Diamond",
            Self::Triangle => "Triangle",
            Self::Slash => "Slash",
            Self::Square => "Square",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccidentalDisplay {
    #[default]
    Auto,
    Show,
    Hide,
    Courtesy,
    Parenthesized,
}
impl AccidentalDisplay {
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Show,
        Self::Hide,
        Self::Courtesy,
        Self::Parenthesized,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Show => "Forced",
            Self::Hide => "Hidden",
            Self::Courtesy => "Courtesy",
            Self::Parenthesized => "Parenthesized",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccidentalOverride {
    pub pitch: u8,
    pub display: AccidentalDisplay,
}
fn visible() -> bool {
    true
}
fn scale() -> f32 {
    1.0
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryStyle {
    pub anchor: Anchor,
    #[serde(default)]
    pub stem: StemDirection,
    #[serde(default)]
    pub beam: BeamStyle,
    #[serde(default = "visible")]
    pub visible: bool,
    #[serde(default = "scale")]
    pub scale: f32,
    #[serde(default)]
    pub notehead: Notehead,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accidentals: Vec<AccidentalOverride>,
    /// None preserves the historical event-wide tie; Some lists exact MIDI pitches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tie_pitches: Option<Vec<u8>>,
    #[serde(default)]
    pub tie_direction: SlurDirection,
}
impl EntryStyle {
    pub fn new(anchor: Anchor) -> Self {
        Self {
            anchor,
            stem: Default::default(),
            beam: Default::default(),
            visible: true,
            scale: 1.0,
            notehead: Default::default(),
            accidentals: Vec::new(),
            tie_pitches: None,
            tie_direction: Default::default(),
        }
    }
    pub fn is_default(&self) -> bool {
        *self == Self::new(self.anchor)
    }
    pub fn accidental(&self, pitch: u8) -> AccidentalDisplay {
        self.accidentals
            .iter()
            .find(|a| a.pitch == pitch)
            .map_or(AccidentalDisplay::Auto, |a| a.display)
    }
}
pub fn at<'a>(score: &'a Score, anchor: &Anchor) -> Option<&'a EntryStyle> {
    score
        .notation
        .entries
        .iter()
        .find(|style| style.anchor == *anchor)
}
/// A renderer/exporter owns one index for its immutable score snapshot.
pub struct Index<'a> {
    entries: BTreeMap<Anchor, &'a EntryStyle>,
}
impl<'a> Index<'a> {
    pub fn new(score: &'a Score) -> Self {
        Self {
            entries: score
                .notation
                .entries
                .iter()
                .map(|style| (style.anchor, style))
                .collect(),
        }
    }
    pub fn get(&self, anchor: &Anchor) -> Option<&'a EntryStyle> {
        self.entries.get(anchor).copied()
    }
    pub fn ties_forward(&self, anchor: &Anchor, note: &Note, midi: u8) -> bool {
        self.get(anchor)
            .and_then(|style| style.tie_pitches.as_ref())
            .map_or_else(
                || note.tied && note.pitches.iter().any(|pitch| pitch.midi() == midi),
                |ties| ties.contains(&midi),
            )
    }
}

fn check_note(note: &Note, style: &EntryStyle) -> Result<(), String> {
    if !style.scale.is_finite() || !(0.4..=2.0).contains(&style.scale) {
        return Err("Entry scale must be 40–200 percent.".into());
    }
    if style.accidentals.len() > 16 {
        return Err("An entry has too many accidental overrides.".into());
    }
    let mut seen = BTreeSet::new();
    for mark in &style.accidentals {
        if !seen.insert(mark.pitch) || !note.pitches.iter().any(|pitch| pitch.midi() == mark.pitch)
        {
            return Err("Accidental overrides must identify unique pitches in their entry.".into());
        }
    }
    if let Some(ties) = &style.tie_pitches {
        let mut seen = BTreeSet::new();
        for pitch in ties {
            if !seen.insert(*pitch) || !note.pitches.iter().any(|own| own.midi() == *pitch) {
                return Err("Per-pitch ties must identify unique pitches in their entry.".into());
            }
        }
    }
    Ok(())
}
fn onset_index(score: &Score) -> BTreeMap<Anchor, &Note> {
    score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(part, staff)| {
            staff
                .measures
                .iter()
                .enumerate()
                .flat_map(move |(measure, bar)| {
                    bar.notes.iter().map(move |note| {
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
    if score.notation.entries.len() > crate::model::MAX_EVENTS {
        return Err("Too many entry engraving overrides.".into());
    }
    if score.notation.entries.is_empty() {
        return Ok(());
    }
    let onsets = onset_index(score);
    let mut seen = BTreeSet::new();
    for style in &score.notation.entries {
        let note = onsets
            .get(&style.anchor)
            .ok_or("An engraving override must attach to an existing entry.")?;
        check_note(note, style)?;
        if !seen.insert(style.anchor) {
            return Err("Duplicate entry engraving overrides.".into());
        }
    }
    Ok(())
}
pub fn assign(score: &mut Score, style: EntryStyle) -> Result<(), String> {
    let note = note_at(score, &style.anchor)
        .ok_or("An engraving override must attach to an existing entry.")?;
    check_note(note, &style)?;
    let anchor = style.anchor;
    if let Some(index) = score
        .notation
        .entries
        .iter()
        .position(|prior| prior.anchor == anchor)
    {
        if style.is_default() {
            score.notation.entries.remove(index);
        } else {
            score.notation.entries[index] = style;
        }
    } else if !style.is_default() {
        if score.notation.entries.len() >= crate::model::MAX_EVENTS {
            return Err("Too many engraving overrides.".into());
        }
        score.notation.entries.push(style);
    }
    Ok(())
}
pub fn ties_forward(score: &Score, anchor: &Anchor, midi: u8) -> bool {
    if let Some(ties) = at(score, anchor).and_then(|style| style.tie_pitches.as_ref()) {
        ties.contains(&midi)
    } else {
        note_at(score, anchor)
            .is_some_and(|note| note.tied && note.pitches.iter().any(|pitch| pitch.midi() == midi))
    }
}
pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
    score
        .notation
        .entries
        .retain(|style| style.anchor != *anchor);
}
pub fn remove_part(score: &mut Score, part: usize) {
    score
        .notation
        .entries
        .retain(|style| style.anchor.part != part);
    for style in &mut score.notation.entries {
        if style.anchor.part > part {
            style.anchor.part -= 1;
        }
    }
}
pub fn prune(score: &mut Score) {
    let mut entries = std::mem::take(&mut score.notation.entries);
    let onsets = onset_index(score);
    entries.retain(|style| {
        onsets
            .get(&style.anchor)
            .is_some_and(|note| check_note(note, style).is_ok())
    });
    score.notation.entries = entries;
}
/// Carry pitch-specific display/tie choices through a deliberate pitch edit.
pub fn synchronize_note(score: &mut Score, anchor: &Anchor, old: &Note) {
    let Some(note) = note_at(score, anchor).cloned() else {
        remove_anchor(score, anchor);
        return;
    };
    if let Some(style) = score
        .notation
        .entries
        .iter_mut()
        .find(|style| style.anchor == *anchor)
    {
        let remap = |midi: u8| {
            old.pitches
                .iter()
                .position(|pitch| pitch.midi() == midi)
                .and_then(|index| note.pitches.get(index))
                .map(|pitch| pitch.midi())
        };
        style.accidentals = style
            .accidentals
            .iter()
            .filter_map(|mark| remap(mark.pitch).map(|pitch| AccidentalOverride { pitch, ..*mark }))
            .collect();
        if let Some(ties) = &mut style.tie_pitches {
            *ties = ties.iter().filter_map(|pitch| remap(*pitch)).collect();
            ties.sort_unstable();
            ties.dedup();
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn musicxml_attributes(score: &Score, anchor: &Anchor) -> String {
    xml_attributes(at(score, anchor))
}
pub fn xml_attributes(style: Option<&EntryStyle>) -> String {
    style
        .filter(|style| !style.visible)
        .map_or_else(String::new, |_| {
            " print-object=\"no\" print-spacing=\"yes\"".into()
        })
}
pub fn musicxml_accidental(score: &Score, anchor: &Anchor, midi: Option<u8>) -> String {
    xml_accidental(at(score, anchor), note_at(score, anchor), midi)
}
pub fn xml_accidental(style: Option<&EntryStyle>, note: Option<&Note>, midi: Option<u8>) -> String {
    let Some(midi) = midi else {
        return String::new();
    };
    let Some(style) = style else {
        return String::new();
    };
    let display = style.accidental(midi);
    if display == AccidentalDisplay::Auto {
        return String::new();
    }
    let Some(pitch) = note.and_then(|note| note.pitches.iter().find(|pitch| pitch.midi() == midi))
    else {
        return String::new();
    };
    let name = match pitch.alter {
        -2 => "flat-flat",
        -1 => "flat",
        0 => "natural",
        1 => "sharp",
        2 => "double-sharp",
        _ => return String::new(),
    };
    format!(
        "<accidental{}{}{}>{name}</accidental>",
        if display == AccidentalDisplay::Hide {
            " print-object=\"no\""
        } else {
            ""
        },
        if display == AccidentalDisplay::Courtesy {
            " cautionary=\"yes\""
        } else {
            ""
        },
        if display == AccidentalDisplay::Parenthesized {
            " parentheses=\"yes\""
        } else {
            ""
        }
    )
}
pub fn musicxml_elements(score: &Score, anchor: &Anchor, midi: Option<u8>, chord: bool) -> String {
    xml_elements(at(score, anchor), note_at(score, anchor), midi, chord)
}
pub fn xml_elements(
    style: Option<&EntryStyle>,
    note: Option<&Note>,
    midi: Option<u8>,
    chord: bool,
) -> String {
    let Some(style) = style else {
        return String::new();
    };
    let mut out = String::new();
    if !chord && style.stem != StemDirection::Auto {
        let stem = match style.stem {
            StemDirection::Up => "up",
            StemDirection::Down => "down",
            StemDirection::None => "none",
            _ => "none",
        };
        out.push_str(&format!("<stem>{stem}</stem>"));
    }
    if midi.is_some() && (style.notehead != Notehead::Normal || style.scale != 1.0) {
        let name = match style.notehead {
            Notehead::Normal => "normal",
            Notehead::Cross => "x",
            Notehead::Diamond => "diamond",
            Notehead::Triangle => "triangle",
            Notehead::Slash => "slash",
            Notehead::Square => "square",
        };
        out.push_str(&format!(
            "<notehead font-size=\"{}\">{name}</notehead>",
            20.0 * style.scale
        ));
    }
    if !chord
        && note.is_some_and(|note| note.duration.ticks() <= crate::model::Duration::Eighth.ticks())
    {
        match style.beam {
            BeamStyle::Break => out.push_str("<beam number=\"1\">begin</beam>"),
            BeamStyle::Join | BeamStyle::Flat => out.push_str("<beam number=\"1\">continue</beam>"),
            _ => {}
        }
    }
    out
}
/// The standard elements above provide portable notation. A hidden, valid
/// MusicXML other-notation retains exact Encore overrides when XML returns.
pub fn musicxml_notations(score: &Score, anchor: &Anchor, chord: bool) -> String {
    xml_notations(at(score, anchor), chord)
}
pub fn xml_notations(style: Option<&EntryStyle>, chord: bool) -> String {
    if chord {
        return String::new();
    }
    style.and_then(|style|serde_json::to_string(style).ok()).map_or_else(String::new,|json|format!("<other-notation type=\"single\" print-object=\"no\">encore-entry-style:{}</other-notation>",escape(&json)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Pitch};
    fn fixture() -> (Score, Anchor) {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes.push(Note {
            voice: 0,
            voice_launch: None,
            start: 0,
            layer: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(60), Pitch::from_midi(64)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: true,
        });
        (
            score,
            Anchor {
                voice: 0,
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
            },
        )
    }
    #[test]
    fn partial_ties_override_legacy_flag_and_clear_to_automatic() {
        let (mut score, anchor) = fixture();
        let mut style = EntryStyle::new(anchor);
        style.tie_pitches = Some(vec![64]);
        assign(&mut score, style).unwrap();
        assert!(!ties_forward(&score, &anchor, 60));
        assert!(ties_forward(&score, &anchor, 64));
        assign(&mut score, EntryStyle::new(anchor)).unwrap();
        assert!(score.notation.entries.is_empty());
        assert!(ties_forward(&score, &anchor, 60));
    }
    #[test]
    fn invalid_override_is_atomic_and_duplicate_native_anchors_are_rejected() {
        let (mut score, anchor) = fixture();
        let before = score.clone();
        for style in [
            EntryStyle {
                scale: f32::NAN,
                ..EntryStyle::new(anchor)
            },
            EntryStyle {
                tie_pitches: Some(vec![61]),
                ..EntryStyle::new(anchor)
            },
        ] {
            assert!(assign(&mut score, style.clone()).is_err());
            assert_eq!(score, before);
        }
        let style = EntryStyle {
            visible: false,
            ..EntryStyle::new(anchor)
        };
        score.notation.entries = vec![style.clone(), style];
        assert!(score.validate().unwrap_err().contains("Duplicate"));
    }
    #[test]
    fn pitch_edit_carries_only_surviving_pitch_choices_and_native_serialization() {
        let (mut score, anchor) = fixture();
        let old = note_at(&score, &anchor).unwrap().clone();
        let mut style = EntryStyle::new(anchor);
        style.tie_pitches = Some(vec![60, 64]);
        style.accidentals = vec![AccidentalOverride {
            pitch: 64,
            display: AccidentalDisplay::Parenthesized,
        }];
        assign(&mut score, style).unwrap();
        score.parts[0].measures[0].notes[0].pitches = vec![Pitch::from_midi(62)];
        synchronize_note(&mut score, &anchor, &old);
        let style = at(&score, &anchor).unwrap();
        assert_eq!(style.tie_pitches, Some(vec![62]));
        assert!(style.accidentals.is_empty());
        let saved = serde_json::to_vec(&score).unwrap();
        let loaded: Score = serde_json::from_slice(&saved).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded, score);
    }
    #[test]
    fn pruning_removes_invalid_source_anchors_without_losing_valid_overrides() {
        let (mut score, anchor) = fixture();
        score.notation.entries = vec![
            EntryStyle {
                visible: false,
                ..EntryStyle::new(anchor)
            },
            EntryStyle::new(Anchor {
                measure: 100,
                ..anchor
            }),
        ];
        prune(&mut score);
        assert_eq!(score.notation.entries.len(), 1);
        score.validate().unwrap();
    }
}
