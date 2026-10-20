//! Controller regressions plus a display-server-independent rendering smoke test.
use super::*;
use std::collections::HashMap;

#[test]
fn midi_import_requires_a_timing_choice_and_reports_quantization_without_changing_the_source() {
    let directory = ImportTestDirectory::new();
    let path = directory.0.join("Detached performance.mid");
    let mut score = Score::blank();
    let mut short = event(0, Duration::SixtyFourth, &[60]);
    short.articulations = vec![Articulation::Staccatissimo];
    score.parts[0].measures[0].notes = vec![short, event(30, Duration::SixtyFourth, &[62])];
    let bytes = encore::export::midi(&score);
    std::fs::write(&path, &bytes).unwrap();
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let original = Score::demo();
    let mut app = controller(original.clone());
    app.begin_open_document(path.clone()).unwrap();
    assert_eq!(app.score, original);
    assert!(app.midi_import.is_some());
    assert!(app.midi_grid.is_none());
    let mut frame = 0;
    for quantized in [false, true] {
        if quantized {
            app.midi_grid = Some(15);
        }
        let mut button = None;
        for _ in 0..3 {
            let output = ctx.run(raw_input(frame, vec![], egui::Modifiers::NONE), |ctx| {
                app.ui(ctx)
            });
            button = visible_text_rect(&output, "Import");
            frame += 1;
        }
        let position = button.unwrap().center();
        for pressed in [true, false] {
            let _ = ctx.run(
                raw_input(
                    frame,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    egui::Modifiers::NONE,
                ),
                |ctx| app.ui(ctx),
            );
            frame += 1;
        }
        if !quantized {
            assert!(app.midi_import_error.is_some());
            assert_eq!(app.score, original);
        }
    }
    assert!(app.midi_import.is_none());
    assert!(app.midi_import_error.is_none());
    assert!(app.path.is_none() && app.needs_native_save && app.dirty());
    assert!(
        app.import_warnings
            .iter()
            .any(|warning| warning.to_lowercase().contains("quantiz"))
    );
    assert_eq!(app.score.parts[0].measures[0].notes[0].ticks(), 15);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    app.score.validate().unwrap();
}

#[test]
fn staff_clicks_use_the_inverse_spacing_map_in_a_dense_passage() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut score = Score::blank();
    score.parts[0].measures[0].notes = (0..8)
        .map(|i| event(i * 15, Duration::OneTwentyEighth, &[72]))
        .collect();
    score.parts[0].measures[0]
        .notes
        .push(event(960, Duration::Quarter, &[72]));
    let mut app = controller(score);
    app.write_mode = true;
    app.duration = Duration::Eighth;
    let mut paper = None;
    for frame in 0..3 {
        let output = ctx.run(raw_input(frame, vec![], egui::Modifiers::NONE), |ctx| {
            app.ui(ctx)
        });
        paper = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == Color32::WHITE
                    && rect.rect.width() > 700.0
                    && rect.rect.height() > 900.0 =>
            {
                Some(rect.rect)
            }
            _ => None,
        });
    }
    let staff = &app.cached_pages[0].staves[0];
    let position = paper.unwrap().min
        + Vec2::new(staff.x_at_tick(720.0), staff.y + 4.0 * staff.spacing) * app.zoom;
    for (frame, pressed) in [(3, true), (4, false)] {
        let _ = ctx.run(
            raw_input(
                frame,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            ),
            |ctx| app.ui(ctx),
        );
    }
    let inserted = app.score.parts[0].measures[0]
        .notes
        .iter()
        .find(|note| note.start == 720)
        .expect("click must insert at the displayed beat");
    assert_eq!(inserted.pitches[0].midi(), 64);
    assert_eq!(inserted.ticks(), 240);
    app.score.validate().unwrap();
}

#[test]
fn clicking_write_allows_immediate_keyboard_entry_and_rest_continuation() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    let plain = egui::Modifiers::NONE;
    let mut output = ctx.run(raw_input(0, vec![], plain), |ctx| app.ui(ctx));
    for frame in 1..3 {
        output = ctx.run(raw_input(frame, vec![], plain), |ctx| app.ui(ctx));
    }
    let position = visible_text_rect(&output, "Write").unwrap().center();
    for (frame, pressed) in [(3, true), (4, false)] {
        let _ = ctx.run(
            raw_input(
                frame,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: plain,
                    },
                ],
                plain,
            ),
            |ctx| app.ui(ctx),
        );
    }
    assert!(app.write_mode);
    let _ = ctx.run(
        raw_input(5, vec![key(egui::Key::C, true, plain)], plain),
        |ctx| app.ui(ctx),
    );
    assert_eq!(app.score.parts[0].measures[0].notes.len(), 1);
    let _ = ctx.run(
        raw_input(
            6,
            vec![
                key(egui::Key::C, false, plain),
                key(egui::Key::Tab, true, plain),
            ],
            plain,
        ),
        |ctx| app.ui(ctx),
    );
    let _ = ctx.run(
        raw_input(
            7,
            vec![
                key(egui::Key::Tab, false, plain),
                key(egui::Key::D, true, plain),
            ],
            plain,
        ),
        |ctx| app.ui(ctx),
    );
    assert_eq!(app.score.parts[0].measures[0].notes.len(), 3);
}

#[test]
fn tab_rest_keeps_score_entry_focus_for_the_next_pitch() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.write_mode = true;
    let plain = egui::Modifiers::NONE;
    let _ = ctx.run(raw_input(0, vec![], plain), |ctx| app.ui(ctx));
    let _ = ctx.run(
        raw_input(1, vec![key(egui::Key::Tab, true, plain)], plain),
        |ctx| app.ui(ctx),
    );
    let _ = ctx.run(
        raw_input(
            2,
            vec![
                key(egui::Key::Tab, false, plain),
                key(egui::Key::C, true, plain),
            ],
            plain,
        ),
        |ctx| app.ui(ctx),
    );
    let notes = &app.score.parts[0].measures[0].notes;
    assert_eq!(notes.len(), 2);
    assert!(notes[0].pitches.is_empty());
    assert_eq!(notes[1].pitches[0].midi(), 60);
    assert_eq!(notes[1].start, 480);
}

