//! Original vector music engraving. Coordinates are printer points scaled to a
//! comfortable 850 × 1100 page. Musical symbols are constructed here, without a
//! music font or third-party glyph data.
use crate::model::{Clef, Duration, Note, PPQ, Pitch, Score, Selection, Step};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

mod articulations;

const INK: [u8; 3] = [29, 34, 42];
const STAFF: [u8; 3] = [75, 82, 92];
const ACCENT: [u8; 3] = [42, 111, 192];
const PAGE_W: f32 = 850.0;
const PAGE_H: f32 = 1100.0;
const SPACING: f32 = 10.0;

#[derive(Clone, Debug, Default)]
pub struct EngravingOptions {
    pub selected: Option<Selection>,
    /// Absolute position measured in quarter-note beats.
    pub playhead: Option<f32>,
    pub part_filter: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum Primitive {
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: [u8; 3],
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        fill: Option<[u8; 3]>,
        stroke: Option<[u8; 3]>,
        width: f32,
    },
    Path {
        points: Vec<(f32, f32)>,
        closed: bool,
        fill: Option<[u8; 3]>,
        stroke: Option<[u8; 3]>,
        width: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: [u8; 3],
        serif: bool,
    },
}

#[derive(Clone, Debug)]
pub struct StaffRegion {
    pub part: usize,
    pub measure: usize,
    pub x: f32,
    /// Top staff line, in page coordinates.
    pub y: f32,
    pub width: f32,
    pub spacing: f32,
    pub content_x: f32,
    pub content_width: f32,
    // Shared piecewise-linear measure time map: (tick, normalized x position).
    temporal_anchors: Arc<[(f32, f32)]>,
}

impl StaffRegion {
    /// Convert a measure-relative tick to a page x coordinate. All displayed
    /// staves in this measure share the same temporal spacing.
    pub fn x_at_tick(&self, tick: f32) -> f32 {
        let tick = if tick.is_nan() { 0.0 } else { tick };
        self.content_x
            + interpolate_anchors(&self.temporal_anchors, tick, false) * self.content_width
    }

    /// Inverse of `x_at_tick`, clamped to the measure's time bounds.
    pub fn tick_at_x(&self, x: f32) -> f32 {
        let fraction = (x - self.content_x) / self.content_width.max(1.0);
        let fraction = if fraction.is_nan() { 0.0 } else { fraction };
        interpolate_anchors(&self.temporal_anchors, fraction, true)
    }
}

fn interpolate_anchors(anchors: &[(f32, f32)], value: f32, inverse: bool) -> f32 {
    let coordinate = |anchor: &(f32, f32)| if inverse { anchor.1 } else { anchor.0 };
    let result = |anchor: &(f32, f32)| if inverse { anchor.0 } else { anchor.1 };
    let Some(first) = anchors.first() else {
        return 0.0;
    };
    let last = anchors.last().unwrap_or(first);
    if value <= coordinate(first) {
        return result(first);
    }
    if value >= coordinate(last) {
        return result(last);
    }
    let end = anchors.partition_point(|anchor| coordinate(anchor) < value);
    let left = &anchors[end - 1];
    let right = &anchors[end];
    // Round-off from adding/subtracting page coordinates must not move an
    // exact note onset to the preceding input grid cell in the editor.
    let epsilon = if inverse { 0.000_001 } else { 0.000_1 };
    if (value - coordinate(left)).abs() <= epsilon {
        return result(left);
    }
    if (value - coordinate(right)).abs() <= epsilon {
        return result(right);
    }
    let fraction = (value - coordinate(left)) / (coordinate(right) - coordinate(left));
    result(left) + fraction * (result(right) - result(left))
}

/// Onsets, ends, and beats from every visible staff form shared spacing anchors.
/// A minimum glyph allowance plus square-root time gives short-note bursts room
/// while sustained notes consume less space than a linear tick scale would.
fn temporal_spacing(score: &Score, parts: &[usize], measure: usize) -> Arc<[(f32, f32)]> {
    let ticks = score.measure_ticks().max(1);
    let mut times = BTreeSet::from([0, ticks]);
    let beat = (PPQ * 4 / u32::from(score.time_denominator.max(1))).max(1);
    for position in (0..ticks).step_by(beat as usize) {
        times.insert(position);
    }
    for &part in parts {
        if let Some(measure) = score.parts[part].measures.get(measure) {
            for note in &measure.notes {
                times.insert(note.start.min(ticks));
                times.insert(note.start.saturating_add(note.ticks()).min(ticks));
            }
        }
    }
    let mut anchors = Vec::with_capacity(times.len());
    let mut cumulative = 0.0;
    let mut previous = 0;
    for time in times {
        if time > 0 {
            let quarters = (time - previous) as f32 / PPQ as f32;
            cumulative += 16.0 + 12.0 * quarters.sqrt();
        }
        anchors.push((time as f32, cumulative));
        previous = time;
    }
    let total = cumulative.max(1.0);
    for (_, position) in &mut anchors {
        *position /= total;
    }
    anchors.into()
}

#[derive(Clone, Debug)]
pub struct NoteHit {
    pub selection: Selection,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub items: Vec<Primitive>,
    pub staves: Vec<StaffRegion>,
    pub hits: Vec<NoteHit>,
}

impl Page {
    fn new() -> Self {
        Self {
            width: PAGE_W,
            height: PAGE_H,
            items: vec![],
            staves: vec![],
            hits: vec![],
        }
    }
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: [u8; 3]) {
        self.items.push(Primitive::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            color,
        });
    }
    fn path(
        &mut self,
        points: Vec<(f32, f32)>,
        closed: bool,
        fill: Option<[u8; 3]>,
        stroke: Option<[u8; 3]>,
        width: f32,
    ) {
        self.items.push(Primitive::Path {
            points,
            closed,
            fill,
            stroke,
            width,
        });
    }
    #[allow(clippy::too_many_arguments)]
    fn ellipse(
        &mut self,
        x: f32,
        y: f32,
        rx: f32,
        ry: f32,
        fill: Option<[u8; 3]>,
        stroke: Option<[u8; 3]>,
        width: f32,
    ) {
        self.items.push(Primitive::Ellipse {
            cx: x,
            cy: y,
            rx,
            ry,
            fill,
            stroke,
            width,
        });
    }
    fn text(
        &mut self,
        x: f32,
        y: f32,
        text: impl Into<String>,
        size: f32,
        color: [u8; 3],
        serif: bool,
    ) {
        self.items.push(Primitive::Text {
            x,
            y,
            text: text.into(),
            size,
            color,
            serif,
        });
    }
}

fn bottom_step(clef: Clef) -> i32 {
    match clef {
        Clef::Treble => 30,
        Clef::Bass => 18,
        Clef::Alto => 24,
    }
}
fn pitch_y(pitch: &Pitch, clef: Clef, y: f32) -> f32 {
    y + 40.0 - (pitch.staff_step() - bottom_step(clef)) as f32 * 5.0
}
fn centered_text(page: &mut Page, center: f32, y: f32, text: &str, size: f32, serif: bool) {
    let face = text_face(serif);
    let scale = size / face.units_per_em() as f32;
    let width = text
        .chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .unwrap_or(face.units_per_em() / 2) as f32
                * scale
        })
        .sum::<f32>();
    page.text(center - width * 0.5, y, text, size, INK, serif);
}

