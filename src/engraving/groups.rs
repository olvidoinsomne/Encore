//! Explicit visual groups, independently resolved staff and inter-staff bars.
use super::{INK, Page, StaffRegion, brace, text_width};
use crate::{
    groups::{GroupBarlines, GroupSymbol, StaffGroup},
    layout::DocumentLayout,
    model::Score,
};

pub(super) struct Index<'a> {
    groups: Option<&'a [StaffGroup]>,
    visible: Vec<bool>,
    lanes: Vec<usize>,
    lane_step: f32,
    symbol_width: f32,
    label_width: f32,
}
impl<'a> Index<'a> {
    pub(super) fn new(layout: &'a DocumentLayout, parts: &[usize]) -> Self {
        let groups = layout.groups.as_deref();
        let mut lanes = vec![0; groups.map_or(0, <[StaffGroup]>::len)];
        let mut occupied: Vec<Vec<(usize, usize)>> = vec![];
        let mut order = groups
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, g)| g.symbol != GroupSymbol::None && visible(g, parts))
            .collect::<Vec<_>>();
        order.sort_by_key(|(i, g)| (g.last_staff - g.first_staff, *i));
        for (i, group) in order {
            let lane = occupied
                .iter()
                .position(|ranges| {
                    ranges
                        .iter()
                        .all(|&(a, b)| b < group.first_staff || a > group.last_staff)
                })
                .unwrap_or(occupied.len());
            if lane == occupied.len() {
                occupied.push(vec![]);
            }
            occupied[lane].push((group.first_staff, group.last_staff));
            lanes[i] = lane;
        }
        // Dense nested groups stay inside a bounded label gutter. Their saved
        // manual offsets remain available for intentionally crowded layouts.
        let lane_step = (72.0 / occupied.len().max(1) as f32).min(12.0);
        let symbol_width = if occupied.is_empty() {
            0.0
        } else {
            20.0 + (occupied.len() - 1) as f32 * lane_step
        };
        let label_width = groups
            .into_iter()
            .flatten()
            .filter(|g| g.show_name && visible(g, parts))
            .flat_map(|g| [&g.name, &g.abbreviation])
            .map(|s| text_width(s, 11.0, true))
            .fold(0.0_f32, f32::max)
            .min(110.0);
        Self {
            groups,
            visible: groups
                .into_iter()
                .flatten()
                .map(|g| visible(g, parts))
                .collect(),
            lanes,
            lane_step,
            symbol_width,
            label_width,
        }
    }
    pub(super) fn legacy(&self) -> bool {
        self.groups.is_none()
    }
    pub(super) fn name_padding(&self) -> f32 {
        if self.label_width > 0.0 {
            self.label_width + 12.0
        } else {
            0.0
        }
    }
    pub(super) fn symbol_padding(&self) -> f32 {
        self.symbol_width
    }
    pub(super) fn staff_bars(&self, part: usize) -> bool {
        self.groups.is_none_or(|groups| {
            crate::groups::barline_group(groups, part, part)
                .is_none_or(|g| g.barlines != GroupBarlines::Between)
        })
    }
    pub(super) fn draw(
        &self,
        page: &mut Page,
        score: &Score,
        parts: &[usize],
        measure: usize,
        bars: usize,
        label_x: f32,
    ) {
        let Some(groups) = self.groups else {
            return;
        };
        let staves = parts
            .iter()
            .filter_map(|&p| {
                page.staves
                    .iter()
                    .find(|s| s.part == p && s.measure == measure)
                    .cloned()
            })
            .collect::<Vec<_>>();
        for pair in staves.windows(2) {
            if crate::groups::barline_group(
                groups,
                pair[0].part.min(pair[1].part),
                pair[0].part.max(pair[1].part),
            )
            .is_none_or(|g| g.barlines == GroupBarlines::Separate)
            {
                continue;
            }
            let upper = &pair[0];
            let lower = &pair[1];
            let top = upper.bottom_line_y() + if upper.lines == 1 { 5.0 } else { 0.0 };
            let bottom = lower.top_line_y() - if lower.lines == 1 { 5.0 } else { 0.0 };
            page.line(upper.x, top, upper.x, bottom, 1.0, INK);
            for m in measure..measure + bars {
                let Some(region) = page
                    .staves
                    .iter()
                    .find(|s| s.part == upper.part && s.measure == m)
                    .cloned()
                else {
                    continue;
                };
                if m + 1 == score.measure_count() {
                    page.line(
                        region.x + region.width - 5.0,
                        top,
                        region.x + region.width - 5.0,
                        bottom,
                        0.8,
                        INK,
                    );
                    page.line(
                        region.x + region.width - 1.5,
                        top,
                        region.x + region.width - 1.5,
                        bottom,
                        2.8,
                        INK,
                    );
                } else {
                    page.line(
                        region.x + region.width,
                        top,
                        region.x + region.width,
                        bottom,
                        0.9,
                        INK,
                    );
                }
                for repeat in &score.notation.repeats {
                    if m == repeat.start {
                        repeat_gap(page, &region, top, bottom, true);
                    }
                    if repeat.backward_at(m) {
                        repeat_gap(page, &region, top, bottom, false);
                    }
                }
            }
        }
        let mut label_intervals: Vec<(f32, f32)> = vec![];
        for (index, group) in groups.iter().enumerate() {
            let members = staves
                .iter()
                .filter(|s| group.contains(s.part))
                .collect::<Vec<_>>();
            if members.is_empty() || !self.visible[index] {
                continue;
            }
            let first = members[0];
            let last = members[members.len() - 1];
            let top = first.top_line_y() + group.offset[1];
            let bottom = last.bottom_line_y() + group.offset[2];
            // Top/bottom edits may cross: preserve a useful minimum glyph span.
            let bottom = bottom.max(top + 8.0);
            let x = first.x - 12.0 - self.lanes[index] as f32 * self.lane_step - group.offset[0];
            match group.symbol {
                GroupSymbol::None => {}
                GroupSymbol::Brace => brace(page, x - 5.0, top, bottom),
                GroupSymbol::Line => page.line(x, top, x, bottom, 1.3, INK),
                GroupSymbol::Square => {
                    page.line(x, top, x, bottom, 1.4, INK);
                    page.line(x, top, x + 8.0, top, 1.4, INK);
                    page.line(x, bottom, x + 8.0, bottom, 1.4, INK);
                }
                GroupSymbol::Bracket => {
                    page.line(x, top, x, bottom, 3.0, INK);
                    page.path(
                        vec![
                            (x - 1.5, top),
                            (x + 7.0, top - 3.0),
                            (x + 5.0, top + 1.5),
                            (x + 1.5, top + 3.0),
                        ],
                        true,
                        Some(INK),
                        None,
                        0.0,
                    );
                    page.path(
                        vec![
                            (x - 1.5, bottom),
                            (x + 7.0, bottom + 3.0),
                            (x + 5.0, bottom - 1.5),
                            (x + 1.5, bottom - 3.0),
                        ],
                        true,
                        Some(INK),
                        None,
                        0.0,
                    );
                }
            }
            if group.show_name && self.label_width > 0.0 {
                let text = if measure == 0 || group.abbreviation.trim().is_empty() {
                    &group.name
                } else {
                    &group.abbreviation
                };
                let lines = label_lines(text, self.label_width);
                let height = lines.len() as f32 * 13.0;
                let mut y = (top + bottom - height) * 0.5;
                for &(a, b) in &label_intervals {
                    if y < b + 3.0 && y + height > a - 3.0 {
                        y = b + 3.0;
                    }
                }
                for (row, text) in lines.iter().enumerate() {
                    page.text(
                        label_x + (self.label_width - text_width(text, 11.0, true)).max(0.0),
                        y + 11.0 + row as f32 * 13.0,
                        text,
                        11.0,
                        INK,
                        true,
                    );
                }
                label_intervals.push((y, y + height));
            }
        }
    }
}
fn visible(group: &StaffGroup, parts: &[usize]) -> bool {
    let count = parts.iter().filter(|&&p| group.contains(p)).count();
    count > 1 || count == 1 && group.show_single
}
fn repeat_gap(page: &mut Page, staff: &StaffRegion, top: f32, bottom: f32, forward: bool) {
    let thick = if forward {
        staff.x + 2.0
    } else {
        staff.x + staff.width - 2.0
    };
    let thin = thick + if forward { 5.0 } else { -5.0 };
    page.line(thick, top, thick, bottom, 3.2, INK);
    page.line(thin, top, thin, bottom, 1.1, INK);
}
fn label_lines(text: &str, width: f32) -> Vec<String> {
    let mut lines = vec![String::new()];
    for c in text.chars() {
        let line = lines.last_mut().unwrap();
        let mut candidate = line.clone();
        candidate.push(c);
        if !line.is_empty() && text_width(&candidate, 11.0, true) > width {
            if lines.len() == 4 {
                let line = lines.last_mut().unwrap();
                line.pop();
                line.push('…');
                break;
            }
            lines.push(c.to_string());
        } else {
            line.push(c);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engraving::{EngravingOptions, Primitive, engrave, pdf, svg},
        groups::StaffGroup,
        model::Instrument,
    };
    fn score() -> Score {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(2);
        score.add_part(Instrument::Flute);
        score.notation.layout.measures_per_system = 1;
        score
    }
    fn bar(page: &Page, x: f32, top: f32, bottom: f32, width: f32) -> bool {
        page.items.iter().any(|p| matches!(p, Primitive::Line {x1,y1,x2,y2,width:w,..} if (*x1-x).abs()<0.01 && (*x2-x).abs()<0.01 && (*y1-top).abs()<0.01 && (*y2-bottom).abs()<0.01 && (*w-width).abs()<0.01))
    }
    #[test]
    fn explicit_empty_suppresses_automatic_group_decorations_without_moving_music() {
        let mut score = score();
        let auto = engrave(&score, &EngravingOptions::default());
        score.notation.layout.groups = Some(vec![]);
        let explicit = engrave(&score, &EngravingOptions::default());
        let upper = &auto[0].staves[0];
        let lower = auto[0]
            .staves
            .iter()
            .find(|r| r.part == 1 && r.measure == 0)
            .unwrap();
        assert_eq!(upper.x, explicit[0].staves[0].x);
        assert!(bar(
            &auto[0],
            upper.x - 4.0,
            upper.top_line_y(),
            lower.bottom_line_y(),
            1.3
        ));
        assert!(!bar(
            &explicit[0],
            upper.x - 4.0,
            upper.top_line_y(),
            lower.bottom_line_y(),
            1.3
        ));
    }
    #[test]
    fn through_and_between_use_physical_staff_bounds_for_ordinary_final_and_repeat_bars() {
        let mut score = score();
        score.parts[1].percussion = Some(crate::percussion::Staff {
            lines: 1,
            ..Default::default()
        });
        let mut group = StaffGroup::new("winds".into(), 0, 1);
        group.barlines = GroupBarlines::Through;
        score.notation.layout.groups = Some(vec![group]);
        score
            .notation
            .repeats
            .push(crate::repeats::RepeatRegion::simple(0, 1));
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        let upper = page
            .staves
            .iter()
            .find(|r| r.part == 0 && r.measure == 0)
            .unwrap();
        let lower = page
            .staves
            .iter()
            .find(|r| r.part == 1 && r.measure == 0)
            .unwrap();
        let top = upper.bottom_line_y();
        let bottom = lower.top_line_y() - 5.0;
        assert!(bar(page, upper.x + upper.width, top, bottom, 0.9));
        assert!(bar(page, upper.x + 2.0, top, bottom, 3.2));
        let final_upper = page
            .staves
            .iter()
            .find(|r| r.part == 0 && r.measure == 1)
            .unwrap();
        let final_lower = page
            .staves
            .iter()
            .find(|r| r.part == 1 && r.measure == 1)
            .unwrap();
        assert!(bar(
            page,
            final_upper.x + final_upper.width - 1.5,
            final_upper.bottom_line_y(),
            final_lower.top_line_y() - 5.0,
            2.8
        ));
        score.notation.layout.groups.as_mut().unwrap()[0].barlines = GroupBarlines::Between;
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        let upper = &page.staves[0];
        assert!(!bar(
            page,
            upper.x,
            upper.top_line_y(),
            upper.bottom_line_y(),
            1.0
        ));
        assert!(!bar(
            page,
            upper.x + 2.0,
            upper.top_line_y(),
            upper.bottom_line_y(),
            3.2
        ));
        assert!(
            page.items
                .iter()
                .any(|p| matches!(p,Primitive::Ellipse{rx,ry,..} if *rx==1.7 && *ry==1.7))
        );
    }
    #[test]
    fn names_abbreviations_nested_policies_and_offsets_are_independent() {
        let mut score = score();
        score.add_part(Instrument::Oboe);
        let mut outer = StaffGroup::new("outer".into(), 0, 2);
        outer.name = "Woodwinds".into();
        outer.abbreviation = "Ww.".into();
        outer.barlines = GroupBarlines::Through;
        let mut inner = StaffGroup::new("inner".into(), 1, 2);
        inner.symbol = GroupSymbol::Brace;
        inner.barlines = GroupBarlines::Separate;
        inner.offset = [8.0, 3.0, -2.0];
        inner.show_name = false;
        score.notation.layout.groups = Some(vec![outer, inner]);
        let index = Index::new(&score.notation.layout, &[0, 1, 2]);
        assert!(index.staff_bars(1));
        assert!(index.lanes[0] > index.lanes[1]);
        let pages = engrave(&score, &EngravingOptions::default());
        let all = pages.iter().map(svg).collect::<String>();
        assert!(all.contains("Woodwinds"));
        assert!(all.contains("Ww."));
        assert!(!all.contains("NaN"));
        let page = &pages[0];
        let a = page
            .staves
            .iter()
            .find(|r| r.part == 1 && r.measure == 0)
            .unwrap();
        let b = page
            .staves
            .iter()
            .find(|r| r.part == 2 && r.measure == 0)
            .unwrap();
        assert!(!bar(
            page,
            a.x + a.width,
            a.bottom_line_y(),
            b.top_line_y(),
            0.9
        ));
        assert!(pdf(&pages).starts_with(b"%PDF"));
    }
}
