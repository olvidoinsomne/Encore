use super::*;
use encore::parts::{self, ChordFilter, LinkedPart, PageSize, StaffVoicing, Transposition};

impl EncoreApp {
    pub(super) fn activate_linked_part(&mut self, index: usize) {
        let definitions = parts::definitions(&self.score);
        let Some(part) = definitions.get(index) else {
            return;
        };
        self.selected_linked_part = index;
        self.part_view = true;
        if !part.staves.contains(&self.active_part) {
            self.active_part = part.staves[0];
        }
        self.selected = None;
        self.clear_region_selection();
        self.notation_editor = Default::default();
        if self
            .grace_editor
            .selected
            .is_some_and(|selection| !part.staves.contains(&selection.anchor.part))
        {
            self.grace_editor.open = false;
            self.grace_editor.selected = None;
        }
        if self.chord_editor.active {
            self.chord_click(self.active_part, self.cursor_measure, self.cursor_tick);
        }
    }

    pub(super) fn selected_part_projection(&self) -> Result<Option<parts::Projection>, String> {
        if !self.part_view {
            return Ok(None);
        }
        let definitions = parts::definitions(&self.score);
        let definition = definitions
            .get(self.selected_linked_part)
            .or_else(|| definitions.first())
            .ok_or("The score has no linked parts.")?;
        parts::project(&self.score, definition).map(Some)
    }

    pub(super) fn export_all_linked_parts(&mut self) {
        if self.worker.is_some() {
            self.status = "Please wait for the current export to finish.".into();
            return;
        }
        if let Some(directory) = rfd::FileDialog::new()
            .set_title("Export linked parts as PDFs")
            .pick_folder()
        {
            let score = self.score.clone();
            let (tx, rx) = mpsc::channel();
            self.worker = Some(rx);
            self.status = format!("Exporting linked-part PDFs to {}…", directory.display());
            std::thread::spawn(move || {
                let result = parts::export_pdf_parts(&score, &directory).map(|_| directory);
                let _ = tx.send(JobResult::Export(result));
            });
        }
    }

