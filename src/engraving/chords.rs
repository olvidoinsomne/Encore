//! Original vector chord roots, suffixes, alternate basses and string diagrams.
use super::*;
use crate::chords::{BassStyle, ChordSymbol, Fretboard};

fn symbol_width(symbol: &ChordSymbol) -> f32 {
    let text = text_width(&symbol.chord.label(), 15., false) + 12.;
    if symbol.show_fretboard && symbol.fretboard.is_some() {
        text.max(72.)
    } else {
        text
    }
}
pub(super) fn widths(score: &Score, parts: &[usize], measure: usize) -> BTreeMap<u32, f32> {
    let mut result = BTreeMap::<u32, f32>::new();
    for symbol in score.notation.chords.iter().filter(|symbol| {
        symbol.visible && symbol.anchor.measure == measure && parts.contains(&symbol.anchor.part)
    }) {
        result
            .entry(symbol.anchor.tick)
            .and_modify(|width| *width = width.max(symbol_width(symbol)))
            .or_insert_with(|| symbol_width(symbol));
    }
    result
}
pub(super) fn clearance(score: &Score, part: usize, measure: usize, bars: usize) -> f32 {
    score
        .notation
        .chords
        .iter()
        .filter(|s| {
            s.visible
                && s.anchor.part == part
                && (measure..measure + bars).contains(&s.anchor.measure)
        })
        .map(|symbol| {
            45. + f32::from(symbol.slot) * 150.
                + symbol.offset[1].abs()
                + if let Some(frame) = symbol.fretboard.as_ref().filter(|_| symbol.show_fretboard) {
                    f32::from(frame.displayed_frets) * 11. + 35.
                } else {
                    0.
                }
        })
        .fold(0., f32::max)
}
pub(super) fn draw(page: &mut Page, score: &Score) {
    let staves = page.staves.clone();
    for symbol in &score.notation.chords {
        if !symbol.visible {
            continue;
        }
        let Some(region) = staves
            .iter()
            .find(|r| r.part == symbol.anchor.part && r.measure == symbol.anchor.measure)
        else {
            continue;
        };
        let x = region.x_at_tick(symbol.anchor.tick as f32) + symbol.offset[0];
        let system = staves
            .iter()
            .filter(|r| r.part == region.part && (r.y - region.y).abs() < 0.1)
            .collect::<Vec<_>>();
        let envelope = slurs::above_envelope(score, &system).unwrap_or(region.y - 15.);
        let mut y =
            (region.y - 34.).min(envelope - 18.) + symbol.offset[1] - f32::from(symbol.slot) * 150.;
        let mut bounds_top = y - 18.;
        let mut bounds_bottom =
            y + if symbol.bass_style == BassStyle::Under && symbol.chord.bass.is_some() {
                22.
            } else {
                7.
            };
        if symbol.chord.kind == crate::chords::ChordKind::None {
            page.text(x, y, "N.C.", 15., INK, false);
        } else {
            let root = symbol.chord.root.unwrap();
            let root_text = root.label();
            page.text(x, y, &root_text, 16., INK, false);
            let root_width = text_width(&root_text, 16., false);
            let suffix = symbol.chord.suffix();
            page.text(x + root_width, y - 3., &suffix, 12., INK, false);
            if let Some(bass) = symbol.chord.bass {
                let prefix_width = root_width + text_width(&suffix, 12., false);
                match symbol.bass_style {
                    BassStyle::Slash => page.text(
                        x + prefix_width,
                        y,
                        format!("/{}", bass.label()),
                        15.,
                        INK,
                        false,
                    ),
                    BassStyle::LowerRight => {
                        page.line(
                            x + prefix_width + 2.,
                            y + 4.,
                            x + prefix_width + 9.,
                            y - 9.,
                            1.,
                            INK,
                        );
                        page.text(
                            x + prefix_width + 11.,
                            y + 6.,
                            bass.label(),
                            13.,
                            INK,
                            false,
                        );
                    }
                    BassStyle::Under => {
                        let width = prefix_width.max(text_width(&bass.label(), 14., false));
                        page.line(x - 1., y + 4., x + width + 2., y + 4., 0.8, INK);
                        page.text(
                            x + (width - text_width(&bass.label(), 14., false)) / 2.,
                            y + 20.,
                            bass.label(),
                            14.,
                            INK,
                            false,
                        );
                        y -= 2.;
                    }
                }
            }
        }
        if symbol.show_fretboard
            && let Some(frame) = &symbol.fretboard
        {
            let frame_height = f32::from(frame.displayed_frets) * 11. + 24.;
            let frame_y = y - frame_height - 15.;
            draw_frame(page, frame, x + symbol_width(symbol) * 0.5 - 26., frame_y);
            bounds_top = bounds_top.min(frame_y - 12.);
            bounds_bottom = bounds_bottom.max(frame_y + frame_height);
        }
        page.chord_hits.push(ChordHit {
            anchor: symbol.anchor,
            slot: symbol.slot,
            x: x - 5.,
            y: bounds_top - 3.,
            width: symbol_width(symbol) + 10.,
            height: bounds_bottom - bounds_top + 6.,
        });
    }
}
fn draw_frame(page: &mut Page, frame: &Fretboard, x: f32, y: f32) {
    let width = 52.;
    let height = f32::from(frame.displayed_frets) * 11.;
    let string_gap = width / (frame.frets.len() - 1) as f32;
    for string in 0..frame.frets.len() {
        let sx = x + string as f32 * string_gap;
        page.line(sx, y, sx, y + height, 0.65, INK);
    }
    for fret in 0..=frame.displayed_frets {
        let fy = y + f32::from(fret) * 11.;
        page.line(
            x,
            fy,
            x + width,
            fy,
            if fret == 0 && frame.first_fret == 1 {
                2.3
            } else {
                0.7
            },
            INK,
        );
    }
    if frame.first_fret > 1 {
        page.text(
            x + width + 5.,
            y + 9.,
            format!("{}fr", frame.first_fret),
            9.,
            INK,
            false,
        );
    }
    for (index, fret) in frame.frets.iter().enumerate() {
        let sx = x + index as f32 * string_gap;
        match fret {
            None => {
                page.line(sx - 2., y - 9., sx + 2., y - 5., 0.8, INK);
                page.line(sx - 2., y - 5., sx + 2., y - 9., 0.8, INK);
            }
            Some(0) => page.items.push(Primitive::Ellipse {
                cx: sx,
                cy: y - 7.,
                rx: 2.,
                ry: 2.,
                fill: None,
                stroke: Some(INK),
                width: 0.8,
            }),
            Some(fret) => {
                let fy = y + (f32::from(*fret) - f32::from(frame.first_fret) + 0.5) * 11.;
                page.items.push(Primitive::Ellipse {
                    cx: sx,
                    cy: fy,
                    rx: 2.9,
                    ry: 2.9,
                    fill: Some(INK),
                    stroke: None,
                    width: 0.,
                });
                if let Some(finger) = frame.fingers.get(index).copied().flatten() {
                    page.text(
                        sx - 2.5,
                        y + height + 12.,
                        finger.to_string(),
                        8.,
                        INK,
                        false,
                    );
                }
            }
        }
    }
    for barre in &frame.barres {
        let fy = y + (f32::from(barre.fret) - f32::from(frame.first_fret) + 0.5) * 11.;
        page.line(
            x + barre.from as f32 * string_gap,
            fy,
            x + barre.to as f32 * string_gap,
            fy,
            5.,
            INK,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chords::{self, ChordSymbol},
        document::Anchor,
    };
    #[test]
    fn offbeat_chord_frames_are_vector_objects_with_spacing_and_clearance() {
        let mut score = Score::blank();
        let mut symbol = ChordSymbol::new(
            Anchor {
                part: 0,
                measure: 0,
                tick: 240,
                layer: 0,
            },
            chords::parse("Cmaj7/E").unwrap(),
        );
        symbol.fretboard = chords::suggest_fretboards(&symbol.chord).into_iter().next();
        score.notation.chords.push(symbol);
        let pages = crate::engraving::engrave(&score, &Default::default());
        assert!(widths(&score, &[0], 0).contains_key(&240));
        assert!(clearance(&score, 0, 0, 1) >= 120.);
        assert!(pages.iter().any(|page| {
            page.items
                .iter()
                .any(|item| matches!(item,Primitive::Text{text,..}if text=="maj7"))
        }));
    }
}
