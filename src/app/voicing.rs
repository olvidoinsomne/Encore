//! Voiced parts show a derived selection of the source music. Match Finale's
//! source-editing workflow so hidden voices cannot be changed through a filter.
use super::*;
impl EncoreApp {
    pub(super) fn view_is_voiced(&self) -> bool {
        self.part_view
            && self
                .score
                .notation
                .parts
                .get(self.selected_linked_part)
                .or_else(|| self.score.notation.parts.first())
                .is_some_and(|part| part.is_voiced())
    }
    pub(super) fn allow_source_edit(&mut self) -> bool {
        if !self.view_is_voiced() {
            return true;
        }
        self.status =
            "This voiced part is read-only. Choose Edit source score to change its music.".into();
        false
    }
    pub(super) fn extract_viewed_part(&mut self) {
        let definitions = encore::parts::definitions(&self.score);
        let Some(definition) = definitions.get(self.selected_linked_part) else {
            return;
        };
        let extracted = match encore::parts::extract(&self.score, definition, false) {
            Ok(score) => score,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Extract standalone part in concert pitch")
            .add_filter("Encore project", &["encore"])
            .set_file_name(format!("{}.encore", safe_filename(&extracted.title)))
            .save_file()
        {
            match extracted.save(&path) {
                Ok(()) => {
                    self.status = format!(
                        "Extracted {}. The original score and linked part are unchanged.",
                        path.display()
                    )
                }
                Err(error) => self.status = error,
            }
        }
    }
    pub(super) fn edit_voicing_source(&mut self) {
        self.part_view = false;
        self.write_mode = false;
        self.clear_region_selection();
        if let Some(selection) = self.selected {
            self.active_part = selection.part;
            self.cursor_measure = selection.measure;
            if let Some(anchor) = encore::document::Anchor::from_selection(&self.score, selection) {
                self.cursor_tick = anchor.tick;
                self.navigation.reveal = Some((anchor.part, anchor.measure, anchor.tick));
            }
        }
        self.status =
            "Editing the source score. The voiced part will update from these notes.".into();
    }
    pub(super) fn close_voiced_editors(&mut self) {
        if !self.view_is_voiced() {
            return;
        }
        self.write_mode = false;
        self.show_region_tools = false;
        self.show_entry_style = false;
        self.notation_editor = Default::default();
        self.expressions_editor.open = false;
        self.grace_editor.open = false;
        self.lines_editor.open = false;
        self.ornaments_editor.open = false;
        self.chord_editor.active = false;
        self.chord_editor.open = false;
    }
    /// A final boundary for UI routes such as instrument replacement or a dialog
    /// left open while switching views. Layout, part definitions and mixer state
    /// remain usable; history operations deliberately bypass this boundary.
    pub(super) fn protect_voiced_music(&mut self, before: &Score) {
        if !self.view_is_voiced() || self.skip_history {
            return;
        }
        let mut permitted = before.clone();
        permitted.notation.parts = self.score.notation.parts.clone();
        permitted.notation.layout = self.score.notation.layout.clone();
        permitted.title = self.score.title.clone();
        permitted.composer = self.score.composer.clone();
        permitted.tempo = self.score.tempo;
        for (old, now) in permitted.parts.iter_mut().zip(&self.score.parts) {
            old.volume = now.volume;
            old.pan = now.pan;
            old.muted = now.muted;
            old.solo = now.solo;
        }
        if self.score != permitted {
            self.score = permitted;
            self.selected = None;
            self.clear_region_selection();
            self.close_voiced_editors();
            self.status =
                "This voiced part is read-only. Choose Edit source score to change its music."
                    .into();
        }
    }
    pub(super) fn voiced_inspector(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        let projection = self.selected_part_projection().ok().flatten();
        egui::SidePanel::right("inspector").resizable(true).default_width(211.0).width_range(180.0..=300.0)
            .frame(egui::Frame::new().fill(colors.surface).inner_margin(16)).show(ctx,|ui|{
            egui::ScrollArea::vertical().id_salt("voiced-inspector").show(ui,|ui|{
            ui.label(RichText::new("VOICED PART").size(11.0).strong().color(colors.muted));
            ui.add_space(6.0);
            if ui.button("Edit source score").clicked(){self.edit_voicing_source();}
            if ui.button("Extract standalone part…").clicked(){self.extract_viewed_part();}
            ui.add_space(8.0);
            if let Some(p)=&projection {
                if let Some(s)=self.selected.and_then(|s|p.project_selection(s)) {
                    let n=&p.score.parts[s.part].measures[s.measure].notes[s.note];
                    ui.label(RichText::new(if n.pitches.is_empty(){"Rest".into()}else{n.pitches.iter().map(|p|p.name()).collect::<Vec<_>>().join(" · ")}).size(21.0).strong());
                    ui.label(format!("Measure {} · Layer {} · Voice {}",s.measure+1,n.layer+1,n.voice+1));
                }
                for warning in &p.warnings {ui.label(warning);}
            }
            ui.add_space(12.0);
            ui.label("This part shows selected voices or chord tones. Edit the source score to change its music.");
            ui.label("Part formatting, source-score playback, saving and export remain available.");
            });
        });
    }
}