/// Lay out the complete concert-pitch score, or one selected part. Large
/// ensembles are divided into readable staff groups on consecutive pages.
pub fn engrave(score: &Score, options: &EngravingOptions) -> Vec<Page> {
    let parts: Vec<usize> = match options.part_filter {
        Some(i) if i < score.parts.len() => vec![i],
        Some(_) => vec![],
        None => (0..score.parts.len()).collect(),
    };
    let mut pages = vec![Page::new()];
    let title = if score.title.trim().is_empty() {
        "Untitled score"
    } else {
        &score.title
    };
    centered_text(&mut pages[0], PAGE_W * 0.5, 75.0, title, 28.0, true);
    if !score.composer.trim().is_empty() {
        let face = text_face(true);
        let width = score
            .composer
            .chars()
            .map(|c| {
                face.glyph_index(c)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .unwrap_or(512) as f32
            })
            .sum::<f32>()
            * 13.0
            / face.units_per_em() as f32;
        let cx = 802.0 - width;
        pages[0].text(cx.max(440.0), 111.0, &score.composer, 13.0, INK, true);
    }
    pages[0].text(49.0, 113.0, "ENCORE", 9.0, [116, 126, 140], false);
    if parts.is_empty() {
        centered_text(
            &mut pages[0],
            PAGE_W * 0.5,
            240.0,
            "Add an instrument to begin your score",
            15.0,
            true,
        );
        return pages;
    }
    let count = score.measure_count().max(1);
    let ticks = score.measure_ticks().max(1);
    let measure_spacing = (0..count)
        .map(|measure| temporal_spacing(score, &parts, measure))
        .collect::<Vec<_>>();
    let key = score.key_fifths.clamp(-7, 7);
    let left = if parts.len() == 1 { 94.0 } else { 133.0 };
    let right = 802.0;
    let available = right - left;
    let group_size = 8;
    let mut cursor_y = 180.0;
    let mut measure = 0;
    while measure < count {
        // Give dense passages more horizontal room instead of squeezing glyphs.
        let mut bars = (count - measure).min(4);
        while bars > 1 {
            let header =
                49.0 + key.unsigned_abs() as f32 * 7.0 + if measure == 0 { 26.0 } else { 0.0 };
            let cw = (available - header) / bars as f32 - 20.0;
            let dense = (measure..measure + bars)
                .map(|m| {
                    parts
                        .iter()
                        .map(|&p| score.parts[p].measures.get(m).map_or(0, |v| v.notes.len()))
                        .max()
                        .unwrap_or(0)
                })
                .max()
                .unwrap_or(0);
            if dense as f32 * 15.0 <= cw {
                break;
            }
            bars -= 1;
        }
        let mut chunk_start = 0;
        while chunk_start < parts.len() {
            let mut chunk_end = (chunk_start + group_size).min(parts.len());
            let (staff_offsets, system_height) = loop {
                let result = system_layout(score, &parts[chunk_start..chunk_end], measure, bars);
                if result.1 <= 910.0 || chunk_end == chunk_start + 1 {
                    break result;
                }
                chunk_end -= 1;
            };
            let chunk = &parts[chunk_start..chunk_end];
            if cursor_y + system_height > 1031.0 {
                pages.push(Page::new());
                let last = pages.len() - 1;
                pages[last].text(49.0, 45.0, title, 12.0, [93, 103, 117], true);
                cursor_y = 95.0;
            }
            let page = pages.last_mut().unwrap();
            if measure == 0 {
                // A quarter note metronome indication, also independent of fonts.
                note_head(page, left + 5.0, cursor_y - 37.0, false, false, INK);
                page.line(
                    left + 10.0,
                    cursor_y - 37.0,
                    left + 10.0,
                    cursor_y - 59.0,
                    1.4,
                    INK,
                );
                page.text(
                    left + 19.0,
                    cursor_y - 35.0,
                    format!("= {}", score.tempo),
                    13.0,
                    INK,
                    true,
                );
            }
            let first_y = cursor_y + staff_offsets[0];
            let last_y = cursor_y + staff_offsets[chunk.len() - 1] + 40.0;
            if chunk.len() > 1 {
                page.line(left - 4.0, first_y, left - 4.0, last_y, 1.3, INK);
                if chunk.len() == 2
                    && score.parts[chunk[0]].instrument == crate::model::Instrument::Piano
                    && score.parts[chunk[1]].instrument == crate::model::Instrument::Piano
                {
                    brace(page, left - 14.0, first_y, last_y);
                } else {
                    page.line(left - 9.0, first_y, left - 9.0, last_y, 3.0, INK);
                    page.line(left - 9.0, first_y, left - 2.0, first_y, 2.4, INK);
                    page.line(left - 9.0, last_y, left - 2.0, last_y, 2.4, INK);
                }
            }
            if chunk.len() > 2 {
                for pair in 0..chunk.len() - 1 {
                    if score.parts[chunk[pair]].instrument == crate::model::Instrument::Piano
                        && score.parts[chunk[pair + 1]].instrument
                            == crate::model::Instrument::Piano
                        && score.parts[chunk[pair]].clef == Clef::Treble
                        && score.parts[chunk[pair + 1]].clef == Clef::Bass
                    {
                        brace(
                            page,
                            left - 17.0,
                            cursor_y + staff_offsets[pair],
                            cursor_y + staff_offsets[pair + 1] + 40.0,
                        );
                    }
                }
            }
            let header =
                49.0 + key.unsigned_abs() as f32 * 7.0 + if measure == 0 { 26.0 } else { 0.0 };
            let common = (available - header) / bars as f32;
            for (staff, &part_index) in chunk.iter().enumerate() {
                let part = &score.parts[part_index];
                let y = cursor_y + staff_offsets[staff];
                // Wrapping labels avoids running through the first barline.
                let name = if part.name.trim().is_empty() {
                    part.instrument.name()
                } else {
                    &part.name
                };
                let max_chars = if parts.len() == 1 { 8 } else { 14 };
                let (name1, name2) = wrap_label(name, max_chars);
                page.text(
                    47.0,
                    y + if name2.is_empty() { 24.0 } else { 17.0 },
                    name1,
                    11.0,
                    INK,
                    true,
                );
                if !name2.is_empty() {
                    page.text(47.0, y + 31.0, name2, 11.0, INK, true);
                }
                for line in 0..5 {
                    page.line(
                        left,
                        y + line as f32 * SPACING,
                        right,
                        y + line as f32 * SPACING,
                        0.7,
                        STAFF,
                    );
                }
                page.line(left, y, left, y + 40.0, 1.0, INK);
                clef(page, left + 17.0, y, part.clef);
                key_signature(page, left + 38.0, y, part.clef, key);
                if measure == 0 {
                    time_signature(
                        page,
                        left + 42.0 + key.unsigned_abs() as f32 * 7.0,
                        y,
                        score.time_numerator,
                        score.time_denominator,
                    );
                }
                let mut x = left;
                for offset in 0..bars {
                    let m = measure + offset;
                    let width = common + if offset == 0 { header } else { 0.0 };
                    let content_x = x + if offset == 0 { header } else { 13.0 };
                    let content_width = (width - (content_x - x) - 12.0).max(20.0);
                    let region = StaffRegion {
                        part: part_index,
                        measure: m,
                        x,
                        y,
                        width,
                        spacing: SPACING,
                        content_x,
                        content_width,
                        temporal_anchors: measure_spacing[m].clone(),
                    };
                    if staff == 0 {
                        page.text(
                            x + if offset == 0 { 3.0 } else { 5.0 },
                            y - 12.0,
                            (m + 1).to_string(),
                            10.0,
                            [106, 115, 128],
                            false,
                        );
                    }
                    draw_measure(page, score, &region, options);
                    page.staves.push(region);
                    if m + 1 == count {
                        page.line(x + width - 5.0, y, x + width - 5.0, y + 40.0, 0.8, INK);
                        page.line(x + width - 1.5, y, x + width - 1.5, y + 40.0, 2.8, INK);
                    } else {
                        page.line(x + width, y, x + width, y + 40.0, 0.9, INK);
                    }
                    x += width;
                }
            }
            if let Some(beat) = options.playhead {
                let absolute = beat * PPQ as f32;
                let first = measure as f32 * ticks as f32;
                let end = (measure + bars) as f32 * ticks as f32;
                if absolute >= first && absolute < end {
                    let m = (absolute / ticks as f32).floor() as usize;
                    let within_measure = absolute % ticks as f32;
                    if let Some(region) = page
                        .staves
                        .iter()
                        .find(|r| r.measure == m && r.part == chunk[0])
                    {
                        let x = region.x_at_tick(within_measure);
                        page.line(x, first_y - 8.0, x, last_y + 8.0, 2.0, [215, 113, 38]);
                    }
                }
            }
            cursor_y += system_height + 31.0;
            chunk_start = chunk_end;
        }
        measure += bars;
    }
    let page_count = pages.len();
    for (i, page) in pages.iter_mut().enumerate() {
        centered_text(
            page,
            PAGE_W * 0.5,
            1066.0,
            &format!("{} / {}", i + 1, page_count),
            10.0,
            false,
        );
    }
    pages
}

// Ledger lines and stems must have room between neighboring instruments. A
// high flute or very low bass therefore expands its own staff, not the glyphs.
fn system_layout(score: &Score, parts: &[usize], measure: usize, bars: usize) -> (Vec<f32>, f32) {
    let mut extents = Vec::with_capacity(parts.len());
    for &p in parts {
        let part = &score.parts[p];
        let (mut above, mut below) = (25.0_f32, 66.0_f32);
        for m in measure..measure + bars {
            if let Some(bar) = part.measures.get(m) {
                let multiple_layers = bar
                    .notes
                    .first()
                    .is_some_and(|first| bar.notes.iter().any(|note| note.layer != first.layer));
                if bar
                    .notes
                    .iter()
                    .any(|note| !note.pitches.is_empty() && note.velocity > 0)
                {
                    below = below.max(
                        staff_dynamic_baseline(&bar.notes, part.clef, 0.0, multiple_layers) + 7.0,
                    );
                }
                for note in &bar.notes {
                    if note.pitches.is_empty() {
                        continue;
                    }
                    let ys: Vec<f32> = note
                        .pitches
                        .iter()
                        .map(|p| pitch_y(p, part.clef, 0.0))
                        .collect();
                    let up = if multiple_layers {
                        note.layer.is_multiple_of(2)
                    } else {
                        ys.iter().sum::<f32>() / ys.len() as f32 >= 20.0
                    };
                    let stem = note.duration.has_stem();
                    if let Some(clearance) = articulation_clearance(note) {
                        if up {
                            let bottom = ys.iter().copied().fold(40.0, f32::max);
                            below = below.max(bottom + clearance);
                        } else {
                            let top = ys.iter().copied().fold(0.0, f32::min);
                            above = above.max(-top + clearance);
                        }
                    }
                    for y in ys {
                        above = above.max(
                            -y + if up && stem {
                                stem_length(note.duration) + 4.0
                            } else {
                                9.0
                            } + if note.tuplet.is_some() { 20.0 } else { 0.0 },
                        );
                        below = below.max(
                            y + if !up && stem {
                                stem_length(note.duration) + 4.0
                            } else {
                                11.0
                            } + if note.tuplet.is_some() { 20.0 } else { 0.0 },
                        );
                        if note.velocity > 0 {
                            below = below
                                .max(65.0_f32.max(y + if !up && stem { 50.0 } else { 25.0 }) + 7.0);
                        }
                    }
                }
            }
        }
        extents.push((above, below));
    }
    let mut offsets = Vec::with_capacity(parts.len());
    offsets.push((extents[0].0 - 45.0).max(0.0));
    for i in 1..parts.len() {
        offsets.push(offsets[i - 1] + extents[i - 1].1 + extents[i].0 + 5.0);
    }
    let height = offsets[parts.len() - 1] + extents[parts.len() - 1].1 + 36.0;
    (offsets, height)
}

