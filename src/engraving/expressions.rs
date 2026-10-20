//! Original vector expression text, rehearsal boxes and hairpin fragments.
use super::*;
use crate::expressions::{ExpressionKind, HairpinKind, Placement, TimePoint};
pub(super) fn clearance(
    score: &Score,
    part: usize,
    measure: usize,
    bars: usize,
    top_staff: bool,
) -> (f32, f32) {
    let mut above = 0.0_f32;
    let mut below = 0.0_f32;
    for mark in score.notation.expressions.iter().filter(|mark| {
        ((mark.system && top_staff) || (!mark.system && mark.anchor.part == part))
            && (measure..measure + bars).contains(&mark.anchor.measure)
    }) {
        match mark.placement {
            Placement::Above => above = above.max(20.0 + mark.offset[1].abs()),
            Placement::Below => below = below.max(22.0 + mark.offset[1].abs()),
        }
    }
    for line in score.notation.spanners.iter().filter(|line| {
        line.start.part == part
            && line.start.measure < measure + bars
            && line.end.measure >= measure
    }) {
        match line.placement {
            Placement::Above => above = above.max(54.0 + line.offset.abs()),
            Placement::Below => below = below.max(30.0 + line.offset.abs()),
        }
    }
    if top_staff
        && score.notation.tempo.iter().any(|mark| {
            mark.at.measure < measure + bars
                && mark
                    .ramp
                    .as_ref()
                    .map_or(mark.at.measure, |ramp| ramp.end.measure)
                    >= measure
        })
    {
        above = above.max(45.0);
    }
    (above, below)
}
fn base(score: &Score, region: &StaffRegion, placement: Placement) -> f32 {
    match placement {
        Placement::Above => region.y - 34.0,
        Placement::Below => score.parts[region.part]
            .measures
            .get(region.measure)
            .map_or(region.y + 65.0, |measure| {
                staff_dynamic_baseline(
                    &measure.notes,
                    score.parts[region.part].clef,
                    region.y,
                    measure.notes.iter().any(|note| note.layer != 0),
                )
            }),
    }
}
pub(super) fn draw(page: &mut Page, score: &Score) {
    let staves = page.staves.clone();
    for mark in &score.notation.expressions {
        for region in staves.iter().filter(|region| {
            region.measure == mark.anchor.measure
                && (region.part == mark.anchor.part || mark.system)
        }) {
            if mark.system
                && staves
                    .iter()
                    .any(|other| other.measure == region.measure && other.y < region.y)
            {
                continue;
            }
            let x = region.x_at_tick(mark.anchor.tick as f32) + mark.offset[0];
            let mut y = base(score, region, mark.placement) + mark.offset[1];
            if mark.placement == Placement::Above {
                let system = staves
                    .iter()
                    .filter(|staff| staff.part == region.part && (staff.y - region.y).abs() < 0.1)
                    .collect::<Vec<_>>();
                if let Some(envelope) = slurs::above_envelope(score, &system) {
                    y = y.min(envelope - 16.0 + mark.offset[1]);
                }
            }
            if mark.kind == ExpressionKind::Dynamic
                && [
                    "pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "sfz", "fp",
                ]
                .contains(&mark.text.as_str())
            {
                dynamic(page, x - 4.0, y, &mark.text);
            } else {
                let size = if mark.kind == ExpressionKind::Rehearsal {
                    14.0
                } else {
                    12.0
                };
                page.text(x, y, &mark.text, size, INK, true);
                if mark.kind == ExpressionKind::Rehearsal {
                    let width = text_width(&mark.text, size, true) + 12.0;
                    for (a, b, c, d) in [
                        (x - 5.0, y - 16.0, x + width - 5.0, y - 16.0),
                        (x - 5.0, y + 5.0, x + width - 5.0, y + 5.0),
                        (x - 5.0, y - 16.0, x - 5.0, y + 5.0),
                        (x + width - 5.0, y - 16.0, x + width - 5.0, y + 5.0),
                    ] {
                        page.line(a, b, c, d, 1.0, INK);
                    }
                }
            }
        }
    }
    let mut systems: BTreeMap<(usize, u32), Vec<&StaffRegion>> = BTreeMap::new();
    for region in &staves {
        systems
            .entry((region.part, region.y.to_bits()))
            .or_default()
            .push(region);
    }
    for ((part, _), mut regions) in systems {
        regions.sort_by_key(|region| region.measure);
        let first = regions[0];
        let last = *regions.last().unwrap();
        for line in score.notation.spanners.iter().filter(|line| {
            line.start.part == part
                && line.start.measure <= last.measure
                && line.end.measure >= first.measure
        }) {
            let left = regions
                .iter()
                .find(|region| region.measure == line.start.measure)
                .map_or(first.content_x, |region| {
                    region.x_at_tick(line.start.tick as f32)
                });
            let right = regions
                .iter()
                .find(|region| region.measure == line.end.measure)
                .map_or(last.x + last.width - 8.0, |region| {
                    region.x_at_tick(line.end.tick as f32)
                });
            if right <= left + 2.0 {
                continue;
            }
            let y = match line.placement {
                Placement::Above => {
                    regions
                        .iter()
                        .map(|region| base(score, region, line.placement))
                        .fold(f32::INFINITY, f32::min)
                        - 15.0
                }
                Placement::Below => {
                    regions
                        .iter()
                        .map(|region| base(score, region, line.placement))
                        .fold(0.0, f32::max)
                        + 18.0
                        + lyrics::extra_below(
                            score,
                            part,
                            first.measure,
                            last.measure - first.measure + 1,
                        )
                }
            } + line.offset;
            let total = crate::expressions::tick(score, line.end.into())
                - crate::expressions::tick(score, line.start.into());
            let first_tick = crate::expressions::tick(
                score,
                TimePoint {
                    measure: first.measure,
                    tick: 0,
                },
            )
            .max(crate::expressions::tick(score, line.start.into()));
            let last_tick = crate::expressions::tick(
                score,
                TimePoint {
                    measure: last.measure,
                    tick: score.measure_ticks(),
                },
            )
            .min(crate::expressions::tick(score, line.end.into()));
            let openness = |at: u64| {
                let fraction =
                    (at - crate::expressions::tick(score, line.start.into())) as f32 / total as f32;
                6.0 * if line.kind == HairpinKind::Crescendo {
                    fraction
                } else {
                    1.0 - fraction
                }
            };
            let a = openness(first_tick);
            let b = openness(last_tick);
            page.line(left, y - a, right, y - b, 1.1, INK);
            page.line(left, y + a, right, y + b, 1.1, INK);
        }
    }
    for mark in &score.notation.tempo {
        for region in staves
            .iter()
            .filter(|region| region.measure == mark.at.measure)
        {
            if staves
                .iter()
                .any(|other| other.measure == region.measure && other.y < region.y)
            {
                continue;
            }
            let x = region.x_at_tick(mark.at.tick as f32);
            let system = staves
                .iter()
                .filter(|staff| staff.part == region.part && (staff.y - region.y).abs() < 0.1)
                .collect::<Vec<_>>();
            let y = slurs::above_envelope(score, &system).map_or(region.y - 58.0, |envelope| {
                (region.y - 58.0).min(envelope - 38.0)
            });
            let text = if mark.text.is_empty() {
                format!("BPM {}", mark.bpm)
            } else {
                format!("{} · {}", mark.text, mark.bpm)
            };
            page.text(x, y, text, 12.0, INK, true);
            if let Some(ramp) = &mark.ramp {
                page.text(x, y - 15.0, format!("→ {} BPM", ramp.bpm), 10.0, INK, true);
            }
        }
    }
}
