//! Original vector measured-tremolo strokes over principal entry geometry.
use super::{INK, NoteLayout, Page, StaffRegion, display};
use crate::{
    document::Anchor,
    model::{Duration, Note, Score},
    tremolos::Tremolo,
};

#[derive(Clone, Copy)]
pub(super) struct Stroke {
    pub strokes: u8,
    offset: [f32; 2],
    paired: bool,
}
impl From<&Tremolo> for Stroke {
    fn from(mark: &Tremolo) -> Self {
        Self {
            strokes: mark.strokes,
            offset: mark.offset,
            paired: mark.end.is_some(),
        }
    }
}
pub(super) fn stem_length(duration: Duration, strokes: Option<u8>) -> f32 {
    let ordinary = super::stem_length(duration);
    strokes.map_or(ordinary, |s| {
        ordinary.max(23.0 + f32::from(s.saturating_sub(1)) * 7.0 + duration.flags() as f32 * 7.0)
    })
}
/// Pair means are computed once, without rescanning a dense measure per note.
pub(super) struct Index<'a> {
    core: crate::tremolos::Index<'a>,
    means: std::collections::BTreeMap<Anchor, f32>,
}
impl<'a> Index<'a> {
    pub(super) fn new(score: &'a Score, pitches: &display::Index) -> Self {
        let core = crate::tremolos::Index::new(score);
        let mut needed = std::collections::BTreeSet::new();
        for mark in &score.notation.tremolos {
            if let Some(end) = mark.end {
                needed.insert(mark.anchor);
                needed.insert(end);
            }
        }
        let mut principals = std::collections::BTreeMap::new();
        if !needed.is_empty() {
            for (part, staff) in score.parts.iter().enumerate() {
                for (measure, bar) in staff.measures.iter().enumerate() {
                    for note in &bar.notes {
                        let at = Anchor {
                            part,
                            measure,
                            tick: note.start,
                            layer: note.layer,
                            voice: note.voice,
                        };
                        if needed.contains(&at) {
                            principals.insert(at, note);
                        }
                    }
                }
            }
        }
        let mut means = std::collections::BTreeMap::new();
        for mark in &score.notation.tremolos {
            let Some(end) = mark.end else {
                continue;
            };
            let Some(first) = principals.get(&mark.anchor) else {
                continue;
            };
            let Some(last) = principals.get(&end) else {
                continue;
            };
            let count = first.pitches.len() + last.pitches.len();
            if count == 0 {
                continue;
            }
            let mean = (first
                .pitches
                .iter()
                .map(|p| pitches.y(mark.anchor, *p, 0.0))
                .sum::<f32>()
                + last
                    .pitches
                    .iter()
                    .map(|p| pitches.y(end, *p, 0.0))
                    .sum::<f32>())
                / count as f32;
            means.insert(mark.anchor, mean);
            means.insert(end, mean);
        }
        Self { core, means }
    }
    pub(super) fn at(&self, anchor: &Anchor) -> Option<(&'a Tremolo, bool)> {
        self.core.at(anchor)
    }
    pub(super) fn display_duration(&self, anchor: &Anchor, note: &Note) -> Duration {
        self.core.display_duration(anchor, note)
    }
    pub(super) fn pair_mean(&self, anchor: Anchor, y: f32) -> Option<f32> {
        self.means.get(&anchor).map(|v| v + y)
    }
}
pub(super) fn clearance(
    _note: &Note,
    mark: Option<(&Tremolo, bool)>,
    ys: &[f32],
    up: bool,
    _stem: bool,
    scale: f32,
) -> (f32, f32) {
    let Some((mark, _)) = mark else {
        return (0.0, 0.0);
    };
    let depth = (18.0 + f32::from(mark.strokes.saturating_sub(1)) * 7.0) * scale;
    let top = ys.iter().copied().fold(0.0_f32, f32::min);
    let bottom = ys.iter().copied().fold(40.0_f32, f32::max);
    // Opposite manual stems make paired strokes float. Reserve both exteriors
    // for that explicit editing case rather than clipping a connector.
    (
        (if up || mark.end.is_some() {
            -top + depth - mark.offset[1]
        } else {
            0.0
        })
        .max(0.0),
        (if !up || mark.end.is_some() {
            bottom + depth + mark.offset[1]
        } else {
            0.0
        })
        .max(0.0),
    )
}
pub(super) fn single(page: &mut Page, n: &NoteLayout, up: bool) {
    let Some(mark) = n.tremolo.filter(|m| !m.paired) else {
        return;
    };
    let sign = if up { -1.0 } else { 1.0 };
    let stemmed = n.stem && n.duration.has_stem();
    let x = n.x + if stemmed { -sign * 5.0 * n.scale } else { 0.0 } + mark.offset[0];
    let y = if up { n.top } else { n.bottom } + sign * 13.0 * n.scale + mark.offset[1];
    for stroke in 0..mark.strokes {
        let y = y + sign * f32::from(stroke) * 7.0 * n.scale;
        slash(
            page,
            x - 7.0 * n.scale,
            y + 3.0 * n.scale,
            x + 7.0 * n.scale,
            y - 3.0 * n.scale,
            3.0 * n.scale,
        );
    }
}
pub(super) fn pairs(
    page: &mut Page,
    notes: &[NoteLayout],
    index: &Index<'_>,
    region: &StaffRegion,
    layer: u8,
    voice: u8,
) {
    let by_tick = notes
        .iter()
        .map(|n| (n.start, n))
        .collect::<std::collections::BTreeMap<_, _>>();
    for first in notes {
        let anchor = Anchor {
            part: region.part,
            measure: region.measure,
            tick: first.start,
            layer,
            voice,
        };
        let Some((mark, false)) = index.at(&anchor) else {
            continue;
        };
        let Some(end) = mark.end else {
            continue;
        };
        let Some(last) = by_tick.get(&end.tick) else {
            continue;
        };
        let up = first.up;
        let sign = if up { -1.0 } else { 1.0 };
        let scale = first.scale.min(last.scale);
        let attached = first.stem
            && last.stem
            && first.duration.has_stem()
            && last.duration.has_stem()
            && first.up == last.up;
        let x1 = first.x
            + if attached {
                -sign * 5.0 * first.scale
            } else {
                8.0 * scale
            }
            + mark.offset[0];
        let x2 = last.x
            + if attached {
                -sign * 5.0 * last.scale
            } else {
                -8.0 * scale
            }
            + mark.offset[0];
        if x2 <= x1 + 2.0 {
            continue;
        }
        let base = if up {
            first.top.min(last.top)
        } else {
            first.bottom.max(last.bottom)
        };
        let y = base + sign * 14.0 * scale + mark.offset[1];
        let slope = ((last.mid - first.mid) * 0.12).clamp(-5.0, 5.0);
        for stroke in 0..mark.strokes {
            let y1 = y + sign * f32::from(stroke) * 7.0 * scale;
            slash(page, x1, y1, x2, y1 + slope, 3.5 * scale);
        }
        if attached {
            let exterior =
                y + sign * (f32::from(mark.strokes.saturating_sub(1)) * 7.0 + 3.0) * scale;
            // Extend into the shared connector for wide pitch intervals. These
            // stems still belong to the two original principal note entries.
            page.line(
                x1 - mark.offset[0],
                if up { first.bottom } else { first.top },
                x1 - mark.offset[0],
                exterior,
                1.25 * first.scale,
                INK,
            );
            page.line(
                x2 - mark.offset[0],
                if up { last.bottom } else { last.top },
                x2 - mark.offset[0],
                exterior + slope,
                1.25 * last.scale,
                INK,
            );
        }
    }
}
fn slash(page: &mut Page, x1: f32, y1: f32, x2: f32, y2: f32, width: f32) {
    page.path(
        vec![
            (x1, y1 - width * 0.5),
            (x2, y2 - width * 0.5),
            (x2, y2 + width * 0.5),
            (x1, y1 + width * 0.5),
        ],
        true,
        Some(INK),
        None,
        0.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engraving::{EngravingOptions, Primitive, engrave, svg},
        entry_style::{EntryStyle, StemDirection},
        grace::{GraceGroup, GraceNote},
        percussion::{Staff, token},
    };
    fn at(tick: u32) -> Anchor {
        Anchor {
            part: 0,
            measure: 0,
            tick,
            layer: 0,
            voice: 0,
        }
    }
    fn fixture(duration: Duration) -> Score {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes = (0..2)
            .map(|i| Note {
                start: i * duration.ticks(),
                duration,
                layer: 0,
                voice: 0,
                voice_launch: None,
                dotted: false,
                tuplet: None,
                pitches: vec![crate::model::Pitch::from_midi(if i == 0 { 64 } else { 72 })],
                velocity: 0,
                staccato: false,
                articulations: vec![],
                tied: false,
            })
            .collect();
        let mut mark = Tremolo::new(at(0));
        mark.end = Some(at(duration.ticks()));
        score.notation.tremolos.push(mark);
        score
    }
    fn connectors(page: &Page) -> usize {
        page.items.iter().filter(|p| matches!(p,Primitive::Path{points,closed:true,fill:Some(c),stroke:None,..} if *c==INK && points.len()==4 && points[1].0-points[0].0>25.0)).count()
    }
    #[test]
    fn paired_principals_change_heads_and_stems_without_changing_source_time_or_graces() {
        let mut score = fixture(Duration::Quarter);
        score.validate().unwrap();
        score.notation.grace.push(GraceGroup::new(
            at(0),
            vec![GraceNote::new(
                crate::model::Pitch::from_midi(62),
                Duration::Eighth,
            )],
        ));
        let pitches = display::Index::new(&score);
        let index = Index::new(&score, &pitches);
        assert_eq!(
            index.display_duration(&at(0), &score.parts[0].measures[0].notes[0]),
            Duration::Half
        );
        assert_eq!(index.pair_mean(at(0), 0.0), index.pair_mean(at(480), 0.0));
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        assert_eq!(connectors(page), 3);
        assert_eq!(page.hits.len(), 2);
        assert_eq!(page.grace_hits.len(), 1);
        assert_eq!(score.parts[0].measures[0].notes[0].ticks(), 480);
        assert_eq!(score.notation.grace[0].notes[0].duration, Duration::Eighth);
        // Both principal heads are open; the separately engraved grace is filled.
        assert_eq!(page.items.iter().filter(|p|matches!(p,Primitive::Path{fill:Some([255,255,255]),stroke:Some(c),..}if *c==INK)).count(),2);
    }
    #[test]
    fn paired_half_notes_are_whole_heads_with_floating_strokes_and_no_principal_stems() {
        let score = fixture(Duration::Half);
        score.validate().unwrap();
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        assert_eq!(connectors(page), 3);
        for hit in &page.hits {
            assert!(!page.items.iter().any(|p|matches!(p,Primitive::Line{x1,y1,x2,y2,..} if (*x1-*x2).abs()<0.01 && ((*x1-hit.x).abs()-5.0).abs()<0.01 && (*y1-*y2).abs()>15.0 && (*y1-hit.y).abs()<1.0)));
        }
    }
    #[test]
    fn eight_single_slashes_extend_stems_and_percussion_rolls_use_mapped_heads() {
        let mut score = fixture(Duration::Quarter);
        score.parts[0].measures[0].notes.truncate(1);
        score.parts[0].percussion = Some(Staff::default());
        score.parts[0].measures[0].notes[0].pitches = vec![token(42)];
        let mark = &mut score.notation.tremolos[0];
        mark.end = None;
        mark.strokes = 8;
        let mut style = EntryStyle::new(at(0));
        style.stem = StemDirection::Down;
        score.notation.entries.push(style);
        score.validate().unwrap();
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        assert_eq!(page.sound_hits.len(), 1);
        assert_eq!(page.items.iter().filter(|p|matches!(p,Primitive::Path{points,closed:true,fill:Some(c),stroke:None,..}if *c==INK && points.len()==4 && (points[1].0-points[0].0-14.0).abs()<0.01)).count(),8);
        let hit = &page.hits[0];
        assert!(page.items.iter().any(|p|matches!(p,Primitive::Line{x1,y1,x2,y2,..}if (*x1-*x2).abs()<0.01 && (*x1-(hit.x-5.0)).abs()<0.01 && (*y2-*y1)>65.0)));
        assert!(!svg(page).contains("NaN"));
    }
    #[test]
    fn hidden_endpoint_suppresses_incomplete_pair_and_manual_opposed_stems_stay_finite() {
        let mut score = fixture(Duration::Quarter);
        let mut style = EntryStyle::new(at(480));
        style.visible = false;
        score.notation.entries.push(style);
        assert_eq!(
            connectors(&engrave(&score, &EngravingOptions::default())[0]),
            0
        );
        score.notation.entries[0].visible = true;
        score.notation.entries[0].stem = StemDirection::Down;
        let pages = engrave(&score, &EngravingOptions::default());
        assert_eq!(connectors(&pages[0]), 3);
        assert!(!svg(&pages[0]).contains("NaN"));
    }
    #[test]
    fn grace_to_main_connector_clears_a_whole_note_roll() {
        let mut score = fixture(Duration::Half);
        score.parts[0].measures[0].notes.truncate(1);
        score.parts[0].measures[0].notes[0].duration = Duration::Whole;
        score.notation.tremolos[0].end = None;
        let mut style = EntryStyle::new(at(0));
        style.stem = StemDirection::Down;
        score.notation.entries.push(style);
        let mut g = GraceGroup::new(
            at(0),
            vec![GraceNote::new(
                crate::model::Pitch::from_midi(64),
                Duration::Sixteenth,
            )],
        );
        g.slur_to_main = true;
        score.notation.grace.push(g);
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        let region = &page.staves[0];
        let (_, bottom) = display::Index::new(&score).tremolo_bounds(at(0)).unwrap();
        let principal = &page.hits[0];
        // Grace connector is an open sampled curve terminating left of main.
        let end = page
            .items
            .iter()
            .find_map(|p| {
                if let Primitive::Path {
                    points,
                    closed: false,
                    stroke: Some(c),
                    ..
                } = p
                {
                    let end = *points.last()?;
                    (*c == INK && (end.0 - (principal.x - 5.0)).abs() < 0.01).then_some(end)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(end.1 >= region.y + bottom + 7.0);
    }
    #[test]
    fn automatic_slurs_and_underlay_clear_new_tremolo_obstacles() {
        let mut score = fixture(Duration::Quarter);
        score
            .notation
            .slurs
            .push(crate::slurs::Slur::new(at(0), at(480)));
        let pages = engrave(&score, &EngravingOptions::default());
        let display = display::Index::new(&score);
        let region = &pages[0].staves[0];
        let (top, _) = display.tremolo_bounds(at(0)).unwrap();
        let handle = pages[0]
            .slur_hits
            .iter()
            .find(|h| h.handle == super::super::SlurHandle::Start)
            .unwrap();
        assert!(handle.y <= region.y + top - 9.0);
        score.notation.slurs.clear();
        score.notation.tremolos[0].end = None;
        score.notation.tremolos[0].strokes = 8;
        let mut style = EntryStyle::new(at(0));
        style.stem = StemDirection::Down;
        score.notation.entries.push(style);
        score
            .notation
            .lyrics
            .push(crate::lyrics::Lyric::new(at(0), "roll"));
        let pages = engrave(&score, &EngravingOptions::default());
        let display = display::Index::new(&score);
        let (_, bottom) = display.tremolo_bounds(at(0)).unwrap();
        let baseline = super::super::staff_dynamic_baseline(
            &score.parts[0].measures[0].notes,
            &display,
            0,
            0,
            0.0,
            false,
        );
        assert!(baseline >= bottom + 18.0);
        let lyric = pages[0]
            .items
            .iter()
            .find_map(|p| {
                if let Primitive::Text { text, y, .. } = p {
                    (text == "roll").then_some(*y)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(lyric > pages[0].staves[0].y + bottom + 35.0);
    }
}
