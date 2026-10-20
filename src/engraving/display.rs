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
        Self {
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
        }
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
