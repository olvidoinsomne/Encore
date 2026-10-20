//! Small vector entries share the principal staff's temporal spacing.
use super::*;
use crate::grace::{GraceGroup, GracePlacement, GraceSelection};

fn note_width(note: &crate::grace::GraceNote, scale: f32) -> f32 {
    (22.0
        + if note.pitches.iter().any(|p| p.alter != 0) {
            12.0
        } else {
            0.0
        }
        + if note.dotted { 5.0 } else { 0.0 })
        * scale
}
pub(super) fn width(group: &GraceGroup) -> f32 {
    group
        .notes
        .iter()
        .map(|n| note_width(n, group.scale))
        .sum::<f32>()
        + 8.0
}
pub(super) fn widths(score: &Score, parts: &[usize], measure: usize) -> BTreeMap<u32, (f32, f32)> {
    let mut result = BTreeMap::<u32, (f32, f32)>::new();
    for group in score
        .notation
        .grace
        .iter()
        .filter(|g| g.anchor.measure == measure && parts.contains(&g.anchor.part))
    {
        let entry = result.entry(group.anchor.tick).or_default();
        if group.placement == GracePlacement::Before {
            entry.0 = entry.0.max(width(group));
        } else {
            entry.1 = entry.1.max(width(group));
        }
    }
    result
}
pub(super) fn prefix(score: &Score, parts: &[usize], measure: usize) -> f32 {
    widths(score, parts, measure).get(&0).map_or(0.0, |v| v.0)
}
pub(super) fn horizontal_requirement(score: &Score, parts: &[usize], measure: usize) -> f32 {
    let groups = widths(score, parts, measure);
    if groups.is_empty() {
        return 0.0;
    }
    let ticks = score.measure_ticks();
    let beat = (PPQ * 4 / u32::from(score.time_denominator.max(1))).max(1);
    let mut times = BTreeSet::from([0, ticks]);
    for time in (0..ticks).step_by(beat as usize) {
        times.insert(time);
    }
    for &part in parts {
        if let Some(m) = score.parts[part].measures.get(measure) {
            for n in &m.notes {
                times.insert(n.start);
                times.insert((n.start + n.ticks()).min(ticks));
            }
        }
    }
    let mut previous = 0;
    let mut normal = 0.0;
    for time in times {
        if time > 0 {
            normal += 16.0 + 12.0 * ((time - previous) as f32 / PPQ as f32).sqrt();
        }
        previous = time;
    }
    normal + groups.values().map(|(a, b)| a + b).sum::<f32>()
}
pub(super) fn clearance(score: &Score, part: usize, measure: usize, bars: usize) -> (f32, f32) {
    let pitch_index = crate::lines::PitchIndex::new(score);
    let mut above = 0.0f32;
    let mut below = 0.0f32;
    for g in score.notation.grace.iter().filter(|g| {
        g.anchor.part == part && g.anchor.measure >= measure && g.anchor.measure < measure + bars
    }) {
        for n in &g.notes {
            for p in &n.pitches {
                let pitch = pitch_index.written_pitch(g.anchor, *p);
                let y = pitch_y(&pitch, score.parts[part].clef, 0.0);
                above = above.max(
                    -y + if n.duration.has_stem() {
                        stem_length(n.duration) * g.scale + 8.0
                    } else {
                        10.0
                    },
                );
                below = below.max(y + if g.slur_to_main { 18.0 } else { 10.0 });
            }
        }
    }
    (above, below)
}
pub(super) fn draw(page: &mut Page, score: &Score) {
    let pitch_index = crate::lines::PitchIndex::new(score);
    let groups = score
        .notation
        .grace
        .iter()
        .filter(|g| {
            page.staves
                .iter()
                .any(|s| s.part == g.anchor.part && s.measure == g.anchor.measure)
        })
        .collect::<Vec<_>>();
    for group in groups {
        let Some(region) = page
            .staves
            .iter()
            .find(|s| s.part == group.anchor.part && s.measure == group.anchor.measure)
            .cloned()
        else {
            continue;
        };
        let clef = score.parts[group.anchor.part].clef;
        let main_x = region.x_at_tick(group.anchor.tick as f32);
        let desired = width(group);
        let start = if group.placement == GracePlacement::Before {
            main_x - desired - 6.0
        } else {
            main_x + 18.0
        };
        let available = if group.placement == GracePlacement::Before {
            desired
        } else {
            (region.x + region.width - 10.0 - start).max(8.0)
        };
        let squeeze = (available / desired).min(1.0);
        let mut x = start;
        let mut layouts = Vec::new();
        let mut state = BTreeMap::new();
        for (index, note) in group.notes.iter().enumerate() {
            let s = group.scale;
            let has_alter = note.pitches.iter().any(|p| p.alter != 0);
            let cx = x + (if has_alter { 17.0 } else { 7.0 }) * s * squeeze;
            let ys = note
                .pitches
                .iter()
                .map(|p| pitch_y(&pitch_index.written_pitch(group.anchor, *p), clef, region.y))
                .collect::<Vec<_>>();
            let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
            let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mid = ys.iter().sum::<f32>() / ys.len() as f32;
            let style = crate::entry_style::EntryStyle {
                scale: s,
                ..crate::entry_style::EntryStyle::new(group.anchor)
            };
            let mut order = (0..note.pitches.len()).collect::<Vec<_>>();
            order.sort_by_key(|i| note.pitches[*i].staff_step());
            let mut last = None;
            let mut displaced = false;
            for i in order {
                let pitch = pitch_index.written_pitch(group.anchor, note.pitches[i]);
                let py = ys[i];
                displaced =
                    last.is_some_and(|p: i32| (pitch.staff_step() - p).abs() == 1) && !displaced;
                last = Some(pitch.staff_step());
                let hx = cx + if displaced { 10.0 * s } else { 0.0 };
                let mut ly = region.y - 10.0;
                while py <= ly + 1.0 {
                    page.line(hx - 7.0 * s, ly, hx + 7.0 * s, ly, 0.8 * s, INK);
                    ly -= 10.0;
                }
                let mut ly = region.y + 50.0;
                while py >= ly - 1.0 {
                    page.line(hx - 7.0 * s, ly, hx + 7.0 * s, ly, 0.8 * s, INK);
                    ly += 10.0;
                }
                let expected = state
                    .get(&(pitch.step.index(), pitch.octave))
                    .copied()
                    .unwrap_or(key_alter(pitch.step, score.key_fifths));
                if expected != pitch.alter {
                    scaled_accidental(page, cx - 11.0 * s, py, pitch.alter, s);
                }
                state.insert((pitch.step.index(), pitch.octave), pitch.alter);
                entry_style::draw_notehead(
                    page,
                    &style,
                    hx,
                    py,
                    matches!(
                        note.duration,
                        Duration::DoubleWhole | Duration::Whole | Duration::Half
                    ),
                    !note.duration.has_stem(),
                );
                if note.dotted {
                    page.ellipse(
                        hx + 10.0 * s,
                        py - 3.0 * s,
                        1.5 * s,
                        1.5 * s,
                        Some(INK),
                        None,
                        0.0,
                    );
                }
            }
            page.grace_hits.push(GraceHit {
                selection: GraceSelection {
                    anchor: group.anchor,
                    placement: group.placement,
                    note: index,
                },
                x: cx,
                y: mid,
            });
            layouts.push(NoteLayout {
                x: cx,
                top,
                bottom,
                mid,
                up: true,
                duration: note.duration,
                start: index as u32,
                end: index as u32 + 1,
                scale: s,
                stem: note.duration.has_stem(),
                forced_up: Some(true),
                beam: note.beam,
            });
            x += note_width(note, s) * squeeze;
        }
        let mut i = 0;
        while i < layouts.len() {
            let mut end = i + 1;
            if layouts[i].duration.flags() > 0 {
                while end < layouts.len()
                    && layouts[end].duration.flags() > 0
                    && layouts[end].beam != crate::entry_style::BeamStyle::Break
                {
                    end += 1;
                }
            }
            if end - i > 1 {
                beam_group(page, &layouts[i..end]);
            } else {
                stem_flag(page, &layouts[i]);
            }
            i = end;
        }
        if group.slashed
            && let Some(first) = layouts.iter().find(|n| n.duration.has_stem())
        {
            let y = first.top - stem_length(first.duration) * group.scale * 0.65;
            page.line(
                first.x - 4.0 * group.scale,
                y + 4.0 * group.scale,
                first.x + 12.0 * group.scale,
                y - 6.0 * group.scale,
                1.3 * group.scale,
                INK,
            );
        }
        if group.slur_to_main
            && let Some(main) = crate::document::note_at(score, &group.anchor)
        {
            let main_y = main
                .pitches
                .iter()
                .map(|p| pitch_y(&pitch_index.written_pitch(group.anchor, *p), clef, region.y))
                .fold(region.y + 20.0, f32::max)
                + 8.0;
            let first = &layouts[0];
            let last = layouts.last().unwrap();
            let (a, b) = if group.placement == GracePlacement::Before {
                ((first.x, first.bottom + 7.0), (main_x - 5.0, main_y))
            } else {
                ((main_x + 5.0, main_y), (last.x, last.bottom + 7.0))
            };
            let mut points = vec![a];
            let span = (b.0 - a.0).abs();
            cubic(
                &mut points,
                a,
                (a.0 + span * 0.25, a.1 + 10.0),
                (b.0 - span * 0.25, b.1 + 10.0),
                b,
                24,
            );
            page.path(points, false, None, Some(INK), 1.0 * group.scale);
        }
    }
}
fn scaled_accidental(page: &mut Page, x: f32, y: f32, alter: i8, scale: f32) {
    let mut temp = Page::new();
    accidental(&mut temp, 0.0, 0.0, alter);
    scale_page(&mut temp, scale);
    for mut item in temp.items {
        match &mut item {
            Primitive::Line { x1, y1, x2, y2, .. } => {
                *x1 += x;
                *x2 += x;
                *y1 += y;
                *y2 += y;
            }
            Primitive::Ellipse { cx, cy, .. } => {
                *cx += x;
                *cy += y;
            }
            Primitive::Path { points, .. } => {
                for (p, q) in points {
                    *p += x;
                    *q += y;
                }
            }
            Primitive::Text { x: tx, y: ty, .. } => {
                *tx += x;
                *ty += y;
            }
        }
        page.items.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::Anchor,
        grace::{GraceGroup, GraceNote},
    };
    #[test]
    fn grace_hits_are_separate_from_principal_hits_and_prefix_clears_header() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![Note {
            start: 0,
            layer: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(72)],
            velocity: 80,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        }];
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let mut group = GraceGroup::new(
            anchor,
            vec![
                GraceNote::new(Pitch::from_midi(73), Duration::Sixteenth),
                GraceNote::new(Pitch::from_midi(74), Duration::Sixteenth),
            ],
        );
        group.slashed = true;
        group.slur_to_main = true;
        crate::grace::assign(&mut score, group).unwrap();
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        assert_eq!(page.grace_hits.len(), 2);
        assert_eq!(page.hits.len(), 1);
        let staff = &page.staves[0];
        assert!(page.grace_hits[0].x > staff.x + 75.0);
        assert!(page.grace_hits[1].x < page.hits[0].x - 10.0);
        let svg = super::svg(page);
        assert!(!svg.contains("NaN"));
    }
}