#[test]
fn replacing_a_document_clears_entry_groups_and_layers_but_preserves_preferences() {
    let mut app = controller(Score::blank());
    app.theme = Theme::Light;
    app.entry_profile = EntryProfile::Speedy;
    app.active_layer = 3;
    app.tuplet = Some(Tuplet::TRIPLET);
    app.tuplet_remaining = Some(2);
    app.duration = Duration::OneTwentyEighth;
    app.dotted = true;
    app.rest = true;
    app.accidental = Some(2);
    app.entry_pitch = Pitch::from_midi(84);
    app.octave = 6;
    app.replace_document(Score::blank(), None);
    assert_eq!(app.active_layer, 0);
    assert!(app.tuplet.is_none() && app.tuplet_remaining.is_none());
    assert_eq!(app.duration, Duration::Quarter);
    assert!(!app.dotted && !app.rest && app.accidental.is_none());
    assert_eq!(app.entry_pitch, Pitch::from_midi(60));
    assert_eq!(app.octave, 4);
    assert_eq!(app.theme, Theme::Light);
    assert_eq!(app.entry_profile, EntryProfile::Speedy);
}

#[test]
fn document_confirmation_stops_later_key_and_paste_events_in_the_same_frame() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.write_mode = true;
    app.score.title = "Unsaved title".into();
    let original = app.score.clone();
    let plain = egui::Modifiers::NONE;
    let command = egui::Modifiers {
        ctrl: !cfg!(target_os = "macos"),
        mac_cmd: cfg!(target_os = "macos"),
        command: true,
        ..plain
    };
    let _ = ctx.run(raw_input(0, vec![], plain), |ctx| app.ui(ctx));
    let _ = ctx.run(
        raw_input(
            1,
            vec![
                key(egui::Key::N, true, command),
                key(egui::Key::D, true, plain),
                egui::Event::Paste("invalid clipboard text".into()),
            ],
            command,
        ),
        |ctx| app.ui(ctx),
    );
    assert!(app.pending.is_some());
    assert_eq!(app.score, original);
    assert!(app.undo.is_empty());
}

#[test]
fn native_copy_and_paste_events_use_the_score_clipboard_and_record_undo() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.write_mode = true;
    let plain = egui::Modifiers::NONE;
    let _ = ctx.run(raw_input(0, vec![], plain), |ctx| app.ui(ctx));
    let _ = ctx.run(
        raw_input(1, vec![key(egui::Key::C, true, plain)], plain),
        |ctx| app.ui(ctx),
    );
    let output = ctx.run(
        raw_input(
            2,
            vec![key(egui::Key::C, false, plain), egui::Event::Copy],
            plain,
        ),
        |ctx| app.ui(ctx),
    );
    let copied = output
        .platform_output
        .commands
        .iter()
        .find_map(|command| match command {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("native copy emits clipboard data");
    assert_eq!(app.undo.len(), 1);
    let _ = ctx.run(
        raw_input(3, vec![egui::Event::Paste(copied)], plain),
        |ctx| app.ui(ctx),
    );
    assert_eq!(app.score.parts[0].measures[0].notes.len(), 2);
    assert_eq!(app.score.parts[0].measures[0].notes[1].start, 480);
    assert_eq!(app.undo.len(), 2);
    app.undo();
    assert_eq!(app.score.parts[0].measures[0].notes.len(), 1);
}

fn controller(score: Score) -> EncoreApp {
    EncoreApp::from_score(score, None, "Ready".into())
}

fn screenshot_theme(app: &mut EncoreApp, ctx: &egui::Context) {
    if std::env::var("ENCORE_SCREENSHOT_THEME").as_deref() == Ok("light") {
        app.set_theme(ctx, Theme::Light);
    }
}

#[derive(Default)]
struct PreferenceStorage(HashMap<String, String>);

impl eframe::Storage for PreferenceStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.into(), value);
    }

    fn flush(&mut self) {}
}

#[test]
fn appearance_preferences_restore_and_save_without_serializing_the_score() {
    assert_eq!(Theme::from_storage(None), Theme::Dark);
    let mut storage = PreferenceStorage::default();
    assert_eq!(Theme::from_storage(Some(&storage)), Theme::Dark);
    for (value, expected) in [
        ("light", Theme::Light),
        ("dark", Theme::Dark),
        ("", Theme::Dark),
        ("damaged-preference", Theme::Dark),
    ] {
        storage.0.insert("encore.theme".into(), value.into());
        assert_eq!(Theme::from_storage(Some(&storage)), expected);
    }

    let ctx = egui::Context::default();
    configure_context(&ctx);
    let score = Score::demo();
    let mut app = controller(score.clone());
    storage
        .0
        .insert("unrelated.preference".into(), "keep".into());
    for theme in [Theme::Light, Theme::Dark] {
        app.set_theme(&ctx, theme);
        eframe::App::save(&mut app, &mut storage);
        assert_eq!(Theme::from_storage(Some(&storage)), theme);
        assert_eq!(storage.0["encore.theme"], theme.key());
        assert_eq!(storage.0["unrelated.preference"], "keep");
        assert_eq!(storage.0["encore.entry-profile"], app.entry_profile.key());
        assert_eq!(storage.0.len(), 3);
        assert_eq!(app.score, score);
        assert!(!app.dirty());
        assert!(app.undo.is_empty() && app.redo.is_empty());
    }
}

#[test]
fn default_dark_appearance_is_monochromatic_with_readable_control_text() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let app = controller(Score::blank());
    assert_eq!(app.theme, Theme::Dark);
    assert!(ctx.style().visuals.dark_mode);
    let colors = Palette::for_theme(app.theme);
    let neutral = |color: Color32| {
        assert_eq!(color.r(), color.g(), "red and green must be neutral");
        assert_eq!(color.g(), color.b(), "green and blue must be neutral");
    };
    for color in [
        colors.text,
        colors.muted,
        colors.accent,
        colors.surface,
        colors.raised,
        colors.border,
        colors.selected,
        colors.workspace,
        colors.hovered,
        colors.key_ink,
        colors.key_hover,
        colors.playhead,
        ctx.style().visuals.warn_fg_color,
        ctx.style().visuals.error_fg_color,
    ] {
        neutral(color);
    }
    let luminance = |color: Color32| {
        let srgb = f64::from(color.r()) / 255.0;
        if srgb <= 0.04045 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        }
    };
    let contrast = |a, b| {
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    for background in [
        colors.surface,
        colors.raised,
        colors.selected,
        colors.workspace,
        colors.hovered,
    ] {
        assert!(contrast(colors.text, background) >= 4.5);
        assert!(contrast(colors.muted, background) >= 4.5);
    }
    assert!(contrast(colors.accent, colors.workspace) >= 7.0);
}