    pub(super) fn linked_parts_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_parts {
            return;
        }
        let mut open = true;
        let mut definitions = parts::definitions(&self.score);
        let original = definitions.clone();
        let mut reset = false;
        let mut export = false;
        let mut remove = false;
        egui::Window::new("Manage linked parts")
            .open(&mut open).default_width(700.0).default_height(660.0).vscroll(true)
            .show(ctx, |ui| {
                ui.label("Parts share all notes, lyrics, slurs and repeats with the score. Each part keeps its own page format and written transposition.");
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Generate from instruments").clicked() { reset = true; }
                    if ui.add_enabled(definitions.len() < 128, egui::Button::new("New part")).clicked() {
                        let mut ordinal = 1;
                        while definitions.iter().any(|part| part.id == format!("custom-{ordinal}")) { ordinal += 1; }
                        definitions.push(LinkedPart {
                            id: format!("custom-{ordinal}"),
                            name: format!("Part {}", definitions.len() + 1),
                            staves: vec![self.active_part.min(self.score.parts.len().saturating_sub(1))],
                            voicing: Vec::new(),
                            transposition: Transposition::CONCERT,
                            layout: Default::default(),
                        });
                        self.selected_linked_part = definitions.len() - 1;
                    }
                    if ui.button("Export all PDFs…").clicked() { export = true; }
                });
                self.selected_linked_part = self.selected_linked_part.min(definitions.len().saturating_sub(1));
                egui::ComboBox::from_label("Linked part")
                    .selected_text(definitions.get(self.selected_linked_part).map(|part| part.name.as_str()).unwrap_or("No parts"))
                    .show_ui(ui, |ui| {
                        for (index, part) in definitions.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_linked_part, index, &part.name);
                        }
                    });
                let definition_count = definitions.len();
                let Some(part) = definitions.get_mut(self.selected_linked_part) else { return; };
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut part.name);
                    if ui.add_enabled(definition_count > 1, egui::Button::new("Delete part")).clicked() { remove = true; }
                });
                ui.separator();
                ui.label("Included staves (score order)");
                egui::ScrollArea::vertical().id_salt("part-staves").max_height(145.0).show(ui, |ui| {
                    for (index, staff) in self.score.parts.iter().enumerate() {
                        let mut included = part.staves.contains(&index);
                        if ui.add_enabled(!included || part.staves.len() > 1,
                            egui::Checkbox::new(&mut included, format!("{} · {}", index + 1, staff.name))).changed() {
                            if included { part.staves.push(index); part.staves.sort_unstable(); }
                            else { part.staves.retain(|staff| *staff != index); part.voicing.retain(|rule| rule.staff != index); }
                        }
                    }
                });
                ui.separator();
                ui.collapsing("Staff voicing · layers, voices and chord positions", |ui| {
                    ui.small("Voiced parts are read-only views. Edit their notes in the source score.");
                    for &staff in &part.staves {
                        ui.push_id(("staff-voicing", staff), |ui| {
                            let mut enabled = part.voicing.iter().any(|rule| rule.staff == staff);
                            if ui.checkbox(&mut enabled, format!("Custom voicing: {}", self.score.parts[staff].name)).changed() {
                                if enabled { part.voicing.push(StaffVoicing::new(staff)); }
                                else { part.voicing.retain(|rule| rule.staff != staff); }
                            }
                            if let Some(rule) = part.voicing.iter_mut().find(|rule| rule.staff == staff) {
                                staff_voicing_controls(ui, rule);
                            }
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Written transposition");
                    if ui.button("Concert pitch").clicked() { part.transposition = Transposition::CONCERT; }
                    if ui.button("Instrument default").clicked()
                        && let Some(staff) = part.staves.first().and_then(|index| self.score.parts.get(*index)) {
                        part.transposition = Transposition::for_instrument(staff.instrument);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Semitones"); ui.add(egui::DragValue::new(&mut part.transposition.semitones).range(-36..=36));
                    ui.label("Diatonic steps"); ui.add(egui::DragValue::new(&mut part.transposition.diatonic_steps).range(-21..=21));
                });
                ui.small("Written pitch = sounding pitch + this interval. B♭ clarinet: +2 semitones, +1 step.");
                ui.separator();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("Paper").selected_text(part.layout.page_size.name()).show_ui(ui, |ui| {
                        for &size in PageSize::ALL { ui.selectable_value(&mut part.layout.page_size, size, size.name()); }
                    });
                    ui.checkbox(&mut part.layout.landscape, "Landscape");
                });
                ui.horizontal(|ui| {
                    ui.label("Measures per system");
                    ui.add(egui::DragValue::new(&mut part.layout.measures_per_system).range(1..=8));
                    ui.label("Extra staff spacing");
                    ui.add(egui::DragValue::new(&mut part.layout.staff_gap).range(0.0..=80.0).speed(1.0));
                });
                ui.small("Dense passages may use fewer measures to preserve readable spacing.");
                match parts::project(&self.score, part) {
                    Ok(projection) => {
                        if projection.is_voiced() { ui.small("Voiced part: use Edit source score to change musical content."); }
                        for warning in projection.warnings { ui.small(warning); }
                    }
                    Err(error) => { ui.colored_label(Palette::for_theme(self.theme).muted, format!("Preview unavailable: {error}")); }
                }
                ui.separator();
                if self.part_view && part.is_voiced() && ui.button("Edit source score").clicked() {
                    self.edit_voicing_source();
                }
                ui.horizontal(|ui| {
                    if ui.button("View selected part").clicked() {
                        self.part_view = true;
                        if let Some(staff) = part.staves.first() { self.active_part = *staff; }
                        self.selected=None;
                        self.clear_region_selection();
                        self.grace_editor.open=false;
                        self.exit_chord_entry();
                        self.cached_score = None;
                    }
                    if ui.button("Return to full score").clicked() {
                        self.part_view = false;
                        self.cached_score = None;
                    }
                });
            });
        if reset {
            self.score.notation.parts.clear();
            self.selected_linked_part = 0;
            self.cached_score = None;
            self.status = "Generated linked parts from the current instruments.".into();
        } else {
            if remove {
                definitions.remove(self.selected_linked_part);
                self.selected_linked_part = self
                    .selected_linked_part
                    .min(definitions.len().saturating_sub(1));
            }
            if definitions != original {
                match parts::validate_definitions(&definitions, self.score.parts.len()) {
                    Ok(()) => {
                        self.score.notation.parts = definitions;
                        self.cached_score = None;
                    }
                    Err(error) => self.status = format!("Part definition unchanged: {error}"),
                }
            }
        }
        self.show_parts = open;
        if export {
            self.export_all_linked_parts();
        }
    }
}

