//! Repeat barlines and numbered volta brackets in score and linked-part views.
use super::{INK, Page, StaffRegion};
use crate::model::Score;

pub(super) fn draw(page: &mut Page, score: &Score) {
    if score.notation.repeats.is_empty() && score.notation.navigation.is_empty() {
        return;
    }
    let staves = page.staves.clone();
    let mut bracket_ceiling = std::collections::BTreeMap::<(usize, u32), f32>::new();
    for staff in &staves {
        for repeat in &score.notation.repeats {
            if staff.measure == repeat.start {
                barline(page, staff, true);
            }
            if repeat.backward_at(staff.measure) {
                barline(page, staff, false);
            }
        }
    }
    // A separate bracket on each visible staff remains legible in independently
    // paginated orchestral groups and extracted parts. Carry it across systems.
    for repeat in &score.notation.repeats {
        let nesting = score
            .notation
            .repeats
            .iter()
            .filter(|outer| {
                !std::ptr::eq(*outer, repeat)
                    && outer.start <= repeat.start
                    && outer.end >= repeat.end
            })
            .count();
        for ending in &repeat.endings {
            let mut segments: Vec<Vec<&StaffRegion>> = Vec::new();
            for staff in staves
                .iter()
                .filter(|staff| (ending.start..=ending.end).contains(&staff.measure))
            {
                if let Some(segment) = segments.iter_mut().find(|segment| {
                    segment[0].part == staff.part && (segment[0].y - staff.y).abs() < 0.1
                }) {
                    segment.push(staff);
                } else {
                    segments.push(vec![staff]);
                }
            }
            for mut segment in segments {
                segment.sort_by_key(|staff| staff.measure);
                let first = segment[0];
                let last = *segment.last().unwrap();
                let x1 = first.x + 2.0;
                let x2 = last.x + last.width - 2.0;
                let system = staves
                    .iter()
                    .filter(|staff| staff.part == first.part && (staff.y - first.y).abs() < 0.1)
                    .collect::<Vec<_>>();
                let baseline = super::slurs::above_envelope(score, &system)
                    .map_or(first.y - 28.0, |envelope| {
                        (first.y - 28.0).min(envelope - 22.0)
                    });
                let y = baseline - nesting as f32 * 17.0;
                bracket_ceiling
                    .entry((first.part, first.y.to_bits()))
                    .and_modify(|ceiling| *ceiling = ceiling.min(y))
                    .or_insert(y);
                page.line(x1, y, x2, y, 1.2, INK);
                if first.measure == ending.start {
                    page.line(x1, y, x1, y + 12.0, 1.2, INK);
                }
                if last.measure == ending.end && !ending.passes.contains(&repeat.times) {
                    page.line(x2, y, x2, y + 12.0, 1.2, INK);
                }
                let label = format!(
                    "{}{}",
                    crate::repeats::pass_text(&ending.passes),
                    if first.measure == ending.start {
                        "."
                    } else {
                        " (cont.)"
                    }
                );
                page.text(x1 + 6.0, y + 13.0, label, 10.0, INK, true);
            }
        }
    }
    for staff in &staves {
        for mark in score
            .notation
            .navigation
            .iter()
            .filter(|mark| mark.measure == staff.measure)
        {
            use crate::repeats::NavigationKind;
            let system = staves
                .iter()
                .filter(|region| region.part == staff.part && (region.y - staff.y).abs() < 0.1)
                .collect::<Vec<_>>();
            let mut y = super::slurs::above_envelope(score, &system)
                .map_or(staff.y - 48.0, |envelope| {
                    (staff.y - 48.0).min(envelope - 16.0)
                });
            if let Some(ceiling) = bracket_ceiling.get(&(staff.part, staff.y.to_bits())) {
                y = y.min(*ceiling - 20.0);
            }
            let x = if mark.kind.is_start() {
                staff.content_x + 2.0
            } else {
                (staff.x + staff.width - mark.kind.name().len() as f32 * 6.5 - 5.0)
                    .max(staff.content_x)
            };
            match mark.kind {
                NavigationKind::Coda => {
                    page.ellipse(x + 8.0, y, 7.0, 7.0, None, Some(INK), 1.3);
                    page.line(x - 2.0, y, x + 18.0, y, 1.3, INK);
                    page.line(x + 8.0, y - 10.0, x + 8.0, y + 10.0, 1.3, INK);
                    page.text(x + 23.0, y + 4.0, "Coda", 11.0, INK, true);
                }
                NavigationKind::Segno => {
                    // Original S-shaped polyline, diagonal stroke and two dots.
                    page.path(
                        vec![
                            (x + 14.0, y - 6.0),
                            (x + 10.0, y - 10.0),
                            (x + 5.0, y - 9.0),
                            (x + 3.0, y - 5.0),
                            (x + 5.0, y - 1.0),
                            (x + 11.0, y + 2.0),
                            (x + 13.0, y + 6.0),
                            (x + 10.0, y + 10.0),
                            (x + 5.0, y + 10.0),
                            (x + 1.0, y + 7.0),
                        ],
                        false,
                        None,
                        Some(INK),
                        2.1,
                    );
                    page.line(x, y + 10.0, x + 16.0, y - 11.0, 1.3, INK);
                    page.ellipse(x + 1.0, y - 3.0, 1.6, 1.6, Some(INK), None, 0.0);
                    page.ellipse(x + 15.0, y + 3.0, 1.6, 1.6, Some(INK), None, 0.0);
                }
                kind => page.text(x, y + 4.0, kind.name(), 12.0, INK, true),
            }
        }
    }
}