fn wrap_label(text: &str, limit: usize) -> (String, String) {
    if text.chars().count() <= limit {
        return (text.into(), String::new());
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 1 {
        let mut first = String::new();
        let mut second = String::new();
        for word in words {
            if first.len() + word.len() < limit && second.is_empty() {
                if !first.is_empty() {
                    first.push(' ');
                }
                first.push_str(word);
            } else {
                if !second.is_empty() {
                    second.push(' ');
                }
                second.push_str(word);
            }
        }
        (first, second.chars().take(limit + 2).collect())
    } else {
        (
            text.chars().take(limit).collect(),
            text.chars().skip(limit).take(limit).collect(),
        )
    }
}

#[derive(Clone)]
struct NoteLayout {
    x: f32,
    top: f32,
    bottom: f32,
    mid: f32,
    up: bool,
    duration: Duration,
    start: u32,
    end: u32,
}

fn draw_measure(page: &mut Page, score: &Score, region: &StaffRegion, options: &EngravingOptions) {
    let Some(measure) = score.parts[region.part].measures.get(region.measure) else {
        return;
    };
    if measure.notes.is_empty() {
        whole_rest(
            page,
            region.content_x + region.content_width * 0.5,
            region.y + 10.0,
        );
        return;
    }
    let layers = (0..4)
        .filter(|layer| measure.notes.iter().any(|note| note.layer == *layer))
        .collect::<Vec<_>>();
    for &layer in &layers {
        draw_measure_layer(page, score, region, options, layer, layers.len() > 1);
    }
    let part = &score.parts[region.part];
    let mut previous = part
        .measures
        .iter()
        .take(region.measure)
        .rev()
        .find_map(|measure| {
            dynamic_onsets(&measure.notes)
                .last_key_value()
                .map(|(_, note)| crate::model::dynamic_name(note.velocity))
        });
    let baseline = staff_dynamic_baseline(&measure.notes, part.clef, region.y, layers.len() > 1);
    for (start, mark) in staff_dynamic_changes(&measure.notes, &mut previous) {
        let x = region.x_at_tick(start as f32);
        dynamic(page, x - 4.0, baseline, mark);
    }
}

fn draw_measure_layer(
    page: &mut Page,
    score: &Score,
    region: &StaffRegion,
    options: &EngravingOptions,
    layer: u8,
    multiple_layers: bool,
) {
    let part = &score.parts[region.part];
    let notes = part
        .measures
        .get(region.measure)
        .map(|m| m.notes.as_slice())
        .unwrap_or(&[]);
    let ticks = score.measure_ticks().max(1);
    if notes.is_empty() {
        whole_rest(
            page,
            region.content_x + region.content_width * 0.5,
            region.y + 10.0,
        );
        return;
    }
    let mut rest_region = region.clone();
    if multiple_layers {
        rest_region.y += match layer {
            0 => -10.0,
            1 => 10.0,
            2 => -20.0,
            _ => 20.0,
        };
    }
    let mut order: Vec<usize> = (0..notes.len())
        .filter(|index| notes[*index].layer == layer)
        .collect();
    order.sort_by_key(|&n| notes[n].start);
    let mut layouts = vec![];
    let mut accidental_state: BTreeMap<(i32, i8), i8> = BTreeMap::new();
    let mut previous_end = 0;
    for &n in &order {
        let note = &notes[n];
        if note.start > previous_end {
            gap_rests(page, &rest_region, previous_end, note.start.min(ticks));
        }
        let x = region.x_at_tick(note.start as f32);
        let selection = Selection {
            part: region.part,
            measure: region.measure,
            note: n,
        };
        let ys: Vec<f32> = note
            .pitches
            .iter()
            .map(|p| pitch_y(p, part.clef, region.y))
            .collect();
        let mid = if ys.is_empty() {
            region.y + 20.0
        } else {
            ys.iter().sum::<f32>() / ys.len() as f32
        };
        page.hits.push(NoteHit {
            selection,
            x,
            y: mid,
        });
        if options.selected == Some(selection) {
            page.ellipse(
                x,
                mid,
                11.0,
                ((ys.iter().copied().fold(mid, f32::max) - ys.iter().copied().fold(mid, f32::min))
                    * 0.5
                    + 10.0)
                    .max(11.0),
                Some([222, 237, 255]),
                Some(ACCENT),
                1.2,
            );
        }
        if note.pitches.is_empty() {
            rest(page, x, rest_region.y, note.duration, note.dotted);
        } else {
            let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
            let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let up = if multiple_layers {
                layer.is_multiple_of(2)
            } else {
                mid >= region.y + 20.0
            };
            let mut sorted: Vec<usize> = (0..note.pitches.len()).collect();
            sorted.sort_by_key(|&i| note.pitches[i].staff_step());
            let mut last_step = None;
            let mut displaced = false;
            for &i in &sorted {
                let pitch = &note.pitches[i];
                let py = ys[i];
                if last_step.is_some_and(|s: i32| (pitch.staff_step() - s).abs() == 1) {
                    displaced = !displaced;
                } else {
                    displaced = false;
                }
                let hx = x + if displaced {
                    if up { 10.0 } else { -10.0 }
                } else {
                    0.0
                };
                ledger_lines(page, hx, py, region.y);
                let expected = *accidental_state
                    .get(&(pitch.staff_step(), pitch.octave))
                    .unwrap_or(&key_alter(pitch.step, score.key_fifths));
                if expected != pitch.alter {
                    accidental(page, x - 14.0 - (i % 2) as f32 * 3.0, py, pitch.alter);
                    accidental_state.insert((pitch.staff_step(), pitch.octave), pitch.alter);
                }
                note_head(
                    page,
                    hx,
                    py,
                    matches!(
                        note.duration,
                        Duration::DoubleWhole | Duration::Whole | Duration::Half
                    ),
                    !note.duration.has_stem(),
                    INK,
                );
                if note.duration == Duration::DoubleWhole {
                    for dx in [-12.0, -9.0, 9.0, 12.0] {
                        page.line(hx + dx, py - 7.0, hx + dx, py + 7.0, 1.2, INK);
                    }
                }
                if note.dotted {
                    page.ellipse(
                        hx + 11.0,
                        if ((py - region.y) / 5.0).round() as i32 % 2 == 0 {
                            py - 5.0
                        } else {
                            py
                        },
                        1.7,
                        1.7,
                        Some(INK),
                        None,
                        0.0,
                    );
                }
                last_step = Some(pitch.staff_step());
            }
            articulations::draw(
                page,
                note,
                x,
                articulations::Placement {
                    staff_top: region.y,
                    staff_bottom: region.y + 40.0,
                    top_note_y: top,
                    bottom_note_y: bottom,
                    stem_up: up,
                },
            );
            let tie_y = if up { bottom + 7.0 } else { top - 7.0 };
            if note.tied {
                draw_tie(page, score, region, note, x, tie_y, up);
            }
            if note.start == 0 && region.measure > 0 {
                let incoming = part.measures.get(region.measure - 1).is_some_and(|m| {
                    m.notes.iter().any(|p| {
                        p.tied
                            && p.layer == layer
                            && p.start.saturating_add(p.ticks()) == ticks
                            && p.pitches.iter().any(|pitch| note.pitches.contains(pitch))
                    })
                });
                if incoming {
                    tie_arc(page, (x - 24.0).max(region.x + 2.0), x - 4.0, tie_y, up);
                }
            }
            layouts.push(NoteLayout {
                x,
                top,
                bottom,
                mid,
                up,
                duration: note.duration,
                start: note.start,
                end: note.start.saturating_add(note.ticks()),
            });
        }
        previous_end = previous_end.max(note.start.saturating_add(note.ticks()));
    }
    if previous_end < ticks {
        gap_rests(page, &rest_region, previous_end, ticks);
    }
    // Beam within beats. A rest or duration boundary always breaks a group.
    let beat = if score.time_denominator == 8
        && score.time_numerator >= 6
        && score.time_numerator.is_multiple_of(3)
    {
        PPQ * 3 / 2
    } else {
        PPQ
    };
    let mut i = 0;
    while i < layouts.len() {
        if layouts[i].duration.flags() > 0 {
            let mut end = i + 1;
            while end < layouts.len()
                && layouts[end].duration.flags() > 0
                && layouts[end - 1].end == layouts[end].start
                && layouts[end].start / beat == layouts[i].start / beat
            {
                end += 1;
            }
            if end - i > 1 {
                beam_group(page, &layouts[i..end]);
            } else {
                stem_flag(page, &layouts[i]);
            }
            i = end;
        } else {
            stem_flag(page, &layouts[i]);
            i += 1;
        }
    }
    let layer_notes = notes
        .iter()
        .filter(|note| note.layer == layer)
        .collect::<Vec<_>>();
    draw_tuplets(
        page,
        region,
        part.clef,
        &layer_notes,
        multiple_layers && !layer.is_multiple_of(2),
    );
}

/// Tuplet ratios are event metadata. Consecutive equal ratios form groups of
/// `actual` written notes; rests remain inside the visible bracket.
fn draw_tuplets(page: &mut Page, region: &StaffRegion, clef: Clef, notes: &[&Note], below: bool) {
    let mut index = 0;
    while index < notes.len() {
        let Some(ratio) = notes[index].tuplet else {
            index += 1;
            continue;
        };
        let first = index;
        index += 1;
        while index < notes.len()
            && index - first < usize::from(ratio.actual)
            && notes[index].tuplet == Some(ratio)
            && notes[index - 1].start + notes[index - 1].ticks() == notes[index].start
        {
            index += 1;
        }
        let first_note = &notes[first];
        let last_note = &notes[index - 1];
        let x1 = region.x_at_tick(first_note.start as f32) - 7.0;
        let last_x = region.x_at_tick(last_note.start as f32);
        let x2 = (last_x + 10.0).max(x1 + 28.0);
        let highest = notes[first..index]
            .iter()
            .flat_map(|note| note.pitches.iter())
            .map(|pitch| pitch_y(pitch, clef, region.y))
            .fold(region.y, f32::min);
        let max_flags = notes[first..index]
            .iter()
            .map(|note| note.duration.flags())
            .max()
            .unwrap_or(0);
        let lowest = notes[first..index]
            .iter()
            .flat_map(|note| note.pitches.iter())
            .map(|pitch| pitch_y(pitch, clef, region.y))
            .fold(region.y + 40.0, f32::max);
        let y = if below {
            lowest + 46.0 + max_flags.saturating_sub(2) as f32 * 7.0
        } else {
            highest - 46.0 - max_flags.saturating_sub(2) as f32 * 7.0
        };
        let label = if ratio == crate::model::Tuplet::TRIPLET {
            "3".to_string()
        } else {
            format!("{}:{}", ratio.actual, ratio.normal)
        };
        let center = (x1 + x2) / 2.0;
        let gap = label.len() as f32 * 3.2 + 3.0;
        page.line(x1, y + if below { -6.0 } else { 6.0 }, x1, y, 0.9, INK);
        page.line(x1, y, center - gap, y, 0.9, INK);
        page.line(center + gap, y, x2, y, 0.9, INK);
        page.line(x2, y, x2, y + if below { -6.0 } else { 6.0 }, 0.9, INK);
        centered_text(page, center, y + 3.5, &label, 11.0, false);
    }
}

/// One automatic staff-level mark per onset. The loudest simultaneous attack
/// represents the staff; individual layer velocities still control playback.
fn dynamic_onsets(notes: &[Note]) -> BTreeMap<u32, &Note> {
    let mut onsets: BTreeMap<u32, &Note> = BTreeMap::new();
    for note in notes
        .iter()
        .filter(|note| !note.pitches.is_empty() && note.velocity > 0)
    {
        onsets
            .entry(note.start)
            .and_modify(|prior| {
                if note.velocity > prior.velocity
                    || (note.velocity == prior.velocity && note.layer < prior.layer)
                {
                    *prior = note;
                }
            })
            .or_insert(note);
    }
    onsets
}

fn staff_dynamic_changes(
    notes: &[Note],
    previous: &mut Option<&'static str>,
) -> Vec<(u32, &'static str)> {
    dynamic_onsets(notes)
        .into_iter()
        .filter_map(|(start, note)| dynamic_change(previous, note).map(|mark| (start, mark)))
        .collect()
}

fn articulation_clearance(note: &Note) -> Option<f32> {
    if note.articulations.is_empty() {
        return note.staccato.then_some(14.0);
    }
    let count = crate::model::Articulation::ALL
        .iter()
        .filter(|mark| note.articulations.contains(mark))
        .count();
    let staccato_dot = note.staccato
        && !note
            .articulations
            .contains(&crate::model::Articulation::Staccatissimo);
    Some(16.0 + count.saturating_sub(1) as f32 * 11.0 + if staccato_dot { 9.0 } else { 0.0 })
}

fn staff_dynamic_baseline(notes: &[Note], clef: Clef, y: f32, multiple_layers: bool) -> f32 {
    notes
        .iter()
        .filter(|note| !note.pitches.is_empty())
        .map(|note| {
            let (bottom, sum) = note
                .pitches
                .iter()
                .map(|pitch| pitch_y(pitch, clef, y))
                .fold((f32::NEG_INFINITY, 0.0), |(bottom, sum), position| {
                    (bottom.max(position), sum + position)
                });
            let up = if multiple_layers {
                note.layer.is_multiple_of(2)
            } else {
                sum / note.pitches.len() as f32 >= y + 20.0
            };
            let stem_clearance = if !up && note.duration.has_stem() {
                stem_length(note.duration) + 19.0
            } else {
                25.0
            };
            let tuplet_clearance = if !up && multiple_layers && note.tuplet.is_some() {
                64.0 + note.duration.flags().saturating_sub(2) as f32 * 7.0
            } else {
                0.0
            };
            let baseline = bottom + stem_clearance.max(tuplet_clearance);
            if up && let Some(clearance) = articulation_clearance(note) {
                baseline.max(bottom.max(y + 40.0) + clearance + 18.0)
            } else {
                baseline
            }
        })
        .fold(y + 65.0, f32::max)
}

fn dynamic_change(previous: &mut Option<&'static str>, note: &Note) -> Option<&'static str> {
    if note.pitches.is_empty() || note.velocity == 0 {
        return None;
    }
    let mark = crate::model::dynamic_name(note.velocity);
    if *previous == Some(mark) {
        return None;
    }
    *previous = Some(mark);
    Some(mark)
}

// Original italic music-dynamic lettering, independent of text fonts.
fn dynamic(page: &mut Page, mut x: f32, y: f32, label: &str) {
    let origin = x;
    let first = page.items.len();
    for letter in label.chars() {
        match letter {
            'p' => {
                page.line(x + 3.0, y - 10.0, x - 1.0, y + 6.0, 1.7, INK);
                let mut p = vec![(x + 2.0, y - 6.0)];
                cubic(
                    &mut p,
                    (x + 2.0, y - 6.0),
                    (x + 6.0, y - 14.0),
                    (x + 13.0, y - 11.0),
                    (x + 10.0, y - 4.0),
                    14,
                );
                cubic(
                    &mut p,
                    (x + 10.0, y - 4.0),
                    (x + 8.0, y + 1.0),
                    (x + 3.0, y + 1.0),
                    (x + 2.0, y - 3.0),
                    14,
                );
                page.path(p, false, None, Some(INK), 1.7);
                x += 13.0;
            }
            'f' => {
                let mut p = vec![(x - 1.0, y + 5.0)];
                cubic(
                    &mut p,
                    (x - 1.0, y + 5.0),
                    (x + 8.0, y + 9.0),
                    (x + 4.0, y - 14.0),
                    (x + 11.0, y - 14.0),
                    20,
                );
                cubic(
                    &mut p,
                    (x + 11.0, y - 14.0),
                    (x + 15.0, y - 14.0),
                    (x + 13.0, y - 9.0),
                    (x + 10.0, y - 11.0),
                    12,
                );
                page.path(p, false, None, Some(INK), 1.9);
                page.line(x + 1.0, y - 7.0, x + 12.0, y - 7.0, 1.5, INK);
                x += 12.0;
            }
            'm' => {
                page.line(x + 2.0, y - 10.0, x, y, 1.6, INK);
                let mut p = vec![(x + 1.0, y - 5.0)];
                cubic(
                    &mut p,
                    (x + 1.0, y - 5.0),
                    (x + 7.0, y - 15.0),
                    (x + 9.0, y - 10.0),
                    (x + 6.0, y),
                    16,
                );
                cubic(
                    &mut p,
                    (x + 6.0, y),
                    (x + 10.0, y - 12.0),
                    (x + 16.0, y - 14.0),
                    (x + 13.0, y - 2.0),
                    16,
                );
                cubic(
                    &mut p,
                    (x + 13.0, y - 2.0),
                    (x + 12.0, y + 1.0),
                    (x + 15.0, y),
                    (x + 17.0, y - 2.0),
                    10,
                );
                page.path(p, false, None, Some(INK), 1.6);
                x += 19.0;
            }
            _ => {}
        }
    }
    // Compact dynamics fit adjacent eighth notes while retaining italic shapes.
    for item in &mut page.items[first..] {
        match item {
            Primitive::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                ..
            } => {
                *x1 = origin + (*x1 - origin) * 0.52;
                *x2 = origin + (*x2 - origin) * 0.52;
                *y1 = y + (*y1 - y) * 0.70;
                *y2 = y + (*y2 - y) * 0.70;
                *width *= 0.7;
            }
            Primitive::Path { points, width, .. } => {
                for (px, py) in points {
                    *px = origin + (*px - origin) * 0.52;
                    *py = y + (*py - y) * 0.70;
                }
                *width *= 0.7;
            }
            _ => {}
        }
    }
}

fn key_alter(step: Step, fifths: i8) -> i8 {
    let fifths = fifths.clamp(-7, 7);
    let sharps = [
        Step::F,
        Step::C,
        Step::G,
        Step::D,
        Step::A,
        Step::E,
        Step::B,
    ];
    let flats = [
        Step::B,
        Step::E,
        Step::A,
        Step::D,
        Step::G,
        Step::C,
        Step::F,
    ];
    if fifths > 0 && sharps[..fifths.min(7) as usize].contains(&step) {
        1
    } else if fifths < 0 && flats[..(-fifths).min(7) as usize].contains(&step) {
        -1
    } else {
        0
    }
}

fn gap_rests(page: &mut Page, r: &StaffRegion, mut from: u32, to: u32) {
    while from < to {
        let remaining = to - from;
        let Some(duration) = Duration::ALL
            .iter()
            .copied()
            .find(|d| d.ticks() <= remaining && (from == 0 || from.is_multiple_of(d.ticks())))
        else {
            // An unfinished tuplet can leave a fractional ordinary-rest gap.
            // Do not draw a conventional rest with an incorrect duration.
            break;
        };
        let x = r.x_at_tick(from as f32);
        rest(page, x, r.y, duration, false);
        from = from.saturating_add(duration.ticks());
    }
}

fn note_head(page: &mut Page, x: f32, y: f32, open: bool, whole: bool, color: [u8; 3]) {
    let (rx, ry, angle) = if whole {
        (7.0, 4.2, 0.1_f32)
    } else {
        (6.0, 4.1, -0.35_f32)
    };
    let points = (0..32)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / 32.0;
            let u = a.cos() * rx;
            let v = a.sin() * ry;
            (
                x + u * angle.cos() - v * angle.sin(),
                y + u * angle.sin() + v * angle.cos(),
            )
        })
        .collect();
    page.path(
        points,
        true,
        Some(if open { [255, 255, 255] } else { color }),
        Some(color),
        if open { 1.6 } else { 0.6 },
    );
}
fn ledger_lines(page: &mut Page, x: f32, y: f32, top: f32) {
    let mut ly = top - 10.0;
    while y <= ly + 1.0 {
        page.line(x - 9.0, ly, x + 9.0, ly, 1.0, INK);
        ly -= 10.0;
    }
    let mut ly = top + 50.0;
    while y >= ly - 1.0 {
        page.line(x - 9.0, ly, x + 9.0, ly, 1.0, INK);
        ly += 10.0;
    }
}
fn stem_flag(page: &mut Page, n: &NoteLayout) {
    if !n.duration.has_stem() {
        return;
    }
    let x = n.x + if n.up { 5.0 } else { -5.0 };
    let y0 = if n.up { n.bottom } else { n.top };
    let y1 = if n.up {
        n.top - stem_length(n.duration)
    } else {
        n.bottom + stem_length(n.duration)
    };
    page.line(x, y0, x, y1, 1.25, INK);
    let flags = n.duration.flags();
    for f in 0..flags {
        let direction = if n.up { 1.0 } else { -1.0 };
        let y = y1 + direction * f as f32 * 7.0;
        let mut p = vec![(x, y)];
        cubic(
            &mut p,
            (x, y),
            (x + 12.0, y + direction * 5.0),
            (x + 12.0, y + direction * 17.0),
            (x + 5.0, y + direction * 22.0),
            16,
        );
        cubic(
            &mut p,
            (x + 5.0, y + direction * 22.0),
            (x + 9.0, y + direction * 14.0),
            (x + 5.0, y + direction * 9.0),
            (x, y + direction * 7.0),
            16,
        );
        page.path(p, true, Some(INK), None, 0.0);
    }
}
fn stem_length(duration: Duration) -> f32 {
    31.0 + duration.flags().saturating_sub(2) as f32 * 7.0
}
fn beam_group(page: &mut Page, notes: &[NoteLayout]) {
    let stem_length = notes
        .iter()
        .map(|note| stem_length(note.duration))
        .fold(31.0, f32::max);
    let up = notes.iter().filter(|n| n.up).count() * 2 >= notes.len();
    let first = &notes[0];
    let last = &notes[notes.len() - 1];
    let x1 = first.x + if up { 5.0 } else { -5.0 };
    let x2 = last.x + if up { 5.0 } else { -5.0 };
    let slope = ((last.mid - first.mid) * 0.22).clamp(-6.0, 6.0);
    let mut y1 = if up {
        notes.iter().map(|n| n.top).fold(f32::INFINITY, f32::min) - stem_length
    } else {
        notes
            .iter()
            .map(|n| n.bottom)
            .fold(f32::NEG_INFINITY, f32::max)
            + stem_length
    };
    if (up && slope > 0.0) || (!up && slope < 0.0) {
        y1 -= slope;
    }
    let beam_y = |x: f32| y1 + (x - x1) / (x2 - x1).max(1.0) * slope;
    for n in notes {
        let x = n.x + if up { 5.0 } else { -5.0 };
        page.line(
            x,
            if up { n.bottom } else { n.top },
            x,
            beam_y(x),
            1.25,
            INK,
        );
    }
    let sign = if up { 1.0 } else { -1.0 };
    beam_segment(page, x1, beam_y(x1), x2, beam_y(x2), sign);
    for level in 2..=5 {
        for (index, n) in notes.iter().enumerate() {
            if n.duration.flags() < level {
                continue;
            }
            let nx = n.x + if up { 5.0 } else { -5.0 };
            let offset = sign * (level - 1) as f32 * 7.0;
            if index + 1 < notes.len() && notes[index + 1].duration.flags() >= level {
                let ex = notes[index + 1].x + if up { 5.0 } else { -5.0 };
                beam_segment(page, nx, beam_y(nx) + offset, ex, beam_y(ex) + offset, sign);
            } else if index == 0 || notes[index - 1].duration.flags() < level {
                let ex = if index + 1 < notes.len() {
                    nx + 9.0
                } else {
                    nx - 9.0
                };
                beam_segment(page, nx, beam_y(nx) + offset, ex, beam_y(ex) + offset, sign);
            }
        }
    }
}
fn beam_segment(page: &mut Page, x1: f32, y1: f32, x2: f32, y2: f32, sign: f32) {
    page.path(
        vec![
            (x1, y1),
            (x2, y2),
            (x2, y2 + sign * 4.0),
            (x1, y1 + sign * 4.0),
        ],
        true,
        Some(INK),
        None,
        0.0,
    );
}