fn staff_voicing_controls(ui: &mut egui::Ui, rule: &mut StaffVoicing) {
    ui.horizontal(|ui| {
        ui.label("Layers");
        if ui.small_button("All voices").clicked() {
            rule.lanes = u8::MAX;
        }
        for layer in 0..4u8 {
            if ui.small_button(format!("{} only", layer + 1)).clicked() {
                rule.lanes = (1 << layer) | (1 << (layer + 4));
                rule.multiple_layers = None;
            }
        }
    });
    egui::Grid::new("voicing-lanes").show(ui, |ui| {
        for voice in 0..2u8 {
            ui.label(format!("Voice {}", voice + 1));
            for layer in 0..4u8 {
                let bit = 1u8 << (layer + 4 * voice);
                let mut selected = rule.lanes & bit != 0;
                if ui
                    .add_enabled(
                        !selected || rule.lanes.count_ones() > 1,
                        egui::Checkbox::new(&mut selected, format!("Layer {}", layer + 1)),
                    )
                    .changed()
                {
                    if selected {
                        rule.lanes |= bit;
                    } else {
                        rule.lanes &= !bit;
                    }
                    if rule
                        .multiple_layers
                        .is_some_and(|layer| rule.lanes & ((1 << layer) | (1 << (layer + 4))) == 0)
                    {
                        rule.multiple_layers = None;
                    }
                }
            }
            ui.end_row();
        }
    });
    egui::ComboBox::from_label("Chord notes")
        .selected_text(rule.chord.name())
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut rule.chord, ChordFilter::All, "All chord notes");
            ui.selectable_value(&mut rule.chord, ChordFilter::Highest, "Highest note");
            ui.selectable_value(&mut rule.chord, ChordFilter::Lowest, "Lowest note");
            if ui
                .selectable_label(
                    matches!(rule.chord, ChordFilter::Ranked { .. }),
                    "Selected chord positions",
                )
                .clicked()
            {
                rule.chord = ChordFilter::Ranked {
                    from_top: true,
                    ranks: vec![1],
                };
            }
        });
    if let ChordFilter::Ranked { from_top, ranks } = &mut rule.chord {
        ui.horizontal(|ui| {
            ui.selectable_value(from_top, true, "Count from highest");
            ui.selectable_value(from_top, false, "Count from lowest");
        });
        ui.horizontal_wrapped(|ui| {
            for rank in 1..=16u8 {
                let mut selected = ranks.contains(&rank);
                if ui
                    .add_enabled(
                        !selected || ranks.len() > 1,
                        egui::Checkbox::new(&mut selected, rank.to_string()),
                    )
                    .changed()
                {
                    if selected {
                        ranks.push(rank);
                        ranks.sort_unstable();
                    } else {
                        ranks.retain(|old| *old != rank);
                    }
                }
            }
        });
    }
    ui.add_enabled(
        rule.chord != ChordFilter::All,
        egui::Checkbox::new(
            &mut rule.include_single_notes,
            "Include single-note passages",
        ),
    );
    let mut conditional = rule.multiple_layers.is_some();
    if ui
        .checkbox(
            &mut conditional,
            "When a measure uses multiple layers, show one layer's whole chords",
        )
        .changed()
    {
        rule.multiple_layers = conditional.then(|| {
            (0..4u8)
                .find(|layer| rule.lanes & ((1 << layer) | (1 << (layer + 4))) != 0)
                .unwrap_or(0)
        });
    }
    if let Some(layer) = &mut rule.multiple_layers {
        egui::ComboBox::from_label("Multiple-layer fallback")
            .selected_text(format!("Layer {}", *layer + 1))
            .show_ui(ui, |ui| {
                for candidate in 0..4u8 {
                    ui.add_enabled_ui(
                        rule.lanes & ((1 << candidate) | (1 << (candidate + 4))) != 0,
                        |ui| {
                            ui.selectable_value(
                                layer,
                                candidate,
                                format!("Layer {}", candidate + 1),
                            );
                        },
                    );
                }
            });
    }
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_parts_manager_and_viewing_each_part_does_not_dirty_score() {
        let score = Score::demo();
        let mut app = EncoreApp::from_score(score.clone(), None, "Ready".into());
        let ctx = egui::Context::default();
        configure_context(&ctx);
        app.show_parts = true;
        for index in 0..parts::definitions(&score).len() {
            app.selected_linked_part = index;
            app.part_view = true;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                app.linked_parts_dialog(ctx)
            });
            assert!(app.selected_part_projection().unwrap().is_some());
            assert!(!app.dirty());
            assert_eq!(app.score, score);
            assert!(app.undo.is_empty());
        }
    }

    #[test]
    fn part_view_keeps_entry_routed_to_visible_staves_after_sidebar_selection() {
        let score = Score::demo();
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.part_view = true;
        app.selected_linked_part = parts::definitions(&app.score).len() - 1;
        let ctx = egui::Context::default();
        configure_context(&ctx);
        for frame in 0..2 {
            if frame == 1 {
                app.active_part = 0;
            }
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 1000.0))),
                    time: Some(f64::from(frame) / 60.0),
                    ..Default::default()
                },
                |ctx| app.score_canvas(ctx),
            );
            if app.part_view {
                let projection = app.selected_part_projection().unwrap().unwrap();
                assert!(
                    projection.source_staves.contains(&app.active_part),
                    "A linked part must never write using its transposition into a hidden source staff"
                );
            }
        }
    }

    #[test]
    fn linked_piano_view_maps_both_hands_to_source_and_reflects_source_edits() {
        let score = Score::demo();
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.part_view = true;
        let projection = app.selected_part_projection().unwrap().unwrap();
        assert_eq!(projection.source_staves, vec![0, 1]);
        let display = Selection {
            part: 1,
            measure: 0,
            note: 0,
        };
        let source = projection.source_selection(display).unwrap();
        app.selected = Some(source);
        app.score.parts[source.part].measures[source.measure].notes[source.note].velocity = 105;
        let changed = app.selected_part_projection().unwrap().unwrap();
        assert_eq!(
            changed.score.parts[display.part].measures[display.measure].notes[display.note]
                .velocity,
            105
        );
        assert_eq!(
            changed.project_selection(app.selected.unwrap()),
            Some(display)
        );
        assert_eq!(app.score.parts[1].measures[0].notes[0].velocity, 105);
    }
    #[test]
    fn voiced_part_manager_keeps_source_and_exact_selection_without_dirtying() {
        let mut score = Score::demo();
        score.notation.parts = parts::default_definitions(&score);
        let mut rule = StaffVoicing::new(0);
        rule.chord = ChordFilter::Highest;
        rule.lanes = 1;
        score.notation.parts[0].voicing.push(rule);
        let mut app = EncoreApp::from_score(score.clone(), None, "Ready".into());
        app.activate_linked_part(0);
        assert!(app.view_is_voiced());
        app.show_parts = true;
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.linked_parts_dialog(ctx)
        });
        let projection = app.selected_part_projection().unwrap().unwrap();
        let displayed = Selection {
            part: 0,
            measure: 0,
            note: 0,
        };
        let source = projection.source_selection(displayed).unwrap();
        app.selected = Some(source);
        app.edit_voicing_source();
        assert!(!app.part_view);
        assert_eq!(app.selected, Some(source));
        assert_eq!(app.score, score);
        assert!(!app.dirty());
        assert!(app.undo.is_empty());
    }
}
