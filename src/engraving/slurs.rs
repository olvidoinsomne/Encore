use super::*;
use crate::document::{Anchor, note_at};
use crate::lyrics::position;
use crate::slurs::SlurDirection;

pub(super) fn clearance(score: &Score, part: usize, measure: usize, bars: usize) -> (f32, f32) {
    let mut above = 0.0_f32;
    let mut below = 0.0_f32;
    for s in score.notation.slurs.iter().filter(|s| {
        s.start.part == part && s.start.measure < measure + bars && s.end.measure >= measure
    }) {
        let size = s.height + 15.0 + s.start_offset[1].abs().max(s.end_offset[1].abs());
        if s.direction == SlurDirection::Below
            || (s.direction == SlurDirection::Auto && !s.start.layer.is_multiple_of(2))
        {
            below = below.max(size);
        } else {
            above = above.max(size);
        }
    }
    (above, below)
}
fn endpoint(score: &Score, anchor: &Anchor, region: &StaffRegion, above: bool) -> f32 {
    note_at(score, anchor)
        .map(|n| {
            n.pitches
                .iter()
                .map(|p| pitch_y(p, score.parts[anchor.part].clef, region.y))
                .reduce(|a, b| if above { a.min(b) } else { a.max(b) })
                .unwrap_or(region.y + 20.0)
        })
        .unwrap_or(region.y + 20.0)
        + if above { -10.0 } else { 10.0 }
}
fn bezier(a: (f32, f32), b: (f32, f32), height: f32, t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let dx = b.0 - a.0;
    (
        u * u * u * a.0
            + 3.0 * u * u * t * (a.0 + dx / 3.0)
            + 3.0 * u * t * t * (b.0 - dx / 3.0)
            + t * t * t * b.0,
        u * u * u * a.1
            + 3.0 * u * u * t * (a.1 + height)
            + 3.0 * u * t * t * (b.1 + height)
            + t * t * t * b.1,
    )
}
pub(super) fn draw(page: &mut Page, score: &Score) {
    draw_internal(page, score, false);
}

/// Minimum printed y over phrase fragments on the supplied staff system.
/// Uses the exact same sampled Bézier geometry as drawing, including the
/// automatic collision arch, so endings can clear the visible curve.
pub(super) fn above_envelope(score: &Score, regions: &[&StaffRegion]) -> Option<f32> {
    let mut page = Page::new();
    page.staves = regions.iter().map(|r| (*r).clone()).collect();
    draw_internal(&mut page, score, true);
    page.items
        .into_iter()
        .filter_map(|item| {
            if let Primitive::Path { points, .. } = item {
                points.into_iter().map(|(_, y)| y).reduce(f32::min)
            } else {
                None
            }
        })
        .reduce(f32::min)
}

fn draw_internal(page: &mut Page, score: &Score, above_only: bool) {
    let mut systems: BTreeMap<(usize, u32), Vec<StaffRegion>> = BTreeMap::new();
    for r in &page.staves {
        systems
            .entry((r.part, r.y.to_bits()))
            .or_default()
            .push(r.clone());
    }
    for ((part, _), mut regions) in systems {
        regions.sort_by_key(|r| r.measure);
        let first = &regions[0];
        let last = regions.last().unwrap();
        for s in score.notation.slurs.iter().filter(|s| {
            s.start.part == part
                && position(&s.end) >= (first.measure, 0)
                && position(&s.start) < (last.measure, score.measure_ticks())
        }) {
            let above = match s.direction {
                SlurDirection::Above => true,
                SlurDirection::Below => false,
                SlurDirection::Auto => s.start.layer.is_multiple_of(2),
            };
            if above_only && !above {
                continue;
            }
            let start_region = regions.iter().find(|r| r.measure == s.start.measure);
            let end_region = regions.iter().find(|r| r.measure == s.end.measure);
            let a = if let Some(r) = start_region {
                (
                    r.x_at_tick(s.start.tick as f32) + s.start_offset[0],
                    endpoint(score, &s.start, r, above) + s.start_offset[1],
                )
            } else {
                (
                    first.content_x - 5.0,
                    first.y + if above { -14.0 } else { 54.0 },
                )
            };
            let b = if let Some(r) = end_region {
                (
                    r.x_at_tick(s.end.tick as f32) + s.end_offset[0],
                    endpoint(score, &s.end, r, above) + s.end_offset[1],
                )
            } else {
                (
                    last.x + last.width - 7.0,
                    last.y + if above { -14.0 } else { 54.0 },
                )
            };
            if b.0 <= a.0 + 2.0 {
                continue;
            }
            let mut height = s.height;
            // Make the apex clear interior noteheads even when pitches rise
            // sharply. Editable offsets remain applied only to true endpoints.
            for r in &regions {
                if let Some(m) = score.parts[part].measures.get(r.measure) {
                    for n in &m.notes {
                        let anchor = Anchor {
                            part,
                            measure: r.measure,
                            tick: n.start,
                            layer: n.layer,
                        };
                        if n.layer != s.start.layer
                            || position(&anchor) <= position(&s.start)
                            || position(&anchor) >= position(&s.end)
                        {
                            continue;
                        }
                        let x = r.x_at_tick(n.start as f32);
                        let t = ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
                        let weight = 3.0 * t * (1.0 - t);
                        if weight < 0.1 {
                            continue;
                        }
                        let linear = a.1 + (b.1 - a.1) * t;
                        for p in &n.pitches {
                            let y = pitch_y(p, score.parts[part].clef, r.y);
                            let needed = if above {
                                linear - y + 14.0
                            } else {
                                y - linear + 14.0
                            };
                            height = height.max(needed / weight);
                        }
                    }
                }
            }
            let signed = height.min(160.0) * if above { -1.0 } else { 1.0 };
            let mut points = (0..=40)
                .map(|i| bezier(a, b, signed, i as f32 / 40.0))
                .collect::<Vec<_>>();
            points.extend((0..=40).rev().map(|i| {
                let t = i as f32 / 40.0;
                let mut p = bezier(a, b, signed, t);
                p.1 += if above { 1.65 } else { -1.65 } * (std::f32::consts::PI * t).sin();
                p
            }));
            page.path(points, true, Some(INK), None, 0.0);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::slurs::{self, Slur};
    #[test]
    fn cross_system_slur_draws_vector_fragments() {
        let mut s = Score::demo();
        slurs::add(
            &mut s,
            Slur::new(
                Anchor {
                    part: 0,
                    measure: 0,
                    tick: 0,
                    layer: 0,
                },
                Anchor {
                    part: 0,
                    measure: 5,
                    tick: 0,
                    layer: 0,
                },
            ),
        )
        .unwrap();
        let pages = engrave(&s, &EngravingOptions::default());
        assert!(
            pages
                .iter()
                .flat_map(|p| &p.items)
                .filter(|p| matches!(p,Primitive::Path{points,closed:true,..}if points.len()==82))
                .count()
                >= 2
        );
    }
}