fn draw_tie(
    page: &mut Page,
    score: &Score,
    r: &StaffRegion,
    n: &Note,
    x: f32,
    y: f32,
    below: bool,
) {
    let end = n.start.saturating_add(n.ticks());
    let next = score.parts[r.part].measures.get(r.measure).and_then(|m| {
        m.notes
            .iter()
            .find(|v| v.layer == n.layer && v.start >= end)
    });
    let x2 = next
        .map(|next| r.x_at_tick(next.start as f32) - 4.0)
        .unwrap_or(r.x + r.width - 8.0)
        .max(x + 11.0);
    tie_arc(page, x + 3.0, x2, y, below);
}
fn tie_arc(page: &mut Page, x1: f32, x2: f32, y: f32, below: bool) {
    let bend = if below { 8.0 } else { -8.0 };
    let w = x2 - x1;
    let mut p = vec![(x1, y)];
    cubic(
        &mut p,
        (x1, y),
        (x1 + w * 0.25, y + bend),
        (x2 - w * 0.25, y + bend),
        (x2, y),
        20,
    );
    cubic(
        &mut p,
        (x2, y),
        (x2 - w * 0.25, y + bend * 0.62),
        (x1 + w * 0.25, y + bend * 0.62),
        (x1, y),
        20,
    );
    page.path(p, true, Some(INK), None, 0.0);
}

