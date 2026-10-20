//! Original vector ornaments, indexed once per immutable engraving snapshot.
use super::*;
use crate::{
    document::Anchor,
    ornaments::{Ornament, OrnamentKind},
    slurs::SlurDirection,
};
struct Mark<'a> {
    mark: &'a Ornament,
    top: f32,
    bottom: f32,
    y: f32,
    left: f32,
    aux: Option<(Pitch, Pitch)>,
}
pub(super) struct Index<'a> {
    marks: BTreeMap<(usize, usize), Vec<Mark<'a>>>,
}
impl<'a> Index<'a> {
    pub(super) fn new(score: &'a Score) -> Self {
        let mut result = Self {
            marks: BTreeMap::new(),
        };
        if score.notation.ornaments.is_empty() {
            return result;
        }
        let pitches = display::Index::new(score);
        let styles = crate::entry_style::Index::new(score);
        let mut entries = BTreeMap::new();
        for (part, staff) in score.parts.iter().enumerate() {
            for (measure, bar) in staff.measures.iter().enumerate() {
                let paired = paired_layers(&bar.notes);
                let multiple = bar
                    .notes
                    .first()
                    .is_some_and(|first| bar.notes.iter().any(|n| n.lane() != first.lane()));
                let dynamic =
                    staff_dynamic_baseline(&bar.notes, &pitches, part, measure, 0.0, multiple);
                for note in &bar.notes {
                    entries.insert(
                        Anchor {
                            part,
                            measure,
                            tick: note.start,
                            layer: note.layer,
                            voice: note.voice,
                        },
                        (note, paired, multiple, dynamic),
                    );
                }
            }
        }
        for mark in &score.notation.ornaments {
            let Some(&(note, paired, multiple, dynamic)) = entries.get(&mark.anchor) else {
                continue;
            };
            let style = styles.get(&mark.anchor);
            if note.pitches.is_empty()
                || style.is_some_and(|s| !s.visible)
                || pitches.unpitched(mark.anchor.part)
            {
                continue;
            }
            let ys = note
                .pitches
                .iter()
                .map(|p| pitches.y(mark.anchor, *p, 0.0))
                .collect::<Vec<_>>();
            let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
            let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let automatic = default_stem_up(
                paired,
                note,
                ys.iter().sum::<f32>() / ys.len() as f32,
                multiple,
            );
            let up = style.map_or(automatic, |s| entry_style::stem_up(s, automatic));
            let above = match mark.placement {
                SlurDirection::Above => true,
                SlurDirection::Below => false,
                SlurDirection::Auto => {
                    note.voice == 0
                        && (paired & (1 << note.layer) != 0 || note.layer.is_multiple_of(2))
                }
            };
            let articulation_room = if above != up {
                (note.articulations.len() as f32 + if note.staccato { 1.0 } else { 0.0 }) * 11.0
            } else {
                0.0
            };
            let y = if above {
                (top - 22.0 - articulation_room).min(-22.0)
            } else {
                (bottom + 22.0 + articulation_room).max(dynamic + 21.0)
            };
            let accidental = !pitches.unpitched(mark.anchor.part)
                && (note
                    .pitches
                    .iter()
                    .any(|p| p.alter != key_alter(p.step, score.key_fifths))
                    || style.is_some_and(|s| !s.accidentals.is_empty()));
            // Leave the chord's accidental column clear before drawing a roll.
            let parenthesized = !pitches.unpitched(mark.anchor.part)
                && style.is_some_and(|s| {
                    s.accidentals.iter().any(|a| {
                        matches!(
                            a.display,
                            crate::entry_style::AccidentalDisplay::Courtesy
                                | crate::entry_style::AccidentalDisplay::Parenthesized
                        )
                    })
                });
            let seconds = note.pitches.iter().any(|a| {
                note.pitches.iter().any(|b| {
                    a != b && {
                        let distance = (pitches.position(mark.anchor, *a)
                            - pitches.position(mark.anchor, *b))
                        .abs();
                        distance == 1 || (pitches.unpitched(mark.anchor.part) && distance == 0)
                    }
                })
            });
            let left = if parenthesized {
                44.0
            } else if accidental {
                31.0
            } else if !up && seconds {
                27.0
            } else {
                17.0
            };
            let aux = if mark.kind.chord() || pitches.unpitched(mark.anchor.part) {
                None
            } else {
                crate::ornaments::auxiliary_pitches_for_note(score, mark, note).ok()
            };
            result
                .marks
                .entry((mark.anchor.part, mark.anchor.measure))
                .or_default()
                .push(Mark {
                    mark,
                    top,
                    bottom,
                    y,
                    left,
                    aux,
                });
        }
        result
    }
    pub(super) fn roll_prefix(&self, anchor: &Anchor) -> f32 {
        self.marks
            .get(&(anchor.part, anchor.measure))
            .into_iter()
            .flatten()
            .find(|m| m.mark.anchor == *anchor && m.mark.kind.chord())
            .map_or(0.0, |m| (m.left + 7.0 - m.mark.offset[0]).max(0.0))
    }
    pub(super) fn anchor_widths(&self) -> BTreeMap<Anchor, (f32, f32)> {
        self.marks
            .values()
            .flatten()
            .map(|m| {
                let x = m.mark.offset[0];
                (
                    m.mark.anchor,
                    (
                        if m.mark.kind.chord() {
                            m.left + 7.0 - x
                        } else {
                            13.0 - x
                        },
                        13.0 + x,
                    ),
                )
            })
            .collect()
    }
    pub(super) fn widths(&self, parts: &[usize], measure: usize) -> BTreeMap<u32, (f32, f32)> {
        let mut widths = BTreeMap::<u32, (f32, f32)>::new();
        for part in parts {
            if let Some(marks) = self.marks.get(&(*part, measure)) {
                for m in marks {
                    let x = m.mark.offset[0];
                    let w = widths.entry(m.mark.anchor.tick).or_default();
                    w.0 = w.0.max(if m.mark.kind.chord() {
                        m.left + 7.0 - x
                    } else {
                        13.0 - x
                    });
                    w.1 = w.1.max(13.0 + x);
                }
            }
        }
        widths
    }
    pub(super) fn clearance(
        &self,
        _score: &Score,
        part: usize,
        measure: usize,
        bars: usize,
    ) -> (f32, f32) {
        let (mut above, mut below) = (0.0f32, 0.0f32);
        for bar in measure..measure + bars {
            if let Some(marks) = self.marks.get(&(part, bar)) {
                for m in marks {
                    let extra = if m.mark.upper.is_some() || m.mark.lower.is_some() {
                        22.0
                    } else {
                        9.0
                    };
                    let top = if m.mark.kind.chord() {
                        m.top - 9.0
                    } else {
                        m.y - extra
                    } + m.mark.offset[1];
                    let bottom = if m.mark.kind.chord() {
                        m.bottom + 9.0
                    } else {
                        m.y + extra
                    } + m.mark.offset[1];
                    above = above.max(-top);
                    below = below.max(bottom);
                }
            }
        }
        (above, below)
    }
    pub(super) fn draw(&self, page: &mut Page, _score: &Score) {
        for region in page.staves.clone() {
            if let Some(marks) = self.marks.get(&(region.part, region.measure)) {
                for m in marks {
                    let mark = m.mark;
                    let x = region.x_at_anchor(&mark.anchor) + mark.offset[0];
                    let y = region.y + m.y + mark.offset[1];
                    if mark.kind.chord() {
                        draw_roll(
                            page,
                            mark.kind,
                            x - m.left,
                            region.y + m.top + mark.offset[1] - 4.0,
                            region.y + m.bottom + mark.offset[1] + 4.0,
                        );
                    } else {
                        draw_sign(page, mark.kind, x, y);
                        if let Some((upper, lower)) = m.aux {
                            if mark.upper.is_some() {
                                small_accidental(page, x, y - 14.0, upper.alter);
                            }
                            if mark.lower.is_some() {
                                small_accidental(page, x, y + 14.0, lower.alter);
                            }
                        }
                    }
                }
            }
        }
    }
}
fn draw_sign(page: &mut Page, kind: OrnamentKind, x: f32, y: f32) {
    match kind {
        OrnamentKind::Turn | OrnamentKind::InvertedTurn => {
            let flip = if kind == OrnamentKind::InvertedTurn {
                -1.0
            } else {
                1.0
            };
            let p = |dx: f32, dy: f32| (x + dx, y + dy * flip);
            let mut points = vec![p(-9.0, 2.5)];
            cubic(
                &mut points,
                p(-9.0, 2.5),
                p(-17.0, -6.0),
                p(-3.0, -9.0),
                p(0.0, 0.0),
                20,
            );
            cubic(
                &mut points,
                p(0.0, 0.0),
                p(3.0, 9.0),
                p(17.0, 6.0),
                p(9.0, -2.5),
                20,
            );
            page.path(points, false, None, Some(INK), 1.8);
        }
        OrnamentKind::Mordent | OrnamentKind::UpperMordent => {
            page.path(
                vec![
                    (x - 10.0, y + 3.0),
                    (x - 6.0, y - 3.0),
                    (x - 2.0, y + 3.0),
                    (x + 2.0, y - 3.0),
                    (x + 6.0, y + 3.0),
                    (x + 10.0, y - 3.0),
                ],
                false,
                None,
                Some(INK),
                1.8,
            );
            if kind == OrnamentKind::Mordent {
                page.line(x, y - 7.0, x, y + 7.0, 1.3, INK);
            }
        }
        _ => {}
    }
}
fn draw_roll(page: &mut Page, kind: OrnamentKind, x: f32, top: f32, bottom: f32) {
    if kind == OrnamentKind::NonArpeggiate {
        page.path(
            vec![(x + 5.0, top), (x, top), (x, bottom), (x + 5.0, bottom)],
            false,
            None,
            Some(INK),
            1.2,
        );
        return;
    }
    let height = (bottom - top).max(8.0);
    let cycles = (height / 7.0).ceil();
    let points = (0..=(cycles as usize * 16))
        .map(|i| {
            let t = i as f32 / (cycles * 16.0);
            (
                x + 2.1 * (t * cycles * std::f32::consts::TAU).sin(),
                top + t * height,
            )
        })
        .collect();
    page.path(points, false, None, Some(INK), 1.4);
    let (y, d) = if kind == OrnamentKind::ArpeggioDown {
        (bottom + 4.0, -1.0)
    } else {
        (top - 4.0, 1.0)
    };
    page.path(
        vec![(x - 3.5, y + 5.0 * d), (x, y), (x + 3.5, y + 5.0 * d)],
        false,
        None,
        Some(INK),
        1.4,
    );
}
fn small_accidental(page: &mut Page, x: f32, y: f32, alter: i8) {
    let begin = page.items.len();
    accidental(page, 0.0, 0.0, alter);
    for item in &mut page.items[begin..] {
        match item {
            Primitive::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                ..
            } => {
                *x1 = *x1 * 0.52 + x;
                *x2 = *x2 * 0.52 + x;
                *y1 = *y1 * 0.52 + y;
                *y2 = *y2 * 0.52 + y;
                *width *= 0.52;
            }
            Primitive::Path { points, width, .. } => {
                for p in points {
                    p.0 = p.0 * 0.52 + x;
                    p.1 = p.1 * 0.52 + y;
                }
                *width *= 0.52;
            }
            Primitive::Ellipse {
                cx,
                cy,
                rx,
                ry,
                width,
                ..
            } => {
                *cx = *cx * 0.52 + x;
                *cy = *cy * 0.52 + y;
                *rx *= 0.52;
                *ry *= 0.52;
                *width *= 0.52;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabling_ornament_playback_keeps_printed_mark_and_hidden_principal_hides_it() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes.push(Note {
            layer: 0,
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(64)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: false,
        });
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        let mut mark = Ornament::new(anchor, OrnamentKind::Mordent);
        mark.placement = SlurDirection::Above;
        mark.upper = Some(1);
        mark.lower = Some(-2);
        score.notation.ornaments.push(mark);
        score.validate().unwrap();
        let rendered = engrave(&score, &Default::default());
        score.notation.ornaments[0].playback = false;
        let display_only = engrave(&score, &Default::default());
        assert_eq!(svg(&rendered[0]), svg(&display_only[0]));
        let index = Index::new(&score);
        let (above, _) = index.clearance(&score, 0, 0, 1);
        assert!(above >= 44.0);
        let mut style = crate::entry_style::EntryStyle::new(anchor);
        style.visible = false;
        score.notation.entries.push(style);
        let hidden = Index::new(&score);
        assert!(hidden.marks.is_empty());
    }
}
