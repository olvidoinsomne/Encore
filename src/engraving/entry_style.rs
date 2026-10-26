//! Original alternate noteheads and explicit accidental presentation.
use super::*;
use crate::entry_style::{AccidentalDisplay, EntryStyle, Notehead, StemDirection};
pub(super) fn draw_notehead(
    page: &mut Page,
    style: &EntryStyle,
    x: f32,
    y: f32,
    open: bool,
    whole: bool,
) {
    let s = style.scale;
    let rx = if whole { 7.1 } else { 5.9 } * s;
    let ry = 3.8 * s;
    let fill = (!open).then_some(INK);
    match style.notehead {
        Notehead::Normal => {
            if (s - 1.0).abs() < f32::EPSILON {
                note_head(page, x, y, open, whole, INK);
            } else {
                page.ellipse(x, y, rx, ry, fill, Some(INK), 1.1 * s);
            }
        }
        Notehead::Cross => {
            page.line(x - rx, y - ry, x + rx, y + ry, 1.7 * s, INK);
            page.line(x - rx, y + ry, x + rx, y - ry, 1.7 * s, INK);
        }
        Notehead::Diamond => page.path(
            vec![
                (x - rx, y),
                (x, y - ry - 1.0 * s),
                (x + rx, y),
                (x, y + ry + 1.0 * s),
            ],
            true,
            fill,
            Some(INK),
            1.2 * s,
        ),
        Notehead::Triangle => page.path(
            vec![(x - rx, y + ry), (x, y - ry - 1.2 * s), (x + rx, y + ry)],
            true,
            fill,
            Some(INK),
            1.2 * s,
        ),
        Notehead::Slash => page.path(
            vec![
                (x - rx, y + ry),
                (x + rx * 0.55, y - ry - 2.0 * s),
                (x + rx, y - ry),
                (x - rx * 0.55, y + ry + 2.0 * s),
            ],
            true,
            fill,
            Some(INK),
            1.2 * s,
        ),
        Notehead::Square => page.path(
            vec![
                (x - rx, y - ry),
                (x + rx, y - ry),
                (x + rx, y + ry),
                (x - rx, y + ry),
            ],
            true,
            fill,
            Some(INK),
            1.2 * s,
        ),
    }
}
pub(super) fn draw_accidental(
    page: &mut Page,
    display: AccidentalDisplay,
    x: f32,
    y: f32,
    alter: i8,
    automatic: bool,
) {
    if display == AccidentalDisplay::Hide || display == AccidentalDisplay::Auto && !automatic {
        return;
    }
    accidental(page, x, y, alter);
    if display == AccidentalDisplay::Parenthesized {
        page.path(
            vec![
                (x - 4.0, y - 10.0),
                (x - 6.5, y - 6.0),
                (x - 7.0, y),
                (x - 6.5, y + 6.0),
                (x - 4.0, y + 10.0),
            ],
            false,
            None,
            Some(INK),
            0.8,
        );
        page.path(
            vec![
                (x + 10.0, y - 10.0),
                (x + 12.5, y - 6.0),
                (x + 13.0, y),
                (x + 12.5, y + 6.0),
                (x + 10.0, y + 10.0),
            ],
            false,
            None,
            Some(INK),
            0.8,
        );
    }
}
pub(super) fn stem_up(style: &EntryStyle, automatic: bool) -> bool {
    match style.stem {
        StemDirection::Up => true,
        StemDirection::Down => false,
        _ => automatic,
    }
}