#[test]
fn score_preview_colors_follow_the_theme_without_changing_page_primitives() {
    let selected_note = [42, 111, 192];
    let selection_fill = [222, 237, 255];
    for original in [selected_note, selection_fill, [29, 34, 42], [255; 3]] {
        let dark = page_preview_color(original, true);
        assert_eq!(dark.r(), dark.g());
        assert_eq!(dark.g(), dark.b());
        assert_eq!(
            page_preview_color(original, false),
            Color32::from_rgb(original[0], original[1], original[2])
        );
    }
    assert!(page_preview_color(selected_note, true).r() < 128);
    assert!(page_preview_color(selection_fill, true).r() > 220);
}

struct ImportTestDirectory(PathBuf);

impl ImportTestDirectory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("encore-import-ui-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for ImportTestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn foreign_scores_require_a_native_save_and_never_adopt_the_source_path() {
    let directory = ImportTestDirectory::new();
    let foreign_path = directory.0.join("Imported composition.musicxml");
    let native_path = directory.0.join("Native composition.encore");
    let mut source = Score::blank();
    source.title = "Import keeps its source".into();
    source.parts[0].measures[0].notes = vec![event(0, Duration::Quarter, &[60, 64, 67])];
    let foreign_bytes = encore::export::musicxml(&source).into_bytes();
    std::fs::write(&foreign_path, &foreign_bytes).unwrap();
    source.save(&native_path).unwrap();
    let native_bytes = std::fs::read(&native_path).unwrap();

    let mut app = controller(Score::demo());
    app.open_document(foreign_path.clone()).unwrap();
    assert_eq!(app.score.title, source.title);
    assert_eq!(
        app.score.parts[0].measures[0].notes[0].pitches,
        source.parts[0].measures[0].notes[0].pitches
    );
    assert!(app.path.is_none(), "Save must prompt for a new Encore path");
    assert!(app.needs_native_save && app.dirty());
    assert_eq!(app.score, app.saved);
    assert!(app.undo.is_empty() && app.redo.is_empty());
    assert_eq!(std::fs::read(&foreign_path).unwrap(), foreign_bytes);

    app.open_document(native_path.clone()).unwrap();
    assert_eq!(app.path.as_ref(), Some(&native_path));
    assert_eq!(app.score, source);
    assert!(!app.needs_native_save && !app.dirty());
    assert_eq!(std::fs::read(&native_path).unwrap(), native_bytes);
    assert_eq!(std::fs::read(&foreign_path).unwrap(), foreign_bytes);
}

#[test]
fn a_failed_foreign_open_preserves_the_current_document_and_undo_history() {
    let directory = ImportTestDirectory::new();
    let native_path = directory.0.join("Keep this.encore");
    let foreign_path = directory.0.join("Broken score.musicxml");
    let saved = Score::demo();
    saved.save(&native_path).unwrap();
    std::fs::write(&foreign_path, "<score-partwise><part>").unwrap();
    let mut app = EncoreApp::from_score(saved.clone(), Some(native_path.clone()), "Ready".into());
    app.score.title = "Unsaved changes stay open".into();
    app.undo.push(saved.clone());
    app.redo.push(Score::blank());
    let score = app.score.clone();
    let undo = app.undo.clone();
    let redo = app.redo.clone();
    let error = app.open_document(foreign_path).unwrap_err();
    assert!(error.contains("XML"));
    assert_eq!(app.score, score);
    assert_eq!(app.saved, saved);
    assert_eq!(app.path, Some(native_path));
    assert_eq!(app.undo, undo);
    assert_eq!(app.redo, redo);
    assert!(app.dirty());
    assert!(!app.needs_native_save);
}

#[test]
fn clicking_the_staff_enters_the_pitch_at_the_written_position() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.write_mode = true;
    let mut paper = None;
    for frame in 0..3 {
        let output = ctx.run(raw_input(frame, vec![], Default::default()), |ctx| {
            app.ui(ctx)
        });
        paper = output.shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Rect(r)
                if r.fill == Color32::WHITE
                    && r.rect.width() > 700.0
                    && r.rect.height() > 900.0 =>
            {
                Some(r.rect)
            }
            _ => None,
        });
    }
    let paper = paper.expect("the editor must display a score page");
    let staff = &app.cached_pages[0].staves[0];
    let position = paper.min + Vec2::new(staff.content_x, staff.y + 4.0 * staff.spacing) * app.zoom;
    for (frame, pressed) in [(3, true), (4, false)] {
        let _ = ctx.run(
            raw_input(
                frame,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                Default::default(),
            ),
            |ctx| app.ui(ctx),
        );
    }
    let note = &app.score.parts[0].measures[0].notes[0];
    assert_eq!(note.start, 0);
    assert_eq!(
        note.pitches[0],
        Pitch {
            step: Step::E,
            octave: 4,
            alter: 0
        }
    );
    app.score.validate().unwrap();
}

fn event(start: u32, duration: Duration, pitches: &[u8]) -> Note {
    Note {
        start,
        duration,
        tuplet: None,
        layer: 0,
        dotted: false,
        pitches: pitches.iter().map(|midi| Pitch::from_midi(*midi)).collect(),
        velocity: 88,
        staccato: false,
        articulations: vec![],
        tied: false,
    }
}

#[test]
fn undo_removed_active_staff_clamps_cursor_before_the_next_panel() {
    let original = Score::blank();
    let mut expanded = original.clone();
    expanded.add_part(Instrument::Flute);
    expanded.add_measures(4);
    let mut app = controller(expanded);
    app.undo.push(original.clone());
    app.active_part = 2;
    app.cursor_measure = 7;
    app.cursor_tick = 10_000;
    app.selected = Some(Selection {
        part: 2,
        measure: 7,
        note: 0,
    });

    app.undo();
    assert_eq!(app.score, original);
    assert!(app.active_part < app.score.parts.len());
    assert!(app.cursor_measure < app.score.measure_count());
    assert!(app.cursor_tick < app.score.measure_ticks());
    assert!(app.selected.is_none());

    // Transport and inspector index the active staff during the same frame.
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let _ = ctx.run(raw_input(0, vec![], egui::Modifiers::default()), |ctx| {
        app.ui(ctx)
    });
    app.redo();
    assert_eq!(app.score.parts.len(), 3);
    assert_eq!(app.score.measure_count(), 8);
    app.score.validate().unwrap();
}