fn barline(page: &mut Page, staff: &StaffRegion, forward: bool) {
    let (thick, thin, dots) = if forward {
        let x = staff.x + 2.0;
        (x, x + 5.0, x + 11.0)
    } else {
        let x = staff.x + staff.width - 2.0;
        (x, x - 5.0, x - 11.0)
    };
    let top = staff.top_line_y()
        - if staff.lines == 1 {
            staff.spacing * 0.5
        } else {
            0.0
        };
    let bottom = staff.bottom_line_y()
        + if staff.lines == 1 {
            staff.spacing * 0.5
        } else {
            0.0
        };
    page.line(thick, top, thick, bottom, 3.2, INK);
    page.line(thin, top, thin, bottom, 1.1, INK);
    for offset in [1.5, 2.5] {
        page.ellipse(
            dots,
            staff.y + offset * staff.spacing,
            1.7,
            1.7,
            Some(INK),
            None,
            0.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        engraving::{EngravingOptions, Primitive, engrave},
        model::Score,
        repeats::{Ending, RepeatRegion},
    };
    #[test]
    fn engraved_repeat_marks_and_ending_labels_survive_part_filter() {
        let mut score = Score::blank();
        score.notation.repeats.push(RepeatRegion {
            start: 0,
            end: 3,
            times: 2,
            endings: vec![
                Ending {
                    start: 2,
                    end: 2,
                    passes: vec![1],
                },
                Ending {
                    start: 3,
                    end: 3,
                    passes: vec![2],
                },
            ],
        });
        let pages = engrave(
            &score,
            &EngravingOptions {
                part_filter: Some(1),
                ..Default::default()
            },
        );
        let text: Vec<_> = pages
            .iter()
            .flat_map(|page| &page.items)
            .filter_map(|item| match item {
                Primitive::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains(&"1."));
        assert!(text.contains(&"2."));
        assert!(pages.iter().flat_map(|page| &page.items).any(
            |item| matches!(item, Primitive::Ellipse { rx, ry, .. } if *rx == 1.7 && *ry == 1.7)
        ));
    }
    #[test]
    fn volta_bracket_and_navigation_clear_the_rendered_slur_envelope() {
        use crate::{
            document::Anchor,
            model::{Duration, Note, Pitch},
            repeats::{NavigationKind, NavigationMark},
            slurs::{Slur, SlurDirection},
        };
        let mut score = Score::blank();
        score.parts.truncate(1);
        for (measure, bar) in score.parts[0].measures.iter_mut().enumerate() {
            bar.notes = vec![Note {
                voice: 0,
                voice_launch: None,
                start: 0,
                duration: Duration::Quarter,
                dotted: false,
                tuplet: None,
                layer: 0,
                pitches: vec![Pitch::from_midi(if measure == 1 { 84 } else { 60 })],
                velocity: 80,
                staccato: false,
                articulations: vec![],
                tied: false,
            }];
        }
        let start = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let mut slur = Slur::new(
            start,
            Anchor {
                measure: 3,
                ..start
            },
        );
        slur.direction = SlurDirection::Above;
        score.notation.slurs.push(slur);
        score.notation.repeats.push(RepeatRegion {
            start: 0,
            end: 3,
            times: 2,
            endings: vec![
                Ending {
                    start: 2,
                    end: 2,
                    passes: vec![1],
                },
                Ending {
                    start: 3,
                    end: 3,
                    passes: vec![2],
                },
            ],
        });
        score.notation.navigation = vec![
            NavigationMark {
                measure: 1,
                kind: NavigationKind::Fine,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 3,
                kind: NavigationKind::DaCapoAlFine,
                play_repeats_after_jump: false,
            },
        ];
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        let staff = page.staves.iter().find(|staff| staff.measure == 2).unwrap();
        let system = page
            .staves
            .iter()
            .filter(|region| region.part == 0 && (region.y - staff.y).abs() < 0.1)
            .collect::<Vec<_>>();
        let envelope = super::super::slurs::above_envelope(&score, &system).unwrap();
        let bracket = page
            .items
            .iter()
            .find_map(|item| match item {
                Primitive::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    ..
                } if *width == 1.2
                    && *y1 == *y2
                    && (*x1 - (staff.x + 2.0)).abs() < 0.1
                    && (*x2 - (staff.x + staff.width - 2.0)).abs() < 0.1 =>
                {
                    Some(*y1)
                }
                _ => None,
            })
            .unwrap();
        assert!(bracket + 13.0 <= envelope - 8.0);
        let jump_y = page
            .items
            .iter()
            .find_map(|item| match item {
                Primitive::Text { y, text, .. } if text == "D.C. al Fine" => Some(*y),
                _ => None,
            })
            .unwrap();
        assert!(jump_y <= bracket - 16.0);
    }
}
