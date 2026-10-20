//! Anchored expressive marks and one shared performed tempo map.
use crate::{
    document::Anchor,
    model::{Note, PPQ, Score},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    Above,
    #[default]
    Below,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpressionKind {
    #[default]
    Dynamic,
    Text,
    Technique,
    Rehearsal,
}
impl ExpressionKind {
    pub const ALL: [Self; 4] = [Self::Dynamic, Self::Text, Self::Technique, Self::Rehearsal];
    pub fn name(self) -> &'static str {
        match self {
            Self::Dynamic => "Dynamic",
            Self::Text => "Expressive text",
            Self::Technique => "Technique",
            Self::Rehearsal => "Rehearsal",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub anchor: Anchor,
    pub kind: ExpressionKind,
    pub text: String,
    #[serde(default)]
    pub velocity: Option<u8>,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub offset: [f32; 2],
}
impl Expression {
    pub fn dynamic(anchor: Anchor, text: impl Into<String>, velocity: u8) -> Self {
        Self {
            anchor,
            kind: ExpressionKind::Dynamic,
            text: text.into(),
            velocity: Some(velocity),
            placement: Placement::Below,
            system: false,
            offset: [0.0; 2],
        }
    }
    pub fn text(anchor: Anchor, text: impl Into<String>) -> Self {
        Self {
            anchor,
            kind: ExpressionKind::Text,
            text: text.into(),
            velocity: None,
            placement: Placement::Above,
            system: false,
            offset: [0.0; 2],
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HairpinKind {
    #[default]
    Crescendo,
    Diminuendo,
}
impl HairpinKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Crescendo => "Crescendo",
            Self::Diminuendo => "Diminuendo",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hairpin {
    pub start: Anchor,
    pub end: Anchor,
    pub kind: HairpinKind,
    #[serde(default)]
    pub start_velocity: Option<u8>,
    #[serde(default)]
    pub end_velocity: Option<u8>,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub offset: f32,
}
impl Hairpin {
    pub fn new(start: Anchor, end: Anchor) -> Self {
        Self {
            start,
            end,
            kind: HairpinKind::Crescendo,
            start_velocity: None,
            end_velocity: None,
            placement: Placement::Below,
            offset: 0.0,
        }
    }
}
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct TimePoint {
    pub measure: usize,
    pub tick: u32,
}
impl From<Anchor> for TimePoint {
    fn from(anchor: Anchor) -> Self {
        Self {
            measure: anchor.measure,
            tick: anchor.tick,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoRampEnd {
    pub end: TimePoint,
    pub bpm: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoChange {
    pub at: TimePoint,
    pub bpm: u16,
    #[serde(default)]
    pub ramp: Option<TempoRampEnd>,
    #[serde(default)]
    pub text: String,
}
impl TempoChange {
    pub fn new(at: TimePoint, bpm: u16) -> Self {
        Self {
            at,
            bpm,
            ramp: None,
            text: String::new(),
        }
    }
}
pub fn tick(score: &Score, at: TimePoint) -> u64 {
    at.measure as u64 * u64::from(score.measure_ticks()) + u64::from(at.tick)
}
fn point(score: &Score, at: TimePoint, end: bool) -> bool {
    at.measure < score.measure_count()
        && at.tick <= score.measure_ticks()
        && (end || at.tick < score.measure_ticks())
}
fn anchor(score: &Score, at: Anchor, end: bool) -> bool {
    at.part < score.parts.len() && at.layer < 4 && at.voice < 2 && point(score, at.into(), end)
}
fn text_ok(text: &str) -> bool {
    text.chars().count() <= 256 && !text.chars().any(char::is_control)
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.expressions.len() > 20_000
        || score.notation.spanners.len() > 10_000
        || score.notation.tempo.len() > 4096
    {
        return Err("Expression, hairpin or tempo count exceeds the document limit.".into());
    }
    let mut dynamics = BTreeSet::new();
    for mark in &score.notation.expressions {
        if !anchor(score, mark.anchor, false)
            || mark.text.trim().is_empty()
            || !text_ok(&mark.text)
            || mark.velocity.is_some_and(|velocity| velocity > 127)
            || mark
                .offset
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 160.0)
        {
            return Err("Invalid expression position, text, velocity or offset.".into());
        }
        if mark.kind == ExpressionKind::Dynamic
            && (!dynamics.insert((
                mark.anchor.part,
                mark.anchor.measure,
                mark.anchor.tick,
                mark.anchor.layer,
            )) || mark.system)
        {
            return Err("A staff/layer position supports one staff dynamic.".into());
        }
    }
    for (index, hairpin) in score.notation.spanners.iter().enumerate() {
        if !anchor(score, hairpin.start, false)
            || !anchor(score, hairpin.end, true)
            || hairpin.start.part != hairpin.end.part
            || hairpin.start.layer != hairpin.end.layer
            || tick(score, hairpin.start.into()) >= tick(score, hairpin.end.into())
            || !hairpin.offset.is_finite()
            || hairpin.offset.abs() > 160.0
            || [hairpin.start_velocity, hairpin.end_velocity]
                .iter()
                .flatten()
                .any(|v| *v > 127)
        {
            return Err("A hairpin needs later endpoints on the same staff/layer and bounded playback values.".into());
        }
        if score.notation.spanners[..index].iter().any(|prior| {
            prior.start.part == hairpin.start.part
                && prior.start.layer == hairpin.start.layer
                && tick(score, prior.start.into()) < tick(score, hairpin.end.into())
                && tick(score, hairpin.start.into()) < tick(score, prior.end.into())
        }) {
            return Err("Overlapping hairpins on one staff/layer are ambiguous.".into());
        }
    }
    let mut tempo: Vec<_> = score.notation.tempo.iter().collect();
    tempo.sort_by_key(|change| change.at);
    for (index, change) in tempo.iter().enumerate() {
        if !point(score, change.at, false)
            || !(20..=400).contains(&change.bpm)
            || !text_ok(&change.text)
        {
            return Err(
                "Tempo marks need a valid rhythmic position, 20–400 BPM and bounded text.".into(),
            );
        }
        if index > 0 && tempo[index - 1].at == change.at {
            return Err("Conflicting tempo marks at one position.".into());
        }
        if let Some(ramp) = &change.ramp
            && (!point(score, ramp.end, true)
                || ramp.end <= change.at
                || !(20..=400).contains(&ramp.bpm)
                || tempo.get(index + 1).is_some_and(|next| next.at < ramp.end))
        {
            return Err(
                "Tempo ramps need a later endpoint and cannot overlap another tempo change.".into(),
            );
        }
    }
    TempoMap::new(score).map(|_| ())
}
#[derive(Default)]
struct Lane {
    dynamics: Vec<(u64, u8)>,
    printed_dynamics: Vec<(u64, Option<u8>)>,
    hairpins: Vec<Hairpin>,
}
/// Sorted staff/layer indexes keep playback lookup logarithmic in score size.
pub struct ExpressionMap {
    width: u64,
    lanes: std::collections::BTreeMap<(usize, u8), Lane>,
}
impl ExpressionMap {
    pub fn new(score: &Score) -> Self {
        let width = u64::from(score.measure_ticks());
        let mut lanes = std::collections::BTreeMap::<(usize, u8), Lane>::new();
        for mark in &score.notation.expressions {
            let parts: Vec<_> = if mark.system {
                (0..score.parts.len()).collect()
            } else {
                vec![mark.anchor.part]
            };
            for part in parts {
                let lane = lanes.entry((part, mark.anchor.layer)).or_default();
                if let Some(velocity) = mark.velocity {
                    lane.dynamics
                        .push((tick(score, mark.anchor.into()), velocity));
                }
                if mark.kind == ExpressionKind::Dynamic {
                    let label = match mark.text.as_str() {
                        "pppp" | "ppp" => Some(28),
                        "pp" => Some(36),
                        "p" => Some(48),
                        "mp" => Some(64),
                        "mf" => Some(80),
                        "f" => Some(101),
                        "ff" => Some(115),
                        "fff" | "ffff" => Some(125),
                        _ => None,
                    };
                    lane.printed_dynamics
                        .push((tick(score, mark.anchor.into()), mark.velocity.or(label)));
                }
            }
        }
        for line in &score.notation.spanners {
            lanes
                .entry((line.start.part, line.start.layer))
                .or_default()
                .hairpins
                .push(line.clone());
        }
        for lane in lanes.values_mut() {
            lane.dynamics.sort_by_key(|(tick, _)| *tick);
            lane.printed_dynamics.sort_by_key(|(tick, _)| *tick);
            lane.hairpins
                .sort_by_key(|line| (line.start.measure, line.start.tick));
        }
        Self { width, lanes }
    }
    pub fn velocity(&self, at: Anchor, note: &Note) -> u8 {
        if note.velocity == 0 {
            return 0;
        }
        let Some(lane) = self.lanes.get(&(at.part, at.layer)) else {
            return note.playback_velocity();
        };
        let now = at.measure as u64 * self.width + u64::from(at.tick);
        let position = |anchor: Anchor| anchor.measure as u64 * self.width + u64::from(anchor.tick);
        let dynamic = |tick: u64, fallback: u8| {
            let index = lane.dynamics.partition_point(|(at, _)| *at <= tick);
            if index == 0 {
                fallback
            } else {
                lane.dynamics[index - 1].1
            }
        };
        let mut value = dynamic(now, note.velocity);
        let index = lane
            .hairpins
            .partition_point(|line| position(line.start) <= now);
        if index > 0 {
            let line = &lane.hairpins[index - 1];
            let start = position(line.start);
            let end = position(line.end);
            let later_dynamic = lane.printed_dynamics.partition_point(|(at, _)| *at <= now);
            let superseded = now >= end
                && later_dynamic > 0
                && lane.printed_dynamics[later_dynamic - 1].0 >= end;
            if !superseded {
                let initial = line
                    .start_velocity
                    .unwrap_or_else(|| dynamic(start, note.velocity));
                let target = line.end_velocity.unwrap_or_else(|| {
                    let index = lane.printed_dynamics.partition_point(|(at, _)| *at < end);
                    lane.printed_dynamics
                        .get(index)
                        .filter(|(at, _)| *at == end)
                        .and_then(|(_, velocity)| *velocity)
                        .unwrap_or_else(|| match line.kind {
                            HairpinKind::Crescendo => initial.saturating_add(24).min(127),
                            HairpinKind::Diminuendo => initial.saturating_sub(24),
                        })
                });
                let fraction = (now.saturating_sub(start) as f64 / (end - start).max(1) as f64)
                    .clamp(0.0, 1.0);
                value = (f64::from(initial) + (f64::from(target) - f64::from(initial)) * fraction)
                    .round()
                    .clamp(0.0, 127.0) as u8;
            }
        }
        let boost = note.playback_velocity().saturating_sub(note.velocity);
        value.saturating_add(boost).min(127)
    }
}
/// Convenience for editing/audition; full-score schedulers reuse ExpressionMap.
pub fn velocity(score: &Score, at: Anchor, note: &Note) -> u8 {
    ExpressionMap::new(score).velocity(at, note)
}
pub fn remove_part(score: &mut Score, part: usize) {
    score
        .notation
        .expressions
        .retain(|mark| mark.anchor.part != part);
    score
        .notation
        .spanners
        .retain(|line| line.start.part != part);
}
pub fn reindex_parts(score: &mut Score, part: usize) {
    for mark in &mut score.notation.expressions {
        if mark.anchor.part > part {
            mark.anchor.part -= 1;
        }
    }
    for line in &mut score.notation.spanners {
        if line.start.part > part {
            line.start.part -= 1;
            line.end.part -= 1;
        }
    }
}
pub fn move_anchor(score: &mut Score, from: Anchor, to: Anchor) {
    for mark in &mut score.notation.expressions {
        if mark.anchor == from {
            mark.anchor = to;
        }
    }
    for line in &mut score.notation.spanners {
        if line.start == from {
            line.start = to;
        }
        if line.end == from {
            line.end = to;
        }
    }
}
pub fn remove_anchor(_score: &mut Score, _at: &Anchor) {} // Beat-attached expressions survive note deletion.
pub fn prune(score: &mut Score) {
    let count = score.measure_count();
    let parts = score.parts.len();
    score
        .notation
        .expressions
        .retain(|mark| mark.anchor.part < parts && mark.anchor.measure < count);
    score
        .notation
        .spanners
        .retain(|line| line.start.part < parts && line.end.measure < count);
    score.notation.tempo.retain(|change| {
        change.at.measure < count
            && change
                .ramp
                .as_ref()
                .is_none_or(|ramp| ramp.end.measure < count)
    });
}

#[derive(Clone, Debug)]
struct Segment {
    start: f64,
    end: f64,
    bpm: f64,
    end_bpm: f64,
    seconds: f64,
    end_seconds: f64,
}
impl Segment {
    fn elapsed(&self, ticks: f64) -> f64 {
        let width = self.end - self.start;
        let ticks = ticks.clamp(0.0, width);
        let slope = (self.end_bpm - self.bpm) / width;
        if slope.abs() < 1e-12 {
            ticks * 60.0 / (f64::from(PPQ) * self.bpm)
        } else {
            60.0 / (f64::from(PPQ) * slope) * ((self.bpm + slope * ticks) / self.bpm).ln()
        }
    }
    fn ticks(&self, seconds: f64) -> f64 {
        let slope = (self.end_bpm - self.bpm) / (self.end - self.start);
        if slope.abs() < 1e-12 {
            seconds * f64::from(PPQ) * self.bpm / 60.0
        } else {
            self.bpm * ((seconds * f64::from(PPQ) * slope / 60.0).exp() - 1.0) / slope
        }
    }
}
#[derive(Clone, Debug)]
pub struct TempoMap {
    segments: Vec<Segment>,
    ticks: f64,
    seconds: f64,
}
impl TempoMap {
    pub fn new(score: &Score) -> Result<Self, String> {
        let order = crate::repeats::playback_order(score)?;
        let width = u64::from(score.measure_ticks());
        let mut changes: Vec<_> = score
            .notation
            .tempo
            .iter()
            .map(|change| (tick(score, change.at), change))
            .collect();
        changes.sort_by_key(|(at, _)| *at);
        let mut boundaries = BTreeSet::from([0, width * score.measure_count() as u64]);
        for (at, change) in &changes {
            boundaries.insert(*at);
            if let Some(ramp) = &change.ramp {
                boundaries.insert(tick(score, ramp.end));
            }
        }
        let bpm = |position: u64, inclusive: bool| {
            let index = changes.partition_point(|(at, _)| {
                if inclusive {
                    *at <= position
                } else {
                    *at < position
                }
            });
            if index == 0 {
                return f64::from(score.tempo.max(1));
            }
            let (start, change) = changes[index - 1];
            if let Some(ramp) = &change.ramp {
                let end = tick(score, ramp.end);
                let fraction = position.saturating_sub(start) as f64 / (end - start).max(1) as f64;
                f64::from(change.bpm)
                    + (f64::from(ramp.bpm) - f64::from(change.bpm)) * fraction.clamp(0.0, 1.0)
            } else {
                f64::from(change.bpm)
            }
        };
        let mut segments = Vec::new();
        let mut seconds = 0.0;
        for (performed, &source) in order.iter().enumerate() {
            let left = source as u64 * width;
            let right = left + width;
            let mut points = vec![left];
            points.extend(
                boundaries
                    .range((
                        std::ops::Bound::Excluded(left),
                        std::ops::Bound::Excluded(right),
                    ))
                    .copied(),
            );
            points.push(right);
            for pair in points.windows(2) {
                if segments.len() >= 200_000 {
                    return Err("Tempo/repeat expansion exceeds 200,000 timing segments.".into());
                }
                let start = performed as f64 * width as f64 + (pair[0] - left) as f64;
                let end = start + (pair[1] - pair[0]) as f64;
                let mut segment = Segment {
                    start,
                    end,
                    bpm: bpm(pair[0], true),
                    end_bpm: bpm(pair[1], false),
                    seconds,
                    end_seconds: 0.0,
                };
                seconds += segment.elapsed(end - start);
                segment.end_seconds = seconds;
                segments.push(segment);
            }
        }
        Ok(Self {
            segments,
            ticks: order.len() as f64 * width as f64,
            seconds,
        })
    }
    pub fn duration_seconds(&self) -> f64 {
        self.seconds
    }
    pub fn seconds_at_tick(&self, tick: f64) -> f64 {
        let tick = if tick.is_finite() {
            tick.clamp(0.0, self.ticks)
        } else {
            0.0
        };
        let index = self.segments.partition_point(|segment| segment.end <= tick);
        self.segments.get(index).map_or(self.seconds, |segment| {
            segment.seconds + segment.elapsed(tick - segment.start)
        })
    }
    pub fn tick_at_seconds(&self, seconds: f64) -> f64 {
        let seconds = if seconds.is_finite() {
            seconds.clamp(0.0, self.seconds)
        } else {
            0.0
        };
        let index = self
            .segments
            .partition_point(|segment| segment.end_seconds <= seconds);
        self.segments.get(index).map_or(self.ticks, |segment| {
            segment.start + segment.ticks(seconds - segment.seconds)
        })
    }
    pub fn midi_tempos(&self) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let step = (f64::from(PPQ) / 8.0).max((self.ticks / 100_000.0).ceil());
        for segment in &self.segments {
            let mut at = segment.start;
            while at < segment.end {
                let next = (at + step).min(segment.end);
                let midpoint = (at + next) * 0.5;
                let fraction = (midpoint - segment.start) / (segment.end - segment.start);
                let bpm = segment.bpm + (segment.end_bpm - segment.bpm) * fraction;
                let micros = (60_000_000.0 / bpm).round() as u32;
                if out.last().is_none_or(|(_, prior)| *prior != micros) {
                    out.push((at as u32, micros));
                }
                at = next;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tempo_steps_and_linear_ramps_use_invertible_integrals() {
        let mut score = Score::blank();
        score.tempo = 120;
        score.notation.tempo = vec![
            TempoChange {
                at: TimePoint {
                    measure: 0,
                    tick: 0,
                },
                bpm: 60,
                ramp: Some(TempoRampEnd {
                    end: TimePoint {
                        measure: 1,
                        tick: 0,
                    },
                    bpm: 120,
                }),
                text: "accel.".into(),
            },
            TempoChange::new(
                TimePoint {
                    measure: 2,
                    tick: 0,
                },
                90,
            ),
        ];
        let map = TempoMap::new(&score).unwrap();
        assert!((map.seconds_at_tick(1920.0) - 4.0 * 2.0_f64.ln()).abs() < 1e-9);
        for tick in [0.0, 500.0, 1920.0, 3000.0, 5000.0, 7680.0] {
            assert!((map.tick_at_seconds(map.seconds_at_tick(tick)) - tick).abs() < 1e-8);
        }
    }
    #[test]
    fn explicit_dynamics_and_hairpins_change_velocity_without_editing_notes() {
        let mut score = Score::demo();
        let at = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        score
            .notation
            .expressions
            .push(Expression::dynamic(at, "p", 48));
        score.notation.spanners.push(Hairpin {
            end: Anchor { tick: 1440, ..at },
            end_velocity: Some(96),
            ..Hairpin::new(at, Anchor { tick: 1440, ..at })
        });
        let note = &score.parts[0].measures[0].notes[0];
        assert_eq!(velocity(&score, at, note), 48);
        assert_eq!(velocity(&score, Anchor { tick: 720, ..at }, note), 72);
        assert_eq!(velocity(&score, Anchor { tick: 1440, ..at }, note), 96);
    }
    #[test]
    fn repeat_jumps_reset_tempo_from_written_positions() {
        let mut score = Score::blank();
        score.tempo = 120;
        score
            .parts
            .iter_mut()
            .for_each(|part| part.measures.truncate(2));
        score
            .notation
            .repeats
            .push(crate::repeats::RepeatRegion::simple(0, 1));
        score.notation.tempo.push(TempoChange::new(
            TimePoint {
                measure: 1,
                tick: 0,
            },
            60,
        ));
        let map = TempoMap::new(&score).unwrap();
        assert_eq!(map.duration_seconds(), 12.0);
        assert_eq!(map.seconds_at_tick(3840.0), 6.0);
    }
    #[test]
    fn ramp_midi_sampling_tracks_analytic_duration_with_submillisecond_error() {
        let mut score = Score::blank();
        score.notation.tempo.push(TempoChange {
            at: TimePoint {
                measure: 0,
                tick: 0,
            },
            bpm: 120,
            ramp: Some(TempoRampEnd {
                end: TimePoint {
                    measure: 1,
                    tick: 0,
                },
                bpm: 60,
            }),
            text: String::new(),
        });
        let map = TempoMap::new(&score).unwrap();
        let mut samples = map.midi_tempos();
        samples.push(((score.measure_count() as u32) * score.measure_ticks(), 0));
        let seconds: f64 = samples
            .windows(2)
            .map(|pair| {
                f64::from(pair[1].0 - pair[0].0) / f64::from(PPQ) * f64::from(pair[0].1)
                    / 1_000_000.0
            })
            .sum();
        assert!((seconds - map.duration_seconds()).abs() < 0.001);
    }
}