#[test]
fn chord_entry_near_barline_uses_existing_event_duration() {
    let mut score = Score::blank();
    score.parts[0].measures[0].notes = vec![event(1440, Duration::Quarter, &[60])];
    let mut app = controller(score);
    app.duration = Duration::Whole;
    app.enter_note(0, 0, 1440, Pitch::from_midi(64), true);

    let note = &app.score.parts[0].measures[0].notes[0];
    assert_eq!(note.duration, Duration::Quarter);
    assert_eq!(
        note.pitches.iter().map(|p| p.midi()).collect::<Vec<_>>(),
        vec![60, 64]
    );
    app.score.validate().unwrap();
}

#[test]
fn overlapping_entry_leaves_existing_music_intact() {
    let mut app = controller(Score::blank());
    app.duration = Duration::Half;
    app.enter_note(0, 0, 0, Pitch::from_midi(60), false);
    let before = app.score.clone();
    app.duration = Duration::Quarter;
    app.enter_note(0, 0, 480, Pitch::from_midi(64), false);

    assert_eq!(app.score, before);
    assert!(app.status.contains("overlap"));
    app.score.validate().unwrap();
}

#[test]
fn endpoint_transposition_rejects_the_whole_chord_without_duplicates() {
    for (pitches, semitones) in [([126, 127], 1), ([0, 1], -1)] {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![event(0, Duration::Quarter, &pitches)];
        let mut app = controller(score.clone());
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        app.transpose(semitones);
        assert_eq!(app.score, score);
        app.score.validate().unwrap();
    }
    let mut score = Score::blank();
    score.parts[0].measures[0].notes = vec![event(0, Duration::Quarter, &[60, 64, 67])];
    let mut app = controller(score);
    app.selected = Some(Selection {
        part: 0,
        measure: 0,
        note: 0,
    });
    app.transpose(12);
    assert_eq!(
        app.score.parts[0].measures[0].notes[0]
            .pitches
            .iter()
            .map(|p| p.midi())
            .collect::<Vec<_>>(),
        vec![72, 76, 79]
    );
    app.score.validate().unwrap();
}

#[test]
fn meter_shrink_repositions_cursor_so_keyboard_entry_can_continue() {
    let mut app = controller(Score::blank());
    app.cursor_measure = 2;
    app.cursor_tick = 1440;
    app.duration = Duration::Quarter;
    app.score.time_numerator = 3;
    app.score.time_denominator = 8;
    app.clamp_cursor();
    assert_eq!(app.cursor_tick, 0);
    app.enter_note(
        app.active_part,
        app.cursor_measure,
        app.cursor_tick,
        Pitch::from_midi(60),
        false,
    );
    assert_eq!(app.score.parts[0].measures[2].notes.len(), 1);
    assert_eq!(app.score.parts[0].measures[2].notes[0].start, 0);
    assert_eq!(app.cursor_tick, 480);
    app.score.validate().unwrap();
}

#[test]
fn chord_entry_rejects_enharmonic_duplicates() {
    let mut app = controller(Score::blank());
    app.enter_note(0, 0, 0, Pitch::from_midi(61), false);
    app.enter_note(
        0,
        0,
        0,
        Pitch {
            step: Step::D,
            octave: 4,
            alter: -1,
        },
        true,
    );
    assert_eq!(app.score.parts[0].measures[0].notes[0].pitches.len(), 1);
    app.score.validate().unwrap();
}

fn raw_input(frame: u32, events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 1000.0))),
        time: Some(f64::from(frame) / 60.0),
        events,
        modifiers,
        focused: true,
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(1.0);
    input
}

fn key(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers,
    }
}

#[test]
fn keyboard_note_entry_undo_and_redo_are_recorded_by_real_ui_frames() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.write_mode = true;
    let plain = egui::Modifiers::default();
    let _ = ctx.run(raw_input(0, vec![], plain), |ctx| app.ui(ctx));
    let _ = ctx.run(
        raw_input(1, vec![key(egui::Key::C, true, plain)], plain),
        |ctx| app.ui(ctx),
    );
    assert_eq!(app.score.parts[0].measures[0].notes.len(), 1);
    assert_eq!(
        app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
        60
    );
    assert!(app.dirty());
    assert_eq!(app.undo.len(), 1);

    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    let _ = ctx.run(
        raw_input(
            2,
            vec![
                key(egui::Key::C, false, plain),
                key(egui::Key::Z, true, command),
            ],
            command,
        ),
        |ctx| app.ui(ctx),
    );
    assert!(app.score.parts[0].measures[0].notes.is_empty());
    assert!(!app.dirty());
    let _ = ctx.run(
        raw_input(
            3,
            vec![
                key(egui::Key::Z, false, command),
                key(egui::Key::Y, true, command),
            ],
            command,
        ),
        |ctx| app.ui(ctx),
    );
    assert_eq!(app.score.parts[0].measures[0].notes.len(), 1);
    app.score.validate().unwrap();
}

#[derive(Clone)]
struct Texture {
    size: [usize; 2],
    pixels: Vec<Color32>,
}
impl Texture {
    /// Match a GPU's clamp-to-edge linear sampler, including the half-texel offset.
    fn sample(&self, uv: Pos2) -> [f32; 4] {
        let x = uv.x.clamp(0.0, 1.0) * self.size[0] as f32 - 0.5;
        let y = uv.y.clamp(0.0, 1.0) * self.size[1] as f32 - 0.5;
        let ix = x.floor() as i32;
        let iy = y.floor() as i32;
        let fx = x - x.floor();
        let fy = y - y.floor();
        let texel = |x: i32, y: i32| {
            self.pixels[y.clamp(0, self.size[1] as i32 - 1) as usize * self.size[0]
                + x.clamp(0, self.size[0] as i32 - 1) as usize]
                .to_array()
        };
        let a = texel(ix, iy);
        let b = texel(ix + 1, iy);
        let c = texel(ix, iy + 1);
        let d = texel(ix + 1, iy + 1);
        std::array::from_fn(|i| {
            (f32::from(a[i]) * (1.0 - fx) + f32::from(b[i]) * fx) * (1.0 - fy)
                + (f32::from(c[i]) * (1.0 - fx) + f32::from(d[i]) * fx) * fy
        })
    }
}

