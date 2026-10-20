//! Whole-measure selection and editing controls. Source staff indices are kept
//! throughout, including selections made inside a noncontiguous linked part.
use super::*;
use encore::editing::{self, PasteMode, Region};

pub(super) struct Editor {
    pub semitones: i8,
    pub diatonic_steps: i8,
    pub preserve_spelling: bool,
    pub diatonic_scale: bool,
    pub insert_count: usize,
    pub paste_mode: PasteMode,
    anchor: Option<(usize, usize)>,
    pub insert_paste_once: bool,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            semitones: 12,
            diatonic_steps: 7,
            preserve_spelling: true,
            diatonic_scale: false,
            insert_count: 1,
            paste_mode: PasteMode::Replace,
            anchor: None,
            insert_paste_once: false,
        }
    }
}

impl EncoreApp {
    pub(super) fn request_region_insert_paste(&mut self, ctx: &egui::Context) {
        self.show_region_tools = false;
        self.region_editor.insert_paste_once = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    pub(super) fn clear_region_selection(&mut self) {
        self.region_selection = None;
        self.region_editor.anchor = None;
        self.region_editor.insert_paste_once = false;
    }
    pub(super) fn region_visible_staves(&self) -> Vec<usize> {
        self.selected_part_projection().ok().flatten().map_or_else(
            || (0..self.score.parts.len()).collect(),
            |projection| projection.source_staves,
        )
    }
    pub(super) fn select_region_all(&mut self) {
        let staves = self.region_visible_staves();
        self.region_editor.anchor = staves.first().map(|staff| (*staff, 0));
        self.region_selection = Some(Region {
            staves,
            start: 0,
            end: self.score.measure_count() - 1,
        });
        self.selected = None;
        self.write_mode = false;
        self.notation_editor.tool = notation::Tool::Select;
        self.status = "Selected all measures in the visible score or linked part.".into();
    }
    pub(super) fn select_region_at(&mut self, staff: usize, measure: usize, extend: bool) {
        let visible = self.region_visible_staves();
        let Some(clicked) = visible.iter().position(|source| *source == staff) else {
            return;
        };
        if measure >= self.score.measure_count() {
            return;
        }
        let (anchor_staff, anchor_bar) = if extend {
            if let Some(region) = self.region_selection.as_ref() {
                self.region_editor
                    .anchor
                    .or_else(|| region.staves.first().map(|staff| (*staff, region.start)))
                    .unwrap_or((staff, measure))
            } else {
                self.selected
                    .map(|selection| (selection.part, selection.measure))
                    .unwrap_or((self.active_part, self.cursor_measure))
            }
        } else {
            (staff, measure)
        };
        let anchor_index = visible
            .iter()
            .position(|source| *source == anchor_staff)
            .unwrap_or(clicked);
        let anchor_bar = anchor_bar.min(self.score.measure_count() - 1);
        self.region_editor.anchor = Some((visible[anchor_index], anchor_bar));
        self.region_selection = Some(Region {
            staves: visible[anchor_index.min(clicked)..=anchor_index.max(clicked)].to_vec(),
            start: anchor_bar.min(measure),
            end: anchor_bar.max(measure),
        });
        self.active_part = staff;
        self.cursor_measure = measure;
        self.cursor_tick = 0;
        self.selected = None;
        self.write_mode = false;
        self.notation_editor.tool = notation::Tool::Select;
    }
    /// Finale's whole-measure region keys. Plain Shift+Left/Right remains
    /// available for future entry-level selections; full-bar expansion uses the
    /// documented Control modifier on Windows and Option on macOS.
    pub(super) fn region_keyboard(
        &mut self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        mac: bool,
    ) -> bool {
        if modifiers.command && !(modifiers.shift && !mac && modifiers.ctrl) {
            return false;
        }
        if matches!(key, egui::Key::Backspace | egui::Key::Delete)
            && !modifiers.command
            && !modifiers.alt
            && self.region_selection.is_some()
        {
            let stack = self
                .region_selection
                .as_ref()
                .is_some_and(|region| region.is_full_stack(&self.score));
            let clear = (key == egui::Key::Backspace && !mac) || (mac && modifiers.shift);
            if stack && !clear {
                self.delete_selected_measures();
            } else {
                self.clear_selected_region();
            }
            return true;
        }
        if !modifiers.shift {
            return false;
        }
        let horizontal = if mac {
            modifiers.alt && !modifiers.ctrl && !modifiers.command
        } else {
            modifiers.ctrl && !modifiers.alt
        };
        let plain = !modifiers.command && !modifiers.ctrl && !modifiers.alt;
        let visible = self.region_visible_staves();
        let Some(row) = visible.iter().position(|staff| *staff == self.active_part) else {
            return false;
        };
        let mut target_staff = self.active_part;
        let mut target_bar = self.cursor_measure;
        match key {
            egui::Key::ArrowLeft if horizontal => target_bar = target_bar.saturating_sub(1),
            egui::Key::ArrowRight if horizontal => {
                target_bar = (target_bar + 1).min(self.score.measure_count() - 1)
            }
            egui::Key::ArrowUp if plain => target_staff = visible[row.saturating_sub(1)],
            egui::Key::ArrowDown if plain => {
                target_staff = visible[(row + 1).min(visible.len() - 1)]
            }
            egui::Key::Home if plain => target_bar = 0,
            egui::Key::End if plain => target_bar = self.score.measure_count() - 1,
            egui::Key::PageUp if plain => target_staff = visible[0],
            egui::Key::PageDown if plain => target_staff = *visible.last().unwrap(),
            _ => return false,
        }
        if self.region_selection.is_none() {
            self.select_region_at(self.active_part, self.cursor_measure, false);
        }
        self.select_region_at(target_staff, target_bar, true);
        true
    }

    pub(super) fn clear_selected_region(&mut self) {
        let Some(region) = self.region_selection.clone() else {
            self.status = "Select a measure region to clear.".into();
            return;
        };
        match editing::clear_region(&mut self.score, &region) {
            Ok(()) => {
                self.selected = None;
                self.cached_score = None;
                self.status = "Cleared the selected measures without shifting later music.".into();
            }
            Err(error) => self.status = format!("Cannot clear region: {error}"),
        }
    }
    pub(super) fn delete_selected_measures(&mut self) {
        let Some(region) = self.region_selection.clone() else {
            self.status = "Select a full staff stack to delete measures.".into();
            return;
        };
        if let Err(error) = region.validate(&self.score) {
            self.status = format!("Cannot delete measures: {error}");
            return;
        }
        if !region.is_full_stack(&self.score) {
            self.status = "Measure deletion affects the entire score. Select all staves, or use Clear to retain the measure stack.".into();
            return;
        }
        match editing::delete_measures(&mut self.score, region.start, region.end) {
            Ok(()) => {
                self.region_selection = None;
                self.selected = None;
                self.clamp_cursor();
                self.cached_score = None;
                self.status =
                    "Deleted the selected measure stacks and shifted later notation.".into();
            }
            Err(error) => self.status = format!("Cannot delete measures: {error}"),
        }
    }
    pub(super) fn insert_region_measures(&mut self) {
        let at = self
            .region_selection
            .as_ref()
            .map_or(self.cursor_measure, |region| region.start);
        match editing::insert_measures(&mut self.score, at, self.region_editor.insert_count) {
            Ok(()) => {
                self.region_selection = None;
                self.selected = None;
                self.cursor_measure = at;
                self.cursor_tick = 0;
                self.cached_score = None;
                self.status = format!(
                    "Inserted {} blank measure stacks.",
                    self.region_editor.insert_count
                );
            }
            Err(error) => self.status = format!("Cannot insert measures: {error}"),
        }
    }
    pub(super) fn transpose_selected_region(&mut self) {
        let Some(region) = self.region_selection.clone() else {
            self.status = "Select a measure region to transpose.".into();
            return;
        };
        let result = if self.region_editor.diatonic_scale {
            editing::transpose_diatonic_region(
                &mut self.score,
                &region,
                self.region_editor.diatonic_steps,
            )
        } else if self.region_editor.preserve_spelling {
            editing::transpose_region(
                &mut self.score,
                &region,
                encore::parts::Transposition {
                    semitones: self.region_editor.semitones,
                    diatonic_steps: self.region_editor.diatonic_steps,
                },
            )
        } else {
            editing::transpose_chromatic_region(
                &mut self.score,
                &region,
                self.region_editor.semitones,
            )
        };
        match result {
            Ok(()) => {
                self.cached_score = None;
                self.status =
                    "Transposed the selected region; concert pitches remain linked in every part."
                        .into();
            }
            Err(error) => self.status = format!("Cannot transpose region: {error}"),
        }
    }
    pub(super) fn region_tools_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_region_tools {
            return;
        }
        let mut open = true;
        let mut copy = false;
        let mut cut = false;
        let mut paste = false;
        let mut clear = false;
        let mut delete = false;
        let mut insert = false;
        let mut transpose = false;
        egui::Window::new("Measure region").open(&mut open).default_width(490.0).show(ctx, |ui| {
            ui.label("Select whole measures with the region gesture, then extend with Shift. Linked-part selections keep their source staff mapping.");
            if ui.button("Select all visible measures").clicked() { self.select_region_all(); }
            let count = self.score.measure_count();
            if let Some(region) = &mut self.region_selection {
                ui.label(format!("{} source staves · {} measures", region.staves.len(), region.measure_count()));
                ui.horizontal(|ui| {
                    let mut first = region.start + 1; let mut last = region.end + 1;
                    if ui.add(egui::DragValue::new(&mut first).range(1..=count).prefix("First ")).changed() { region.start = first - 1; }
                    if ui.add(egui::DragValue::new(&mut last).range(1..=count).prefix("Last ")).changed() { region.end = last - 1; }
                });
                ui.horizontal(|ui| { copy = ui.button("Copy").clicked(); cut = ui.button("Cut").clicked(); clear = ui.button("Clear music").clicked(); });
                delete = ui.add_enabled(region.is_full_stack(&self.score), egui::Button::new("Delete selected measure stacks")).clicked();
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Semitones"); ui.add(egui::DragValue::new(&mut self.region_editor.semitones).range(-36..=36));
                    ui.label("Diatonic steps"); ui.add(egui::DragValue::new(&mut self.region_editor.diatonic_steps).range(-21..=21));
                });
                ui.checkbox(&mut self.region_editor.diatonic_scale, "Follow the key's diatonic scale (steps only)");
                if !self.region_editor.diatonic_scale { ui.checkbox(&mut self.region_editor.preserve_spelling, "Preserve interval spelling"); }
                ui.small("Octave: 12 semitones / 7 steps. Major second: 2 / 1. Key signatures are unchanged.");
                transpose = ui.button("Transpose selected region").clicked();
            } else { ui.label("No measure region is selected. Paste begins at the active staff and measure."); }
            ui.separator();
            egui::ComboBox::from_label("Paste behavior").selected_text(match self.region_editor.paste_mode { PasteMode::Replace => "Replace destination measures", PasteMode::EmptyOnly => "Require empty measures", PasteMode::Insert => "Insert measure stacks" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.region_editor.paste_mode, PasteMode::Replace, "Replace destination measures");
                ui.selectable_value(&mut self.region_editor.paste_mode, PasteMode::EmptyOnly, "Require empty measures");
                ui.selectable_value(&mut self.region_editor.paste_mode, PasteMode::Insert, "Insert measure stacks");
            });
            paste = ui.button("Paste region at selection or cursor").clicked();
            ui.horizontal(|ui| { ui.add(egui::DragValue::new(&mut self.region_editor.insert_count).range(1..=MAX_MEASURES).prefix("Blank measures ")); insert = ui.button("Insert before selection/cursor").clicked(); });
        });
        self.show_region_tools = open;
        if copy {
            self.copy_selected_region(ctx, false);
        }
        if cut {
            self.copy_selected_region(ctx, true);
        }
        if paste {
            // Native clipboard delivery occurs in the next frame, after this
            // modal tool window would otherwise suppress score keyboard input.
            self.show_region_tools = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        if clear {
            self.clear_selected_region();
        }
        if delete {
            self.delete_selected_measures();
        }
        if insert {
            self.insert_region_measures();
        }
        if transpose {
            self.transpose_selected_region();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(frame: u32, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 1000.0))),
            time: Some(f64::from(frame) / 60.0),
            events,
            focused: true,
            ..Default::default()
        }
    }
    #[test]
    fn linked_view_rectangle_uses_visible_order_without_hidden_source_staves() {
        let mut score = Score::demo();
        let mut definition = encore::parts::default_definitions(&score)[0].clone();
        definition.staves = vec![5, 2];
        definition.transposition = encore::parts::Transposition::CONCERT;
        score.notation.parts = vec![definition];
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.part_view = true;
        app.select_region_at(5, 0, false);
        app.select_region_at(2, 2, true);
        assert_eq!(
            app.region_selection.as_ref().unwrap(),
            &Region {
                staves: vec![5, 2],
                start: 0,
                end: 2
            }
        );
        assert!(!app.dirty());
    }
    #[test]
    fn region_keys_follow_platform_whole_measure_modifiers_and_vertical_selection() {
        let mut app = EncoreApp::from_score(Score::demo(), None, "Ready".into());
        app.select_region_at(0, 2, false);
        assert!(app.region_keyboard(
            egui::Key::ArrowRight,
            egui::Modifiers {
                shift: true,
                ctrl: true,
                command: true,
                ..egui::Modifiers::NONE
            },
            false
        ));
        assert_eq!(
            (
                app.region_selection.as_ref().unwrap().start,
                app.region_selection.as_ref().unwrap().end
            ),
            (2, 3)
        );
        assert!(app.region_keyboard(egui::Key::ArrowDown, egui::Modifiers::SHIFT, false));
        assert_eq!(app.region_selection.as_ref().unwrap().staves, vec![0, 1]);
        assert!(app.region_keyboard(
            egui::Key::ArrowLeft,
            egui::Modifiers {
                shift: true,
                alt: true,
                ..egui::Modifiers::NONE
            },
            true
        ));
        assert_eq!(app.region_selection.as_ref().unwrap().end, 2);
        assert!(app.region_keyboard(egui::Key::End, egui::Modifiers::SHIFT, false));
        assert_eq!(app.region_selection.as_ref().unwrap().end, 7);
    }
    #[test]
    fn deleting_a_linked_subset_clears_music_and_full_stack_delete_shifts_measures() {
        let mut app = EncoreApp::from_score(Score::demo(), None, "Ready".into());
        let count = app.score.measure_count();
        app.region_selection = Some(Region {
            staves: vec![2],
            start: 0,
            end: 0,
        });
        assert!(app.region_keyboard(egui::Key::Delete, egui::Modifiers::NONE, false));
        assert_eq!(app.score.measure_count(), count);
        assert!(app.score.parts[2].measures[0].notes.is_empty());
        app.region_selection = Some(Region {
            staves: (0..app.score.parts.len()).collect(),
            start: 0,
            end: 0,
        });
        app.region_keyboard(egui::Key::Delete, egui::Modifiers::NONE, false);
        assert_eq!(app.score.measure_count(), count - 1);
        app.score.validate().unwrap();
    }
    #[test]
    fn invalid_staff_regions_never_enable_whole_score_measure_deletion() {
        let mut app = EncoreApp::from_score(Score::demo(), None, "Ready".into());
        app.region_selection = Some(Region {
            staves: vec![0; app.score.parts.len()],
            start: 0,
            end: 0,
        });
        let before = app.score.clone();
        app.delete_selected_measures();
        assert_eq!(app.score, before);
        assert!(
            !app.region_selection
                .as_ref()
                .unwrap()
                .is_full_stack(&app.score)
        );
    }

    #[test]
    fn region_cut_paste_and_score_undo_are_driven_by_real_editor_frames() {
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let original: Score =
            serde_json::from_str(include_str!("../../examples/Cantabile.encore")).unwrap();
        let mut app = EncoreApp::from_score(original.clone(), None, "Ready".into());
        app.region_selection = Some(Region {
            staves: vec![0],
            start: 2,
            end: 2,
        });
        let _ = ctx.run(input(0, vec![]), |ctx| app.ui(ctx));
        let output = ctx.run(input(1, vec![egui::Event::Cut]), |ctx| app.ui(ctx));
        let copied = output
            .platform_output
            .commands
            .iter()
            .find_map(|command| match command {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap();
        assert!(app.score.parts[0].measures[2].notes.is_empty());
        assert_eq!(app.undo.len(), 1);
        let cut = app.score.clone();
        app.region_selection = Some(Region {
            staves: vec![1],
            start: 6,
            end: 6,
        });
        app.active_part = 1;
        app.cursor_measure = 6;
        let _ = ctx.run(input(2, vec![egui::Event::Paste(copied)]), |ctx| {
            app.ui(ctx)
        });
        assert_eq!(
            app.score.parts[1].measures[6],
            original.parts[0].measures[2]
        );
        assert!(
            app.score
                .notation
                .lyrics
                .iter()
                .any(|lyric| lyric.anchor.part == 1 && lyric.anchor.measure == 6)
        );
        assert_eq!(app.undo.len(), 2);
        app.undo();
        assert_eq!(app.score, cut);
        app.undo();
        assert_eq!(app.score, original);
    }
}
