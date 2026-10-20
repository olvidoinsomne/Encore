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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlurStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}
impl SlurStyle {
    pub const ALL: [Self; 3] = [Self::Solid, Self::Dashed, Self::Dotted];
    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
        }
    }
}
fn solid(style: &SlurStyle) -> bool {
    *style == SlurStyle::Solid
}
fn default_dash() -> f32 {
    6.0
}
fn default_gap() -> f32 {
    4.0
}
fn dash_default(value: &f32) -> bool {
    *value == default_dash()
}
fn gap_default(value: &f32) -> bool {
    *value == default_gap()
}
/// Control insets are fractions of the visible fragment width. Heights are
/// signed distances away from the selected side of the staff. A manual shape
/// freezes automatic collision lifting, while retaining attached endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlurShape {
    pub inset: [f32; 2],
    pub heights: [f32; 2],
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
    #[serde(default, skip_serializing_if = "solid")]
    pub style: SlurStyle,
    #[serde(default = "default_dash", skip_serializing_if = "dash_default")]
    pub dash_length: f32,
    #[serde(default = "default_gap", skip_serializing_if = "gap_default")]
    pub gap: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<SlurShape>,
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
            style: SlurStyle::Solid,
            dash_length: default_dash(),
            gap: default_gap(),
            shape: None,
        }
    }
    pub fn shape_or_default(&self) -> SlurShape {
        self.shape.unwrap_or(SlurShape {
            inset: [1.0 / 3.0; 2],
            heights: [self.height; 2],
        })
    }
    pub fn reset_curve(&mut self) {
        self.height = default_height();
        self.start_offset = [0.0; 2];
        self.end_offset = [0.0; 2];
        self.shape = None;
    }
}
fn check_geometry(slur: &Slur) -> Result<(), String> {
    if slur.start.part != slur.end.part {
        return Err("Cross-staff slurs require grouped staves and are not supported yet.".into());
    }
    if position(&slur.start) >= position(&slur.end) {
        return Err("A slur must join a later pitched entry on the same staff.".into());
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
    if [slur.dash_length, slur.gap]
        .iter()
        .any(|v| !v.is_finite() || !(2.0..=24.0).contains(v))
    {
        return Err("Slur dash and gap lengths must be 2–24 points.".into());
    }
    if let Some(shape) = slur.shape {
        if shape
            .inset
            .iter()
            .any(|v| !v.is_finite() || !(0.05..=0.9).contains(v))
            || shape.inset.iter().sum::<f32>() > 1.0
        {
            return Err("Slur control insets must be 5–90% and total at most 100%.".into());
        }
        if shape
            .heights
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 240.0)
        {
            return Err("Slur control heights must be within 240 points.".into());
        }
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
    for slur in &score.notation.slurs {
        check_geometry(slur)?;
        if !anchors.contains(&slur.start) || !anchors.contains(&slur.end) {
            return Err("Slur endpoints must attach to existing pitched notes.".into());
        }
        if !endpoints.insert((slur.start, slur.end)) {
            return Err("Duplicate slur endpoints.".into());
        }
    }
    MusicXmlNumbering::new(score)?;
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
    score
        .notation
        .slurs
        .retain(|s| s.start.part != part && s.end.part != part);
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
            && s.start.part == s.end.part
            && s.start.lane() == s.end.lane()
            && s.start.lane() == anchor.lane()
            && position(&s.start) <= position(anchor)
            && position(anchor) < position(&s.end)
    })
}

