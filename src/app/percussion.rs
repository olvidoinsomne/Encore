//! Named unpitched entry and independent notation/playback map editing.
use super::*;
use encore::{
    entry_style::Notehead,
    percussion::{Sound, Staff},
};

#[derive(Default)]
pub(super) struct Editor {
    pub open: bool,
    part: usize,
    source: Option<Part>,
    draft: Option<Staff>,
    selected: usize,
    search: String,
    error: Option<String>,
}

impl EncoreApp {
    pub(super) fn active_percussion(&self) -> Option<&Staff> {
        self.score.parts.get(self.active_part)?.percussion.as_ref()
    }

    pub(super) fn percussion_name(&self, part: usize, pitch: Pitch) -> String {
        self.score
            .parts
            .get(part)
            .and_then(|part| part.percussion.as_ref())
            .and_then(|staff| staff.sound(pitch.midi()))
            .map_or_else(|| pitch.name(), |sound| sound.name.clone())
    }

    /// Staff changes must not leave a melodic caret masquerading as a drum ID.
    pub(super) fn reconcile_percussion_caret(&mut self) {
        if let Some(staff) = self.active_percussion() {
            let token = self.entry_pitch.midi();
            if self.entry_pitch != encore::percussion::token(token) || staff.sound(token).is_none()
            {
                let id = staff
                    .sound(38)
                    .or_else(|| staff.sounds.first())
                    .map(|sound| sound.id);
                if let Some(id) = id {
                    self.entry_pitch = encore::percussion::token(id);
                }
            }
            self.accidental = None;
        }
    }

    pub(super) fn add_percussion_staff(&mut self) {
        if !self.allow_source_edit() {
            return;
        }
        let before = self.score.parts.len();
        self.score.add_part(Instrument::GeneralMidi(118));
        if self.score.parts.len() == before {
            self.status = "The score has reached its staff limit.".into();
            return;
        }
        let part = self.score.parts.last_mut().unwrap();
        part.name = "Drum kit".into();
        part.percussion = Some(Staff::general_midi());
        self.active_part = before;
        self.part_view = false;
        self.selected = None;
        self.active_voice = 0;
        self.active_voice_launch = None;
        self.clear_region_selection();
        self.reconcile_percussion_caret();
        self.status = "Drum staff added. Choose a named sound below; Shift-click enters it.".into();
    }

    pub(super) fn open_percussion(&mut self) {
        if !self.allow_source_edit() {
            return;
        }
        let part = &self.score.parts[self.active_part];
        if part.percussion.is_none() && part.measures.iter().any(|bar| !bar.notes.is_empty()) {
            self.status =
                "Add a drum staff, or choose an empty staff before assigning a percussion map."
                    .into();
            return;
        }
        self.percussion_editor = Editor {
            open: true,
            part: self.active_part,
            source: Some(part.clone()),
            draft: Some(part.percussion.clone().unwrap_or_default()),
            selected: 0,
            search: String::new(),
            error: None,
        };
    }

    fn apply_percussion_map(&mut self) -> bool {
        if !self.allow_source_edit() {
            return false;
        }
        let editor = &self.percussion_editor;
        if self.score.parts.get(editor.part) != editor.source.as_ref() {
            self.status = "This staff changed while the map was open. Reopen Percussion map to edit its current version.".into();
            self.percussion_editor.error = Some(self.status.clone());
            return false;
        }
        let Some(draft) = editor.draft.clone() else {
            return false;
        };
        let mut candidate = self.score.clone();
        candidate.parts[editor.part].percussion = Some(draft);
        match candidate.validate() {
            Ok(()) => {
                self.score = candidate;
                self.active_part = editor.part;
                self.percussion_editor.open = false;
                self.reconcile_percussion_caret();
                self.status =
                    "Percussion map saved. Staff positions and playback keys are independent."
                        .into();
                true
            }
            Err(error) => {
                self.status = format!("Cannot save percussion map: {error}");
                self.percussion_editor.error = Some(self.status.clone());
                false
            }
        }
    }

