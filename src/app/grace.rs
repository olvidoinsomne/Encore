//! A grace group editor keeps ornaments separate from the rhythmic caret.
use super::*;
use encore::{
    document::Anchor,
    entry_style::BeamStyle,
    grace::{self, GraceGroup, GraceNote, GracePlacement, GraceSelection, GraceTiming},
};

#[derive(Default)]
pub(super) struct Editor {
    pub open: bool,
    pub selected: Option<GraceSelection>,
    draft: Option<GraceGroup>,
    original: Option<GracePlacement>,
}
impl EncoreApp {
    pub(super) fn open_grace(&mut self) {
        let Some(anchor) = self
            .selected
            .and_then(|selection| Anchor::from_selection(&self.score, selection))
        else {
            self.status = "Select a pitched principal note before editing grace notes.".into();
            return;
        };
        self.open_grace_at(anchor, GracePlacement::Before, 0);
    }
    fn open_grace_at(&mut self, anchor: Anchor, placement: GracePlacement, index: usize) {
        let Some(note) =
            encore::document::note_at(&self.score, &anchor).filter(|n| !n.pitches.is_empty())
        else {
            self.status = "Grace groups attach to pitched principal notes.".into();
            return;
        };
        let existing = grace::at(&self.score, &anchor, placement).cloned();
        let mut group = existing.clone().unwrap_or_else(|| {
            GraceGroup::new(
                anchor,
                vec![GraceNote::new(note.pitches[0], Duration::Eighth)],
            )
        });
        group.placement = placement;
        if placement == GracePlacement::After {
            group.timing = GraceTiming::StealMain;
        }
        self.grace_editor.original = existing.map(|g| g.placement);
        self.grace_editor.selected = Some(GraceSelection {
            anchor,
            placement,
            note: index.min(group.notes.len() - 1),
        });
        self.grace_editor.draft = Some(group);
        self.grace_editor.open = true;
        self.write_mode = false;
    }
    pub(super) fn select_grace(&mut self, selection: GraceSelection) {
        self.selected = selection.anchor.selection(&self.score);
        self.active_part = selection.anchor.part;
        self.active_layer = selection.anchor.layer;
        self.active_voice = selection.anchor.voice;
        self.cursor_measure = selection.anchor.measure;
        self.cursor_tick = selection.anchor.tick;
        self.open_grace_at(selection.anchor, selection.placement, selection.note);
    }
    fn apply_grace_draft(&mut self, group: GraceGroup) -> bool {
        let mut candidate = self.score.clone();
        if let Some(side) = self.grace_editor.original {
            grace::remove(&mut candidate, &group.anchor, side);
        }
        match grace::assign(&mut candidate, group.clone()).and_then(|()| candidate.validate()) {
            Ok(()) => {
                self.score = candidate;
                self.grace_editor.original = Some(group.placement);
                self.status =
                    "Grace group updated; the principal note's written timing is unchanged.".into();
                true
            }
            Err(error) => {
                self.status = error;
                false
            }
        }
    }
    pub(super) fn grace_dialog(&mut self, ctx: &egui::Context) {
        if !self.grace_editor.open {
            return;
        }
        let Some(mut group) = self.grace_editor.draft.clone() else {
            self.grace_editor.open = false;
            return;
        };
        if encore::document::note_at(&self.score, &group.anchor)
            .is_none_or(|n| n.pitches.is_empty())
        {
            self.grace_editor.open = false;
            self.status = "The grace group's principal entry was removed.".into();
            return;
        }
        let mut open = true;
        let mut index = self
            .grace_editor
            .selected
            .map_or(0, |s| s.note)
            .min(group.notes.len() - 1);
        let mut apply = false;
        let mut remove = false;
        let mut switch = None;
        let anchor = group.anchor;
        egui::Window::new("Grace notes")
            .open(&mut open)
            .default_width(520.0)
            .default_pos(egui::pos2(
                ((ctx.content_rect().width() - 560.0) * 0.5).max(16.0),
                40.0,
            ))
            .max_width((ctx.content_rect().width() - 48.0).max(280.0))
            .max_height((ctx.content_rect().height() - 120.0).max(240.0))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} · measure {} · layer {}",
                    self.score.parts[group.anchor.part].name,
                    group.anchor.measure + 1,
                    group.anchor.layer + 1
                ));
                ui.label("The principal note keeps its written onset and duration.");
                ui.horizontal(|ui| {
                    apply = ui.button("Apply grace group").clicked();
                    remove = ui
                        .add_enabled(
                            self.grace_editor.original.is_some(),
                            egui::Button::new("Delete group"),
                        )
                        .clicked();
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for side in GracePlacement::ALL {
                        if ui
                            .selectable_label(group.placement == side, side.name())
                            .clicked()
                            && group.placement != side
                        {
                            switch = Some(side);
                        }
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut group.slashed, "Slash");
                    ui.checkbox(&mut group.slur_to_main, "Slur to principal");
                    let mut percent = group.scale * 100.0;
                    ui.add(
                        egui::DragValue::new(&mut percent)
                            .range(40.0..=100.0)
                            .suffix("% size"),
                    );
                    group.scale = percent / 100.0;
                });
                ui.horizontal_wrapped(|ui| {
                    if group.placement == GracePlacement::Before {
                        egui::ComboBox::from_id_salt("grace-timing")
                            .selected_text(group.timing.name())
                            .show_ui(ui, |ui| {
                                for value in GraceTiming::ALL {
                                    ui.selectable_value(&mut group.timing, value, value.name());
                                }
                            });
                    } else {
                        ui.label("Borrow principal note's ending");
                        group.timing = GraceTiming::StealMain;
                    }
                    ui.add(
                        egui::DragValue::new(&mut group.percent)
                            .range(0.0..=75.0)
                            .suffix("% of donor"),
                    );
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    for (i, note) in group.notes.iter().enumerate() {
                        let label = format!(
                            "{}: {}",
                            i + 1,
                            note.pitches
                                .iter()
                                .map(|p| self.written_pitch_at(*p, anchor).unwrap_or(*p).name())
                                .collect::<Vec<_>>()
                                .join(" + ")
                        );
                        ui.selectable_value(&mut index, i, label);
                    }
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            group.notes.len() < grace::MAX_GROUP_NOTES,
                            egui::Button::new("Add note"),
                        )
                        .clicked()
                    {
                        let copy = group.notes[index].clone();
                        group.notes.insert(index + 1, copy);
                        index += 1;
                    }
                    if ui
                        .add_enabled(group.notes.len() > 1, egui::Button::new("Remove note"))
                        .clicked()
                    {
                        group.notes.remove(index);
                        index = index.min(group.notes.len() - 1);
                    }
                    if ui
                        .add_enabled(index > 0, egui::Button::new("Move left"))
                        .clicked()
                    {
                        group.notes.swap(index, index - 1);
                        index -= 1;
                    }
                    if ui
                        .add_enabled(
                            index + 1 < group.notes.len(),
                            egui::Button::new("Move right"),
                        )
                        .clicked()
                    {
                        group.notes.swap(index, index + 1);
                        index += 1;
                    }
                });
                let note = &mut group.notes[index];
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("grace-value")
                        .selected_text(note.duration.name())
                        .show_ui(ui, |ui| {
                            for &value in Duration::ALL {
                                ui.selectable_value(&mut note.duration, value, value.name());
                            }
                        });
                    ui.checkbox(&mut note.dotted, "Dotted");
                    egui::ComboBox::from_id_salt("grace-beam")
                        .selected_text(note.beam.name())
                        .show_ui(ui, |ui| {
                            for value in BeamStyle::ALL {
                                ui.selectable_value(&mut note.beam, value, value.name());
                            }
                        });
                });
                let mut remove_pitch = None;
                let multiple_pitches = note.pitches.len() > 1;
                for (i, pitch) in note.pitches.iter_mut().enumerate() {
                    let mut written = self.written_pitch_at(*pitch, anchor).unwrap_or(*pitch);
                    ui.horizontal(|ui| {
                        ui.label(format!("Pitch {}", i + 1));
                        egui::ComboBox::from_id_salt(("grace-step", i))
                            .selected_text(written.step.name())
                            .show_ui(ui, |ui| {
                                for step in [
                                    Step::C,
                                    Step::D,
                                    Step::E,
                                    Step::F,
                                    Step::G,
                                    Step::A,
                                    Step::B,
                                ] {
                                    ui.selectable_value(&mut written.step, step, step.name());
                                }
                            });
                        ui.add(
                            egui::DragValue::new(&mut written.alter)
                                .range(-2..=2)
                                .prefix("Alter "),
                        );
                        ui.add(
                            egui::DragValue::new(&mut written.octave)
                                .range(-1..=9)
                                .prefix("Octave "),
                        );
                        if ui
                            .add_enabled(multiple_pitches, egui::Button::new("−"))
                            .clicked()
                        {
                            remove_pitch = Some(i);
                        }
                    });
                    if let Ok(concert) = self.concert_pitch_at(written, anchor) {
                        *pitch = concert;
                    }
                }
                if let Some(i) = remove_pitch {
                    note.pitches.remove(i);
                }
                if ui
                    .add_enabled(note.pitches.len() < 16, egui::Button::new("Add chord tone"))
                    .clicked()
                {
                    let midi = note.pitches.last().map_or(60, |p| p.midi());
                    let next = (0u16..128)
                        .map(|d| (u16::from(midi) + d) % 128)
                        .find(|m| !note.pitches.iter().any(|p| u16::from(p.midi()) == *m))
                        .unwrap_or(60);
                    note.pitches.push(Pitch::from_midi(next as u8));
                }
                ui.horizontal(|ui| {
                    let mut own = note.velocity.is_some();
                    ui.checkbox(&mut own, "Override velocity");
                    if own {
                        let v = note.velocity.get_or_insert(80);
                        ui.add(egui::DragValue::new(v).range(0..=127));
                    } else {
                        note.velocity = None;
                    }
                });
            });
        if let Some(side) = switch {
            self.open_grace_at(group.anchor, side, 0);
            return;
        }
        if remove {
            if let Some(side) = self.grace_editor.original {
                grace::remove(&mut self.score, &group.anchor, side);
            }
            self.grace_editor.open = false;
            self.grace_editor.selected = None;
            self.status = "Grace group removed.".into();
            return;
        }
        if apply {
            self.apply_grace_draft(group.clone());
        }
        self.grace_editor.selected = Some(GraceSelection {
            anchor: group.anchor,
            placement: group.placement,
            note: index,
        });
        self.grace_editor.draft = Some(group);
        self.grace_editor.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grace_edit_keeps_main_caret_timing_and_uses_normal_history() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.enter_note(0, 0, 0, Pitch::from_midi(60), false);
        let written = app.score.parts.clone();
        app.open_grace();
        let mut group = app.grace_editor.draft.clone().unwrap();
        group.notes[0].pitches[0] = Pitch::from_midi(62);
        assert!(app.apply_grace_draft(group));
        assert_eq!(written, app.score.parts);
        assert_eq!(app.cursor_tick, 480);
        app.score.validate().unwrap();
        app.select_grace(GraceSelection {
            anchor: app.score.notation.grace[0].anchor,
            placement: GracePlacement::Before,
            note: 0,
        });
        assert_eq!(app.cursor_tick, 0);
    }
}
