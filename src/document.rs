//! Persistent notation anchored to source staves and rhythmic onsets.
use crate::model::{Note, Score, Selection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Anchor {
    pub part: usize,
    pub measure: usize,
    pub tick: u32,
    pub layer: u8,
    #[serde(default, skip_serializing_if = "is_primary_voice")]
    pub voice: u8,
}

fn is_primary_voice(voice: &u8) -> bool {
    *voice == 0
}

impl Anchor {
    pub fn lane(self) -> u8 {
        self.voice.saturating_mul(4).saturating_add(self.layer)
    }
    pub fn from_selection(score: &Score, selection: Selection) -> Option<Self> {
        let note = score
            .parts
            .get(selection.part)?
            .measures
            .get(selection.measure)?
            .notes
            .get(selection.note)?;
        Some(Self {
            part: selection.part,
            measure: selection.measure,
            tick: note.start,
            layer: note.layer,
            voice: note.voice,
        })
    }

    pub fn selection(self, score: &Score) -> Option<Selection> {
        let note = score
            .parts
            .get(self.part)?
            .measures
            .get(self.measure)?
            .notes
            .iter()
            .position(|note| {
                note.start == self.tick && note.layer == self.layer && note.voice == self.voice
            })?;
        Some(Selection {
            part: self.part,
            measure: self.measure,
            note,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notation {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ornaments: Vec<crate::ornaments::Ornament>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub grace: Vec<crate::grace::GraceGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub chords: Vec<crate::chords::ChordSymbol>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<crate::lines::PerformanceLine>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<crate::entry_style::EntryStyle>,
    #[serde(skip_serializing_if = "crate::layout::DocumentLayout::is_default")]
    pub layout: crate::layout::DocumentLayout,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<crate::expressions::Expression>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spanners: Vec<crate::expressions::Hairpin>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tempo: Vec<crate::expressions::TempoChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lyrics: Vec<crate::lyrics::Lyric>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slurs: Vec<crate::slurs::Slur>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repeats: Vec<crate::repeats::RepeatRegion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub navigation: Vec<crate::repeats::NavigationMark>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<crate::parts::LinkedPart>,
}

impl Notation {
    pub fn is_empty(&self) -> bool {
        self.grace.is_empty()
            && self.ornaments.is_empty()
            && self.chords.is_empty()
            && self.lines.is_empty()
            && self.entries.is_empty()
            && self.layout.is_default()
            && self.expressions.is_empty()
            && self.spanners.is_empty()
            && self.tempo.is_empty()
            && self.lyrics.is_empty()
            && self.slurs.is_empty()
            && self.repeats.is_empty()
            && self.navigation.is_empty()
            && self.parts.is_empty()
    }
}

pub fn note_at<'a>(score: &'a Score, anchor: &Anchor) -> Option<&'a Note> {
    score
        .parts
        .get(anchor.part)?
        .measures
        .get(anchor.measure)?
        .notes
        .iter()
        .find(|note| {
            note.start == anchor.tick && note.layer == anchor.layer && note.voice == anchor.voice
        })
}

pub fn validate(score: &Score) -> Result<(), String> {
    let n = &score.notation;
    let anchors = n
        .ornaments
        .iter()
        .map(|o| &o.anchor)
        .chain(n.grace.iter().map(|g| &g.anchor))
        .chain(n.chords.iter().map(|c| &c.anchor))
        .chain(n.lines.iter().flat_map(|s| [&s.start, &s.end]))
        .chain(n.entries.iter().map(|e| &e.anchor))
        .chain(n.expressions.iter().map(|e| &e.anchor))
        .chain(n.spanners.iter().flat_map(|s| [&s.start, &s.end]))
        .chain(
            n.lyrics
                .iter()
                .flat_map(|l| std::iter::once(&l.anchor).chain(l.extend_to.as_ref())),
        )
        .chain(n.slurs.iter().flat_map(|s| [&s.start, &s.end]));
    if anchors.into_iter().any(|a| a.layer > 3 || a.voice > 1) {
        return Err("Notation must refer to one of four layers and two voices per layer.".into());
    }
    crate::grace::validate(score)?;
    crate::ornaments::validate(score)?;
    crate::chords::validate(score)?;
    crate::lines::validate(score)?;
    crate::entry_style::validate(score)?;
    score
        .notation
        .layout
        .validate_for_score(score.measure_count())?;
    crate::expressions::validate(score)?;
    crate::lyrics::validate(score)?;
    crate::slurs::validate(score)?;
    crate::repeats::validate(score)?;
    crate::parts::validate_definitions(&score.notation.parts, score.parts.len())?;
    for part in &score.notation.parts {
        part.layout.validate_for_score(score.measure_count())?;
    }
    Ok(())
}

pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
    crate::ornaments::remove_anchor(score, anchor);
    crate::grace::remove_anchor(score, anchor);
    crate::lines::remove_anchor(score, anchor);
    crate::entry_style::remove_anchor(score, anchor);
    crate::lyrics::remove_anchor(score, anchor);
    crate::slurs::remove_anchor(score, anchor);
}

/// Update notation after a deliberate onset/layer move. Reject conflicting
/// lyrics or broken phrase spans without partially changing the document.
pub fn move_anchor(score: &mut Score, from: Anchor, to: Anchor) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    let old = score.notation.clone();
    for ornament in &mut score.notation.ornaments {
        if ornament.anchor == from {
            ornament.anchor = to;
        }
    }
    for group in &mut score.notation.grace {
        if group.anchor == from {
            group.anchor = to;
        }
    }
    for line in &mut score.notation.lines {
        if matches!(line.kind, crate::lines::LineKind::Glissando) {
            if line.start == from {
                line.start = to;
            }
            if line.end == from {
                line.end = to;
            }
        }
    }
    for style in &mut score.notation.entries {
        if style.anchor == from {
            style.anchor = to;
        }
    }
    for expression in &mut score.notation.expressions {
        if expression.anchor == from {
            expression.anchor = to;
        }
    }
    for span in &mut score.notation.spanners {
        if span.start == from {
            span.start = to;
        }
        if span.end == from {
            span.end = to;
        }
    }
    for lyric in &mut score.notation.lyrics {
        if lyric.anchor == from {
            lyric.anchor = to;
        }
        if lyric.extend_to == Some(from) {
            lyric.extend_to = Some(to);
        }
    }
    for slur in &mut score.notation.slurs {
        if slur.start == from {
            slur.start = to;
        }
        if slur.end == from {
            slur.end = to;
        }
    }
    if let Err(error) = validate(score) {
        score.notation = old;
        return Err(error);
    }
    Ok(())
}

/// Call before removing the source staff, so every remaining anchor stays valid.
pub fn remove_staff(score: &mut Score, staff: usize) {
    crate::ornaments::remove_part(score, staff);
    crate::ornaments::reindex_parts(score, staff);
    crate::grace::remove_part(score, staff);
    crate::grace::reindex_parts(score, staff);
    crate::chords::remove_part(score, staff);
    crate::chords::reindex_parts(score, staff);
    crate::lines::remove_part(score, staff);
    crate::lines::reindex_parts(score, staff);
    crate::entry_style::remove_part(score, staff);
    score
        .notation
        .expressions
        .retain(|expression| expression.anchor.part != staff);
    for expression in &mut score.notation.expressions {
        if expression.anchor.part > staff {
            expression.anchor.part -= 1;
        }
    }
    score
        .notation
        .spanners
        .retain(|span| span.start.part != staff);
    for span in &mut score.notation.spanners {
        if span.start.part > staff {
            span.start.part -= 1;
        }
        if span.end.part > staff {
            span.end.part -= 1;
        }
    }
    crate::lyrics::remove_part(score, staff);
    crate::slurs::remove_part(score, staff);
    crate::lyrics::reindex_parts(score, staff);
    crate::slurs::reindex_parts(score, staff);
    for part in &mut score.notation.parts {
        part.voicing.retain(|rule| rule.staff != staff);
        for rule in &mut part.voicing {
            if rule.staff > staff {
                rule.staff -= 1;
            }
        }
        part.staves.retain(|index| *index != staff);
        for index in &mut part.staves {
            if *index > staff {
                *index -= 1;
            }
        }
    }
    score.notation.parts.retain(|part| !part.staves.is_empty());
}

/// Discard annotations whose source measures have been explicitly removed.
pub fn truncate_measures(score: &mut Score) -> Result<(), String> {
    crate::ornaments::prune(score);
    crate::grace::prune(score);
    crate::chords::prune(score);
    crate::lines::prune(score);
    crate::entry_style::prune(score);
    crate::lyrics::prune(score);
    crate::slurs::prune(score);
    let count = score.measure_count();
    score.notation.repeats.retain(|region| region.end < count);
    score
        .notation
        .navigation
        .retain(|mark| mark.measure < count);
    score.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cantabile_persists_and_projects_all_four_notation_features() {
        let score: Score =
            serde_json::from_str(include_str!("../examples/Cantabile.encore")).unwrap();
        score.validate().unwrap();
        assert_eq!(
            score.playback_order().unwrap(),
            vec![0, 1, 2, 3, 0, 1, 2, 4, 5, 6, 7]
        );
        let projection = crate::parts::project(&score, &score.notation.parts[1]).unwrap();
        assert_eq!(projection.score.parts.len(), 2);
        assert_eq!(projection.score.notation.slurs.len(), 1);
        assert!(projection.score.notation.lyrics.is_empty());
        assert_eq!(projection.score.notation.slurs[0].start.part, 0);
        let reopened: Score =
            serde_json::from_str(&serde_json::to_string(&score).unwrap()).unwrap();
        assert_eq!(score, reopened);
        let mut trimmed = score.clone();
        for staff in &mut trimmed.parts {
            staff.measures.truncate(3);
        }
        truncate_measures(&mut trimmed).unwrap();
        trimmed.validate().unwrap();
        assert_eq!(trimmed.notation.slurs.len(), 1);
        assert!(trimmed.notation.repeats.is_empty());
    }
    #[test]
    fn deleting_a_staff_reindexes_parts_and_anchored_underlay() {
        let mut score: Score =
            serde_json::from_str(include_str!("../examples/Cantabile.encore")).unwrap();
        remove_staff(&mut score, 1);
        score.parts.remove(1);
        score.validate().unwrap();
        assert_eq!(score.notation.parts[1].staves, vec![1]);
        assert_eq!(score.notation.parts[2].staves, vec![2]);
        assert_eq!(score.notation.lyrics.len(), 65);
        assert_eq!(score.notation.slurs.len(), 1);
    }
}

#[cfg(test)]
mod ornament_anchor_tests {
    use super::*;
    use crate::{
        lines::{LineKind, PerformanceLine},
        model::{Duration, Pitch},
    };
    #[test]
    fn principal_edits_move_attached_groups_and_keep_free_beat_marks() {
        let mut score = Score::blank();
        let note = |start, duration, midi| Note {
            voice: 0,
            voice_launch: None,
            start,
            layer: 0,
            duration,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(midi)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: false,
        };
        score.parts[0].measures[0].notes = vec![
            note(0, Duration::Eighth, 60),
            note(480, Duration::Quarter, 64),
        ];
        let old = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor { tick: 480, ..old };
        let new = Anchor { tick: 120, ..old };
        score.notation.grace.push(crate::grace::GraceGroup::new(
            old,
            vec![crate::grace::GraceNote::new(
                Pitch::from_midi(62),
                Duration::Eighth,
            )],
        ));
        score.notation.chords.push(crate::chords::ChordSymbol::new(
            old,
            crate::chords::parse("Cmaj7").unwrap(),
        ));
        score
            .notation
            .lines
            .push(PerformanceLine::new(old, end, LineKind::Glissando));
        score.notation.lines.push(PerformanceLine::new(
            old,
            Anchor { tick: 960, ..old },
            LineKind::Ottava { semitones: 12 },
        ));
        score.parts[0].measures[0].notes[0].start = 120;
        move_anchor(&mut score, old, new).unwrap();
        score.validate().unwrap();
        assert_eq!(score.notation.grace[0].anchor, new);
        assert_eq!(score.notation.lines[0].start, new);
        assert_eq!(score.notation.chords[0].anchor, old);
        assert_eq!(score.notation.lines[1].start, old);
        remove_anchor(&mut score, &new);
        score.parts[0].measures[0].notes.remove(0);
        score.validate().unwrap();
        assert!(score.notation.grace.is_empty());
        assert_eq!(score.notation.lines.len(), 1);
        assert_eq!(score.notation.chords.len(), 1);
    }
}

#[cfg(test)]
mod voice_tests {
    use super::*;
    use crate::{
        entry_style::{EntryStyle, StemDirection},
        lyrics::{Lyric, LyricKind},
        model::{Duration, Pitch},
    };
    fn fixture() -> Score {
        let mut score = Score::blank();
        for start in [0, 480] {
            for voice in [0, 1] {
                score.parts[0].measures[0].notes.push(Note {
                    layer: 0,
                    voice,
                    voice_launch: (voice == 1).then_some(start),
                    start,
                    duration: Duration::Quarter,
                    dotted: false,
                    tuplet: None,
                    pitches: vec![Pitch::from_midi(60 + voice * 7)],
                    velocity: 80,
                    staccato: false,
                    articulations: vec![],
                    tied: false,
                });
            }
        }
        score.normalize();
        score
    }
    fn anchor(voice: u8, tick: u32) -> Anchor {
        Anchor {
            part: 0,
            measure: 0,
            tick,
            layer: 0,
            voice,
        }
    }
    #[test]
    fn underlay_and_entry_styles_keep_simultaneous_voice_identities() {
        let mut score = fixture();
        for voice in [0, 1] {
            crate::lyrics::assign_text(
                &mut score,
                anchor(voice, 0),
                LyricKind::Verse,
                1,
                if voice == 0 {
                    "upper line"
                } else {
                    "lower line"
                },
            )
            .unwrap();
        }
        let mut style = EntryStyle::new(anchor(1, 0));
        style.stem = StemDirection::Down;
        crate::entry_style::assign(&mut score, style).unwrap();
        score.validate().unwrap();
        assert_eq!(score.notation.lyrics.len(), 4);
        assert_eq!(
            crate::lyrics::next_anchor(&score, &anchor(1, 0), false),
            Some(anchor(1, 480))
        );
        assert!(crate::entry_style::at(&score, &anchor(0, 0)).is_none());
        remove_anchor(&mut score, &anchor(1, 0));
        assert_eq!(score.notation.lyrics.len(), 3);
        assert!(
            score
                .notation
                .lyrics
                .iter()
                .any(|l| l.anchor == anchor(0, 0))
        );
        assert!(score.notation.entries.is_empty());
        let mut mixed = crate::slurs::Slur::new(anchor(0, 0), anchor(1, 480));
        crate::slurs::add(&mut score, mixed.clone()).unwrap();
        assert!(!crate::slurs::continues(&score, &anchor(0, 0)));
        let mut cross_staff = mixed.clone();
        cross_staff.end.part = 1;
        assert!(crate::slurs::add(&mut score, cross_staff).is_err());
        mixed.start.voice = 1;
        crate::slurs::add(&mut score, mixed).unwrap();
        score.validate().unwrap();
    }
    #[test]
    fn corrupt_notation_voice_is_rejected_without_arithmetic_panic() {
        let mut score = fixture();
        score
            .notation
            .lyrics
            .push(Lyric::new(anchor(u8::MAX, 0), "invalid"));
        assert!(score.validate().unwrap_err().contains("two voices"));
    }
}