fn whole_rest(page: &mut Page, x: f32, y: f32) {
    page.path(
        vec![
            (x - 6.0, y),
            (x + 6.0, y),
            (x + 6.0, y + 4.0),
            (x - 6.0, y + 4.0),
        ],
        true,
        Some(INK),
        None,
        0.0,
    );
}
fn rest(page: &mut Page, x: f32, top: f32, d: Duration, dotted: bool) {
    match d {
        Duration::DoubleWhole => {
            page.path(
                vec![
                    (x - 4.0, top + 10.0),
                    (x + 4.0, top + 10.0),
                    (x + 4.0, top + 20.0),
                    (x - 4.0, top + 20.0),
                ],
                true,
                Some(INK),
                None,
                0.0,
            );
        }
        Duration::Whole => whole_rest(page, x, top + 10.0),
        Duration::Half => {
            page.path(
                vec![
                    (x - 6.0, top + 16.0),
                    (x + 6.0, top + 16.0),
                    (x + 6.0, top + 20.0),
                    (x - 6.0, top + 20.0),
                ],
                true,
                Some(INK),
                None,
                0.0,
            );
        }
        Duration::Quarter => {
            let y = top + 6.0;
            page.path(
                vec![
                    (x - 3.0, y),
                    (x + 4.0, y + 7.0),
                    (x - 1.0, y + 14.0),
                    (x + 4.0, y + 20.0),
                    (x - 1.0, y + 20.0),
                    (x - 4.0, y + 25.0),
                    (x, y + 30.0),
                    (x - 6.0, y + 26.0),
                    (x - 6.0, y + 22.0),
                    (x - 2.0, y + 17.0),
                    (x - 6.0, y + 12.0),
                    (x - 1.0, y + 7.0),
                ],
                true,
                Some(INK),
                None,
                0.0,
            );
        }
        Duration::Eighth
        | Duration::Sixteenth
        | Duration::ThirtySecond
        | Duration::SixtyFourth
        | Duration::OneTwentyEighth => {
            let count = d.flags();
            page.line(
                x + 4.0,
                top + 10.0,
                x - 3.0 - count.saturating_sub(2) as f32,
                top + 35.0 + count.saturating_sub(2) as f32 * 9.0,
                1.8,
                INK,
            );
            for i in 0..count {
                let y = top + 12.0 + i as f32 * 9.0;
                page.ellipse(x - 3.0, y, 3.2, 3.0, Some(INK), None, 0.0);
                let mut p = vec![(x - 4.0, y + 1.0)];
                cubic(
                    &mut p,
                    (x - 4.0, y + 1.0),
                    (x - 1.0, y + 6.0),
                    (x + 3.0, y + 3.0),
                    (x + 4.0, y - 2.0),
                    14,
                );
                page.path(p, false, None, Some(INK), 1.3);
            }
        }
    }
    if dotted {
        page.ellipse(x + 10.0, top + 15.0, 1.7, 1.7, Some(INK), None, 0.0);
    }
}

fn accidental(page: &mut Page, x: f32, y: f32, alter: i8) {
    match alter {
        1 => sharp(page, x, y),
        -1 => flat(page, x, y),
        0 => {
            page.line(x - 2.0, y - 12.0, x - 2.0, y + 5.0, 1.3, INK);
            page.line(x + 3.0, y - 5.0, x + 3.0, y + 12.0, 1.3, INK);
            page.line(x - 2.0, y - 3.0, x + 3.0, y - 5.0, 2.5, INK);
            page.line(x - 2.0, y + 5.0, x + 3.0, y + 3.0, 2.5, INK);
        }
        2 => {
            page.line(x - 4.0, y - 4.0, x + 4.0, y + 4.0, 2.4, INK);
            page.line(x - 4.0, y + 4.0, x + 4.0, y - 4.0, 2.4, INK);
            for (dx, dy) in [(-4.0, -4.0), (-4.0, 4.0), (4.0, -4.0), (4.0, 4.0)] {
                page.ellipse(x + dx, y + dy, 1.7, 1.7, Some(INK), None, 0.0);
            }
        }
        -2 => {
            flat(page, x - 3.0, y);
            flat(page, x + 3.0, y);
        }
        _ => {}
    }
}
fn sharp(page: &mut Page, x: f32, y: f32) {
    page.line(x - 2.8, y - 11.0, x - 2.8, y + 12.0, 1.0, INK);
    page.line(x + 2.8, y - 12.0, x + 2.8, y + 11.0, 1.0, INK);
    page.line(x - 5.5, y - 3.0, x + 5.5, y - 6.0, 2.8, INK);
    page.line(x - 5.5, y + 5.0, x + 5.5, y + 2.0, 2.8, INK);
}
fn flat(page: &mut Page, x: f32, y: f32) {
    page.line(x - 2.5, y - 15.0, x - 2.5, y + 6.0, 1.4, INK);
    let mut p = vec![(x - 2.5, y - 2.0)];
    cubic(
        &mut p,
        (x - 2.5, y - 2.0),
        (x + 8.0, y - 9.0),
        (x + 8.0, y + 1.0),
        (x - 2.5, y + 6.0),
        22,
    );
    cubic(
        &mut p,
        (x - 2.5, y + 6.0),
        (x + 1.0, y + 2.0),
        (x + 4.0, y - 3.0),
        (x - 2.5, y),
        14,
    );
    page.path(p, true, Some(INK), None, 0.0);
}
fn key_signature(page: &mut Page, x: f32, y: f32, clef: Clef, fifths: i8) {
    let positions = if fifths >= 0 {
        [8, 5, 9, 6, 3, 7, 4]
    } else {
        [4, 7, 3, 6, 2, 5, 1]
    };
    let shift = match clef {
        Clef::Treble => 0,
        Clef::Bass => -2,
        Clef::Alto => -1,
    };
    for (i, position) in positions
        .iter()
        .take(fifths.unsigned_abs().min(7) as usize)
        .enumerate()
    {
        let py = y + 40.0 - (position + shift) as f32 * 5.0;
        accidental(
            page,
            x + i as f32 * 7.0,
            py,
            if fifths >= 0 { 1 } else { -1 },
        );
    }
}