    pub(super) fn percussion_palette(&mut self, ui: &mut egui::Ui) {
        let Some(staff) = self.active_percussion().cloned() else {
            return;
        };
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            ui.strong("PERCUSSION");
            if ui.button("Percussion map…").clicked() {
                self.open_percussion();
            }
            ui.small("Click to select and hear · Shift-click to enter · Enter uses selected sound");
        });
        egui::ScrollArea::horizontal()
            .id_salt("percussion-palette")
            .max_height(65.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for sound in &staff.sounds {
                        let label = if sound.midi.is_none() {
                            format!("{}\nUnmapped", sound.name)
                        } else {
                            sound.name.clone()
                        };
                        if ui
                            .add(
                                egui::Button::selectable(
                                    self.entry_pitch.midi() == sound.id,
                                    label,
                                )
                                .min_size(Vec2::new(90.0, 42.0)),
                            )
                            .on_hover_text(format!(
                                "Sound ID {} · staff position {} · {}",
                                sound.id,
                                sound.position,
                                sound.midi.map_or_else(
                                    || "No playback mapping".into(),
                                    |key| format!("MIDI key {key}")
                                )
                            ))
                            .clicked()
                        {
                            chosen = Some(sound.id);
                        }
                    }
                });
            });
        if let Some(id) = chosen {
            self.entry_pitch = encore::percussion::token(id);
            self.accidental = None;
            if ui.input(|input| input.modifiers.shift) {
                self.enter_note(
                    self.active_part,
                    self.cursor_measure,
                    self.cursor_tick,
                    self.entry_pitch,
                    false,
                );
            } else {
                self.preview_percussion = Some((self.active_part, id));
            }
        }
    }

    pub(super) fn percussion_window(&mut self, ctx: &egui::Context) {
        if !self.percussion_editor.open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut audition = None;
        let mut error = None;
        let editor = &mut self.percussion_editor;
        let Some(staff) = editor.draft.as_mut() else {
            editor.open = false;
            return;
        };
        let viewport = ctx.content_rect();
        let kits = self.audio_engine.drum_kits().to_vec();
        egui::Window::new("Percussion map").open(&mut open)
            .vscroll(true).default_pos(egui::pos2(((viewport.width()-700.0)*0.5).max(16.0),24.0))
            .constrain_to(viewport.shrink(8.0)).default_width(650.0).max_width((viewport.width()-32.0).max(260.0))
            .max_height((viewport.height()-110.0).max(240.0)).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    apply=ui.add_enabled(staff.validate().is_ok(),egui::Button::new("Save percussion map")).clicked();
                    if ui.button("Cancel").clicked() {editor.open=false;}
                });
                if let Some(error)=&editor.error {ui.label(error);}
                ui.label("Choose a sound, then edit its printed position and playback key independently.");
                ui.horizontal_wrapped(|ui| {
                    ui.label("Kit program");
                    ui.add(egui::DragValue::new(&mut staff.kit).range(0..=127));
                    egui::ComboBox::from_id_salt("percussion-kit-presets")
                        .selected_text(kits.iter().find(|(id,_)|*id==staff.kit).map_or_else(||format!("Kit {}",staff.kit),|(_,name)|name.clone())).show_ui(ui,|ui| {
                            for (program,name) in &kits {ui.selectable_value(&mut staff.kit,*program,name);}
                        });
                    ui.label("Lines");
                    ui.selectable_value(&mut staff.lines,1,"1");
                    ui.selectable_value(&mut staff.lines,5,"5");
                });
                ui.small("Kit numbers are zero-based. Playback uses bank 128; the selected SoundFont must contain that exact kit.");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Find sound");ui.text_edit_singleline(&mut editor.search);
                    if ui.button("Add sound").clicked()
                        && let Some(id)=(0..128).find(|id|staff.sound(*id).is_none()) {
                            let mut name=format!("Sound {id}");
                            while staff.sounds.iter().any(|sound|sound.name.eq_ignore_ascii_case(&name)) {name.push('+');}
                            staff.sounds.push(Sound{id,name,midi:None,position:4,notehead:Notehead::Normal});
                            editor.selected=staff.sounds.len()-1;
                            editor.search.clear();
                    }
                });
                egui::ScrollArea::vertical().id_salt("percussion-map-list")
                    .max_height((viewport.height()*0.20).clamp(70.0,150.0)).show(ui,|ui| {
                        let search=editor.search.to_lowercase();
                        for (index,sound) in staff.sounds.iter().enumerate() {
                            if !search.is_empty() && !format!("{} {}",sound.id,sound.name).to_lowercase().contains(&search) {continue;}
                            ui.selectable_value(&mut editor.selected,index,format!("{} · {}",sound.id,sound.name));
                        }
                    });
                editor.selected=editor.selected.min(staff.sounds.len().saturating_sub(1));
                let mut remove=false;
                if let Some(sound)=staff.sounds.get_mut(editor.selected) {
                    ui.separator();
                    egui::Grid::new("percussion-sound-fields").num_columns(2).show(ui,|ui| {
                        ui.label("Sound ID");ui.label(sound.id.to_string());ui.end_row();
                        ui.label("Name");ui.text_edit_singleline(&mut sound.name);ui.end_row();
                        ui.label("Staff position");ui.add(egui::DragValue::new(&mut sound.position).range(-16..=24));ui.end_row();
                        ui.label("Notehead");egui::ComboBox::from_id_salt("percussion-map-head")
                            .selected_text(sound.notehead.name()).show_ui(ui,|ui| {for head in Notehead::ALL {ui.selectable_value(&mut sound.notehead,head,head.name());}});ui.end_row();
                        ui.label("Playback");ui.horizontal(|ui| {
                            let mut mapped=sound.midi.is_some();
                            if ui.checkbox(&mut mapped,"Mapped MIDI key").changed() {sound.midi=mapped.then_some(sound.id);}
                            if let Some(key)=&mut sound.midi {ui.add(egui::DragValue::new(key).range(0..=127));}
                        });ui.end_row();
                    });
                    ui.small("Position 0 is the bottom or sole line; each step is one half-space upward. IDs stay fixed so saved notes retain their identity.");
                    ui.horizontal(|ui| {
                        if ui.button("Audition sound").clicked() {audition=Some(sound.id);}
                        remove=ui.button("Remove sound").clicked();
                    });
                }
                if remove {staff.sounds.remove(editor.selected);editor.selected=editor.selected.saturating_sub(1);}
                if let Err(reason)=staff.validate() {error=Some(reason);}
                if let Some(reason)=&error {ui.label(reason);}

            });
        if !open {
            self.percussion_editor.open = false;
        }
        if let Some(id) = audition {
            let mut part = self.percussion_editor.source.clone().unwrap();
            part.percussion = self.percussion_editor.draft.clone();
            self.audition_part(part, id, self.lab_velocity, 0.9);
        }
        if apply {
            self.apply_percussion_map();
        }
    }

    pub(super) fn audition_part(&mut self, part: Part, token: u8, velocity: u8, hold: f32) {
        self.stop_audition();
        if self.audition_playback.is_none() {
            match synth::Playback::new() {
                Ok(playback) => self.audition_playback = Some(playback),
                Err(error) => {
                    self.status =
                        format!("Audio unavailable: {error}. WAV export is still available.");
                    return;
                }
            }
        }
        let label = part
            .percussion
            .as_ref()
            .and_then(|staff| staff.sound(token))
            .map_or_else(
                || part.instrument.name().to_owned(),
                |sound| sound.name.clone(),
            );
        let (tx, rx) = mpsc::channel();
        self.audition_worker = Some(rx);
        let engine = self.audio_engine.clone();
        self.status = format!("Preparing {label}…");
        std::thread::spawn(move || {
            let result = engine
                .render_part_note(&part, token, velocity, hold)
                .map(|samples| (samples, label));
            let _ = tx.send(result);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::StyleCommand;
    fn drums() -> EncoreApp {
        let mut score = Score::blank();
        score.parts[0].percussion = Some(Staff::general_midi());
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.reconcile_percussion_caret();
        app
    }

    #[test]
    fn drum_caret_ignores_octaves_accidentals_and_linked_transposition() {
        let mut app = drums();
        app.entry_pitch = encore::percussion::token(42);
        app.octave = 8;
        app.accidental = Some(2);
        app.apply_entry_command(EntryCommand::Enter);
        let note = &app.score.parts[0].measures[0].notes[0];
        assert_eq!(note.pitches, vec![encore::percussion::token(42)]);
        assert_eq!(app.cursor_tick, 480);
        assert!(app.status.contains("Closed hi-hat"));
        app.score.validate().unwrap();
    }

    #[test]
    fn drum_pitch_commands_cannot_change_sound_ids_or_create_accidentals() {
        let mut app = drums();
        app.apply_entry_command(EntryCommand::Enter);
        let source = app.score.clone();
        for command in [
            EntryCommand::Pitch(Step::C, false),
            EntryCommand::Pitch(Step::G, true),
            EntryCommand::Interval(3),
            EntryCommand::SetAccidental(1),
            EntryCommand::ToolAccidental(-1),
            EntryCommand::Octave(1),
            EntryCommand::Style(StyleCommand::Respell),
            EntryCommand::Style(StyleCommand::ToggleCourtesy),
        ] {
            app.apply_entry_command(command);
            assert_eq!(app.score, source);
        }
        app.transpose(12);
        assert_eq!(app.score, source);
        assert!(app.accidental.is_none());
    }

    #[test]
    fn percussion_arrow_selection_follows_positions_and_changes_one_chord_sound() {
        let mut app = drums();
        let staff = app.score.parts[0].percussion.as_mut().unwrap();
        staff
            .sounds
            .retain(|sound| [35, 38, 42].contains(&sound.id));
        staff.sounds.iter_mut().for_each(|sound| {
            sound.position = match sound.id {
                35 => 8,
                38 => -4,
                _ => 2,
            }
        });
        app.entry_pitch = encore::percussion::token(38);
        app.apply_entry_command(EntryCommand::Enter);
        app.entry_pitch = encore::percussion::token(35);
        app.apply_entry_command(EntryCommand::Chord);
        app.entry_pitch = encore::percussion::token(38);
        app.apply_entry_command(EntryCommand::Diatonic(1));
        let pitches = &app.score.parts[0].measures[0].notes[0].pitches;
        assert!(pitches.contains(&encore::percussion::token(35)));
        assert!(pitches.contains(&encore::percussion::token(42)));
        assert!(!pitches.contains(&encore::percussion::token(38)));
        app.score.validate().unwrap();
    }

    #[test]
    fn percussion_map_edits_preserve_tokens_and_reject_deleting_used_sounds_atomically() {
        let mut app = drums();
        app.entry_pitch = encore::percussion::token(38);
        app.apply_entry_command(EntryCommand::Enter);
        app.open_percussion();
        let draft = app.percussion_editor.draft.as_mut().unwrap();
        let sound = draft
            .sounds
            .iter_mut()
            .find(|sound| sound.id == 38)
            .unwrap();
        sound.midi = Some(40);
        sound.position = -4;
        sound.notehead = Notehead::Diamond;
        assert!(app.apply_percussion_map());
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches,
            vec![encore::percussion::token(38)]
        );
        assert_eq!(app.active_percussion().unwrap().output_key(38).unwrap(), 40);
        app.open_percussion();
        app.percussion_editor
            .draft
            .as_mut()
            .unwrap()
            .sounds
            .retain(|sound| sound.id != 38);
        let before = app.score.clone();
        assert!(!app.apply_percussion_map());
        assert_eq!(app.score, before);
    }

    #[test]
    fn percussion_map_cannot_overwrite_changed_or_populated_pitched_staves() {
        let mut app = drums();
        app.open_percussion();
        app.score.parts[0].name = "Changed staff".into();
        let before = app.score.clone();
        assert!(!app.apply_percussion_map());
        assert_eq!(app.score, before);
        let mut app = EncoreApp::from_score(Score::demo(), None, "Ready".into());
        let before = app.score.clone();
        app.open_percussion();
        assert!(!app.percussion_editor.open);
        assert_eq!(app.score, before);
    }

    #[test]
    fn adding_a_drum_staff_keeps_existing_pitched_music_and_uses_current_measure_count() {
        let mut app = EncoreApp::from_score(Score::demo(), None, "Ready".into());
        let source = app.score.parts.clone();
        let measures = app.score.measure_count();
        app.add_percussion_staff();
        assert_eq!(&app.score.parts[..source.len()], source.as_slice());
        assert_eq!(app.score.parts.last().unwrap().measures.len(), measures);
        assert!(app.active_percussion().is_some());
        app.score.validate().unwrap();
    }
}
