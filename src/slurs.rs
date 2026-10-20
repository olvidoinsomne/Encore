//! Phrase curves anchored to rhythmic entries, independent from duration ties.
use crate::document::{Anchor, note_at};
use crate::lyrics::position;
use crate::model::Score;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlurDirection {
    #[default]
    Auto,
    Above,
    Below,
}
impl SlurDirection {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Above, Self::Below];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Above => "Above",
            Self::Below => "Below",
        }
    }
}
fn default_height() -> f32 {
    18.0
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slur {
    pub start: Anchor,
    pub end: Anchor,
    #[serde(default)]
    pub direction: SlurDirection,
    #[serde(default = "default_height")]
    pub height: f32,
    #[serde(default)]
    pub start_offset: [f32; 2],
    #[serde(default)]
    pub end_offset: [f32; 2],
}
impl Slur {
    pub fn new(start: Anchor, end: Anchor) -> Self {
        Self {
            start,
            end,
            direction: SlurDirection::Auto,
            height: 18.0,
            start_offset: [0.0; 2],
            end_offset: [0.0; 2],
        }
    }
}
fn check_geometry(slur: &Slur) -> Result<(), String> {
    if slur.start.part != slur.end.part
        || slur.start.lane() != slur.end.lane()
        || position(&slur.start) >= position(&slur.end)
    {
        return Err("A slur must join later notes on the same staff, layer, and voice.".into());
    }
    if !slur.height.is_finite() || !(4.0..=100.0).contains(&slur.height) {
        return Err("Slur curve height must be 4–100 points.".into());
    }
    if slur
        .start_offset
        .iter()
        .chain(&slur.end_offset)
        .any(|v| !v.is_finite() || v.abs() > 80.0)
    {
        return Err("Slur endpoint adjustments must be within 80 points.".into());
    }
    Ok(())
}
fn check(score: &Score, slur: &Slur) -> Result<(), String> {
    check_geometry(slur)?;
    for anchor in [&slur.start, &slur.end] {
        if note_at(score, anchor).is_none_or(|n| n.pitches.is_empty()) {
            return Err("Slur endpoints must attach to existing pitched notes.".into());
        }
    }
    Ok(())
}

pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.slurs.len() > 20_000 {
        return Err("Score exceeds the 20,000 slur limit.".into());
    }
    if score.notation.slurs.is_empty() {
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
        .collect::<std::collections::BTreeSet<_>>();
    let mut endpoints = std::collections::BTreeSet::new();
    let mut events = std::collections::BTreeMap::<(usize, usize, u32), i32>::new();
    for slur in &score.notation.slurs {
        check_geometry(slur)?;
        if !anchors.contains(&slur.start) || !anchors.contains(&slur.end) {
            return Err("Slur endpoints must attach to existing pitched notes.".into());
        }
        if !endpoints.insert((slur.start, slur.end)) {
            return Err("Duplicate slur endpoints.".into());
        }
        *events
            .entry((slur.start.part, slur.start.measure, slur.start.tick))
            .or_default() += 1;
        *events
            .entry((slur.end.part, slur.end.measure, slur.end.tick))
            .or_default() -= 1;
    }
    let mut current_part = usize::MAX;
    let mut active = 0;
    for ((part, _, _), change) in events {
        if part != current_part {
            active = 0;
            current_part = part;
        }
        active += change;
        if active > 16 {
            return Err(
                "At most 16 overlapping slurs per staff can be interchanged reliably.".into(),
            );
        }
    }
    Ok(())
}

pub fn add(score: &mut Score, slur: Slur) -> Result<(), String> {
    check(score, &slur)?;
    if let Some(prior) = score
        .notation
        .slurs
        .iter_mut()
        .find(|s| s.start == slur.start && s.end == slur.end)
    {
        *prior = slur;
    } else {
        if score.notation.slurs.len() >= 20_000 {
            return Err("Score exceeds the slur limit.".into());
        }
        score.notation.slurs.push(slur);
        if let Err(error) = validate(score) {
            score.notation.slurs.pop();
            return Err(error);
        }
    }
    Ok(())
}
pub fn remove_starting_at(score: &mut Score, start: &Anchor) {
    score.notation.slurs.retain(|s| s.start != *start);
}
pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
    score
        .notation
        .slurs
        .retain(|s| s.start != *anchor && s.end != *anchor);
}
pub fn remove_part(score: &mut Score, part: usize) {
    score.notation.slurs.retain(|s| s.start.part != part);
}
pub fn reindex_parts(score: &mut Score, removed: usize) {
    for s in &mut score.notation.slurs {
        if s.start.part > removed {
            s.start.part -= 1;
        }
        if s.end.part > removed {
            s.end.part -= 1;
        }
    }
}
pub fn prune(score: &mut Score) {
    let mut slurs = std::mem::take(&mut score.notation.slurs);
    slurs.retain(|s| check(score, s).is_ok());
    score.notation.slurs = slurs;
}
/// Interior notes use a full legato gate. Explicit short articulations still win.
pub fn continues(score: &Score, anchor: &Anchor) -> bool {
    score.notation.slurs.iter().any(|s| {
        s.start.part == anchor.part
            && s.start.lane() == anchor.lane()
            && position(&s.start) <= position(anchor)
            && position(anchor) < position(&s.end)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phrase_curves_preserve_native_adjustments() {
        let mut score = Score::demo();
        let start = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor {
            voice: 0,
            part: 0,
            measure: 2,
            tick: 0,
            layer: 0,
        };
        let mut slur = Slur::new(start, end);
        slur.direction = SlurDirection::Below;
        slur.height = 30.0;
        slur.start_offset = [2.0, 3.0];
        add(&mut score, slur).unwrap();
        let round: Score = serde_json::from_str(&serde_json::to_string(&score).unwrap()).unwrap();
        assert_eq!(score, round);
        assert!(continues(&score, &start));
        assert!(!continues(&score, &end));
        let old = score.clone();
        assert!(add(&mut score, Slur::new(end, start)).is_err());
        assert_eq!(score, old);
        remove_anchor(&mut score, &start);
        assert!(score.notation.slurs.is_empty());
    }
}
