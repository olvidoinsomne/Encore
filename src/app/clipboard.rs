use super::*;
use serde::{Deserialize, Serialize};

const CLIPBOARD_MARKER: &str = "ENCORE-EVENT\n";
const MAX_CLIPBOARD_BYTES: usize = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClipboardEvent {
    version: u8,
    note: Note,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lyrics: Vec<encore::lyrics::Lyric>,
}

#[cfg(test)]
fn encode_event(note: Note) -> Result<String, String> {
    encode_annotated_event(note, Vec::new())
}

fn encode_annotated_event(
    note: Note,
    lyrics: Vec<encore::lyrics::Lyric>,
) -> Result<String, String> {
    let data = serde_json::to_string(&ClipboardEvent {
        version: if lyrics.is_empty() { 1 } else { 2 },
        note,
        lyrics,
    })
    .map_err(|error| format!("Could not copy event: {error}"))?;
    if data.len() + CLIPBOARD_MARKER.len() > MAX_CLIPBOARD_BYTES {
        return Err("The selected event exceeds the clipboard size limit.".into());
    }
    Ok(format!("{CLIPBOARD_MARKER}{data}"))
}

fn decode_event(text: &str) -> Result<ClipboardEvent, String> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err("Clipboard event exceeds 16 KiB.".into());
    }
    let data = text
        .strip_prefix(CLIPBOARD_MARKER)
        .ok_or("Copy a single note, chord, or rest in Encore before pasting.")?;
    let event: ClipboardEvent = serde_json::from_str(data)
        .map_err(|error| format!("Invalid Encore clipboard event: {error}"))?;
    if ![1, 2].contains(&event.version) {
        return Err("This clipboard event requires a different Encore version.".into());
    }
    Ok(event)
}