fn apply_textures(textures: &mut HashMap<egui::TextureId, Texture>, delta: &egui::TexturesDelta) {
    for (id, change) in &delta.set {
        let egui::ImageData::Color(image) = &change.image;
        if let Some([x, y]) = change.pos {
            let existing = textures
                .get_mut(id)
                .expect("partial update must target an allocated texture");
            assert!(x + image.size[0] <= existing.size[0] && y + image.size[1] <= existing.size[1]);
            for row in 0..image.size[1] {
                let destination = (y + row) * existing.size[0] + x;
                existing.pixels[destination..destination + image.size[0]]
                    .copy_from_slice(&image.pixels[row * image.size[0]..(row + 1) * image.size[0]]);
            }
        } else {
            textures.insert(
                *id,
                Texture {
                    size: image.size,
                    pixels: image.pixels.clone(),
                },
            );
        }
    }
}

struct SoftwarePainter {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}
impl SoftwarePainter {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgb: vec![255; width * height * 3],
        }
    }
    fn paint(
        &mut self,
        primitives: &[egui::ClippedPrimitive],
        textures: &HashMap<egui::TextureId, Texture>,
        scale: f32,
    ) {
        for clipped in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = &clipped.primitive else {
                panic!("Headless test cannot rasterize GPU paint callbacks")
            };
            assert!(mesh.is_valid());
            let texture = textures
                .get(&mesh.texture_id)
                .expect("mesh texture must exist");
            let clip =
                Rect::from_min_max(clipped.clip_rect.min * scale, clipped.clip_rect.max * scale);
            for indices in mesh.indices.as_chunks::<3>().0 {
                self.triangle(
                    [
                        mesh.vertices[indices[0] as usize],
                        mesh.vertices[indices[1] as usize],
                        mesh.vertices[indices[2] as usize],
                    ],
                    clip,
                    texture,
                    scale,
                );
            }
        }
    }
    fn triangle(
        &mut self,
        mut vertices: [egui::epaint::Vertex; 3],
        clip: Rect,
        texture: &Texture,
        scale: f32,
    ) {
        for vertex in &mut vertices {
            vertex.pos *= scale;
        }
        let edge =
            |a: Pos2, b: Pos2, p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let mut area = edge(vertices[0].pos, vertices[1].pos, vertices[2].pos);
        if area.abs() < 1e-6 {
            return;
        }
        if area < 0.0 {
            vertices.swap(1, 2);
            area = -area;
        }
        let [a, b, c] = vertices;
        let min_x = a
            .pos
            .x
            .min(b.pos.x)
            .min(c.pos.x)
            .max(clip.min.x)
            .max(0.0)
            .floor() as usize;
        let max_x = a
            .pos
            .x
            .max(b.pos.x)
            .max(c.pos.x)
            .min(clip.max.x)
            .min(self.width as f32)
            .ceil()
            .max(0.0) as usize;
        let min_y = a
            .pos
            .y
            .min(b.pos.y)
            .min(c.pos.y)
            .max(clip.min.y)
            .max(0.0)
            .floor() as usize;
        let max_y = a
            .pos
            .y
            .max(b.pos.y)
            .max(c.pos.y)
            .min(clip.max.y)
            .min(self.height as f32)
            .ceil()
            .max(0.0) as usize;
        let top_left = |a: Pos2, b: Pos2| b.y < a.y || (b.y == a.y && b.x > a.x);
        let top_left = [
            top_left(b.pos, c.pos),
            top_left(c.pos, a.pos),
            top_left(a.pos, b.pos),
        ];
        let colors = [a.color.to_array(), b.color.to_array(), c.color.to_array()];
        for y in min_y..max_y {
            for x in min_x..max_x {
                let point = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
                if point.x < clip.min.x
                    || point.x >= clip.max.x
                    || point.y < clip.min.y
                    || point.y >= clip.max.y
                {
                    continue;
                }
                let weights = [
                    edge(b.pos, c.pos, point),
                    edge(c.pos, a.pos, point),
                    edge(a.pos, b.pos, point),
                ];
                if weights
                    .iter()
                    .enumerate()
                    .any(|(i, w)| *w < 0.0 || (*w == 0.0 && !top_left[i]))
                {
                    continue;
                }
                let weights = weights.map(|weight| weight / area);
                let uv = Pos2::new(
                    a.uv.x * weights[0] + b.uv.x * weights[1] + c.uv.x * weights[2],
                    a.uv.y * weights[0] + b.uv.y * weights[1] + c.uv.y * weights[2],
                );
                let texel = texture.sample(uv);
                let source: [f32; 4] = std::array::from_fn(|i| {
                    texel[i] / 255.0
                        * (f32::from(colors[0][i]) * weights[0]
                            + f32::from(colors[1][i]) * weights[1]
                            + f32::from(colors[2][i]) * weights[2])
                });
                if source[3] <= 0.0 {
                    continue;
                }
                let pixel = (y * self.width + x) * 3;
                for channel in 0..3 {
                    self.rgb[pixel + channel] = (source[channel]
                        + f32::from(self.rgb[pixel + channel]) * (1.0 - source[3] / 255.0))
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    fn ppm(&self) -> Vec<u8> {
        let mut bytes = format!("P6\n{} {}\n255\n", self.width, self.height).into_bytes();
        bytes.extend_from_slice(&self.rgb);
        bytes
    }
}

#[test]
fn headless_full_editor_renders_notation_and_font_textures() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::demo());
    screenshot_theme(&mut app, &ctx);
    if let Some(path) = std::env::var_os("ENCORE_LAB_SOUNDFONT") {
        app.set_audio_engine(AudioEngine::from_soundfont(&PathBuf::from(path)).unwrap());
    }
    let mut textures = HashMap::new();
    let mut painter = SoftwarePainter::new(1440, 1000);
    for frame in 0..3 {
        let output = ctx.run(
            raw_input(frame, vec![], egui::Modifiers::default()),
            |ctx| app.ui(ctx),
        );
        assert!(!output.shapes.is_empty());
        apply_textures(&mut textures, &output.textures_delta);
        if frame == 2 {
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            assert!(meshes.iter().any(|primitive| matches!(&primitive.primitive,egui::epaint::Primitive::Mesh(mesh) if mesh.indices.len()>100)));
            painter.paint(&meshes, &textures, output.pixels_per_point);
        }
        for id in output.textures_delta.free {
            textures.remove(&id);
        }
    }
    assert!(!app.cached_pages.is_empty());
    app.score.validate().unwrap();
    let dark_pixels = painter
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|rgb| rgb.iter().all(|c| *c < 110))
        .count();
    let paper_pixels = painter
        .rgb
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|rgb| rgb.iter().all(|c| *c > 250))
        .count();
    assert!(
        dark_pixels > 3000,
        "Expected visible staff lines, notes, labels, and fonts; found {dark_pixels} dark pixels"
    );
    assert!(
        paper_pixels > 100_000,
        "Expected a visible white score page"
    );
    if let Some(path) = std::env::var_os("ENCORE_HEADLESS_SCREENSHOT") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, painter.ppm()).unwrap();
        eprintln!("Headless editor screenshot: {}", path.display());
    }
}

