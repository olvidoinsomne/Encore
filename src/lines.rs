//! Portable performance lines. Native pitches always remain sounding pitches.
use crate::{
    document::Anchor,
    expressions::{Placement, tick},
    model::{Pitch, Score},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedalKind {
    #[default]
    Damper,
    Sostenuto,
}
impl PedalKind {
    pub const ALL: [Self; 2] = [Self::Damper, Self::Sostenuto];
    pub fn name(self) -> &'static str {
        match self {
            Self::Damper => "Damper",
            Self::Sostenuto => "Sostenuto",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    Ottava { semitones: i8 },
    Pedal { kind: PedalKind },
    Trill { interval: i8, rate: u16 },
    Glissando,
}
impl LineKind {
    pub const ALL: [Self; 4] = [
        Self::Ottava { semitones: 12 },
        Self::Pedal {
            kind: PedalKind::Damper,
        },
        Self::Trill {
            interval: 2,
            rate: 60,
        },
        Self::Glissando,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Ottava { .. } => "Octave line",
            Self::Pedal { .. } => "Pedal",
            Self::Trill { .. } => "Trill",
            Self::Glissando => "Chromatic glissando",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerformanceLine {
    pub start: Anchor,
    pub end: Anchor,
    pub kind: LineKind,
    #[serde(default)]
    pub placement: Placement,
    #[serde(default)]
    pub offset: f32,
}
impl PerformanceLine {
    pub fn new(start: Anchor, end: Anchor, kind: LineKind) -> Self {
        Self {
            start,
            end,
            kind,
            placement: if matches!(kind, LineKind::Pedal { .. }) {
                Placement::Below
            } else {
                Placement::Above
            },
            offset: 0.0,
        }
    }
}
fn position(score: &Score, at: Anchor, end: bool) -> bool {
    at.part < score.parts.len()
        && at.measure < score.measure_count()
        && at.layer < 4
        && at.tick <= score.measure_ticks()
        && (end || at.tick < score.measure_ticks())
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.lines.len() > 10_000 {
        return Err("Performance line count exceeds 10000.".into());
    }
    let mut lanes = std::collections::BTreeMap::<(usize, u8, u8), Vec<(u64, u64)>>::new();
    let mut entry_positions = std::collections::BTreeMap::<(usize, u8), Vec<u64>>::new();
    if score
        .notation
        .lines
        .iter()
        .any(|line| matches!(line.kind, LineKind::Glissando))
    {
        for (part, p) in score.parts.iter().enumerate() {
            for (measure, m) in p.measures.iter().enumerate() {
                for note in &m.notes {
                    entry_positions.entry((part, note.layer)).or_default().push(
                        measure as u64 * u64::from(score.measure_ticks()) + u64::from(note.start),
                    );
                }
            }
        }
        for lane in entry_positions.values_mut() {
            lane.sort_unstable();
        }
    }
    for line in &score.notation.lines {
        let a = tick(score, line.start.into());
        let b = tick(score, line.end.into());
        if !position(score, line.start, false)
            || !position(score, line.end, true)
            || line.start.part != line.end.part
            || line.start.layer != line.end.layer
            || a >= b
            || !line.offset.is_finite()
            || line.offset.abs() > 160.0
        {
            return Err(
                "A performance line needs later endpoints on one staff/layer and a bounded offset."
                    .into(),
            );
        }
        let family = match line.kind {
            LineKind::Ottava { semitones } => {
                if ![-36, -24, -12, 12, 24, 36].contains(&semitones) {
                    return Err("Octave lines must shift one, two or three octaves.".into());
                }
                0
            }
            LineKind::Pedal {
                kind: PedalKind::Damper,
            } => 1,
            LineKind::Pedal {
                kind: PedalKind::Sostenuto,
            } => 1,
            LineKind::Trill { interval, rate } => {
                if !(1..=12).contains(&interval) || !(30..=480).contains(&rate) {
                    return Err("Trills require a 1–12 semitone upper interval and a 30–480 tick alternation.".into());
                }
                3
            }
            LineKind::Glissando => {
                let first = crate::document::note_at(score, &line.start);
                let last = crate::document::note_at(score, &line.end);
                if first.is_none_or(|n| n.pitches.len() != 1)
                    || last.is_none_or(|n| n.pitches.len() != 1)
                {
                    return Err(
                        "A chromatic glissando needs single-pitch entries at both endpoints."
                            .into(),
                    );
                }
                let lane = entry_positions
                    .get(&(line.start.part, line.start.layer))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let left = lane.partition_point(|at| *at <= a);
                let right = lane.partition_point(|at| *at < b);
                if left != right {
                    return Err("A chromatic glissando currently requires consecutive entries in its layer.".into());
                }
                3
            }
        };
        // Pedals control all four layers of a staff, so one lane per pedal type.
        let layer = if matches!(line.kind, LineKind::Pedal { .. }) {
            0
        } else {
            line.start.layer
        };
        lanes
            .entry((line.start.part, layer, family))
            .or_default()
            .push((a, b));
    }
    for ranges in lanes.values_mut() {
        ranges.sort_unstable();
        if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err("Overlapping lines of the same performance family are ambiguous.".into());
        }
    }
    validate_sostenuto(score)?;
    Ok(())
}
/// OxiSynth lacks CC66 voice capture. Its explicit held gates remain exact
/// when a captured key is not reattacked in the same independent layer.
fn validate_sostenuto(score: &Score) -> Result<(), String> {
    if !score.notation.lines.iter().any(|line| {
        matches!(
            line.kind,
            LineKind::Pedal {
                kind: PedalKind::Sostenuto
            }
        )
    }) {
        return Ok(());
    }
    let mut lanes =
        std::collections::BTreeMap::<(usize, u8), Vec<(u64, &crate::model::Note)>>::new();
    for (part, p) in score.parts.iter().enumerate() {
        for (measure, m) in p.measures.iter().enumerate() {
            for n in &m.notes {
                lanes.entry((part, n.layer)).or_default().push((
                    measure as u64 * u64::from(score.measure_ticks()) + u64::from(n.start),
                    n,
                ));
            }
        }
    }
    for lane in lanes.values_mut() {
        lane.sort_by_key(|(at, _)| *at);
    }
    let styles = crate::entry_style::Index::new(score);
    for line in &score.notation.lines {
        if !matches!(
            line.kind,
            LineKind::Pedal {
                kind: PedalKind::Sostenuto
            }
        ) {
            continue;
        }
        let a = tick(score, line.start.into());
        let b = tick(score, line.end.into());
        for layer in 0..4 {
            let Some(lane) = lanes.get(&(line.start.part, layer)) else {
                continue;
            };
            let index = lane.partition_point(|(at, _)| *at <= a);
            let Some(i) = index.checked_sub(1) else {
                continue;
            };
            let (start, n) = lane[i];
            if n.velocity == 0 {
                continue;
            }
            let captured: std::collections::BTreeSet<_> = n
                .pitches
                .iter()
                .filter(|_| {
                    (start as f64 + f64::from(n.ticks()) * (1.0 - n.playback_shortening()))
                        > a as f64
                })
                .map(|p| p.midi())
                .collect();
            if captured.is_empty() {
                continue;
            }
            for j in index..lane.len() {
                let (at, next) = lane[j];
                if at >= b {
                    break;
                }
                for pitch in &next.pitches {
                    let midi = pitch.midi();
                    if !captured.contains(&midi) || next.velocity == 0 {
                        continue;
                    }
                    let tied_in = j > 0
                        && lane[j - 1].0 + u64::from(lane[j - 1].1.ticks()) == at
                        && lane[j - 1].1.pitches.iter().any(|p| p.midi() == midi)
                        && styles.ties_forward(
                            &Anchor {
                                part: line.start.part,
                                layer,
                                measure: (lane[j - 1].0 / u64::from(score.measure_ticks()))
                                    as usize,
                                tick: lane[j - 1].1.start,
                            },
                            lane[j - 1].1,
                            midi,
                        );
                    if !tied_in {
                        return Err("Sostenuto currently cannot reattack a captured pitch in the same layer before release; use another layer or damper pedal.".into());
                    }
                }
            }
        }
    }
    Ok(())
}
type OctaveLanes = std::collections::BTreeMap<(usize, u8), Vec<(u64, u64, i8)>>;
pub struct PitchIndex {
    width: u64,
    lanes: OctaveLanes,
}
impl PitchIndex {
    pub fn new(score: &Score) -> Self {
        let mut lanes = std::collections::BTreeMap::<_, Vec<_>>::new();
        for line in &score.notation.lines {
            if let LineKind::Ottava { semitones } = line.kind {
                lanes
                    .entry((line.start.part, line.start.layer))
                    .or_default()
                    .push((
                        tick(score, line.start.into()),
                        tick(score, line.end.into()),
                        semitones,
                    ));
            }
        }
        for lane in lanes.values_mut() {
            lane.sort_by_key(|(a, _, _)| *a);
        }
        Self {
            width: u64::from(score.measure_ticks()),
            lanes,
        }
    }
    pub fn written_pitch(&self, at: Anchor, mut pitch: Pitch) -> Pitch {
        if let Some(lane) = self.lanes.get(&(at.part, at.layer)) {
            let now = at.measure as u64 * self.width + u64::from(at.tick);
            let i = lane.partition_point(|(a, _, _)| *a <= now);
            if i > 0 && now < lane[i - 1].1 {
                pitch.octave -= lane[i - 1].2 / 12;
            }
        }
        pitch
    }
}
/// The line records sounding minus written semitones. MusicXML already stores
/// sounding pitch, so importing or adding an octave line never transposes sound.
pub fn written_pitch(score: &Score, at: Anchor, pitch: Pitch) -> Pitch {
    let now = tick(score, at.into());
    if let Some(PerformanceLine {
        kind: LineKind::Ottava { semitones },
        ..
    }) = score.notation.lines.iter().find(|line| {
        line.start.part == at.part
            && line.start.layer == at.layer
            && tick(score, line.start.into()) <= now
            && now < tick(score, line.end.into())
            && matches!(line.kind, LineKind::Ottava { .. })
    }) {
        let mut result = pitch;
        result.octave -= semitones / 12;
        result
    } else {
        pitch
    }
}
pub fn remove_part(score: &mut Score, part: usize) {
    score.notation.lines.retain(|line| line.start.part != part);
}
pub fn reindex_parts(score: &mut Score, removed: usize) {
    for line in &mut score.notation.lines {
        if line.start.part > removed {
            line.start.part -= 1;
            line.end.part -= 1;
        }
    }
}
pub fn move_anchor(score: &mut Score, from: Anchor, to: Anchor) {
    for line in &mut score.notation.lines {
        if line.start == from {
            line.start = to;
        }
        if line.end == from {
            line.end = to;
        }
    }
}
pub fn prune(score: &mut Score) {
    let parts = score.parts.len();
    let measures = score.measure_count();
    score
        .notation
        .lines
        .retain(|line| line.start.part < parts && line.end.measure < measures);
}
pub fn remove_anchor(score: &mut Score, anchor: &Anchor) {
    score.notation.lines.retain(|line| {
        !matches!(line.kind, LineKind::Glissando) || (&line.start != anchor && &line.end != anchor)
    });
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
/// Beat-attached direction pairs; glissando/trill note ornaments are emitted by
/// `musicxml_notations`, preserving their native endpoints through an extension.
fn append_direction(out: &mut String, line: &PerformanceLine, at: Anchor, start: bool) {
    use std::fmt::Write;
    let number = match line.kind {
        LineKind::Pedal {
            kind: PedalKind::Sostenuto,
        } => 2,
        LineKind::Pedal {
            kind: PedalKind::Damper,
        } => 1,
        _ => usize::from(line.start.layer) + 1,
    };
    let body = match line.kind {
        LineKind::Ottava { semitones } => format!(
            "<octave-shift type=\"{}\" size=\"{}\" number=\"{number}\"/>",
            if start {
                if semitones > 0 { "down" } else { "up" }
            } else {
                "stop"
            },
            1 + 7 * (semitones.unsigned_abs() / 12)
        ),
        LineKind::Pedal { kind } => format!(
            "<pedal type=\"{}\" number=\"{number}\" line=\"yes\" sign=\"yes\"/>",
            if start {
                if kind == PedalKind::Sostenuto {
                    "sostenuto"
                } else {
                    "start"
                }
            } else {
                "stop"
            }
        ),
        LineKind::Trill { .. } | LineKind::Glissando => String::new(),
    };
    let native = if start {
        format!(
            "<direction-type><other-direction print-object=\"no\">encore-line:{}</other-direction></direction-type>",
            xml_escape(&serde_json::to_string(line).unwrap_or_default())
        )
    } else {
        String::new()
    };
    if !body.is_empty() || start {
        let _ = writeln!(
            out,
            "      <direction placement=\"{}\">{}{}<offset>{}</offset><voice>{}</voice>{}</direction>",
            if line.placement == Placement::Above {
                "above"
            } else {
                "below"
            },
            if body.is_empty() {
                String::new()
            } else {
                format!("<direction-type>{body}</direction-type>")
            },
            native,
            at.tick,
            at.layer + 1,
            match line.kind {
                LineKind::Pedal { kind } => format!(
                    "<sound {}=\"{}\"/>",
                    if kind == PedalKind::Damper {
                        "damper-pedal"
                    } else {
                        "sostenuto-pedal"
                    },
                    if start { "yes" } else { "no" }
                ),
                _ => String::new(),
            }
        );
    }
}
fn append_notation(out: &mut String, line: &PerformanceLine, start: bool) {
    use std::fmt::Write;
    let number = match line.kind {
        LineKind::Pedal {
            kind: PedalKind::Sostenuto,
        } => 2,
        LineKind::Pedal {
            kind: PedalKind::Damper,
        } => 1,
        _ => usize::from(line.start.layer) + 1,
    };
    match line.kind {
        LineKind::Trill { .. } => {
            let _ = write!(
                out,
                "<ornaments>{}<wavy-line type=\"{}\" number=\"{number}\"/></ornaments>",
                if start { "<trill-mark/>" } else { "" },
                if start { "start" } else { "stop" }
            );
        }
        LineKind::Glissando => {
            let _ = write!(
                out,
                "<glissando type=\"{}\" number=\"{number}\" line-type=\"wavy\">{}</glissando>",
                if start { "start" } else { "stop" },
                if start { "gliss." } else { "" }
            );
        }
        _ => {}
    }
}
#[derive(Default)]
pub struct MusicXmlIndex {
    directions: std::collections::BTreeMap<(usize, usize), String>,
    notations: std::collections::BTreeMap<Anchor, String>,
}
impl MusicXmlIndex {
    pub fn new(score: &Score) -> Self {
        let mut index = Self::default();
        let mut endpoints = Vec::new();
        for line in &score.notation.lines {
            for (at, start) in [(line.start, true), (line.end, false)] {
                endpoints.push((at, !start, line));
            }
        }
        endpoints.sort_by_key(|(at, stop, _)| (*at, !*stop));
        for (at, stop, line) in endpoints {
            append_direction(
                index.directions.entry((at.part, at.measure)).or_default(),
                line,
                at,
                !stop,
            );
            append_notation(index.notations.entry(at).or_default(), line, !stop);
        }
        index
    }
    pub fn directions(&self, part: usize, measure: usize) -> &str {
        self.directions
            .get(&(part, measure))
            .map_or("", String::as_str)
    }
    pub fn notations(&self, at: &Anchor) -> &str {
        self.notations.get(at).map_or("", String::as_str)
    }
}
pub fn musicxml_directions(score: &Score, part: usize, measure: usize) -> String {
    MusicXmlIndex::new(score).directions(part, measure).into()
}
pub fn musicxml_notations(score: &Score, at: Anchor) -> String {
    MusicXmlIndex::new(score).notations(&at).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ottava_is_display_only_and_native_roundtrips() {
        let mut s = Score::blank();
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let b = Anchor { tick: 480, ..a };
        s.notation.lines.push(PerformanceLine::new(
            a,
            b,
            LineKind::Ottava { semitones: 12 },
        ));
        validate(&s).unwrap();
        let p = Pitch::from_midi(84);
        assert_eq!(written_pitch(&s, a, p).midi(), 72);
        assert_eq!(written_pitch(&s, b, p).midi(), 84);
        let reopened: Score = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(reopened.notation.lines, s.notation.lines);
        assert!(musicxml_directions(&s, 0, 0).contains("type=\"down\" size=\"8\""));
    }
    #[test]
    fn pedal_overlap_across_layers_is_rejected() {
        let mut s = Score::blank();
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let b = Anchor { tick: 960, ..a };
        s.notation.lines.push(PerformanceLine::new(
            a,
            b,
            LineKind::Pedal {
                kind: PedalKind::Damper,
            },
        ));
        s.notation.lines.push(PerformanceLine::new(
            Anchor {
                tick: 480,
                layer: 1,
                ..a
            },
            b,
            LineKind::Pedal {
                kind: PedalKind::Damper,
            },
        ));
        assert!(validate(&s).is_err());
    }
}