impl EncoreApp {
    pub(super) fn copy_selection(&mut self, ctx: &egui::Context, cut: bool) {
        let Some(selection) = self.selected else {
            self.status = "Select a single note, chord, or rest to copy.".into();
            return;
        };
        let Some(note) = self
            .score
            .parts
            .get(selection.part)
            .and_then(|part| part.measures.get(selection.measure))
            .and_then(|measure| measure.notes.get(selection.note))
            .cloned()
        else {
            self.status = "The selected event is no longer available.".into();
            return;
        };
        let anchor = encore::document::Anchor::from_selection(&self.score, selection);
        let lyrics = self
            .score
            .notation
            .lyrics
            .iter()
            .filter(|lyric| Some(lyric.anchor) == anchor)
            .cloned()
            .map(|mut lyric| {
                lyric.extend_to = None;
                lyric
            })
            .collect();
        let text = match encode_annotated_event(note.clone(), lyrics) {
            Ok(text) => text,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if cut {
            let mut candidate = self.score.clone();
            if let Some(anchor) = encore::document::Anchor::from_selection(&candidate, selection) {
                encore::document::remove_anchor(&mut candidate, &anchor);
            }
            candidate.parts[selection.part].measures[selection.measure]
                .notes
                .remove(selection.note);
            if let Err(error) = candidate.validate() {
                self.status = format!("Cannot cut event: {error}");
                return;
            }
            self.score = candidate;
            self.selected = None;
            self.active_part = selection.part;
            self.active_layer = note.layer;
            self.cursor_measure = selection.measure;
            self.cursor_tick = note.start;
            self.status = "Cut one event. Paste inserts it at the entry cursor.".into();
        } else {
            self.status = "Copied one event. Paste inserts it at the entry cursor.".into();
        }
        ctx.copy_text(text);
    }

    pub(super) fn paste_selection(&mut self, text: &str) {
        if let Err(error) = self.try_paste_event(text) {
            self.status = format!("Cannot paste: {error}");
        }
    }

    fn try_paste_event(&mut self, text: &str) -> Result<(), String> {
        let ClipboardEvent {
            mut note, lyrics, ..
        } = decode_event(text)?;
        note.start = self.cursor_tick;
        note.layer = self.active_layer;
        let end = note
            .start
            .checked_add(note.checked_ticks()?)
            .ok_or("The event position overflows the score timeline.")?;
        let bar_ticks = self.score.measure_ticks();
        if end > bar_ticks {
            return Err(
                "The event crosses the barline. Move the cursor or choose a shorter event.".into(),
            );
        }
        let mut candidate = self.score.clone();
        let measure = candidate
            .parts
            .get_mut(self.active_part)
            .and_then(|part| part.measures.get_mut(self.cursor_measure))
            .ok_or("Choose an existing staff and measure before pasting.")?;
        if measure.notes.iter().any(|other| {
            other.layer == note.layer
                && note.start < other.start.saturating_add(other.ticks())
                && end > other.start
        }) {
            return Err(
                "The event overlaps existing notation. Move the cursor or remove that event."
                    .into(),
            );
        }
        let index = measure
            .notes
            .partition_point(|other| (other.start, other.layer) < (note.start, note.layer));
        measure.notes.insert(index, note);
        let anchor = encore::document::Anchor {
            part: self.active_part,
            measure: self.cursor_measure,
            tick: self.cursor_tick,
            layer: self.active_layer,
        };
        for mut lyric in lyrics {
            lyric.anchor = anchor;
            lyric.extend_to = None;
            encore::lyrics::assign(&mut candidate, lyric)?;
        }
        candidate.validate()?;
        // Commit only after the complete score validates. A failed paste keeps
        // the score, selection, cursor, and undo history exactly as they were.
        self.score = candidate;
        self.selected = Some(Selection {
            part: self.active_part,
            measure: self.cursor_measure,
            note: index,
        });
        if let Some(pitch) = self.score.parts[self.active_part].measures[self.cursor_measure].notes
            [index]
            .pitches
            .first()
            .copied()
        {
            self.entry_pitch = pitch;
            self.octave = self
                .entry_written_pitch(pitch)
                .map_or(pitch.octave, |written| written.octave);
        }
        self.cursor_tick = end;
        if end == bar_ticks && self.cursor_measure + 1 < self.score.measure_count() {
            self.cursor_measure += 1;
            self.cursor_tick = 0;
        }
        self.status = "Pasted one event at the entry cursor.".into();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> EncoreApp {
        EncoreApp::from_score(Score::blank(), None, "Ready".into())
    }

    fn note() -> Note {
        serde_json::from_str(
            r#"{"start":480,"duration":"Eighth","dotted":false,"tuplet":{"actual":3,"normal":2},"pitches":[{"step":"C","octave":4,"alter":0},{"step":"E","octave":4,"alter":-1}],"velocity":104,"staccato":true,"tied":true}"#,
        )
        .unwrap()
    }

    fn copied_text(ctx: &egui::Context) -> String {
        ctx.output(|output| {
            output
                .commands
                .iter()
                .find_map(|command| match command {
                    egui::OutputCommand::CopyText(text) => Some(text.clone()),
                    _ => None,
                })
                .expect("a clipboard command")
        })
    }

    #[test]
    fn copy_and_paste_preserve_a_chords_rhythm_and_performance_properties() {
        let ctx = egui::Context::default();
        let mut app = app();
        let original = note();
        app.score.parts[0].measures[0].notes.push(original.clone());
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        let source = app.score.clone();
        app.copy_selection(&ctx, false);
        assert_eq!(app.score, source);
        assert_eq!(app.cursor_tick, 0);
        app.active_part = 1;
        app.cursor_measure = 2;
        app.cursor_tick = 800;
        app.paste_selection(&copied_text(&ctx));
        let mut expected = original;
        expected.start = 800;
        assert_eq!(app.score.parts[1].measures[2].notes, vec![expected]);
        assert_eq!(
            app.score.parts[0].measures[0].notes,
            source.parts[0].measures[0].notes
        );
        assert_eq!(app.cursor_tick, 960);
        assert_eq!(
            app.selected,
            Some(Selection {
                part: 1,
                measure: 2,
                note: 0
            })
        );
        assert!(app.undo.is_empty() && app.redo.is_empty());
        app.score.validate().unwrap();
    }

    #[test]
    fn cut_keeps_following_event_positions_and_can_be_pasted_back() {
        let ctx = egui::Context::default();
        let mut app = app();
        let first = note();
        let mut second = first.clone();
        second.start = 960;
        app.score.parts[0].measures[0].notes = vec![first.clone(), second.clone()];
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        app.copy_selection(&ctx, true);
        assert_eq!(app.score.parts[0].measures[0].notes, vec![second.clone()]);
        assert_eq!(app.cursor_tick, first.start);
        assert_eq!(app.selected, None);
        app.paste_selection(&copied_text(&ctx));
        assert_eq!(app.score.parts[0].measures[0].notes, vec![first, second]);
    }

    #[test]
    fn failed_pastes_preserve_the_entire_editor_document_and_cursor() {
        let mut app = app();
        let original = note();
        app.score.parts[0].measures[0].notes.push(original.clone());
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        let valid = encode_event(original.clone()).unwrap();
        let mut invalid_note = original;
        invalid_note.velocity = 255;
        let invalid = encode_event(invalid_note).unwrap();
        for (text, tick) in [
            (valid.clone(), 480), // Same-start overlap must not replace notation.
            (valid.clone(), 500),
            (valid.clone(), 1900), // Barline.
            (invalid, 960),
            ("plain text".into(), 960),
            (
                format!("{CLIPBOARD_MARKER}{{\"version\":2,\"note\":{{}}}}"),
                960,
            ),
            ("x".repeat(MAX_CLIPBOARD_BYTES + 1), 960),
        ] {
            app.cursor_tick = tick;
            let before = app.score.clone();
            let selected = app.selected;
            app.paste_selection(&text);
            assert_eq!(app.score, before);
            assert_eq!(app.cursor_tick, tick);
            assert_eq!(app.cursor_measure, 0);
            assert_eq!(app.active_part, 0);
            assert_eq!(app.selected, selected);
            assert!(app.undo.is_empty() && app.redo.is_empty());
            assert!(app.status.starts_with("Cannot paste:"));
        }
    }

    #[test]
    fn rest_paste_at_bar_end_advances_to_the_next_existing_measure() {
        let mut app = app();
        let mut rest = note();
        rest.pitches.clear();
        rest.tied = false;
        rest.tuplet = None;
        rest.duration = Duration::Quarter;
        app.cursor_tick = 1440;
        app.paste_selection(&encode_event(rest).unwrap());
        assert!(app.score.parts[0].measures[0].notes[0].pitches.is_empty());
        assert_eq!((app.cursor_measure, app.cursor_tick), (1, 0));
        app.score.validate().unwrap();
    }

    #[test]
    fn malformed_clipboard_versions_and_invalid_targets_are_atomic() {
        let mut app = app();
        let original = note();
        let unsupported_version = format!(
            "{CLIPBOARD_MARKER}{}",
            serde_json::to_string(&ClipboardEvent {
                version: 99,
                note: original.clone(),
                lyrics: Vec::new(),
            })
            .unwrap()
        );
        let before = app.score.clone();
        app.paste_selection(&unsupported_version);
        assert!(app.status.contains("different Encore version"));
        assert_eq!(app.score, before);
        let text = encode_event(original).unwrap();
        for (part, measure) in [(usize::MAX, 0), (0, usize::MAX)] {
            app.active_part = part;
            app.cursor_measure = measure;
            app.cursor_tick = 600;
            app.paste_selection(&text);
            assert_eq!(app.score, before);
            assert_eq!((app.active_part, app.cursor_measure), (part, measure));
            assert_eq!(app.cursor_tick, 600);
        }
    }

    #[test]
    fn paste_into_another_layer_preserves_independent_overlapping_notation() {
        let mut app = app();
        let original = note();
        app.score.parts[0].measures[0].notes.push(original.clone());
        app.active_layer = 1;
        app.cursor_tick = original.start;
        app.paste_selection(&encode_event(original.clone()).unwrap());
        let notes = &app.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes.iter().find(|note| note.layer == 0), Some(&original));
        let mut pasted = original;
        pasted.layer = 1;
        assert_eq!(notes.iter().find(|note| note.layer == 1), Some(&pasted));
        app.score.validate().unwrap();
        app.copy_selection(&egui::Context::default(), true);
        assert_eq!(app.active_layer, 1);
        assert_eq!(app.score.parts[0].measures[0].notes.len(), 1);
    }

    #[test]
    fn invalid_score_and_missing_selection_do_not_cut_or_copy() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.copy_selection(&ctx, true);
        assert!(ctx.output(|output| output.commands.is_empty()));
        app.score.parts[0].measures[0].notes.push(note());
        app.score.tempo = 0;
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        app.cursor_tick = 960;
        let before = app.score.clone();
        app.copy_selection(&ctx, true);
        assert_eq!(app.score, before);
        assert_eq!(app.cursor_tick, 960);
        assert!(app.selected.is_some());
        assert!(ctx.output(|output| output.commands.is_empty()));
    }