#[test]
fn instrument_lab_opening_and_adjusting_controls_preserves_the_score() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let original = Score::demo();
    let mut app = controller(original.clone());
    app.lab_open = true;
    app.lab_instrument = Instrument::Timpani;
    app.lab_midi = instruments::profile(Instrument::Timpani).audition;
    app.lab_velocity = 64;
    app.lab_hold = 2.0;
    for frame in 0..3 {
        let _ = ctx.run(raw_input(frame, vec![], Default::default()), |ctx| {
            app.ui(ctx)
        });
    }
    assert!(app.lab_open);
    assert_eq!(app.score, original);
    assert_eq!(app.saved, original);
    assert!(!app.dirty());
    assert!(app.undo.is_empty() && app.redo.is_empty());
    assert!(app.playback.is_none() && app.audition_playback.is_none());
    assert!(app.worker.is_none() && app.audition_worker.is_none());
}

#[test]
fn instrument_lab_blocks_letter_note_entry_in_real_ui_frames() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let original = Score::blank();
    let mut app = controller(original.clone());
    app.write_mode = true;
    app.lab_open = true;
    let plain = egui::Modifiers::default();
    let _ = ctx.run(raw_input(0, vec![], plain), |ctx| app.ui(ctx));
    for (index, letter) in [
        egui::Key::A,
        egui::Key::B,
        egui::Key::C,
        egui::Key::D,
        egui::Key::E,
        egui::Key::F,
        egui::Key::G,
    ]
    .into_iter()
    .enumerate()
    {
        for (offset, pressed) in [(1, true), (2, false)] {
            let _ = ctx.run(
                raw_input(
                    index as u32 * 2 + offset,
                    vec![key(letter, pressed, plain)],
                    plain,
                ),
                |ctx| app.ui(ctx),
            );
        }
    }
    assert_eq!(app.score, original);
    assert_eq!(app.cursor_tick, 0);
    assert!(app.undo.is_empty());
    assert!(app.audition_playback.is_none());
}

#[test]
fn instrument_keyboards_cover_useful_low_notes_without_midi_overflow() {
    for &instrument in Instrument::ALL {
        let info = instruments::profile(instrument);
        let base = keyboard_base_for(instrument);
        let end = u16::from(base) + 47;
        assert_eq!(
            base % 12,
            0,
            "Keyboard must begin with a full octave: {}",
            instrument.name()
        );
        assert!(end <= 127, "Keyboard overflow: {}", instrument.name());
        assert!(
            u16::from(base) <= u16::from(info.audition) && u16::from(info.audition) <= end,
            "Audition is off keyboard: {}",
            instrument.name()
        );
        if instrument != Instrument::Piano {
            assert!(
                base <= info.low && u16::from(info.low) <= end,
                "Low concert pitch is off keyboard: {}",
                instrument.name()
            );
        }
    }
}

#[test]
fn switching_active_instrument_moves_the_keyboard_to_its_register() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut score = Score::blank();
    score.add_part(Instrument::DoubleBass);
    let mut app = controller(score.clone());
    let _ = ctx.run(raw_input(0, vec![], Default::default()), |ctx| app.ui(ctx));
    app.active_part = 2;
    let _ = ctx.run(raw_input(1, vec![], Default::default()), |ctx| app.ui(ctx));
    assert_eq!(app.keyboard_instrument, Instrument::DoubleBass);
    assert_eq!(app.keyboard_base, keyboard_base_for(Instrument::DoubleBass));
    assert!(app.keyboard_base <= 28 && app.keyboard_base + 47 >= 28);
    assert_eq!(app.score, score);
}

#[test]
fn lowest_keyboard_octave_renders_signed_c_minus_one_label() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.keyboard_base = 0;
    app.keyboard_instrument = Instrument::Piano;
    let mut saw_low_c = false;
    for frame in 0..3 {
        let output = ctx.run(raw_input(frame, vec![], Default::default()), |ctx| {
            app.ui(ctx)
        });
        saw_low_c |= output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()=="C-1"));
    }
    assert!(
        saw_low_c,
        "The bottom MIDI octave must use a signed scientific-pitch label"
    );
    assert!(app.audition_playback.is_none());
}

#[test]
fn stopping_audition_drops_queued_audio_without_opening_a_device() {
    let mut app = controller(Score::blank());
    let original = app.score.clone();
    let status = app.status.clone();
    let (sender, receiver) = mpsc::channel();
    app.audition_worker = Some(receiver);
    sender
        .send(Ok((vec![0.0; 128], "stale audition".into())))
        .unwrap();
    app.stop_audition();
    assert!(app.audition_worker.is_none());
    assert!(sender.send(Ok((vec![], "late result".into()))).is_err());
    app.poll_worker();
    assert!(app.audition_playback.is_none());
    assert!(app.playback.is_none());
    assert_eq!(app.score, original);
    assert_eq!(app.status, status);
}

#[test]
fn renderer_and_bank_failures_are_visible_and_preserve_music() {
    let mut app = controller(Score::carol_demo());
    let original = app.score.clone();
    let (sender, receiver) = mpsc::channel();
    app.bank_worker = Some(receiver);
    sender.send(Err("Missing timpani preset".into())).unwrap();
    app.poll_worker();
    assert!(app.bank_worker.is_none());
    assert!(app.status.contains("Missing timpani preset"));
    assert!(matches!(app.audio_engine, AudioEngine::Procedural));

    let (sender, receiver) = mpsc::channel();
    app.worker = Some(receiver);
    app.rendering_audio = true;
    sender
        .send(JobResult::Audio(Err("Invalid sample data".into())))
        .unwrap();
    app.poll_worker();
    assert!(app.status.contains("Invalid sample data"));
    assert!(!app.rendering_audio && app.worker.is_none());
    assert!(app.playback.is_none());
    assert_eq!(app.score, original);
    assert!(!app.dirty());
}

