//! Anchored vocal underlay. Lyrics remain attached to rhythmic entries when
//! noteheads move; verses, choruses and sections use independent text lines.
use crate::document::{Anchor, note_at};
use crate::model::Score;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_LYRICS: usize = 50_000;
pub const MAX_TEXT_CHARS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LyricKind {
    #[default]
    Verse,
    Chorus,
    Section,
}
impl LyricKind {
    pub const ALL: [Self; 3] = [Self::Verse, Self::Chorus, Self::Section];
    pub fn name(self) -> &'static str {
        match self {
            Self::Verse => "Verse",
            Self::Chorus => "Chorus",
            Self::Section => "Section",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Syllabic {
    #[default]
    Single,
    Begin,
    Middle,
    End,
}
impl Syllabic {
    pub const ALL: [Self; 4] = [Self::Single, Self::Begin, Self::Middle, Self::End];
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Begin => "Begin",
            Self::Middle => "Middle",
            Self::End => "End",
        }
    }
    pub fn musicxml(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Begin => "begin",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "single" | "0" => Some(Self::Single),
            "begin" | "1" => Some(Self::Begin),
            "end" | "2" => Some(Self::End),
            "middle" | "3" => Some(Self::Middle),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lyric {
    pub anchor: Anchor,
    #[serde(default)]
    pub kind: LyricKind,
    pub verse: u16,
    pub text: String,
    #[serde(default)]
    pub syllabic: Syllabic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extend_to: Option<Anchor>,
    /// Extra distance below the automatically cleared staff baseline, in points.
    #[serde(default)]
    pub baseline: f32,
}
impl Lyric {
    pub fn new(anchor: Anchor, text: impl Into<String>) -> Self {
        Self {
            anchor,
            text: text.into(),
            kind: LyricKind::Verse,
            verse: 1,
            syllabic: Syllabic::Single,
            extend_to: None,
            baseline: 0.0,
        }
    }
}
pub fn position(anchor: &Anchor) -> (usize, u32) {
    (anchor.measure, anchor.tick)
}
fn check_content(lyric: &Lyric) -> Result<(), String> {
    if !(1..=512).contains(&lyric.verse) {
        return Err("Lyric numbers must be 1–512.".into());
    }
    if lyric.text.trim().is_empty()
        || lyric.text.chars().count() > MAX_TEXT_CHARS
        || lyric.text.chars().any(|c| c.is_control())
    {
        return Err(format!(
            "A lyric syllable must contain 1–{MAX_TEXT_CHARS} printable characters."
        ));
    }
    if !lyric.baseline.is_finite() || !(-24.0..=120.0).contains(&lyric.baseline) {
        return Err("Lyric baseline adjustment must be −24–120 points.".into());
    }
    if let Some(end) = &lyric.extend_to
        && (end.part != lyric.anchor.part
            || end.lane() != lyric.anchor.lane()
            || position(end) <= position(&lyric.anchor))
    {
        return Err(
            "A word extension must end on a later note in the same staff, layer, and voice.".into(),
        );
    }
    Ok(())
}
fn check(score: &Score, lyric: &Lyric) -> Result<(), String> {
    check_content(lyric)?;
    if note_at(score, &lyric.anchor).is_none_or(|n| n.pitches.is_empty()) {
        return Err("A lyric must be attached to an existing pitched note onset.".into());
    }
    if lyric
        .extend_to
        .is_some_and(|end| note_at(score, &end).is_none_or(|n| n.pitches.is_empty()))
    {
        return Err("A word extension must end on an existing pitched note.".into());
    }
    Ok(())
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.lyrics.len() > MAX_LYRICS {
        return Err("Score exceeds the 50,000 lyric syllable limit.".into());
    }
    if score.notation.lyrics.is_empty() {
        return Ok(());
    }
    let anchors = score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(part, p)| {
            p.measures.iter().enumerate().flat_map(move |(measure, m)| {
                m.notes
                    .iter()
                    .filter(|n| !n.pitches.is_empty())
                    .map(move |n| Anchor {
                        part,
                        measure,
                        tick: n.start,
                        layer: n.layer,
                        voice: n.voice,
                    })
            })
        })
        .collect::<BTreeSet<_>>();
    let mut occupied = BTreeSet::new();
    for lyric in &score.notation.lyrics {
        check_content(lyric)?;
        if !anchors.contains(&lyric.anchor)
            || lyric.extend_to.is_some_and(|a| !anchors.contains(&a))
        {
            return Err("A lyric or word extension refers to a missing pitched note.".into());
        }
        let a = &lyric.anchor;
        if !occupied.insert((a.part, a.measure, a.tick, a.lane(), lyric.kind, lyric.verse)) {
            return Err("A note has duplicate syllables on the same lyric line.".into());
        }
    }
    Ok(())
}

/// Replace one syllable atomically without disturbing other verses or notes.
pub fn assign(score: &mut Score, lyric: Lyric) -> Result<(), String> {
    check(score, &lyric)?;
    if let Some(old) = score.notation.lyrics.iter_mut().find(|old| {
        old.anchor == lyric.anchor && old.kind == lyric.kind && old.verse == lyric.verse
    }) {
        *old = lyric;
    } else {
        if score.notation.lyrics.len() >= MAX_LYRICS {
            return Err("Score exceeds the lyric syllable limit.".into());
        }
        score.notation.lyrics.push(lyric);
    }
    Ok(())
}
pub fn remove(score: &mut Score, anchor: &Anchor, kind: LyricKind, verse: u16) {
    score
        .notation
        .lyrics
        .retain(|l| l.anchor != *anchor || l.kind != kind || l.verse != verse);
}
pub fn set_text(
    score: &mut Score,
    anchor: &Anchor,
    kind: LyricKind,
    verse: u16,
    text: &str,
) -> Result<(), String> {
    if text.is_empty() {
        remove(score, anchor, kind, verse);
        return Ok(());
    }
    let mut lyric = score
        .notation
        .lyrics
        .iter()
        .find(|l| l.anchor == *anchor && l.kind == kind && l.verse == verse)
        .cloned()
        .unwrap_or_else(|| Lyric {
            kind,
            verse,
            ..Lyric::new(*anchor, text)
        });
    lyric.text = text.into();
    assign(score, lyric)
}
pub fn next_anchor(score: &Score, anchor: &Anchor, backward: bool) -> Option<Anchor> {
    let part = score.parts.get(anchor.part)?;
    let mut candidates = part
        .measures
        .iter()
        .enumerate()
        .flat_map(|(measure, m)| {
            m.notes
                .iter()
                .filter(|n| n.lane() == anchor.lane() && !n.pitches.is_empty())
                .map(move |n| Anchor {
                    part: anchor.part,
                    measure,
                    tick: n.start,
                    layer: n.layer,
                    voice: n.voice,
                })
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(position);
    if backward {
        candidates
            .into_iter()
            .rev()
            .find(|a| position(a) < position(anchor))
    } else {
        candidates
            .into_iter()
            .find(|a| position(a) > position(anchor))
    }
}
/// Assign whitespace/hyphen-separated syllables in one undoable operation.
/// Underscores skip melody notes and extend the preceding syllable.
pub fn assign_text(
    score: &mut Score,
    start: Anchor,
    kind: LyricKind,
    verse: u16,
    text: &str,
) -> Result<Anchor, String> {
    enum Action {
        Syllable(String, Syllabic),
        Extend,
    }
    if text.len() > 1_000_000 {
        return Err("Lyric assignment text exceeds 1 MB.".into());
    }
    let mut actions = Vec::new();
    let mut token = String::new();
    let mut prior_hyphen = false;
    for c in text.chars().chain(std::iter::once(' ')) {
        if c == '-' || c == '_' || c.is_whitespace() {
            if !token.is_empty() {
                let syllabic = match (prior_hyphen, c == '-') {
                    (false, false) => Syllabic::Single,
                    (false, true) => Syllabic::Begin,
                    (true, true) => Syllabic::Middle,
                    (true, false) => Syllabic::End,
                };
                actions.push(Action::Syllable(std::mem::take(&mut token), syllabic));
                prior_hyphen = c == '-';
            } else if c == '-' {
                return Err("A hyphen must follow a lyric syllable.".into());
            }
            if c == '_' {
                actions.push(Action::Extend);
            }
        } else {
            token.push(c);
        }
    }
    if actions.is_empty() {
        return Err("Enter at least one lyric syllable.".into());
    }
    if actions.len() > MAX_LYRICS {
        return Err("Too many lyric syllables in one assignment.".into());
    }
    let mut anchors = score
        .parts
        .get(start.part)
        .ok_or("Unknown lyric staff.")?
        .measures
        .iter()
        .enumerate()
        .flat_map(|(measure, m)| {
            m.notes
                .iter()
                .filter(|n| n.lane() == start.lane() && !n.pitches.is_empty())
                .map(move |n| Anchor {
                    part: start.part,
                    measure,
                    tick: n.start,
                    layer: n.layer,
                    voice: n.voice,
                })
        })
        .collect::<Vec<_>>();
    anchors.sort_by_key(position);
    let offset = anchors
        .iter()
        .position(|a| *a == start)
        .ok_or("Lyrics must start on an existing melody note.")?;
    if actions.len() > anchors.len() - offset {
        return Err(
            "There are not enough melody notes for the lyric text; no syllables were changed."
                .into(),
        );
    }
    let mut edited = score.clone();
    let mut syllables = std::mem::take(&mut edited.notation.lyrics)
        .into_iter()
        .map(|l| ((l.anchor, l.kind, l.verse), l))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut last_lyric = None;
    let length = actions.len();
    for (index, action) in actions.into_iter().enumerate() {
        let anchor = anchors[offset + index];
        match action {
            Action::Syllable(text, syllabic) => {
                syllables.insert(
                    (anchor, kind, verse),
                    Lyric {
                        kind,
                        verse,
                        syllabic,
                        ..Lyric::new(anchor, text)
                    },
                );
                last_lyric = Some(anchor);
            }
            Action::Extend => {
                let previous =
                    last_lyric.ok_or("A word extension needs a preceding lyric syllable.")?;
                let lyric = syllables
                    .get_mut(&(previous, kind, verse))
                    .ok_or("Missing lyric syllable.")?;
                lyric.extend_to = Some(anchor);
            }
        }
    }
    edited.notation.lyrics = syllables.into_values().collect();
    validate(&edited)?;
    score.notation.lyrics = edited.notation.lyrics;
    Ok(anchors
        .get(offset + length)
        .copied()
        .unwrap_or_else(|| *anchors.last().unwrap()))
}

pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
    score
        .notation
        .lyrics
        .retain(|l| l.anchor != *anchor && l.extend_to.as_ref() != Some(anchor));
}
pub fn remove_part(score: &mut Score, part: usize) {
    score.notation.lyrics.retain(|l| l.anchor.part != part);
}
pub fn reindex_parts(score: &mut Score, removed: usize) {
    for l in &mut score.notation.lyrics {
        if l.anchor.part > removed {
            l.anchor.part -= 1;
        }
        if let Some(a) = &mut l.extend_to
            && a.part > removed
        {
            a.part -= 1;
        }
    }
}
pub fn prune(score: &mut Score) {
    let mut lyrics = std::mem::take(&mut score.notation.lyrics);
    lyrics.retain(|l| check(score, l).is_ok());
    score.notation.lyrics = lyrics;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Note, Pitch};
    fn score() -> Score {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        for tick in [0, 480, 960, 1440] {
            score.parts[0].measures[0].notes.push(Note {
                voice: 0,
                voice_launch: None,
                start: tick,
                duration: Duration::Quarter,
                dotted: false,
                tuplet: None,
                layer: 0,
                pitches: vec![Pitch::from_midi(60)],
                velocity: 80,
                staccato: false,
                articulations: vec![],
                tied: false,
            });
        }
        score
    }
    fn anchor(tick: u32) -> Anchor {
        Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick,
            layer: 0,
        }
    }
    #[test]
    fn text_assignment_hyphen_and_melisma_are_atomic() {
        let mut s = score();
        assign_text(&mut s, anchor(0), LyricKind::Verse, 1, "Sil-ver _ bells").unwrap();
        assert_eq!(s.notation.lyrics.len(), 3);
        assert_eq!(s.notation.lyrics[0].syllabic, Syllabic::Begin);
        assert_eq!(s.notation.lyrics[1].syllabic, Syllabic::End);
        assert_eq!(s.notation.lyrics[1].extend_to, Some(anchor(960)));
        assert_eq!(s.notation.lyrics[2].anchor, anchor(1440));
        let before = s.clone();
        assert!(
            assign_text(
                &mut s,
                anchor(0),
                LyricKind::Verse,
                1,
                "too ma-ny syl-la-bles"
            )
            .is_err()
        );
        assert_eq!(s, before);
    }
    #[test]
    fn verse_kinds_edit_independently_and_native_roundtrip() {
        let mut s = score();
        assign(&mut s, Lyric::new(anchor(0), "First")).unwrap();
        assign(
            &mut s,
            Lyric {
                kind: LyricKind::Chorus,
                verse: 2,
                ..Lyric::new(anchor(0), "Other")
            },
        )
        .unwrap();
        set_text(&mut s, &anchor(0), LyricKind::Verse, 1, "Changed").unwrap();
        assert_eq!(s.notation.lyrics[1].text, "Other");
        let json = serde_json::to_string(&s).unwrap();
        let reopened: Score = serde_json::from_str(&json).unwrap();
        reopened.validate().unwrap();
        assert_eq!(s, reopened);
        remove(&mut s, &anchor(0), LyricKind::Verse, 1);
        assert_eq!(s.notation.lyrics.len(), 1);
    }
    #[test]
    fn invalid_lyrics_do_not_replace_good_text() {
        let mut s = score();
        assign(&mut s, Lyric::new(anchor(0), "Good")).unwrap();
        let old = s.clone();
        assert!(
            assign(
                &mut s,
                Lyric {
                    baseline: f32::NAN,
                    ..Lyric::new(anchor(0), "Bad")
                }
            )
            .is_err()
        );
        assert_eq!(s, old);
        assert!(
            assign(
                &mut s,
                Lyric {
                    verse: 513,
                    ..Lyric::new(anchor(0), "Bad")
                }
            )
            .is_err()
        );
        assert!(
            assign(
                &mut s,
                Lyric {
                    extend_to: Some(anchor(0)),
                    ..Lyric::new(anchor(0), "Bad")
                }
            )
            .is_err()
        );
    }
}