type XmlPosition = (usize, u8, u32);
fn xml_position(anchor: Anchor) -> XmlPosition {
    (anchor.measure, anchor.lane(), anchor.tick)
}
/// MusicXML level numbers are scoped by document order, including reverse
/// start/stop order when independent voices are serialized in separate runs.
/// Shared with grace connectors so the two kinds of phrase marks never collide.
#[derive(Default)]
pub struct MusicXmlNumbering {
    numbers: Vec<u8>,
    intervals: std::collections::BTreeMap<usize, [Vec<(XmlPosition, XmlPosition)>; 16]>,
}
impl MusicXmlNumbering {
    pub fn new(score: &Score) -> Result<Self, String> {
        if score.notation.slurs.len() > 20_000 {
            return Err("Score exceeds the 20,000 slur limit.".into());
        }
        if score
            .notation
            .slurs
            .iter()
            .any(|s| s.start.part != s.end.part)
        {
            return Err(
                "Cross-staff slurs cannot be exported across separate MusicXML parts.".into(),
            );
        }
        let mut result = Self {
            numbers: vec![0; score.notation.slurs.len()],
            ..Self::default()
        };
        let mut sorted = score
            .notation
            .slurs
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let a = xml_position(s.start);
                let b = xml_position(s.end);
                (s.start.part, a.min(b), a.max(b), i)
            })
            .collect::<Vec<_>>();
        sorted.sort();
        let mut part = usize::MAX;
        let mut active = Vec::<(XmlPosition, u8)>::new();
        for (p, start, end, i) in sorted {
            if p != part {
                active.clear();
                part = p;
            }
            active.retain(|(last, _)| *last >= start);
            let number = (1..=16).find(|n| !active.iter().any(|(_,used)| used == n)).ok_or("At most 16 overlapping slur levels per staff can be interchanged in MusicXML document order.")?;
            active.push((end, number));
            result.numbers[i] = number;
            result.intervals.entry(p).or_default()[number as usize - 1].push((start, end));
        }
        Ok(result)
    }
    pub fn number(&self, index: usize) -> u8 {
        self.numbers.get(index).copied().unwrap_or(1)
    }
    pub fn connector(&self, anchor: &Anchor) -> Option<usize> {
        let at = xml_position(*anchor);
        let Some(levels) = self.intervals.get(&anchor.part) else {
            return Some(16);
        };
        (0..16)
            .rev()
            .find(|n| {
                let intervals = &levels[*n];
                let index = intervals.partition_point(|(start, _)| *start <= at);
                index == 0 || intervals[index - 1].1 < at
            })
            .map(|n| n + 1)
    }
}

