use super::*;
use crate::document::{Anchor, note_at};
use crate::lyrics::position;
use crate::slurs::{Slur, SlurDirection, SlurShape, SlurStyle};

pub(super) fn clearance(score: &Score, part: usize, measure: usize, bars: usize) -> (f32, f32) {
    let mut above = 0.0_f32;
    let mut below = 0.0_f32;
    for s in score.notation.slurs.iter().filter(|s| {
        s.start.part == part && s.start.measure < measure + bars && s.end.measure >= measure
    }) {
        let shape = s.shape_or_default();
        let offsets = s.start_offset[1].abs().max(s.end_offset[1].abs()) + 15.0;
        let outward = shape.heights.into_iter().fold(0.0_f32, f32::max) + offsets;
        let inward = shape
            .heights
            .into_iter()
            .map(|v| -v)
            .fold(0.0_f32, f32::max)
            + offsets;
        if is_above(score, s) {
            above = above.max(outward);
            if s.shape.is_some() {
                below = below.max(inward);
            }
        } else {
            below = below.max(outward);
            if s.shape.is_some() {
                above = above.max(inward);
            }
        }
    }
    (above, below)
}
fn is_above(score: &Score, s: &Slur) -> bool {
    match s.direction {
        SlurDirection::Above => true,
        SlurDirection::Below => false,
        SlurDirection::Auto => automatic_slur_above(score, s.start),
    }
}
fn endpoint(
    score: &Score,
    index: &display::Index,
    anchor: &Anchor,
    region: &StaffRegion,
    above: bool,
    avoid_marks: bool,
) -> f32 {
    let edge = note_at(score, anchor)
        .map(|n| {
            n.pitches
                .iter()
                .map(|p| index.y(*anchor, *p, region.y))
                .reduce(|a, b| if above { a.min(b) } else { a.max(b) })
                .unwrap_or(region.y + 20.0)
        })
        .unwrap_or(region.y + 20.0);
    let edge = if avoid_marks {
        index.tremolo_bounds(*anchor).map_or(edge, |(top, bottom)| {
            if above {
                edge.min(region.y + top)
            } else {
                edge.max(region.y + bottom)
            }
        })
    } else {
        edge
    };
    edge + if above { -10.0 } else { 10.0 }
}
#[derive(Clone)]
struct Fragment<'a> {
    slur: &'a Slur,
    a: (f32, f32),
    b: (f32, f32),
    controls: [(f32, f32); 2],
    above: bool,
    true_start: bool,
    true_end: bool,
}
impl Fragment<'_> {
    fn at(&self, t: f32) -> (f32, f32) {
        let u = 1.0 - t;
        let [c, d] = self.controls;
        (
            u * u * u * self.a.0
                + 3.0 * u * u * t * c.0
                + 3.0 * u * t * t * d.0
                + t * t * t * self.b.0,
            u * u * u * self.a.1
                + 3.0 * u * u * t * c.1
                + 3.0 * u * t * t * d.1
                + t * t * t * self.b.1,
        )
    }
    fn centerline(&self) -> Vec<(f32, f32)> {
        (0..=40).map(|i| self.at(i as f32 / 40.0)).collect()
    }
}
fn fragments<'a>(score: &'a Score, regions: &[StaffRegion]) -> Vec<Fragment<'a>> {
    if score.notation.slurs.is_empty() {
        return Vec::new();
    }
    let pitch_index = display::Index::new(score);
    let mut phrases = BTreeMap::<usize, Vec<&Slur>>::new();
    for slur in &score.notation.slurs {
        phrases.entry(slur.start.part).or_default().push(slur);
    }
    let mut systems: BTreeMap<(usize, u32), Vec<&StaffRegion>> = BTreeMap::new();
    for r in regions {
        systems.entry((r.part, r.y.to_bits())).or_default().push(r);
    }
    let mut result = Vec::new();
    for ((part, _), mut regions) in systems {
        regions.sort_by_key(|r| r.measure);
        let first = regions[0];
        let last = regions.last().unwrap();
        for s in phrases
            .get(&part)
            .into_iter()
            .flatten()
            .copied()
            .filter(|s| {
                position(&s.end) >= (first.measure, 0)
                    && position(&s.start) < (last.measure, score.measure_ticks())
            })
        {
            let above = is_above(score, s);
            let start_region = regions.iter().find(|r| r.measure == s.start.measure);
            let end_region = regions.iter().find(|r| r.measure == s.end.measure);
            let a = if let Some(r) = start_region {
                (
                    r.x_at_anchor(&s.start) + s.start_offset[0],
                    endpoint(score, &pitch_index, &s.start, r, above, s.shape.is_none())
                        + s.start_offset[1],
                )
            } else {
                (
                    first.content_x - 5.0,
                    if above {
                        first.top_line_y() - 14.0
                    } else {
                        first.bottom_line_y() + 14.0
                    },
                )
            };
            let b = if let Some(r) = end_region {
                (
                    r.x_at_anchor(&s.end) + s.end_offset[0],
                    endpoint(score, &pitch_index, &s.end, r, above, s.shape.is_none())
                        + s.end_offset[1],
                )
            } else {
                (
                    last.x + last.width - 7.0,
                    if above {
                        last.top_line_y() - 14.0
                    } else {
                        last.bottom_line_y() + 14.0
                    },
                )
            };
            if b.0 <= a.0 + 2.0 {
                continue;
            }
            let mut height = s.height;
            if s.shape.is_none() {
                for r in &regions {
                    if let Some(m) = score.parts[part].measures.get(r.measure) {
                        for n in &m.notes {
                            let anchor = Anchor {
                                part,
                                measure: r.measure,
                                tick: n.start,
                                layer: n.layer,
                                voice: n.voice,
                            };
                            // Ordinary slurs retain legacy lane-local collision
                            // lifting. A cross-voice phrase clears both lanes.
                            if (n.lane() != s.start.lane() && n.lane() != s.end.lane())
                                || position(&anchor) <= position(&s.start)
                                || position(&anchor) >= position(&s.end)
                            {
                                continue;
                            }
                            let x = r.x_at_anchor(&anchor);
                            let t = ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
                            let weight = 3.0 * t * (1.0 - t);
                            if weight < 0.1 {
                                continue;
                            }
                            let linear = a.1 + (b.1 - a.1) * t;
                            let mut obstacles = n
                                .pitches
                                .iter()
                                .map(|p| pitch_index.y(anchor, *p, r.y))
                                .collect::<Vec<_>>();
                            if let Some((top, bottom)) = pitch_index.tremolo_bounds(anchor) {
                                obstacles.push(r.y + if above { top } else { bottom });
                            }
                            for y in obstacles {
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
            }
            let SlurShape { inset, heights } = s.shape.unwrap_or(SlurShape {
                inset: [1.0 / 3.0; 2],
                heights: [height.min(160.0); 2],
            });
            let sign = if above { -1.0 } else { 1.0 };
            let dx = b.0 - a.0;
            let control_x = if s.shape.is_none() {
                [a.0 + dx / 3.0, b.0 - dx / 3.0]
            } else {
                [a.0 + dx * inset[0], b.0 - dx * inset[1]]
            };
            result.push(Fragment {
                slur: s,
                a,
                b,
                controls: [
                    (control_x[0], a.1 + heights[0] * sign),
                    (control_x[1], b.1 + heights[1] * sign),
                ],
                above,
                true_start: start_region.is_some(),
                true_end: end_region.is_some(),
            });
        }
    }
    result
}
/// Uses the same centerline for every line style, so endings and text remain
/// clear even when the curve consists of dots instead of a filled outline.
pub(super) fn above_envelope(score: &Score, regions: &[&StaffRegion]) -> Option<f32> {
    let regions = regions.iter().map(|r| (*r).clone()).collect::<Vec<_>>();
    fragments(score, &regions)
        .into_iter()
        .filter(|f| f.above || f.slur.shape.is_some())
        .flat_map(|f| {
            let padding = if f.slur.style == SlurStyle::Solid {
                0.0
            } else {
                0.8
            };
            f.centerline()
                .into_iter()
                .map(move |(x, y)| (x, y - padding))
        })
        .map(|(_, y)| y)
        .reduce(f32::min)
}
pub(super) fn draw(page: &mut Page, score: &Score) {
    for f in fragments(score, &page.staves) {
        let points = f.centerline();
        match f.slur.style {
            SlurStyle::Solid => {
                let mut outline = points;
                outline.extend((0..=40).rev().map(|i| {
                    let t = i as f32 / 40.0;
                    let mut p = f.at(t);
                    p.1 += if f.above { 1.65 } else { -1.65 } * (std::f32::consts::PI * t).sin();
                    p
                }));
                page.path(outline, true, Some(INK), None, 0.0);
            }
            SlurStyle::Dashed | SlurStyle::Dotted => draw_pattern(page, &points, f.slur),
        }
        let mut handles = vec![
            (SlurHandle::Control1, f.controls[0]),
            (SlurHandle::Control2, f.controls[1]),
        ];
        if f.true_start {
            handles.push((SlurHandle::Start, f.a));
        }
        if f.true_end {
            handles.push((SlurHandle::End, f.b));
        }
        for (handle, (x, y)) in handles {
            page.slur_hits.push(SlurHit {
                start: f.slur.start,
                end: f.slur.end,
                handle,
                x,
                y,
            });
        }
    }
}
fn draw_pattern(page: &mut Page, points: &[(f32, f32)], slur: &Slur) {
    let mut distances = vec![0.0];
    for pair in points.windows(2) {
        distances
            .push(distances.last().unwrap() + (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1));
    }
    let length = *distances.last().unwrap_or(&0.0);
    let at = |distance: f32| -> (f32, f32) {
        let i = distances
            .partition_point(|v| *v < distance)
            .clamp(1, points.len() - 1);
        let t = ((distance - distances[i - 1]) / (distances[i] - distances[i - 1]).max(0.0001))
            .clamp(0.0, 1.0);
        (
            points[i - 1].0 + (points[i].0 - points[i - 1].0) * t,
            points[i - 1].1 + (points[i].1 - points[i - 1].1) * t,
        )
    };
    let dash = slur.dash_length.clamp(2.0, 24.0);
    let gap = slur.gap.clamp(2.0, 24.0);
    let step = if slur.style == SlurStyle::Dotted {
        gap + 1.6
    } else {
        dash + gap
    };
    let mut cursor = 0.0;
    while cursor <= length {
        if slur.style == SlurStyle::Dotted {
            let p = at(cursor);
            page.ellipse(p.0, p.1, 0.8, 0.8, Some(INK), None, 0.0);
        } else {
            let end = (cursor + dash).min(length);
            let mut path = vec![at(cursor)];
            for (i, &d) in distances.iter().enumerate() {
                if d > cursor && d < end {
                    path.push(points[i]);
                }
            }
            path.push(at(end));
            page.path(path, false, None, Some(INK), 1.5);
        }
        cursor += step;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::slurs::{self, Slur};
    fn cross_voice(style: SlurStyle) -> Score {
        let mut score = Score::demo();
        score.parts.truncate(1);
        let mut n = score.parts[0].measures[0].notes[0].clone();
        n.voice = 1;
        n.voice_launch = Some(0);
        score.parts[0].measures[0].notes.push(n);
        score.parts[0].measures[0]
            .notes
            .sort_by_key(|n| (n.start, n.layer, n.voice));
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 1,
        };
        let b = Anchor {
            part: 0,
            measure: 5,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        let mut slur = Slur::new(a, b);
        slur.direction = SlurDirection::Above;
        slur.style = style;
        slur.shape = Some(SlurShape {
            inset: [0.2, 0.4],
            heights: [62.0, 17.0],
        });
        slur.start_offset = [2.0, -3.0];
        slurs::add(&mut score, slur).unwrap();
        score
    }
    #[test]
    fn patterned_cross_voice_fragments_have_arc_spaced_vectors_and_source_handles() {
        for style in [SlurStyle::Dashed, SlurStyle::Dotted] {
            let s = cross_voice(style);
            let pages = engrave(&s, &EngravingOptions::default());
            let slur = &s.notation.slurs[0];
            let hits = pages.iter().flat_map(|p| &p.slur_hits).collect::<Vec<_>>();
            assert!(hits.len() >= 6);
            assert!(
                hits.iter()
                    .all(|h| h.start == slur.start && h.end == slur.end)
            );
            assert_eq!(
                hits.iter()
                    .filter(|h| h.handle == SlurHandle::Start)
                    .count(),
                1
            );
            assert_eq!(
                hits.iter().filter(|h| h.handle == SlurHandle::End).count(),
                1
            );
            let page = pages.iter().find(|p| !p.slur_hits.is_empty()).unwrap();
            let mut alone = Page::new();
            alone.staves = page.staves.clone();
            draw(&mut alone, &s);
            if style == SlurStyle::Dashed {
                assert!(
                    alone
                        .items
                        .iter()
                        .filter(|p| matches!(p, Primitive::Path { closed: false, .. }))
                        .count()
                        > 4
                );
            } else {
                assert!(
                    alone
                        .items
                        .iter()
                        .filter(|p| matches!(p, Primitive::Ellipse { .. }))
                        .count()
                        > 4
                );
            }
            let refs = page.staves.iter().collect::<Vec<_>>();
            assert!(above_envelope(&s, &refs).is_some());
            assert!(
                alone
                    .slur_hits
                    .iter()
                    .all(|h| h.x.is_finite() && h.y.is_finite())
            );
        }
    }
    #[test]
    fn manual_shape_freezes_lifting_and_insets_follow_fragment_width() {
        let s = cross_voice(SlurStyle::Solid);
        let pages = engrave(&s, &EngravingOptions::default());
        let f = fragments(&s, &pages[0].staves);
        let first = &f[0];
        assert!((first.controls[0].0 - first.a.0 - (first.b.0 - first.a.0) * 0.2).abs() < 0.001);
        assert!((first.controls[0].1 - first.a.1 + 62.0).abs() < 0.001);
        assert!((first.controls[1].1 - first.b.1 + 17.0).abs() < 0.001);
    }
    #[test]
    fn slur_handles_scale_with_their_source_voice_endpoints() {
        let mut s = cross_voice(SlurStyle::Dotted);
        s.notation.layout.staff_scale = 0.7;
        let pages = engrave(&s, &EngravingOptions::default());
        let h = pages
            .iter()
            .flat_map(|p| &p.slur_hits)
            .find(|h| h.handle == SlurHandle::Start)
            .unwrap();
        let r = pages
            .iter()
            .flat_map(|p| &p.staves)
            .find(|r| r.part == h.start.part && r.measure == h.start.measure)
            .unwrap();
        assert!((h.x - r.x_at_anchor(&h.start) - 2.0 * 0.7).abs() < 0.001);
    }
    #[test]
    fn cross_system_slur_draws_vector_fragments() {
        let mut s = Score::demo();
        slurs::add(
            &mut s,
            Slur::new(
                Anchor {
                    voice: 0,
                    part: 0,
                    measure: 0,
                    tick: 0,
                    layer: 0,
                },
                Anchor {
                    voice: 0,
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