// Hand-constructed clefs, sampled cubic curves. No external glyph outlines.
fn clef(page: &mut Page, x: f32, y: f32, c: Clef) {
    match c {
        Clef::Treble => {
            let mut p = vec![(x - 3.0, y + 54.0)];
            for (a, b, c, d) in [
                (
                    (x - 3.0, y + 54.0),
                    (x + 15.0, y + 66.0),
                    (x + 9.0, y + 37.0),
                    (x + 5.0, y + 18.0),
                ),
                (
                    (x + 5.0, y + 18.0),
                    (x + 2.0, y + 3.0),
                    (x - 6.0, y - 14.0),
                    (x + 2.0, y - 19.0),
                ),
                (
                    (x + 2.0, y - 19.0),
                    (x + 15.0, y - 13.0),
                    (x + 4.0, y + 3.0),
                    (x - 5.0, y + 13.0),
                ),
                (
                    (x - 5.0, y + 13.0),
                    (x - 21.0, y + 28.0),
                    (x - 9.0, y + 42.0),
                    (x + 5.0, y + 37.0),
                ),
                (
                    (x + 5.0, y + 37.0),
                    (x + 21.0, y + 31.0),
                    (x + 8.0, y + 14.0),
                    (x - 2.0, y + 20.0),
                ),
                (
                    (x - 2.0, y + 20.0),
                    (x - 9.0, y + 24.0),
                    (x - 7.0, y + 32.0),
                    (x - 1.0, y + 32.0),
                ),
            ] {
                cubic(&mut p, a, b, c, d, 18);
            }
            page.path(p, false, None, Some(INK), 2.6);
            page.ellipse(x - 3.0, y + 54.0, 3.0, 3.7, Some(INK), None, 0.0);
        }
        Clef::Bass => {
            let mut p = vec![(x - 9.0, y + 11.0)];
            cubic(
                &mut p,
                (x - 9.0, y + 11.0),
                (x - 9.0, y - 2.0),
                (x + 11.0, y - 2.0),
                (x + 9.0, y + 13.0),
                24,
            );
            cubic(
                &mut p,
                (x + 9.0, y + 13.0),
                (x + 7.0, y + 27.0),
                (x - 2.0, y + 33.0),
                (x - 11.0, y + 38.0),
                24,
            );
            cubic(
                &mut p,
                (x - 11.0, y + 38.0),
                (x - 1.0, y + 30.0),
                (x + 7.0, y + 19.0),
                (x + 4.0, y + 7.0),
                20,
            );
            cubic(
                &mut p,
                (x + 4.0, y + 7.0),
                (x + 1.0, y - 1.0),
                (x - 5.0, y + 3.0),
                (x - 6.0, y + 10.0),
                18,
            );
            page.path(p, true, Some(INK), None, 0.0);
            page.ellipse(x - 8.0, y + 11.0, 3.5, 3.8, Some(INK), None, 0.0);
            page.ellipse(x + 15.0, y + 5.0, 1.9, 1.9, Some(INK), None, 0.0);
            page.ellipse(x + 15.0, y + 15.0, 1.9, 1.9, Some(INK), None, 0.0);
        }
        Clef::Alto => {
            page.line(x - 11.0, y, x - 11.0, y + 40.0, 3.8, INK);
            page.line(x - 5.0, y, x - 5.0, y + 40.0, 1.4, INK);
            for sign in [-1.0, 1.0] {
                let cy = y + 20.0;
                let mut p = vec![(x - 3.0, cy)];
                cubic(
                    &mut p,
                    (x - 3.0, cy),
                    (x + 3.0, cy - sign * 9.0),
                    (x + 5.0, cy - sign * 16.0),
                    (x + 12.0, cy - sign * 17.0),
                    18,
                );
                cubic(
                    &mut p,
                    (x + 12.0, cy - sign * 17.0),
                    (x + 20.0, cy - sign * 17.0),
                    (x + 19.0, cy - sign * 3.0),
                    (x + 8.0, cy - sign * 3.0),
                    18,
                );
                cubic(
                    &mut p,
                    (x + 8.0, cy - sign * 3.0),
                    (x + 17.0, cy - sign * 8.0),
                    (x + 13.0, cy - sign * 18.0),
                    (x + 9.0, cy - sign * 11.0),
                    18,
                );
                cubic(
                    &mut p,
                    (x + 9.0, cy - sign * 11.0),
                    (x + 6.0, cy - sign * 5.0),
                    (x + 3.0, cy),
                    (x - 3.0, cy),
                    18,
                );
                page.path(p, true, Some(INK), None, 0.0);
            }
        }
    }
}
fn brace(page: &mut Page, x: f32, y1: f32, y2: f32) {
    let mid = (y1 + y2) * 0.5;
    let mut p = vec![(x + 5.0, y1 - 3.0)];
    cubic(
        &mut p,
        (x + 5.0, y1 - 3.0),
        (x - 12.0, y1 + 12.0),
        (x + 5.0, mid - 14.0),
        (x - 9.0, mid),
        24,
    );
    cubic(
        &mut p,
        (x - 9.0, mid),
        (x + 5.0, mid + 14.0),
        (x - 12.0, y2 - 12.0),
        (x + 5.0, y2 + 3.0),
        24,
    );
    page.path(p, false, None, Some(INK), 2.3);
}
fn cubic(
    points: &mut Vec<(f32, f32)>,
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    d: (f32, f32),
    steps: usize,
) {
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let s = 1.0 - t;
        points.push((
            s * s * s * a.0 + 3.0 * s * s * t * b.0 + 3.0 * s * t * t * c.0 + t * t * t * d.0,
            s * s * s * a.1 + 3.0 * s * s * t * b.1 + 3.0 * s * t * t * c.1 + t * t * t * d.1,
        ));
    }
}

fn time_signature(page: &mut Page, x: f32, y: f32, n: u8, d: u8) {
    let draw = |page: &mut Page, value: u8, yy: f32| {
        let s = value.to_string();
        let w = s.len() as f32 * 11.0;
        for (i, c) in s.chars().enumerate() {
            digit(page, x - w * 0.5 + i as f32 * 11.0, yy, c, 1.0);
        }
    };
    draw(page, n, y + 1.0);
    draw(page, d, y + 21.0);
}
fn digit(page: &mut Page, x: f32, y: f32, c: char, scale: f32) {
    // Curved, open numerals deliberately drawn independently of any font.
    let strokes: &[&[(f32, f32)]] = match c {
        '0' => &[&[
            (4., 1.),
            (1., 3.),
            (0., 8.),
            (1., 14.),
            (4., 17.),
            (7., 14.),
            (8., 8.),
            (7., 3.),
            (4., 1.),
        ]],
        '1' => &[&[(1., 4.), (5., 1.), (5., 17.)], &[(1., 17.), (9., 17.)]],
        '2' => &[&[
            (0., 4.),
            (2., 1.),
            (6., 1.),
            (8., 4.),
            (7., 7.),
            (0., 16.),
            (8., 16.),
        ]],
        '3' => &[&[
            (0., 2.),
            (6., 1.),
            (8., 4.),
            (6., 8.),
            (3., 8.),
            (7., 9.),
            (8., 13.),
            (6., 17.),
            (0., 16.),
        ]],
        '4' => &[&[(7., 17.), (7., 1.), (0., 12.), (10., 12.)]],
        '5' => &[&[
            (8., 1.),
            (1., 1.),
            (0., 8.),
            (5., 7.),
            (8., 10.),
            (8., 14.),
            (5., 17.),
            (0., 16.),
        ]],
        '6' => &[&[
            (8., 2.),
            (5., 1.),
            (1., 5.),
            (0., 12.),
            (2., 16.),
            (6., 17.),
            (8., 14.),
            (8., 10.),
            (5., 8.),
            (0., 10.),
        ]],
        '7' => &[&[(0., 2.), (9., 2.), (3., 17.)]],
        '8' => &[&[
            (4., 1.),
            (0., 3.),
            (1., 7.),
            (7., 10.),
            (8., 14.),
            (5., 17.),
            (1., 16.),
            (0., 13.),
            (2., 9.),
            (7., 6.),
            (8., 3.),
            (4., 1.),
        ]],
        '9' => &[&[
            (8., 9.),
            (3., 10.),
            (0., 7.),
            (0., 3.),
            (3., 1.),
            (7., 2.),
            (8., 7.),
            (7., 13.),
            (4., 17.),
            (1., 17.),
        ]],
        _ => &[],
    };
    for stroke in strokes {
        page.path(
            stroke
                .iter()
                .map(|p| (x + p.0 * scale, y + p.1 * scale))
                .collect(),
            false,
            None,
            Some(INK),
            2.4 * scale,
        );
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn color(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}
fn paint(fill: Option<[u8; 3]>, stroke: Option<[u8; 3]>, width: f32) -> String {
    format!(
        "fill=\"{}\" stroke=\"{}\" stroke-width=\"{:.2}\"",
        fill.map(color).unwrap_or_else(|| "none".into()),
        stroke.map(color).unwrap_or_else(|| "none".into()),
        width
    )
}

/// Export a completely standalone SVG. Musical symbols remain vector outlines.
pub fn svg(page: &Page) -> String {
    use std::fmt::Write;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.0} {:.0}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/><g stroke-linejoin=\"round\" stroke-linecap=\"round\">",
        page.width, page.height, page.width, page.height
    );
    for item in &page.items {
        match item {
            Primitive::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                color: c,
            } => {
                let _ = write!(
                    out,
                    "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"{}\" stroke-width=\"{width:.2}\"/>",
                    color(*c)
                );
            }
            Primitive::Ellipse {
                cx,
                cy,
                rx,
                ry,
                fill,
                stroke,
                width,
            } => {
                let _ = write!(
                    out,
                    "<ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" {}/>",
                    paint(*fill, *stroke, *width)
                );
            }
            Primitive::Path {
                points,
                closed,
                fill,
                stroke,
                width,
            } => {
                if points.is_empty() {
                    continue;
                }
                out.push_str("<path d=\"");
                for (i, (x, y)) in points.iter().enumerate() {
                    let _ = write!(out, "{}{x:.2} {y:.2} ", if i == 0 { 'M' } else { 'L' });
                }
                if *closed {
                    out.push('Z');
                }
                let _ = write!(out, "\" {}/>", paint(*fill, *stroke, *width));
            }
            Primitive::Text {
                x,
                y,
                text,
                size,
                color: c,
                serif,
            } => {
                let _ = write!(
                    out,
                    "<text x=\"{x:.2}\" y=\"{y:.2}\" font-size=\"{size:.2}\" font-family=\"{}\" fill=\"{}\" stroke=\"none\">{}</text>",
                    if *serif {
                        "Georgia, Times New Roman, serif"
                    } else {
                        "Arial, sans-serif"
                    },
                    color(*c),
                    xml_escape(text)
                );
            }
        }
    }
    out.push_str("</g></svg>");
    out
}