pub const XML_METADATA_PREFIX: &str = "encore-slur-v1:";
#[derive(Serialize, Deserialize)]
pub(crate) struct XmlGeometry {
    pub number: u8,
    pub height: f32,
    pub shape: Option<SlurShape>,
}
/// Standard line styling and endpoint offsets; Encore's optional normalized
/// control geometry is stored alongside the standard mark for exact round trips.
pub fn musicxml_start(slur: &Slur, number: u8) -> String {
    let placement = match slur.direction {
        SlurDirection::Auto => "",
        SlurDirection::Above => " placement=\"above\"",
        SlurDirection::Below => " placement=\"below\"",
    };
    let style = match slur.style {
        SlurStyle::Solid => "solid",
        SlurStyle::Dashed => "dashed",
        SlurStyle::Dotted => "dotted",
    };
    let pattern = if slur.style == SlurStyle::Solid
        && dash_default(&slur.dash_length)
        && gap_default(&slur.gap)
    {
        String::new()
    } else {
        format!(
            " dash-length=\"{}\" space-length=\"{}\"",
            slur.dash_length, slur.gap
        )
    };
    let sign = if slur.direction == SlurDirection::Below {
        -1.0
    } else {
        1.0
    };
    let height = slur.shape.map_or(slur.height, |s| s.heights[0]) * sign;
    let mut result = format!(
        "<slur type=\"start\" number=\"{number}\"{placement} line-type=\"{style}\"{pattern} relative-x=\"{}\" relative-y=\"{}\" bezier-y=\"{height}\"/>",
        slur.start_offset[0], -slur.start_offset[1]
    );
    if slur.shape.is_some() {
        let json = serde_json::to_string(&XmlGeometry {
            number,
            height: slur.height,
            shape: slur.shape,
        })
        .expect("finite validated slur geometry");
        result.push_str(&format!(
            "<other-notation type=\"single\">{XML_METADATA_PREFIX}{json}</other-notation>"
        ));
    }
    result
}
pub fn musicxml_stop(slur: &Slur, number: u8) -> String {
    let sign = if slur.direction == SlurDirection::Below {
        -1.0
    } else {
        1.0
    };
    let control = slur.shape.map_or(String::new(), |s| {
        format!(" bezier-y=\"{}\"", s.heights[1] * sign)
    });
    format!(
        "<slur type=\"stop\" number=\"{number}\" relative-x=\"{}\" relative-y=\"{}\"{control}/>",
        slur.end_offset[0], -slur.end_offset[1]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(tick: u32, voice: u8) -> Anchor {
        Anchor {
            part: 0,
            measure: 0,
            tick,
            layer: 0,
            voice,
        }
    }
    fn voices() -> Score {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        let mut note = Score::demo().parts[0].measures[0].notes[0].clone();
        note.duration = crate::model::Duration::Quarter;
        note.tuplet = None;
        note.dotted = false;
        score.parts[0].measures[0].notes = (0..2)
            .flat_map(|voice| {
                (0..3).map({
                    let note = note.clone();
                    move |i| {
                        let mut n = note.clone();
                        n.start = i * 480;
                        n.voice = voice;
                        n.voice_launch = None;
                        n
                    }
                })
            })
            .collect();
        score.parts[0].measures[0]
            .notes
            .sort_by_key(|n| (n.start, n.layer, n.voice));
        score
    }
    #[test]
    fn cross_voice_curves_are_native_and_do_not_legato_an_unrelated_lane() {
        let mut score = voices();
        let mut slur = Slur::new(a(0, 1), a(960, 0));
        slur.style = SlurStyle::Dashed;
        slur.dash_length = 9.0;
        slur.gap = 5.0;
        slur.shape = Some(SlurShape {
            inset: [0.2, 0.4],
            heights: [44.0, -12.0],
        });
        add(&mut score, slur).unwrap();
        score.validate().unwrap();
        let decoded: Score = serde_json::from_str(&serde_json::to_string(&score).unwrap()).unwrap();
        assert_eq!(decoded, score);
        assert!(!continues(&score, &a(480, 1)));
        assert!(!continues(&score, &a(480, 0)));
        let s = &mut score.notation.slurs[0];
        s.reset_curve();
        assert!(s.shape.is_none());
        assert_eq!(s.style, SlurStyle::Dashed);
        assert_eq!(s.start_offset, [0.0; 2]);
    }
    #[test]
    fn invalid_control_geometry_and_cross_staff_are_atomic() {
        let mut score = voices();
        let old = score.clone();
        let mut s = Slur::new(a(0, 0), a(480, 1));
        s.shape = Some(SlurShape {
            inset: [0.8, 0.6],
            heights: [20.0; 2],
        });
        assert!(add(&mut score, s.clone()).is_err());
        assert_eq!(score, old);
        s.shape = Some(SlurShape {
            inset: [0.2; 2],
            heights: [f32::NAN, 20.0],
        });
        assert!(add(&mut score, s.clone()).is_err());
        s.shape = None;
        s.end.part = 1;
        assert!(add(&mut score, s).unwrap_err().contains("Cross-staff"));
        assert_eq!(score, old);
    }
    #[test]
    fn numbering_respects_reverse_document_order_and_grace_connectors() {
        let mut score = voices();
        score.notation.slurs = vec![
            Slur::new(a(0, 1), a(480, 0)),
            Slur::new(a(480, 1), a(960, 0)),
        ];
        let n = MusicXmlNumbering::new(&score).unwrap();
        assert_ne!(n.number(0), n.number(1));
        let connector = n.connector(&a(480, 0)).unwrap() as u8;
        assert_ne!(connector, n.number(0));
        assert_ne!(connector, n.number(1));
        score.notation.slurs = vec![
            Slur::new(a(0, 0), a(480, 0)),
            Slur::new(a(240, 1), a(720, 1)),
        ];
        let n = MusicXmlNumbering::new(&score).unwrap();
        assert_eq!(n.number(0), n.number(1));
    }
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
