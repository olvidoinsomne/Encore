//! Persistent notation anchored to source staves and rhythmic onsets.
use crate::model::{Note, Score, Selection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Anchor {
    pub part: usize,
    pub measure: usize,
    pub tick: u32,
    pub layer: u8,
}

impl Anchor {
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
            .position(|note| note.start == self.tick && note.layer == self.layer)?;
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
        self.lyrics.is_empty()
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
        .find(|note| note.start == anchor.tick && note.layer == anchor.layer)
}

pub fn validate(score: &Score) -> Result<(), String> {
    crate::lyrics::validate(score)?;
    crate::slurs::validate(score)?;
    crate::repeats::validate(score)?;
    crate::parts::validate_definitions(&score.notation.parts, score.parts.len())
}

pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
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
    crate::lyrics::remove_part(score, staff);
    crate::slurs::remove_part(score, staff);
    crate::lyrics::reindex_parts(score, staff);
    crate::slurs::reindex_parts(score, staff);
    for part in &mut score.notation.parts {
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
