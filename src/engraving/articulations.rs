//! Original vector marks, placed on the note side opposite the stem.
use super::{INK, Page};
use crate::model::{Articulation, Note};

pub(super) struct Placement {
    pub staff_top: f32,
    pub staff_bottom: f32,
    pub top_note_y: f32,
    pub bottom_note_y: f32,
    pub stem_up: bool,
}

pub(super) fn draw(page: &mut Page, note: &Note, x: f32, placement: Placement) {
    if note.pitches.is_empty() {
        return;
    }
    if note.articulations.is_empty() {
        // Preserve the previous staccato appearance for all existing scores.
        if note.staccato {
            dot(
                page,
                x,
                if placement.stem_up {
                    placement.bottom_note_y + 12.0
                } else {
                    placement.top_note_y - 12.0
                },
            );
        }
        return;
    }
    let above = !placement.stem_up;
    let direction = if above { -1.0 } else { 1.0 };
    let mut y = if above {
        (placement.top_note_y - 12.0).min(placement.staff_top - 9.0)
    } else {
        (placement.bottom_note_y + 12.0).max(placement.staff_bottom + 9.0)
    };
    if note.staccato && !note.articulations.contains(&Articulation::Staccatissimo) {
        dot(page, x, y);
        y += direction * 9.0;
    }
    // A deterministic order avoids moving combined marks after XML round trips.
    for articulation in [
        Articulation::Staccatissimo,
        Articulation::Tenuto,
        Articulation::Accent,
        Articulation::Marcato,
    ] {
        if !note.articulations.contains(&articulation) {
            continue;
        }
        match articulation {
            Articulation::Accent => page.path(
                vec![(x - 5.5, y - 3.0), (x + 5.0, y), (x - 5.5, y + 3.0)],
                false,
                None,
                Some(INK),
                1.6,
            ),
            Articulation::Marcato => page.path(
                vec![
                    (x - 4.5, y - direction * 2.0),
                    (x, y + direction * 4.0),
                    (x + 4.5, y - direction * 2.0),
                ],
                false,
                None,
                Some(INK),
                1.6,
            ),
            Articulation::Tenuto => page.line(x - 5.0, y, x + 5.0, y, 1.7, INK),
            Articulation::Staccatissimo => page.path(
                vec![
                    (x - 2.7, y + direction * 3.5),
                    (x + 2.7, y + direction * 3.5),
                    (x, y - direction * 4.0),
                ],
                true,
                Some(INK),
                None,
                0.0,
            ),
        }
        y += direction * 11.0;
    }
}

fn dot(page: &mut Page, x: f32, y: f32) {
    page.ellipse(x, y, 1.6, 1.6, Some(INK), None, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engraving::Primitive;
    use crate::model::{Duration, Pitch};

    fn note() -> Note {
        Note {
            voice: 0,
            voice_launch: None,
            layer: 0,
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        }
    }

    fn placement(stem_up: bool) -> Placement {
        Placement {
            staff_top: 100.0,
            staff_bottom: 140.0,
            top_note_y: 110.0,
            bottom_note_y: 130.0,
            stem_up,
        }
    }

    #[test]
    fn all_four_marks_are_original_finite_vector_primitives() {
        for &articulation in Articulation::ALL {
            for stem_up in [false, true] {
                let mut page = Page::new();
                let mut note = note();
                note.articulations.push(articulation);
                draw(&mut page, &note, 200.0, placement(stem_up));
                assert_eq!(page.items.len(), 1);
                match &page.items[0] {
                    Primitive::Path { points, .. } => {
                        assert!(points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
                        assert!(points.iter().all(|(_, y)| if stem_up {
                            *y > 140.0
                        } else {
                            *y < 100.0
                        }));
                    }
                    Primitive::Line { x1, y1, x2, y2, .. } => {
                        assert!(x1 < x2);
                        assert_eq!(y1, y2);
                        assert!(if stem_up { *y1 > 140.0 } else { *y1 < 100.0 });
                    }
                    _ => panic!("New articulations should use paths or lines."),
                }
            }
        }
    }

    #[test]
    fn legacy_staccato_position_is_unchanged_and_staccatissimo_suppresses_its_dot() {
        let mut page = Page::new();
        let mut note = note();
        note.staccato = true;
        draw(&mut page, &note, 200.0, placement(true));
        assert!(matches!(
            page.items[0],
            Primitive::Ellipse {
                cx: 200.0,
                cy: 142.0,
                ..
            }
        ));
        note.articulations.push(Articulation::Staccatissimo);
        let mut page = Page::new();
        draw(&mut page, &note, 200.0, placement(true));
        assert_eq!(page.items.len(), 1);
        assert!(matches!(
            page.items[0],
            Primitive::Path { closed: true, .. }
        ));
    }
}
