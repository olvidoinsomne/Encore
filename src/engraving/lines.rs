//! Vector octave, pedal, trill and chromatic glissando fragments.
use super::*;
use crate::{
    document::Anchor,
    expressions::Placement,
    lines::{LineKind, PedalKind, PitchIndex},
};
pub(super) fn clearance(score: &Score, part: usize, measure: usize, bars: usize) -> (f32, f32) {
    let mut above = 0.0_f32;
    let mut below = 0.0_f32;
    let above_ottava = score.notation.lines.iter().any(|line| {
        line.start.part == part
            && matches!(line.kind, LineKind::Ottava { .. })
            && line.placement == Placement::Above
            && line.start.measure < measure + bars
            && line.end.measure >= measure
    });
    let below_ottava = score.notation.lines.iter().any(|line| {
        line.start.part == part
            && matches!(line.kind, LineKind::Ottava { .. })
            && line.placement == Placement::Below
            && line.start.measure < measure + bars
            && line.end.measure >= measure
    });
    for line in score.notation.lines.iter().filter(|line| {
        line.start.part == part
            && line.start.measure < measure + bars
            && line.end.measure >= measure
    }) {
        if matches!(line.kind, LineKind::Glissando) {
            continue;
        }
        let stack = if matches!(line.kind, LineKind::Trill { .. })
            && if line.placement == Placement::Above {
                above_ottava
            } else {
                below_ottava
            } {
            20.0
        } else {
            0.0
        };
        let amount = 65.0 + line.offset.abs() + stack;
        match line.placement {
            Placement::Above => {
                above = above.max(
                    amount
                        .max(chords::clearance(score, part, measure, bars) + 50.0)
                        .max(slurs::clearance(score, part, measure, bars).0 + 45.0),
                )
            }
            Placement::Below => {
                below = below.max(amount + 25.0 + lyrics::extra_below(score, part, measure, bars))
            }
        }
    }
    (above, below)
}
fn dashed(page: &mut Page, a: f32, b: f32, y: f32) {
    let mut x = a;
    while x < b {
        page.line(x, y, (x + 7.0).min(b), y, 1.0, INK);
        x += 12.0;
    }
}
fn wave(page: &mut Page, a: (f32, f32), b: (f32, f32)) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let length = dx.hypot(dy);
    let steps = (length / 2.0).ceil().max(2.0) as usize;
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let v = (t * length / 7.0 * std::f32::consts::TAU).sin() * 1.6;
        points.push((
            a.0 + dx * t - v * dy / length.max(1.0),
            a.1 + dy * t + v * dx / length.max(1.0),
        ));
    }
    page.path(points, false, None, Some(INK), 1.0);
}
fn endpoint(score: &Score, index: &PitchIndex, at: Anchor, region: &StaffRegion) -> f32 {
    crate::document::note_at(score, &at)
        .and_then(|n| n.pitches.first())
        .map_or(region.y + 20.0, |pitch| {
            pitch_y(
                &index.written_pitch(at, *pitch),
                score.parts[at.part].clef,
                region.y,
            )
        })
}
pub(super) fn draw(page: &mut Page, score: &Score) {
    let staves = page.staves.clone();
    let index = PitchIndex::new(score);
    let mut systems = BTreeMap::<(usize, u32), Vec<&StaffRegion>>::new();
    for r in &staves {
        systems.entry((r.part, r.y.to_bits())).or_default().push(r);
    }
    for ((part, _), mut regions) in systems {
        regions.sort_by_key(|r| r.measure);
        let first = regions[0];
        let last = *regions.last().unwrap();
        let bars = last.measure - first.measure + 1;
        let above_ottava = score.notation.lines.iter().any(|line| {
            line.start.part == part
                && matches!(line.kind, LineKind::Ottava { .. })
                && line.placement == Placement::Above
                && line.start.measure <= last.measure
                && line.end.measure >= first.measure
        });
        let below_ottava = score.notation.lines.iter().any(|line| {
            line.start.part == part
                && matches!(line.kind, LineKind::Ottava { .. })
                && line.placement == Placement::Below
                && line.start.measure <= last.measure
                && line.end.measure >= first.measure
        });
        for line in score.notation.lines.iter().filter(|line| {
            line.start.part == part
                && line.start.measure <= last.measure
                && line.end.measure >= first.measure
        }) {
            let start_region = regions.iter().find(|r| r.measure == line.start.measure);
            let end_region = regions.iter().find(|r| r.measure == line.end.measure);
            let x = start_region.map_or(first.content_x, |r| r.x_at_tick(line.start.tick as f32));
            let end = end_region.map_or(last.x + last.width - 8.0, |r| {
                r.x_at_tick(line.end.tick as f32)
            });
            if end <= x + 2.0 {
                continue;
            }
            if matches!(line.kind, LineKind::Glissando) {
                let a =
                    start_region.map_or(first.y + 20.0, |r| endpoint(score, &index, line.start, r));
                let b = end_region.map_or(last.y + 20.0, |r| endpoint(score, &index, line.end, r));
                wave(page, (x + 9.0, a), (end - 9.0, b));
                if start_region.is_some() {
                    page.text(x + 14.0, a - 9.0, "gliss.", 10.0, INK, true);
                }
                continue;
            }
            let mut y = match line.placement {
                Placement::Above => {
                    let above = slurs::above_envelope(score, &regions).unwrap_or(first.y - 16.0);
                    (first.y - 45.0)
                        .min(above - 27.0)
                        .min(first.y - chords::clearance(score, part, first.measure, bars) - 22.0)
                }
                Placement::Below => {
                    first.y + 115.0 + lyrics::extra_below(score, part, first.measure, bars)
                }
            } + line.offset;
            // Keep independent categories apart when a line shares the staff side.
            if matches!(line.kind, LineKind::Trill { .. })
                && if line.placement == Placement::Above {
                    above_ottava
                } else {
                    below_ottava
                }
            {
                y += if line.placement == Placement::Above {
                    -19.0
                } else {
                    19.0
                };
            }
            match line.kind {
                LineKind::Ottava { semitones } => {
                    let label = match semitones {
                        12 => "8va",
                        -12 => "8vb",
                        24 => "15ma",
                        -24 => "15mb",
                        36 => "22ma",
                        _ => "22mb",
                    };
                    let left = if start_region.is_some() {
                        page.text(x, y, label, 12.0, INK, true);
                        x + text_width(label, 12.0, true) + 8.0
                    } else {
                        x
                    };
                    dashed(page, left, end, y - 4.0);
                    if end_region.is_some() {
                        page.line(
                            end,
                            y - 4.0,
                            end,
                            y + if line.placement == Placement::Above {
                                3.0
                            } else {
                                -11.0
                            },
                            1.0,
                            INK,
                        );
                    }
                }
                LineKind::Pedal { kind } => {
                    let label = if kind == PedalKind::Damper {
                        "Ped."
                    } else {
                        "Sost."
                    };
                    let left = if start_region.is_some() {
                        page.text(x, y, label, 11.0, INK, true);
                        x + text_width(label, 11.0, true) + 7.0
                    } else {
                        x
                    };
                    page.line(left, y - 3.0, end, y - 3.0, 1.0, INK);
                    if start_region.is_some() {
                        page.line(left, y - 3.0, left, y - 9.0, 1.0, INK);
                    }
                    if end_region.is_some() {
                        page.line(end, y - 3.0, end, y - 9.0, 1.0, INK);
                    }
                }
                LineKind::Trill { .. } => {
                    let left = if start_region.is_some() {
                        page.text(x, y, "tr", 14.0, INK, true);
                        x + 19.0
                    } else {
                        x
                    };
                    wave(page, (left, y - 4.0), (end, y - 4.0));
                }
                LineKind::Glissando => {}
            }
        }
    }
}