#[test]
fn changing_sounds_discards_pending_playback_without_editing_the_score() {
    let mut app = controller(Score::carol_demo());
    let original = app.score.clone();
    let (sender, receiver) = mpsc::channel();
    app.worker = Some(receiver);
    app.rendering_audio = true;
    sender.send(JobResult::Audio(Ok(vec![0.0; 128]))).unwrap();
    app.set_audio_engine(AudioEngine::Procedural);
    app.poll_worker();
    assert!(app.worker.is_none() && !app.rendering_audio);
    assert!(app.playback.is_none());
    assert_eq!(app.score, original);
    assert!(!app.dirty());
}

#[test]
fn escape_stops_a_queued_audition_while_the_instrument_lab_is_open() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::blank());
    app.lab_open = true;
    let original = app.score.clone();
    let (_sender, receiver) = mpsc::channel();
    app.audition_worker = Some(receiver);
    let _ = ctx.run(
        raw_input(
            0,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            Default::default(),
        ),
        |ctx| app.ui(ctx),
    );
    assert!(app.audition_worker.is_none());
    assert_eq!(app.score, original);
    assert!(app.playback.is_none() && app.audition_playback.is_none());
}

#[test]
fn epic_demo_request_protects_unsaved_music_and_never_opens_audio() {
    let ctx = egui::Context::default();
    let mut app = controller(Score::blank());
    app.score.title = "Keep this unsaved music".into();
    let edited = app.score.clone();
    app.request(Action::CarolDemo, &ctx);
    assert!(matches!(app.pending, Some(Action::CarolDemo)));
    assert_eq!(app.score, edited);
    assert!(app.dirty());
    assert!(app.worker.is_none() && app.audition_worker.is_none());
    assert!(app.playback.is_none() && app.audition_playback.is_none());
    // Cancel leaves the score intact, as the unsaved-changes dialog does.
    app.pending = None;
    assert_eq!(app.score, edited);
    // Once the current score is saved, the same action can replace it safely.
    app.saved = app.score.clone();
    app.request(Action::CarolDemo, &ctx);
    assert!(app.pending.is_none());
    assert!(!app.dirty());
    assert_ne!(app.score, edited);
    app.score.validate().unwrap();
    assert!(app.playback.is_none() && app.audition_playback.is_none());
}

#[test]
fn headless_instrument_lab_renders_timpani_register_and_controls() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let mut app = controller(Score::demo());
    screenshot_theme(&mut app, &ctx);
    app.lab_open = true;
    app.lab_instrument = Instrument::Timpani;
    app.lab_midi = instruments::profile(Instrument::Timpani).audition;
    app.lab_velocity = 96;
    app.lab_hold = 1.8;
    if let Some(path) = std::env::var_os("ENCORE_LAB_SOUNDFONT") {
        app.set_audio_engine(AudioEngine::from_soundfont(&PathBuf::from(path)).unwrap());
    }
    let mut textures = HashMap::new();
    let mut painter = SoftwarePainter::new(1440, 1000);
    let mut saw_title = false;
    let mut saw_timpani = false;
    for frame in 0..20 {
        let output = ctx.run(raw_input(frame, vec![], Default::default()), |ctx| {
            app.ui(ctx)
        });
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                saw_title |= text.galley.text() == "Find your sound.";
                saw_timpani |= text.galley.text() == "Timpani";
            }
        }
        apply_textures(&mut textures, &output.textures_delta);
        if frame == 19 {
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            painter.paint(&meshes, &textures, output.pixels_per_point);
        }
        for id in output.textures_delta.free {
            textures.remove(&id);
        }
    }
    assert!(
        saw_title && saw_timpani,
        "Instrument lab title and selected instrument must render"
    );
    assert!(app.lab_open && app.audition_playback.is_none());
    assert!(
        painter
            .rgb
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|pixel| pixel.iter().all(|c| *c < 110))
            .count()
            > 3000
    );
    if let Some(path) = std::env::var_os("ENCORE_LAB_SCREENSHOT") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, painter.ppm()).unwrap();
        eprintln!("Headless instrument-lab screenshot: {}", path.display());
    }
}

#[test]
fn library_search_and_family_filters_preserve_the_composition() {
    let original = Score::demo();
    let mut app = controller(original.clone());
    assert_eq!(app.library_results().len(), 128);

    app.library_search = "  SAX  ".into();
    let saxophones = app.library_results();
    assert_eq!(saxophones.len(), 4);
    assert!(
        saxophones
            .iter()
            .all(|instrument| (64..=67).contains(&instrument.gm_program()))
    );

    app.library_search.clear();
    app.library_family = Some(Instrument::Flute.family());
    let woodwinds = app.library_results();
    assert_eq!(woodwinds.len(), 8);
    assert!(woodwinds.contains(&Instrument::Flute));
    assert!(
        woodwinds
            .iter()
            .all(|instrument| instrument.family() == Instrument::Flute.family())
    );

    app.library_search = "sax".into();
    assert!(
        app.library_results().is_empty(),
        "Search must narrow the selected family"
    );
    app.library_search = "no such instrument".into();
    assert!(app.library_results().is_empty());
    assert_eq!(app.score, original);
    assert_eq!(app.saved, original);
    assert!(!app.dirty() && app.undo.is_empty() && app.redo.is_empty());
}

#[test]
fn selecting_and_replacing_a_library_voice_keeps_written_music() {
    let original = Score::demo();
    let mut app = controller(original.clone());
    let tenor = Instrument::GeneralMidi(66);
    app.lab_midi = 108;
    app.select_library_instrument(tenor);
    assert_eq!(app.lab_instrument, tenor);
    assert_eq!(app.lab_midi, instruments::profile(tenor).audition);
    assert_eq!(
        app.score, original,
        "Browsing a voice must not change the score"
    );
    app.replace_library_staff();
    assert_eq!(app.score.parts[0].instrument.gm_program(), 66);
    assert_eq!(app.score.parts[0].clef, Clef::Bass);
    assert_eq!(app.score.parts[0].measures, original.parts[0].measures);
    assert_eq!(app.score.parts[0].volume, original.parts[0].volume);
    assert_eq!(app.score.parts[0].pan, original.parts[0].pan);
    assert_eq!(&app.score.parts[1..], &original.parts[1..]);
    assert_eq!(
        app.score.parts[0].name, original.parts[0].name,
        "A custom staff label must survive replacement"
    );
    app.score.validate().unwrap();

    let mut unnamed = controller(Score::blank());
    unnamed.score.parts[0].name = Instrument::Piano.name().into();
    unnamed.select_library_instrument(Instrument::GeneralMidi(65));
    unnamed.replace_library_staff();
    assert_eq!(unnamed.score.parts[0].name, "Alto saxophone");
    assert_eq!(unnamed.score.parts[0].clef, Clef::Treble);
}

