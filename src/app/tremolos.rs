//! Measured single-entry rolls and alternating-entry tremolo controls.
use super::*;
use encore::{document::Anchor, tremolos::Tremolo};

pub(super) struct Editor {
    pub open: bool,
    anchor: Option<Anchor>,
    paired: bool,
    strokes: u8,
    playback: bool,
    offset: [f32; 2],
    error: Option<String>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            anchor: None,
            paired: false,
            strokes: 3,
            playback: true,
            offset: [0.0; 2],
            error: None,
        }
    }
}
fn following(score: &Score, anchor: Anchor) -> Option<Anchor> {
    score
        .parts
        .get(anchor.part)?
        .measures
        .get(anchor.measure)?
        .notes
        .iter()
        .filter(|note| note.lane() == anchor.lane() && note.start > anchor.tick)
        .min_by_key(|note| note.start)
        .map(|note| Anchor {
            tick: note.start,
            ..anchor
        })
}
impl EncoreApp {
    pub(super) fn open_tremolos(&mut self) {
        if !self.allow_source_edit() {
            return;
        }
        let anchor = self
            .selected
            .and_then(|selected| Anchor::from_selection(&self.score, selected));
        if let Some(mark) = anchor
            .and_then(|anchor| {
                self.score
                    .notation
                    .tremolos
                    .iter()
                    .find(|mark| mark.anchor == anchor || mark.end == Some(anchor))
            })
            .cloned()
        {
            self.edit_tremolo(&mark);
        } else {
            self.tremolos_editor = Editor {
                open: true,
                anchor,
                ..Default::default()
            };
        }
        self.write_mode = false;
    }
    fn edit_tremolo(&mut self, mark: &Tremolo) {
        let a = mark.anchor;
        self.selected = a.selection(&self.score);
        self.active_part = a.part;
        self.active_layer = a.layer;
        self.active_voice = a.voice;
        self.cursor_measure = a.measure;
        self.cursor_tick = a.tick;
        if let Some(note) = encore::document::note_at(&self.score, &a) {
            self.active_voice_launch = note.voice_launch.map(|tick| Anchor {
                tick,
                voice: 0,
                ..a
            });
            if let Some(pitch) = note.pitches.first().copied() {
                self.entry_pitch = pitch;
                self.octave = self
                    .written_pitch_at(pitch, a)
                    .map_or(pitch.octave, |written| written.octave);
            }
        }
        self.tremolos_editor = Editor {
            open: true,
            anchor: Some(a),
            paired: mark.end.is_some(),
            strokes: mark.strokes,
            playback: mark.playback,
            offset: mark.offset,
            error: None,
        };
    }
    fn apply_tremolo_draft(&mut self) -> bool {
        if !self.allow_source_edit() {
            return false;
        }
        let editor = &self.tremolos_editor;
        let result: Result<Score, String> = (|| {
            let anchor = editor
                .anchor
                .ok_or("Select a note or chord before assigning a tremolo.")?;
            let end = if editor.paired {
                Some(following(&self.score,anchor).ok_or("An alternating tremolo needs a following entry in the same measure, layer and voice.")?)
            } else {
                None
            };
            let mark = Tremolo {
                anchor,
                end,
                strokes: editor.strokes,
                playback: editor.playback,
                offset: editor.offset,
            };
            let mut candidate = self.score.clone();
            candidate
                .notation
                .tremolos
                .retain(|old| old.anchor != anchor);
            candidate.notation.tremolos.push(mark);
            candidate.validate()?;
            Ok(candidate)
        })();
        match result {
            Ok(score) => {
                self.score = score;
                self.tremolos_editor.error = None;
                self.status = "Measured tremolo saved. Written timing is unchanged.".into();
                true
            }
            Err(error) => {
                self.status = error.clone();
                self.tremolos_editor.error = Some(error);
                false
            }
        }
    }
    pub(super) fn tremolos_window(&mut self, ctx: &egui::Context) {
        if !self.tremolos_editor.open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut load = false;
        let mut done = false;
        let mut edit = None;
        let mut remove = None;
        let viewport = ctx.content_rect();
        egui::Window::new("Measured tremolos").open(&mut open).vscroll(true)
            .default_pos(egui::pos2(((viewport.width()-600.0)*0.5).max(16.0),24.0))
            .constrain_to(viewport.shrink(8.0)).default_width(550.0)
            .max_width((viewport.width()-32.0).max(280.0)).max_height((viewport.height()-110.0).max(240.0))
            .show(ctx,|ui| {
                ui.horizontal_wrapped(|ui|{apply=ui.button("Save tremolo").clicked();load=ui.button("Use selected entry").clicked();if ui.button("Done").clicked(){done=true;}});
                let editor=&mut self.tremolos_editor;
                if let Some(error)=&editor.error{ui.label(error);}
                if let Some(a)=editor.anchor {
                    if let Some(part)=self.score.parts.get(a.part){ui.label(format!("{} · measure {} beat {:.2} · layer {} · voice {}",part.name,a.measure+1,a.tick as f32/PPQ as f32+1.0,a.layer+1,a.voice+1));}
                }else{ui.label("Select a note or chord, then choose Use selected entry.");}
                ui.checkbox(&mut editor.paired,"Alternate with the next entry");
                ui.horizontal_wrapped(|ui|{ui.label("Strokes");ui.add(egui::DragValue::new(&mut editor.strokes).range(1..=8));ui.checkbox(&mut editor.playback,"Play measured pulses");});
                if editor.paired {
                    ui.label("The pair uses consecutive equal quarter or half entries in one measure and voice. Both heads show double their stored value; the pair keeps its original total duration.");
                    if let Some(end)=editor.anchor.and_then(|a|following(&self.score,a)){ui.small(format!("Second entry: beat {:.2}",end.tick as f32/PPQ as f32+1.0));}
                }else{ui.label("One, two or three strokes play eighth, sixteenth or thirty-second pulses on unflagged notes. Existing flags divide further. Dots extend the repeated window; tuplets scale pulse timing.");}
                ui.horizontal(|ui|{let [x,y]=&mut editor.offset;ui.add(egui::DragValue::new(x).range(-160.0..=160.0).prefix("X "));ui.add(egui::DragValue::new(y).range(-160.0..=160.0).prefix("Y "));});
                ui.small("Playback requires subdivisions of at least one tick. Disable playback for finer visual marks. Unmeasured/buzz rolls and complex alternating rhythms remain pending.");
                ui.separator();
                egui::ScrollArea::vertical().id_salt("tremolo-list").max_height(180.0).show(ui,|ui| {
                    for (index,mark) in self.score.notation.tremolos.iter().enumerate(){ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Staff {} · M{} beat {:.2} · L{} V{} · {} strokes{}",mark.anchor.part+1,mark.anchor.measure+1,mark.anchor.tick as f32/PPQ as f32+1.0,mark.anchor.layer+1,mark.anchor.voice+1,mark.strokes,if mark.end.is_some(){" · pair"}else{""})).on_hover_text(&self.score.parts[mark.anchor.part].name);
                        if ui.small_button("Edit").clicked(){edit=Some(mark.clone());}
                        if ui.small_button("Remove").clicked(){remove=Some(index);}
                    });}
                });
            });
        if load {
            let selected = self
                .selected
                .and_then(|selection| Anchor::from_selection(&self.score, selection));
            self.tremolos_editor.anchor = selected;
            self.tremolos_editor.error = None;
        }
        if let Some(mark) = edit {
            self.edit_tremolo(&mark);
        }
        if let Some(index) = remove
            && self.allow_source_edit()
        {
            self.score.notation.tremolos.remove(index);
        }
        if apply {
            self.apply_tremolo_draft();
        }
        if !open || done {
            self.tremolos_editor.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> EncoreApp {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.enter_note(0, 0, 0, Pitch::from_midi(60), false);
        app.enter_note(0, 0, 480, Pitch::from_midi(67), false);
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        app
    }
    #[test]
    fn alternating_editor_retains_source_rhythm_and_can_edit_from_either_endpoint() {
        let mut app = app();
        let source = app.score.parts.clone();
        app.open_tremolos();
        app.tremolos_editor.paired = true;
        assert!(app.apply_tremolo_draft());
        assert_eq!(app.score.parts, source);
        let mark = app.score.notation.tremolos[0].clone();
        assert_eq!(mark.end.unwrap().tick, 480);
        app.selected = mark.end.unwrap().selection(&app.score);
        app.open_tremolos();
        assert_eq!(app.tremolos_editor.anchor, Some(mark.anchor));
        assert_eq!(app.selected, mark.anchor.selection(&app.score));
        assert_eq!(app.entry_pitch, Pitch::from_midi(60));
        app.score.validate().unwrap();
    }
    #[test]
    fn unequal_pair_draft_fails_without_replacing_the_existing_mark() {
        let mut app = app();
        app.open_tremolos();
        assert!(app.apply_tremolo_draft());
        app.score.parts[0].measures[0].notes[1].duration = Duration::Eighth;
        let source = app.score.clone();
        app.tremolos_editor.paired = true;
        assert!(!app.apply_tremolo_draft());
        assert_eq!(app.score, source);
    }
    #[test]
    fn drum_roll_entry_keeps_sound_ids_and_does_not_require_melodic_ornaments() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.score.parts[0].percussion = Some(encore::percussion::Staff::general_midi());
        app.enter_note(0, 0, 0, encore::percussion::token(38), false);
        app.open_tremolos();
        assert!(app.apply_tremolo_draft());
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches,
            vec![encore::percussion::token(38)]
        );
        assert_eq!(app.score.notation.tremolos.len(), 1);
        app.score.validate().unwrap();
    }
}