    #[test]
    fn cutting_and_pasting_a_syllable_reanchors_it_without_a_dangling_melisma() {
        let ctx = egui::Context::default();
        let score: Score =
            serde_json::from_str(include_str!("../../examples/Cantabile.encore")).unwrap();
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.selected = Some(Selection {
            part: 0,
            measure: 2,
            note: 2,
        });
        app.copy_selection(&ctx, true);
        assert!(app.status.starts_with("Cut one"));
        let copied = copied_text(&ctx);
        app.active_part = 1;
        app.cursor_measure = 7;
        app.cursor_tick = 480;
        app.duration = Duration::Quarter;
        // Open a free onset in this destination staff.
        app.score.parts[1].measures[7].notes.clear();
        app.paste_selection(&copied);
        let lyrics = app
            .score
            .notation
            .lyrics
            .iter()
            .filter(|lyric| lyric.anchor.part == 1)
            .collect::<Vec<_>>();
        assert_eq!(lyrics.len(), 3);
        assert!(lyrics.iter().all(|lyric| lyric.anchor.measure == 7
            && lyric.anchor.tick == 480
            && lyric.extend_to.is_none()));
        assert!(lyrics.iter().any(|lyric| lyric.text == "Shine"));
        app.score.validate().unwrap();
    }
}
