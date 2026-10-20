//! One visual identity resolver for pitched entries and unpitched sound tokens.
use super::*;
use crate::{document::Anchor, entry_style::Notehead};
#[derive(Clone, Copy)]
struct Sound {
    position: i32,
    head: Notehead,
}
struct Staff {
    clef: Clef,
    percussion: bool,
    lines: u8,
    sounds: [Option<Sound>; 128],
}
pub(super) struct Index {
    pitched: crate::lines::PitchIndex,
    staves: Vec<Staff>,
    tremolo_bounds: BTreeMap<Anchor, (f32, f32)>,
    tremolo_bottom: BTreeMap<(usize, usize), f32>,
}
impl Index {
    pub(super) fn columns(&self, at: Anchor, pitches: &[Pitch]) -> Vec<f32> {
        columns(
            &pitches
                .iter()
                .map(|p| self.position(at, *p))
                .collect::<Vec<_>>(),
        )
    }
    pub(super) fn new(score: &Score) -> Self {
        let mut result = Self {
            pitched: crate::lines::PitchIndex::new(score),
            staves: score
                .parts
                .iter()
                .map(|part| {
                    let mut staff = Staff {
                        clef: part.clef,
                        percussion: part.percussion.is_some(),
                        lines: part.percussion.as_ref().map_or(5, |p| p.lines),
                        sounds: [None; 128],
                    };
                    if let Some(map) = &part.percussion {
                        for sound in &map.sounds {
                            if let Some(slot) = staff.sounds.get_mut(sound.id as usize) {
                                *slot = Some(Sound {
                                    position: i32::from(sound.position),
                                    head: sound.notehead,
                                });
                            }
                        }
                    }
                    staff
                })
                .collect(),
            tremolo_bounds: BTreeMap::new(),
            tremolo_bottom: BTreeMap::new(),
        };
        if !score.notation.tremolos.is_empty() {
            let principals = crate::ornaments::principal_index(score);
            let styles = crate::entry_style::Index::new(score);
            let marks = super::tremolos::Index::new(score, &result);
            let mut lanes = BTreeMap::new();
            for mark in &score.notation.tremolos {
                let Some(first) = principals.get(&mark.anchor) else {
                    continue;
                };
                let Some(bar) = score
                    .parts
                    .get(mark.anchor.part)
                    .and_then(|p| p.measures.get(mark.anchor.measure))
                else {
                    continue;
                };
                let (paired, multiple) = *lanes
                    .entry((mark.anchor.part, mark.anchor.measure))
                    .or_insert_with(|| {
                        (
                            paired_layers(&bar.notes),
                            bar.notes
                                .first()
                                .is_some_and(|f| bar.notes.iter().any(|n| n.lane() != f.lane())),
                        )
                    });
                let first_mean = marks.pair_mean(mark.anchor, 0.0).unwrap_or_else(|| {
                    first
                        .pitches
                        .iter()
                        .map(|p| result.y(mark.anchor, *p, 0.0))
                        .sum::<f32>()
                        / first.pitches.len().max(1) as f32
                });
                let first_automatic = default_stem_up(paired, first, first_mean, multiple);
                let pair_up = styles.get(&mark.anchor).map_or(first_automatic, |s| {
                    entry_style::stem_up(s, first_automatic)
                });
                for at in std::iter::once(mark.anchor).chain(mark.end) {
                    let Some(note) = principals.get(&at) else {
                        continue;
                    };
                    let style = styles.get(&at);
                    if style.is_some_and(|s| !s.visible) || note.pitches.is_empty() {
                        continue;
                    }
                    let scale = style.map_or(1.0, |s| s.scale);
                    let mut ys = note
                        .pitches
                        .iter()
                        .map(|p| result.y(at, *p, 0.0))
                        .collect::<Vec<_>>();
                    if let Some(end) = mark.end {
                        for other in [mark.anchor, end] {
                            if let Some(n) = principals.get(&other) {
                                ys.extend(n.pitches.iter().map(|p| result.y(other, *p, 0.0)));
                            }
                        }
                    }
                    let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
                    let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let mean = marks
                        .pair_mean(at, 0.0)
                        .unwrap_or_else(|| ys.iter().sum::<f32>() / ys.len() as f32);
                    let automatic = default_stem_up(paired, note, mean, multiple);
                    let up = if mark.end.is_some() {
                        pair_up
                    } else {
                        style.map_or(automatic, |s| entry_style::stem_up(s, automatic))
                    };
                    let depth = (19.0 + f32::from(mark.strokes.saturating_sub(1)) * 7.0) * scale;
                    let bounds = if up {
                        (top - depth + mark.offset[1], bottom)
                    } else {
                        (top, bottom + depth + mark.offset[1])
                    };
                    result.tremolo_bounds.insert(at, bounds);
                    result
                        .tremolo_bottom
                        .entry((at.part, at.measure))
                        .and_modify(|v| *v = v.max(bounds.1))
                        .or_insert(bounds.1);
                }
            }
        }
        result
    }
    pub(super) fn tremolo_bounds(&self, at: Anchor) -> Option<(f32, f32)> {
        self.tremolo_bounds.get(&at).copied()
    }
    pub(super) fn tremolo_bottom(&self, part: usize, measure: usize) -> Option<f32> {
        self.tremolo_bottom.get(&(part, measure)).copied()
    }
    pub(super) fn unpitched(&self, part: usize) -> bool {
        self.staves.get(part).is_some_and(|s| s.percussion)
    }
    pub(super) fn lines(&self, part: usize) -> u8 {
        self.staves
            .get(part)
            .map_or(5, |s| if s.lines == 1 { 1 } else { 5 })
    }
    pub(super) fn bottom(&self, part: usize, frame: f32) -> f32 {
        frame + if self.lines(part) == 1 { 20.0 } else { 40.0 }
    }
    pub(super) fn top(&self, part: usize, frame: f32) -> f32 {
        frame + if self.lines(part) == 1 { 20.0 } else { 0.0 }
    }
    pub(super) fn written_pitch(&self, at: Anchor, pitch: Pitch) -> Pitch {
        if self.unpitched(at.part) {
            pitch
        } else {
            self.pitched.written_pitch(at, pitch)
        }
    }
    pub(super) fn position(&self, at: Anchor, pitch: Pitch) -> i32 {
        let Some(staff) = self.staves.get(at.part) else {
            return 0;
        };
        if staff.percussion {
            staff.sounds[pitch.midi() as usize].map_or(0, |s| s.position)
        } else {
            self.pitched.written_pitch(at, pitch).staff_step() - bottom_step(staff.clef)
        }
    }
    pub(super) fn y(&self, at: Anchor, pitch: Pitch, frame: f32) -> f32 {
        // Keep the pitched arithmetic identical to legacy engraving.
        if !self.unpitched(at.part)
            && let Some(staff) = self.staves.get(at.part)
        {
            return pitch_y(&self.pitched.written_pitch(at, pitch), staff.clef, frame);
        }
        self.bottom(at.part, frame) - self.position(at, pitch) as f32 * 5.0
    }
    pub(super) fn head(&self, at: Anchor, pitch: Pitch, manual: Notehead) -> Notehead {
        if manual != Notehead::Normal {
            return manual;
        }
        self.staves
            .get(at.part)
            .filter(|s| s.percussion)
            .and_then(|s| s.sounds[pitch.midi() as usize])
            .map_or(manual, |s| s.head)
    }
}
/// Separate equal-position sounds as well as adjacent heads. Each sound keeps
/// an independent visual column even when several map to the same line/head.
pub(super) fn columns(positions: &[i32]) -> Vec<f32> {
    let mut order = (0..positions.len()).collect::<Vec<_>>();
    order.sort_by_key(|i| positions[*i]);
    let mut result = vec![0.0; positions.len()];
    let mut prior: Option<(i32, f32)> = None;
    for i in order {
        let p = positions[i];
        let column = prior.map_or(0.0, |(previous, column)| {
            if p == previous {
                column + 1.0
            } else if p - previous == 1 && column == 0.0 {
                1.0
            } else {
                0.0
            }
        });
        result[i] = column;
        prior = Some((p, column));
    }
    result
}
pub(super) fn percussion_span(part: &crate::model::Part, pitches: &[Pitch]) -> f32 {
    part.percussion.as_ref().map_or(0.0, |staff| {
        let positions = pitches
            .iter()
            .map(|p| staff.sound(p.midi()).map_or(0, |s| i32::from(s.position)))
            .collect::<Vec<_>>();
        columns(&positions).into_iter().fold(0.0, f32::max) * 10.0
    })
}
