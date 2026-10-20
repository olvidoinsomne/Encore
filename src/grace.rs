//! Grace groups decorate a principal entry without changing its written rhythm.
use crate::document::{Anchor, note_at};
use crate::entry_style::BeamStyle;
use crate::model::{Duration, Pitch, Score};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_GROUPS: usize = 10_000;
pub const MAX_GRACE_NOTES: usize = 50_000;
pub const MAX_GROUP_NOTES: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GracePlacement {
    #[default]
    Before,
    After,
}
impl GracePlacement {
    pub const ALL: [Self; 2] = [Self::Before, Self::After];
    pub fn name(self) -> &'static str {
        match self {
            Self::Before => "Before principal note",
            Self::After => "After principal note",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraceTiming {
    #[default]
    StealPrevious,
    StealMain,
}
impl GraceTiming {
    pub const ALL: [Self; 2] = [Self::StealPrevious, Self::StealMain];
    pub fn name(self) -> &'static str {
        match self {
            Self::StealPrevious => "Borrow previous note",
            Self::StealMain => "Borrow principal note",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraceSelection {
    pub anchor: Anchor,
    pub placement: GracePlacement,
    pub note: usize,
}
fn default_scale() -> f32 {
    0.6
}
fn default_percent() -> f32 {
    20.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraceNote {
    pub pitches: Vec<Pitch>,
    pub duration: Duration,
    #[serde(default)]
    pub dotted: bool,
    #[serde(default)]
    pub velocity: Option<u8>,
    #[serde(default)]
    pub beam: BeamStyle,
}
impl GraceNote {
    pub fn new(pitch: Pitch, duration: Duration) -> Self {
        Self {
            pitches: vec![pitch],
            duration,
            dotted: false,
            velocity: None,
            beam: BeamStyle::Auto,
        }
    }
    /// Twice the nominal duration, so even dotted 128ths have exact weights.
    pub fn weight(&self) -> u32 {
        self.duration.ticks() * if self.dotted { 3 } else { 2 }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraceGroup {
    pub anchor: Anchor,
    #[serde(default)]
    pub placement: GracePlacement,
    #[serde(default)]
    pub slashed: bool,
    #[serde(default = "default_scale")]
    pub scale: f32,
    pub notes: Vec<GraceNote>,
    #[serde(default)]
    pub timing: GraceTiming,
    #[serde(default = "default_percent")]
    pub percent: f32,
    #[serde(default)]
    pub slur_to_main: bool,
}
impl GraceGroup {
    pub fn new(anchor: Anchor, notes: Vec<GraceNote>) -> Self {
        Self {
            anchor,
            placement: GracePlacement::Before,
            slashed: false,
            scale: default_scale(),
            notes,
            timing: GraceTiming::StealPrevious,
            percent: default_percent(),
            slur_to_main: false,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.notes.is_empty() || self.notes.len() > MAX_GROUP_NOTES {
            return Err(format!(
                "A grace group must contain 1–{MAX_GROUP_NOTES} notes or chords."
            ));
        }
        if !self.scale.is_finite() || !(0.4..=1.0).contains(&self.scale) {
            return Err("Grace-note size must be 40–100%.".into());
        }
        if !self.percent.is_finite() || !(0.0..=75.0).contains(&self.percent) {
            return Err("Grace playback may borrow 0–75% of its donor note.".into());
        }
        if self.placement == GracePlacement::After && self.timing != GraceTiming::StealMain {
            return Err("After-grace groups borrow the principal note's ending.".into());
        }
        for note in &self.notes {
            if note.pitches.is_empty() || note.pitches.len() > 16 {
                return Err("A grace chord must contain 1–16 pitches.".into());
            }
            if note.velocity.is_some_and(|v| v > 127) {
                return Err("Grace velocity must be 0–127.".into());
            }
            let mut pitches = BTreeSet::new();
            for pitch in &note.pitches {
                let midi = (i32::from(pitch.octave) + 1) * 12
                    + pitch.step.semitone()
                    + i32::from(pitch.alter);
                if !(-2..=2).contains(&pitch.alter) || !(0..=127).contains(&midi) {
                    return Err("Grace pitches require MIDI range 0–127 and accidentals within two semitones.".into());
                }
                if !pitches.insert(midi) {
                    return Err("Grace chords cannot duplicate a sounding pitch.".into());
                }
            }
        }
        Ok(())
    }
}

pub fn at<'a>(
    score: &'a Score,
    anchor: &Anchor,
    placement: GracePlacement,
) -> Option<&'a GraceGroup> {
    score
        .notation
        .grace
        .iter()
        .find(|g| g.anchor == *anchor && g.placement == placement)
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.grace.len() > MAX_GROUPS {
        return Err(format!("Score exceeds the {MAX_GROUPS} grace-group limit."));
    }
    if score.notation.grace.is_empty() {
        return Ok(());
    }
    let anchors = pitched_anchors(score);
    let mut keys = BTreeSet::new();
    let mut total = 0;
    for group in &score.notation.grace {
        group.validate()?;
        total += group.notes.len();
        if total > MAX_GRACE_NOTES {
            return Err(format!(
                "Score exceeds the {MAX_GRACE_NOTES} grace-note limit."
            ));
        }
        if !anchors.contains(&group.anchor) {
            return Err("Grace groups must attach to an existing pitched principal entry.".into());
        }
        if !keys.insert((group.anchor, group.placement)) {
            return Err("A principal entry can have one grace group on each side.".into());
        }
    }
    Ok(())
}
fn pitched_anchors(score: &Score) -> BTreeSet<Anchor> {
    score
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
        .collect()
}
/// Replace one group atomically. Written entry durations and onsets never move.
pub fn assign(score: &mut Score, group: GraceGroup) -> Result<(), String> {
    group.validate()?;
    if note_at(score, &group.anchor).is_none_or(|n| n.pitches.is_empty()) {
        return Err("Select a pitched principal note before adding a grace group.".into());
    }
    let index = score
        .notation
        .grace
        .iter()
        .position(|g| g.anchor == group.anchor && g.placement == group.placement);
    let total = score
        .notation
        .grace
        .iter()
        .map(|g| g.notes.len())
        .sum::<usize>()
        + group.notes.len()
        - index.map_or(0, |i| score.notation.grace[i].notes.len());
    if total > MAX_GRACE_NOTES || (index.is_none() && score.notation.grace.len() >= MAX_GROUPS) {
        return Err("Grace-group document limits exceeded.".into());
    }
    if let Some(i) = index {
        score.notation.grace[i] = group;
    } else {
        score.notation.grace.push(group);
    }
    Ok(())
}
pub fn remove(score: &mut Score, anchor: &Anchor, placement: GracePlacement) {
    score
        .notation
        .grace
        .retain(|g| g.anchor != *anchor || g.placement != placement);
}
pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
    score.notation.grace.retain(|g| g.anchor != *anchor);
}
pub fn remove_part(score: &mut Score, part: usize) {
    score.notation.grace.retain(|g| g.anchor.part != part);
}
pub fn reindex_parts(score: &mut Score, removed: usize) {
    for g in &mut score.notation.grace {
        if g.anchor.part > removed {
            g.anchor.part -= 1;
        }
    }
}
pub fn prune(score: &mut Score) {
    let anchors = pitched_anchors(score);
    score.notation.grace.retain(|g| anchors.contains(&g.anchor));
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
/// Choose a connector level distinct from native phrase slurs at this onset.
/// The hidden metadata still retains the connector if all 16 levels are busy.
pub fn connector_number(score: &Score, anchor: &Anchor) -> Option<usize> {
    crate::slurs::MusicXmlNumbering::new(score)
        .ok()?
        .connector(anchor)
}
pub fn musicxml_before(score: &Score, anchor: &Anchor) -> String {
    musicxml_group(score, anchor, GracePlacement::Before)
}
pub fn musicxml_after(score: &Score, anchor: &Anchor) -> String {
    musicxml_group(score, anchor, GracePlacement::After)
}
/// Build interchange fragments once; ordinary notes then need a logarithmic
/// anchor lookup, with no whole-document scan for every written entry.
#[derive(Default)]
pub struct MusicXmlIndex {
    before: BTreeMap<Anchor, String>,
    after: BTreeMap<Anchor, String>,
    main: BTreeMap<Anchor, String>,
}
impl MusicXmlIndex {
    pub fn new(score: &Score) -> Self {
        let mut result = Self::default();
        if score.notation.grace.is_empty() {
            return result;
        }
        let groups = score
            .notation
            .grace
            .iter()
            .map(|g| ((g.anchor, g.placement), g))
            .collect::<BTreeMap<_, _>>();
        let anchors = groups.keys().map(|(a, _)| *a).collect::<BTreeSet<_>>();
        let numbering = crate::slurs::MusicXmlNumbering::new(score).unwrap_or_default();
        let numbers = anchors
            .iter()
            .map(|a| (*a, numbering.connector(a)))
            .collect::<BTreeMap<_, _>>();
        for ((anchor, placement), group) in &groups {
            let number = numbers.get(anchor).copied().flatten();
            let xml = write_musicxml_group(group, number);
            if *placement == GracePlacement::Before {
                result.before.insert(*anchor, xml);
            } else {
                result.after.insert(*anchor, xml);
            }
        }
        for anchor in anchors {
            if let Some(number) = numbers.get(&anchor).copied().flatten() {
                let before = groups
                    .get(&(anchor, GracePlacement::Before))
                    .is_some_and(|g| g.slur_to_main);
                let after = groups
                    .get(&(anchor, GracePlacement::After))
                    .is_some_and(|g| g.slur_to_main);
                result
                    .main
                    .insert(anchor, write_main_notations(number, before, after));
            }
        }
        result
    }
    pub fn before(&self, anchor: &Anchor) -> &str {
        self.before.get(anchor).map_or("", String::as_str)
    }
    pub fn after(&self, anchor: &Anchor) -> &str {
        self.after.get(anchor).map_or("", String::as_str)
    }
    pub fn main_notations(&self, anchor: &Anchor) -> &str {
        self.main.get(anchor).map_or("", String::as_str)
    }
}
fn write_main_notations(number: usize, before: bool, after: bool) -> String {
    if !before && !after {
        return String::new();
    }
    let mut out = format!(
        "<other-notation type=\"single\" print-object=\"no\">encore-grace-connectors:{number}</other-notation>"
    );
    if before {
        out.push_str(&format!("<slur type=\"stop\" number=\"{number}\"/>"));
    }
    if after {
        out.push_str(&format!("<slur type=\"start\" number=\"{number}\"/>"));
    }
    out
}
/// Add inside the principal note's `<notations>` on its first chord tone.
pub fn musicxml_main_notations(score: &Score, anchor: &Anchor) -> String {
    if !score
        .notation
        .grace
        .iter()
        .any(|g| g.anchor == *anchor && g.slur_to_main)
    {
        return String::new();
    }
    let Some(number) = connector_number(score, anchor) else {
        return String::new();
    };
    let mut out = String::new();
    let connectors = score
        .notation
        .grace
        .iter()
        .filter(|g| g.anchor == *anchor && g.slur_to_main)
        .collect::<Vec<_>>();
    if !connectors.is_empty() {
        out.push_str(&format!("<other-notation type=\"single\" print-object=\"no\">encore-grace-connectors:{number}</other-notation>"));
    }
    for group in connectors {
        out.push_str(&format!(
            "<slur type=\"{}\" number=\"{number}\"/>",
            if group.placement == GracePlacement::Before {
                "stop"
            } else {
                "start"
            }
        ));
    }
    out
}
fn musicxml_group(score: &Score, anchor: &Anchor, placement: GracePlacement) -> String {
    let Some(group) = at(score, anchor, placement) else {
        return String::new();
    };
    let number = connector_number(score, anchor);
    write_musicxml_group(group, number)
}
fn write_musicxml_group(group: &GraceGroup, number: Option<usize>) -> String {
    use std::fmt::Write;
    let anchor = group.anchor;
    let placement = group.placement;
    let mut out = String::new();
    for (index, note) in group.notes.iter().enumerate() {
        for (tone, pitch) in note.pitches.iter().enumerate() {
            out.push_str("      <note>");
            let timing = if placement == GracePlacement::After
                || group.timing == GraceTiming::StealPrevious
            {
                "steal-time-previous"
            } else {
                "steal-time-following"
            };
            let percent = f64::from(group.percent) * f64::from(note.weight())
                / group
                    .notes
                    .iter()
                    .map(|n| f64::from(n.weight()))
                    .sum::<f64>();
            let _ = write!(
                out,
                "<grace slash=\"{}\" {timing}=\"{percent}\"/>",
                if group.slashed { "yes" } else { "no" }
            );
            if tone > 0 {
                out.push_str("<chord/>");
            }
            let _ = write!(
                out,
                "<pitch><step>{}</step><alter>{}</alter><octave>{}</octave></pitch><voice>{}</voice><type size=\"cue\">{}</type>",
                pitch.step.name(),
                pitch.alter,
                pitch.octave,
                anchor.lane() + 1,
                note.duration.musicxml_type()
            );
            if note.dotted {
                out.push_str("<dot/>");
            }
            out.push_str("<stem>up</stem>");
            if tone == 0 && note.duration.flags() > 0 {
                for level in 1..=note.duration.flags() {
                    let previous = index.checked_sub(1).and_then(|i| group.notes.get(i));
                    let next = group.notes.get(index + 1);
                    let left = note.beam != BeamStyle::Break
                        && previous.is_some_and(|n| n.duration.flags() >= level);
                    let right = next
                        .is_some_and(|n| n.beam != BeamStyle::Break && n.duration.flags() >= level);
                    let beam = match (left, right) {
                        (false, true) => "begin",
                        (true, false) => "end",
                        (true, true) => "continue",
                        _ => "",
                    };
                    if !beam.is_empty() {
                        let _ = write!(out, "<beam number=\"{level}\">{beam}</beam>");
                    }
                }
            }
            let mut marks = String::new();
            let _ = write!(
                marks,
                "<other-notation type=\"single\" print-object=\"no\">encore-grace-side:{}</other-notation>",
                if placement == GracePlacement::After {
                    "after"
                } else {
                    "before"
                }
            );
            if tone == 0
                && let Ok(json) = serde_json::to_string(note)
            {
                let _ = write!(
                    marks,
                    "<other-notation type=\"single\" print-object=\"no\">encore-grace-note:{}</other-notation>",
                    xml_escape(&json)
                );
            }
            if index == 0
                && tone == 0
                && let Ok(json) = serde_json::to_string(group)
            {
                let _ = write!(
                    marks,
                    "<other-notation type=\"single\" print-object=\"no\">encore-grace:{}</other-notation>",
                    xml_escape(&json)
                );
            }
            if group.slur_to_main
                && tone == 0
                && let Some(number) = number
            {
                if placement == GracePlacement::Before && index == 0 {
                    let _ = write!(marks, "<slur type=\"start\" number=\"{number}\"/>");
                }
                if placement == GracePlacement::After && index + 1 == group.notes.len() {
                    let _ = write!(marks, "<slur type=\"stop\" number=\"{number}\"/>");
                }
            }
            if !marks.is_empty() {
                out.push_str("<notations>");
                out.push_str(&marks);
                out.push_str("</notations>");
            }
            out.push_str("</note>\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Score, Anchor) {
        let score: Score =
            serde_json::from_str(include_str!("../examples/Cantabile.encore")).unwrap();
        let note = &score.parts[0].measures[0].notes[0];
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: note.start,
            layer: note.layer,
            voice: note.voice,
        };
        (score, anchor)
    }
    #[test]
    fn grace_is_native_and_preserves_exact_written_rhythm() {
        let (mut score, anchor) = fixture();
        let parts = score.parts.clone();
        let mut g = GraceGroup::new(
            anchor,
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Sixteenth)],
        );
        g.slashed = true;
        assign(&mut score, g).unwrap();
        validate(&score).unwrap();
        assert_eq!(score.parts, parts);
        let reopened: Score =
            serde_json::from_str(&serde_json::to_string(&score).unwrap()).unwrap();
        assert_eq!(reopened, score);
    }
    #[test]
    fn invalid_replacement_is_atomic_and_orphans_are_pruned() {
        let (mut score, anchor) = fixture();
        let mut group = GraceGroup::new(
            anchor,
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Eighth)],
        );
        assign(&mut score, group.clone()).unwrap();
        let old = score.notation.clone();
        group.percent = f32::NAN;
        assert!(assign(&mut score, group).is_err());
        assert_eq!(old, score.notation);
        score.parts[0].measures[0].notes.clear();
        prune(&mut score);
        assert!(score.notation.grace.is_empty());
    }
    #[test]
    fn dotted_short_values_have_exact_playback_weights_and_bounds() {
        let (_, anchor) = fixture();
        let mut g = GraceGroup::new(
            anchor,
            vec![GraceNote::new(
                Pitch::from_midi(72),
                Duration::OneTwentyEighth,
            )],
        );
        g.notes[0].dotted = true;
        assert_eq!(g.notes[0].weight(), 45);
        g.validate().unwrap();
        g.notes[0].pitches.push(Pitch::from_midi(72));
        assert!(g.validate().is_err());
        g.notes[0].pitches.pop();
        g.placement = GracePlacement::After;
        assert!(g.validate().is_err());
        g.timing = GraceTiming::StealMain;
        g.validate().unwrap();
    }
}