fn pdf_rgb(c: [u8; 3]) -> String {
    format!(
        "{:.4} {:.4} {:.4}",
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0
    )
}
fn text_face(serif: bool) -> ttf_parser::Face<'static> {
    let bytes: &[u8] = if serif {
        include_bytes!("../assets/Serif.ttf")
    } else {
        include_bytes!("../assets/Sans.ttf")
    };
    ttf_parser::Face::parse(bytes, 0).expect("Encore's bundled text fonts are valid")
}

// PDF text is outlined using the same bundled typefaces as the editor. This
// keeps scores portable even on systems with no installed printer fonts.
struct PdfGlyph<'a> {
    out: &'a mut String,
    x: f32,
    y: f32,
    scale: f32,
    pen: (f32, f32),
}
impl ttf_parser::OutlineBuilder for PdfGlyph<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        use std::fmt::Write;
        let _ = writeln!(
            self.out,
            "{:.3} {:.3} m",
            self.x + x * self.scale,
            self.y + y * self.scale
        );
        self.pen = (x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        use std::fmt::Write;
        let _ = writeln!(
            self.out,
            "{:.3} {:.3} l",
            self.x + x * self.scale,
            self.y + y * self.scale
        );
        self.pen = (x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let c1 = (
            self.pen.0 + (x1 - self.pen.0) * 2.0 / 3.0,
            self.pen.1 + (y1 - self.pen.1) * 2.0 / 3.0,
        );
        let c2 = (x + (x1 - x) * 2.0 / 3.0, y + (y1 - y) * 2.0 / 3.0);
        self.curve_to(c1.0, c1.1, c2.0, c2.1, x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        use std::fmt::Write;
        let _ = writeln!(
            self.out,
            "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c",
            self.x + x1 * self.scale,
            self.y + y1 * self.scale,
            self.x + x2 * self.scale,
            self.y + y2 * self.scale,
            self.x + x * self.scale,
            self.y + y * self.scale
        );
        self.pen = (x, y);
    }
    fn close(&mut self) {
        self.out.push_str("h\n");
    }
}
fn pdf_text(
    out: &mut String,
    face: &ttf_parser::Face<'_>,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: [u8; 3],
) {
    use std::fmt::Write;
    out.push_str("% Encore vector text\n");
    let _ = writeln!(out, "{} rg", pdf_rgb(color));
    let scale = size / face.units_per_em() as f32;
    let mut cursor = x;
    let mut baseline = y;
    for c in text.chars() {
        if c == '\n' {
            cursor = x;
            baseline -= size * 1.3;
            continue;
        }
        let glyph = face
            .glyph_index(c)
            .or_else(|| face.glyph_index('\u{fffd}'))
            .or_else(|| face.glyph_index('?'))
            .unwrap_or(ttf_parser::GlyphId(0));
        let mut builder = PdfGlyph {
            out,
            x: cursor,
            y: baseline,
            scale,
            pen: (0.0, 0.0),
        };
        if face.outline_glyph(glyph, &mut builder).is_some() {
            out.push_str("f\n");
        }
        cursor += face
            .glyph_hor_advance(glyph)
            .unwrap_or(face.units_per_em() / 2) as f32
            * scale;
    }
}

fn pdf_style(out: &mut String, fill: Option<[u8; 3]>, stroke: Option<[u8; 3]>, width: f32) {
    use std::fmt::Write;
    if let Some(c) = fill {
        let _ = writeln!(out, "{} rg", pdf_rgb(c));
    }
    if let Some(c) = stroke {
        let _ = writeln!(out, "{} RG {width:.3} w", pdf_rgb(c));
    }
}
fn pdf_finish(out: &mut String, fill: Option<[u8; 3]>, stroke: Option<[u8; 3]>) {
    out.push_str(match (fill, stroke) {
        (Some(_), Some(_)) => "B\n",
        (Some(_), None) => "f\n",
        (None, Some(_)) => "S\n",
        _ => "n\n",
    });
}

/// Minimal standards-compliant multipage PDF, using the same paths as the
/// editor and SVG exporter. No external print utility is required.
pub fn pdf(pages: &[Page]) -> Vec<u8> {
    use std::fmt::Write;
    let serif_face = text_face(true);
    let sans_face = text_face(false);
    let mut objects: Vec<Vec<u8>> = vec![];
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids = (0..pages.len())
        .map(|i| format!("{} 0 R", 5 + i * 2))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(format!("<< /Type /Pages /Count {} /Kids [{kids}] >>", pages.len()).into_bytes());
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    for (i, page) in pages.iter().enumerate() {
        let content_id = 6 + i * 2;
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {content_id} 0 R >>").into_bytes());
        let mut out = format!(
            "q {:.6} 0 0 {:.6} 0 0 cm\n1 J 1 j\n",
            612.0 / page.width,
            792.0 / page.height
        );
        let py = |y: f32| page.height - y;
        for item in &page.items {
            match item {
                Primitive::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    color: c,
                } => {
                    let _ = writeln!(
                        out,
                        "{} RG {width:.3} w {x1:.3} {:.3} m {x2:.3} {:.3} l S",
                        pdf_rgb(*c),
                        py(*y1),
                        py(*y2)
                    );
                }
                Primitive::Ellipse {
                    cx,
                    cy,
                    rx,
                    ry,
                    fill,
                    stroke,
                    width,
                } => {
                    pdf_style(&mut out, *fill, *stroke, *width);
                    let k = 0.552_284_8;
                    let x = *cx;
                    let y = py(*cy);
                    let a = *rx;
                    let b = *ry;
                    let _ = writeln!(
                        out,
                        "{:.3} {y:.3} m {:.3} {:.3} {:.3} {:.3} {x:.3} {:.3} c {:.3} {:.3} {:.3} {:.3} {:.3} {y:.3} c {:.3} {:.3} {:.3} {:.3} {x:.3} {:.3} c {:.3} {:.3} {:.3} {:.3} {:.3} {y:.3} c h",
                        x + a,
                        x + a,
                        y + k * b,
                        x + k * a,
                        y + b,
                        y + b,
                        x - k * a,
                        y + b,
                        x - a,
                        y + k * b,
                        x - a,
                        x - a,
                        y - k * b,
                        x - k * a,
                        y - b,
                        y - b,
                        x + k * a,
                        y - b,
                        x + a,
                        y - k * b,
                        x + a
                    );
                    pdf_finish(&mut out, *fill, *stroke);
                }
                Primitive::Path {
                    points,
                    closed,
                    fill,
                    stroke,
                    width,
                } => {
                    if points.is_empty() {
                        continue;
                    }
                    pdf_style(&mut out, *fill, *stroke, *width);
                    for (j, (x, y)) in points.iter().enumerate() {
                        let _ = writeln!(
                            out,
                            "{x:.3} {:.3} {}",
                            py(*y),
                            if j == 0 { "m" } else { "l" }
                        );
                    }
                    if *closed {
                        out.push_str("h\n");
                    }
                    pdf_finish(&mut out, *fill, *stroke);
                }
                Primitive::Text {
                    x,
                    y,
                    text,
                    size,
                    color: c,
                    serif,
                } => {
                    pdf_text(
                        &mut out,
                        if *serif { &serif_face } else { &sans_face },
                        text,
                        *x,
                        py(*y),
                        *size,
                        *c,
                    );
                }
            }
        }
        out.push_str("Q\n");
        let bytes = out.into_bytes();
        let mut stream = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
        stream.extend(bytes);
        stream.extend_from_slice(b"endstream");
        objects.push(stream);
    }
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_has_every_measure_and_selection() {
        let score = Score::demo();
        let selected = Selection {
            part: 0,
            measure: 0,
            note: 0,
        };
        let pages = engrave(
            &score,
            &EngravingOptions {
                selected: Some(selected),
                ..Default::default()
            },
        );
        assert!(!pages.is_empty());
        assert_eq!(
            pages.iter().map(|p| p.staves.len()).sum::<usize>(),
            score.parts.len() * score.measure_count()
        );
        assert!(
            pages
                .iter()
                .any(|p| p.hits.iter().any(|h| h.selection == selected))
        );
        for page in pages {
            for r in page.staves {
                assert!(r.x >= 0.0 && r.x + r.width <= page.width + 1.0);
                assert!(r.y > 30.0 && r.y + 40.0 < page.height - 40.0);
                assert!(r.content_width > 10.0);
            }
        }
    }
    #[test]
    fn part_filter_and_pagination() {
        let mut score = Score::blank();
        score.add_measures(60);
        let pages = engrave(
            &score,
            &EngravingOptions {
                part_filter: Some(1),
                ..Default::default()
            },
        );
        assert!(pages.len() > 1);
        assert!(pages.iter().flat_map(|p| &p.staves).all(|r| r.part == 1));
        assert_eq!(
            pages.iter().map(|p| p.staves.len()).sum::<usize>(),
            score.measure_count()
        );
    }
    #[test]
    fn dynamic_changes_are_not_repeated_and_rests_do_not_reset_them() {
        let mut note = Note {
            articulations: Vec::new(),
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            tied: false,
        };
        let mut previous = None;
        assert_eq!(dynamic_change(&mut previous, &note), Some("mf"));
        assert_eq!(dynamic_change(&mut previous, &note), None);
        note.velocity = 86;
        assert_eq!(dynamic_change(&mut previous, &note), None);
        note.pitches.clear();
        note.velocity = 120;
        assert_eq!(dynamic_change(&mut previous, &note), None);
        note.pitches = vec![Pitch::from_midi(60)];
        note.velocity = 0;
        assert_eq!(dynamic_change(&mut previous, &note), None);
        note.velocity = 80;
        assert_eq!(dynamic_change(&mut previous, &note), None);
        note.velocity = 120;
        assert_eq!(dynamic_change(&mut previous, &note), Some("ff"));
        assert_eq!(dynamic_change(&mut previous, &note), None);
    }
    #[test]
    fn standalone_exports_have_valid_structure() {
        let pages = engrave(&Score::demo(), &Default::default());
        let svg = svg(&pages[0]);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("<path"));
        assert!(svg.ends_with("</svg>"));
        let bytes = pdf(&pages);
        let text = String::from_utf8_lossy(&bytes);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(text.contains("/Type /Catalog"));
        assert!(text.contains("% Encore vector text"));
        assert!(
            !text.contains(" Tf "),
            "Portable PDFs must outline text rather than depend on printer fonts"
        );
        assert!(text.contains(&format!("/Count {}", pages.len())));
        assert!(text.ends_with("%%EOF\n"));
        let start = text
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert_eq!(&bytes[start..start + 4], b"xref");
    }
    #[test]
    fn triplet_brackets_and_five_level_beams_reach_vector_exports() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        let make_note = |start, duration, tuplet| Note {
            articulations: Vec::new(),
            start,
            duration,
            dotted: false,
            tuplet,
            layer: 0,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            tied: false,
        };
        let mut notes = (0..3)
            .map(|index| {
                make_note(
                    index * 160,
                    Duration::Eighth,
                    Some(crate::model::Tuplet::TRIPLET),
                )
            })
            .collect::<Vec<_>>();
        notes.push(make_note(480, Duration::OneTwentyEighth, None));
        notes.push(make_note(495, Duration::OneTwentyEighth, None));
        score.parts[0].measures[0].notes = notes;
        score.validate().unwrap();
        let pages = engrave(&score, &EngravingOptions::default());
        assert!(pages[0].items.iter().any(|item| matches!(item, Primitive::Text { text, size, .. } if text == "3" && *size == 11.0)));
        assert!(svg(&pages[0]).contains(">3</text>"));
        assert!(pdf(&pages).starts_with(b"%PDF"));
        let mut page = Page::new();
        let layout = |x, start| NoteLayout {
            x,
            top: 20.0,
            bottom: 20.0,
            mid: 20.0,
            up: true,
            duration: Duration::OneTwentyEighth,
            start,
            end: start + 15,
        };
        beam_group(&mut page, &[layout(0.0, 0), layout(20.0, 15)]);
        assert_eq!(
            page.items
                .iter()
                .filter(|item| matches!(item, Primitive::Path { .. }))
                .count(),
            5
        );
    }

    #[test]
    fn layers_keep_selection_hits_and_use_opposed_stems() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes = (0..2)
            .map(|layer| Note {
                articulations: Vec::new(),
                layer,
                start: 0,
                duration: Duration::Quarter,
                dotted: false,
                tuplet: None,
                pitches: vec![Pitch::from_midi(60)],
                velocity: 80,
                staccato: false,
                tied: false,
            })
            .collect();
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        assert_eq!(page.hits.len(), 2);
        assert_eq!(page.hits[0].selection.note, 0);
        assert_eq!(page.hits[1].selection.note, 1);
        let x = page.hits[0].x;
        let y = page.hits[0].y;
        assert!(page.items.iter().any(|item| matches!(item, Primitive::Line { x1, y1, x2, y2, width, .. } if (*x1 - (x + 5.0)).abs() < 0.01 && *x1 == *x2 && (*y1-y).abs() < 0.01 && *y2 < *y1 && *width == 1.25)));
        assert!(page.items.iter().any(|item| matches!(item, Primitive::Line { x1, y1, x2, y2, width, .. } if (*x1 - (x - 5.0)).abs() < 0.01 && *x1 == *x2 && (*y1-y).abs() < 0.01 && *y2 > *y1 && *width == 1.25)));
        assert!(pdf(&pages).starts_with(b"%PDF"));
    }
    #[test]
    fn simultaneous_layer_dynamics_draw_once_below_the_lowest_staff_note() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(2);
        let notes = (0..4)
            .map(|layer| Note {
                articulations: Vec::new(),
                layer,
                start: 0,
                duration: Duration::Whole,
                dotted: false,
                tuplet: None,
                pitches: vec![Pitch::from_midi(72 - layer * 12)],
                velocity: 80,
                staccato: false,
                tied: false,
            })
            .collect::<Vec<_>>();
        score.parts[0].measures[0].notes = notes.clone();
        score.parts[0].measures[1].notes = notes;
        let mut previous = None;
        assert_eq!(
            staff_dynamic_changes(&score.parts[0].measures[0].notes, &mut previous),
            vec![(0, "mf")]
        );
        assert!(staff_dynamic_changes(&score.parts[0].measures[1].notes, &mut previous).is_empty());
        let pages = engrave(&score, &EngravingOptions::default());
        let page = &pages[0];
        let lowest = page
            .hits
            .iter()
            .filter(|hit| hit.selection.measure == 0)
            .map(|hit| hit.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let f_shapes = page
            .items
            .iter()
            .filter_map(|item| match item {
                Primitive::Path { points, width, .. } if (*width - 1.9 * 0.7).abs() < 0.0001 => {
                    Some(points)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            f_shapes.len(),
            1,
            "four simultaneous layers must print one mf mark"
        );
        assert!(f_shapes[0].iter().all(|(_, y)| *y > lowest + 10.0));
    }

    #[test]
    fn staff_dynamics_group_conflicting_attacks_and_retain_later_changes() {
        let mut notes = (0..4)
            .map(|layer| Note {
                articulations: Vec::new(),
                layer,
                start: 0,
                duration: Duration::Quarter,
                dotted: false,
                tuplet: None,
                pitches: vec![Pitch::from_midi(60 + layer)],
                velocity: 80,
                staccato: false,
                tied: false,
            })
            .collect::<Vec<_>>();
        notes[3].velocity = 116;
        let mut later = notes[0].clone();
        later.start = 480;
        later.velocity = 48;
        notes.push(later);
        let mut previous = None;
        assert_eq!(
            staff_dynamic_changes(&notes, &mut previous),
            vec![(0, "ff"), (480, "p")]
        );
    }
    fn burst_score() -> Score {
        let mut score = Score::blank();
        let make_note = |start, duration, layer| Note {
            articulations: Vec::new(),
            layer,
            start,
            duration,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(72)],
            velocity: 80,
            staccato: false,
            tied: false,
        };
        let mut notes = (0..8)
            .map(|index| make_note(index * 60, Duration::ThirtySecond, 0))
            .collect::<Vec<_>>();
        notes.push(make_note(480, Duration::Quarter, 0));
        notes.extend((0..8).map(|index| make_note(960 + index * 30, Duration::SixtyFourth, 0)));
        notes.push(make_note(1200, Duration::Eighth, 0));
        notes.push(make_note(1440, Duration::Quarter, 0));
        notes.push(make_note(0, Duration::Whole, 1));
        score.parts[0].measures[0].notes = notes;
        score.parts[1].measures[0].notes = vec![make_note(960, Duration::Half, 0)];
        score.normalize();
        score.validate().unwrap();
        score
    }

    #[test]
    fn temporal_spacing_gives_short_bursts_room_and_aligns_all_staves() {
        let score = burst_score();
        let pages = engrave(&score, &EngravingOptions::default());
        let first = pages[0]
            .staves
            .iter()
            .find(|staff| staff.part == 0 && staff.measure == 0)
            .unwrap();
        let second = pages[0]
            .staves
            .iter()
            .find(|staff| staff.part == 1 && staff.measure == 0)
            .unwrap();
        for index in 0..7 {
            let start = 960.0 + index as f32 * 30.0;
            let separation = first.x_at_tick(start + 30.0) - first.x_at_tick(start);
            assert!(separation >= 14.0, "64th noteheads need room: {separation}");
        }
        assert!((first.x_at_tick(960.0) - second.x_at_tick(960.0)).abs() < 0.001);
        assert!(Arc::ptr_eq(
            &first.temporal_anchors,
            &second.temporal_anchors
        ));
        assert!(
            first.x_at_tick(960.0) - first.x_at_tick(480.0) < first.content_width * 0.25,
            "long gaps must compress to make room for the burst"
        );
        let top_hit = pages[0]
            .hits
            .iter()
            .find(|hit| {
                hit.selection.part == 0
                    && score.parts[0].measures[0].notes[hit.selection.note].start == 960
            })
            .unwrap();
        let lower_hit = pages[0]
            .hits
            .iter()
            .find(|hit| hit.selection.part == 1)
            .unwrap();
        assert!((top_hit.x - lower_hit.x).abs() < 0.001);
    }

    #[test]
    fn temporal_spacing_inverse_preserves_onsets_and_intermediate_editor_times() {
        let score = burst_score();
        let pages = engrave(&score, &EngravingOptions::default());
        let staff = &pages[0].staves[0];
        for tick in [
            0.0, 15.0, 30.0, 159.25, 160.0, 240.0, 480.0, 960.0, 975.5, 1170.0, 1919.75, 1920.0,
        ] {
            let restored = staff.tick_at_x(staff.x_at_tick(tick));
            assert!((restored - tick).abs() < 0.002, "{tick} became {restored}");
        }
        for &(tick, _) in staff.temporal_anchors.iter() {
            assert_eq!(staff.tick_at_x(staff.x_at_tick(tick)), tick);
        }
        assert_eq!(staff.tick_at_x(staff.content_x - 100.0), 0.0);
        assert_eq!(
            staff.tick_at_x(staff.content_x + staff.content_width + 100.0),
            1920.0
        );
        assert_eq!(staff.x_at_tick(-1.0), staff.content_x);
        assert_eq!(
            staff.x_at_tick(5000.0),
            staff.content_x + staff.content_width
        );
    }
}