fn visible_text_rect(output: &egui::FullOutput, label: &str) -> Option<Rect> {
    output.shapes.iter().find_map(|shape| {
        if let egui::Shape::Text(text) = &shape.shape {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            if text.galley.text() == label && shape.clip_rect.contains_rect(rect) {
                return Some(rect);
            }
        }
        None
    })
}

#[test]
fn header_appearance_switch_clicks_change_both_themes_without_editing_music() {
    for size in [Vec2::new(1440.0, 1000.0), Vec2::new(980.0, 680.0)] {
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let original = Score::demo();
        let mut app = controller(original.clone());
        app.undo.push(Score::blank());
        app.redo.push(original.clone());
        let undo = app.undo.clone();
        let redo = app.redo.clone();
        let viewport = Rect::from_min_size(Pos2::ZERO, size);
        let mut frame = 0;
        for (label, expected) in [("Light mode", Theme::Light), ("Dark mode", Theme::Dark)] {
            let mut button = None;
            for _ in 0..3 {
                let mut input = raw_input(frame, vec![], Default::default());
                input.screen_rect = Some(viewport);
                let output = ctx.run(input, |ctx| app.ui(ctx));
                button = visible_text_rect(&output, label);
                frame += 1;
            }
            let button = button
                .unwrap_or_else(|| panic!("{label} must be visible in the header at {size:?}"));
            assert!(viewport.contains_rect(button));
            let position = button.center();
            for pressed in [true, false] {
                let mut input = raw_input(
                    frame,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    Default::default(),
                );
                input.screen_rect = Some(viewport);
                let _ = ctx.run(input, |ctx| app.ui(ctx));
                frame += 1;
            }
            assert_eq!(app.theme, expected);
            assert_eq!(ctx.style().visuals.dark_mode, expected == Theme::Dark);
            assert_eq!(
                ctx.style().visuals.panel_fill,
                Palette::for_theme(expected).surface
            );
            assert_eq!(app.score, original);
            assert_eq!(app.saved, original);
            assert_eq!(app.undo, undo);
            assert_eq!(app.redo, redo);
            assert!(!app.dirty());
            assert!(app.worker.is_none() && app.audition_worker.is_none());
        }
    }
}

#[test]
fn adding_a_new_library_instrument_is_undoable_through_real_ui_frames() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let original = Score::demo();
    let mut app = controller(original.clone());
    app.lab_open = true;
    app.select_library_instrument(Instrument::GeneralMidi(65));
    let mut add = None;
    for frame in 0..20 {
        let output = ctx.run(raw_input(frame, vec![], Default::default()), |ctx| {
            app.ui(ctx)
        });
        add = visible_text_rect(&output, "+  Add staff");
    }
    let position = add
        .expect("Add staff must be visible without scrolling")
        .center();
    for (frame, pressed) in [(20, true), (21, false)] {
        let _ = ctx.run(
            raw_input(
                frame,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                Default::default(),
            ),
            |ctx| app.ui(ctx),
        );
    }
    assert_eq!(app.score.parts.len(), original.parts.len() + 1);
    assert_eq!(app.active_part, original.parts.len());
    let added = app.score.parts.last().unwrap();
    assert_eq!(added.instrument.gm_program(), 65);
    assert_eq!(added.clef, Clef::Treble);
    assert_eq!(added.measures.len(), original.measure_count());
    assert!(
        added
            .measures
            .iter()
            .all(|measure| measure.notes.is_empty())
    );
    assert_eq!(
        &app.score.parts[..original.parts.len()],
        original.parts.as_slice()
    );
    assert_eq!(app.undo.len(), 1);
    assert!(app.playback.is_none() && app.audition_playback.is_none());
    app.undo();
    assert_eq!(app.score, original);
    app.redo();
    assert_eq!(app.score.parts.last().unwrap().instrument.gm_program(), 65);
    app.score.validate().unwrap();
}

#[test]
fn instrument_library_keeps_primary_controls_visible_at_the_smallest_window() {
    let ctx = egui::Context::default();
    configure_context(&ctx);
    let original = Score::demo();
    let mut app = controller(original.clone());
    screenshot_theme(&mut app, &ctx);
    app.lab_open = true;
    app.select_library_instrument(Instrument::Timpani);
    if let Some(path) = std::env::var_os("ENCORE_LAB_SOUNDFONT") {
        app.set_audio_engine(AudioEngine::from_soundfont(&PathBuf::from(path)).unwrap());
    }
    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 680.0));
    let mut visible = HashMap::new();
    let mut textures = HashMap::new();
    let mut screenshot = std::env::var_os("ENCORE_SMALL_LIBRARY_SCREENSHOT")
        .map(|path| (PathBuf::from(path), SoftwarePainter::new(980, 680)));
    for frame in 0..20 {
        let mut input = raw_input(frame, vec![], Default::default());
        input.screen_rect = Some(viewport);
        let output = ctx.run(input, |ctx| app.ui(ctx));
        if screenshot.is_some() {
            apply_textures(&mut textures, &output.textures_delta);
        }
        if frame == 19 {
            for label in [
                "Find your sound.",
                "Search instruments…",
                "▶  Play note",
                "+  Add staff",
                "Use on active staff",
            ] {
                visible.insert(label, visible_text_rect(&output, label));
            }
            if let Some((_, painter)) = screenshot.as_mut() {
                let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
                painter.paint(&meshes, &textures, output.pixels_per_point);
            }
        }
        for id in output.textures_delta.free {
            textures.remove(&id);
        }
    }
    for (label, rect) in visible {
        let rect =
            rect.unwrap_or_else(|| panic!("{label} must be visible without scrolling at 980×680"));
        assert!(
            viewport.contains_rect(rect),
            "{label} must remain inside the viewport: {rect:?}"
        );
    }
    assert_eq!(app.score, original);
    assert!(app.undo.is_empty());
    if let Some((path, painter)) = screenshot {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, painter.ppm()).unwrap();
        eprintln!(
            "Small-window instrument library screenshot: {}",
            path.display()
        );
    }
}
