//! Note engraving controls and Finale's contextual modify commands.
use super::*;
use crate::shortcuts::StyleCommand;
use encore::{
    document::Anchor,
    entry_style::{
        self, AccidentalDisplay, AccidentalOverride, BeamStyle, EntryStyle, Notehead, StemDirection,
    },
    slurs::SlurDirection,
};
impl EncoreApp {
    pub(super) fn apply_style_command(&mut self, command: StyleCommand) {
        let Some(selection) = self.selected else {
            self.status = "Select a note or rest to change its engraving.".into();
            return;
        };
        let Some(anchor) = Anchor::from_selection(&self.score, selection) else {
            return;
        };
        if command == StyleCommand::Respell {
            self.respell_selected_pitch();
            return;
        }
        let mut style = entry_style::at(&self.score, &anchor)
            .cloned()
            .unwrap_or_else(|| EntryStyle::new(anchor));
        let pitch = self
            .active_pitch_index()
            .and_then(|index| {
                self.selected_entry()
                    .and_then(|note| note.pitches.get(index))
            })
            .copied();
        match command {
            StyleCommand::FlipStem => {
                let automatic = self.selected_entry().is_some_and(|note| {
                    if note.layer > 0 {
                        note.layer.is_multiple_of(2)
                    } else {
                        let center = match self.score.parts[selection.part].clef {
                            Clef::Treble => 34,
                            Clef::Bass => 22,
                            Clef::Alto => 28,
                        };
                        note.pitches
                            .iter()
                            .map(|pitch| pitch.staff_step())
                            .sum::<i32>()
                            < center * note.pitches.len() as i32
                    }
                });
                style.stem = match style.stem {
                    StemDirection::Up => StemDirection::Down,
                    StemDirection::Down => StemDirection::Up,
                    _ => {
                        if automatic {
                            StemDirection::Down
                        } else {
                            StemDirection::Up
                        }
                    }
                };
            }
            StyleCommand::DefaultStem => style.stem = StemDirection::Auto,
            StyleCommand::CycleBeam => {
                style.beam = if style.beam == BeamStyle::Break {
                    BeamStyle::Join
                } else {
                    BeamStyle::Break
                }
            }
            StyleCommand::DefaultBeam => style.beam = BeamStyle::Auto,
            StyleCommand::FlatBeam => style.beam = BeamStyle::Flat,
            StyleCommand::ToggleHidden => style.visible = !style.visible,
            StyleCommand::FlipTie => {
                style.tie_direction = if style.tie_direction == SlurDirection::Below {
                    SlurDirection::Above
                } else {
                    SlurDirection::Below
                }
            }
            StyleCommand::DefaultTie => style.tie_direction = SlurDirection::Auto,
            StyleCommand::ToggleAccidental
            | StyleCommand::ToggleCourtesy
            | StyleCommand::ToggleParentheses
            | StyleCommand::DefaultAccidental => {
                let Some(pitch) = pitch else { return };
                let prior = style.accidental(pitch.midi());
                let display = match command {
                    StyleCommand::DefaultAccidental => AccidentalDisplay::Auto,
                    StyleCommand::ToggleCourtesy => {
                        if prior == AccidentalDisplay::Courtesy {
                            AccidentalDisplay::Hide
                        } else {
                            AccidentalDisplay::Courtesy
                        }
                    }
                    StyleCommand::ToggleParentheses => {
                        if prior == AccidentalDisplay::Parenthesized {
                            AccidentalDisplay::Show
                        } else {
                            AccidentalDisplay::Parenthesized
                        }
                    }
                    _ => {
                        if prior == AccidentalDisplay::Hide {
                            AccidentalDisplay::Show
                        } else {
                            AccidentalDisplay::Hide
                        }
                    }
                };
                style.accidentals.retain(|mark| mark.pitch != pitch.midi());
                if display != AccidentalDisplay::Auto {
                    style.accidentals.push(AccidentalOverride {
                        pitch: pitch.midi(),
                        display,
                    });
                }
            }
            StyleCommand::Respell => unreachable!(),
        }
        match entry_style::assign(&mut self.score, style) {
            Ok(()) => self.status = "Entry engraving updated".into(),
            Err(error) => self.status = error,
        }
    }
    fn respell_selected_pitch(&mut self) {
        let Some(index) = self.active_pitch_index() else {
            return;
        };
        let source = self.selected_entry().unwrap().pitches[index];
        let Ok(written) = self.entry_written_pitch(source) else {
            return;
        };
        let midi = i32::from(written.midi());
        let mut spellings = Vec::new();
        for octave in -1i8..=9 {
            for step in [
                Step::C,
                Step::D,
                Step::E,
                Step::F,
                Step::G,
                Step::A,
                Step::B,
            ] {
                let alter = midi - ((i32::from(octave) + 1) * 12 + step.semitone());
                if (-2..=2).contains(&alter) {
                    spellings.push(Pitch {
                        step,
                        octave,
                        alter: alter as i8,
                    });
                }
            }
        }
        spellings.sort_by_key(|pitch| pitch.staff_step());
        let Some(current) = spellings.iter().position(|pitch| *pitch == written) else {
            return;
        };
        // Prefer the adjacent conventional spelling, then allow double accidentals.
        let next = if written.alter > 0 {
            spellings
                .iter()
                .skip(current + 1)
                .find(|pitch| pitch.alter.abs() <= 1)
                .copied()
        } else if written.alter < 0 {
            spellings[..current]
                .iter()
                .rev()
                .find(|pitch| pitch.alter.abs() <= 1)
                .copied()
        } else {
            None
        }
        .unwrap_or(spellings[(current + 1) % spellings.len()]);
        match self.entry_transposition().concert_pitch(next) {
            Ok(source) => {
                if self.edit_entry(|note| note.pitches[index] = source) {
                    self.entry_pitch = source;
                    self.octave = next.octave;
                    self.status = format!("Enharmonic spelling: {}", next.name());
                }
            }
            Err(error) => self.status = error,
        }
    }
    pub(super) fn toggle_selected_pitch_tie(&mut self) {
        let Some(selection) = self.selected else {
            return;
        };
        let Some(anchor) = Anchor::from_selection(&self.score, selection) else {
            return;
        };
        let Some(index) = self.active_pitch_index() else {
            return;
        };
        let note = self.selected_entry().unwrap();
        let midi = note.pitches[index].midi();
        if note.pitches.len() == 1
            && entry_style::at(&self.score, &anchor).is_none_or(|style| style.tie_pitches.is_none())
        {
            self.edit_entry(|note| note.tied = !note.tied);
            return;
        }
        let mut style = entry_style::at(&self.score, &anchor)
            .cloned()
            .unwrap_or_else(|| EntryStyle::new(anchor));
        let ties = style.tie_pitches.get_or_insert_with(|| {
            if note.tied {
                note.pitches.iter().map(|pitch| pitch.midi()).collect()
            } else {
                Vec::new()
            }
        });
        if ties.contains(&midi) {
            ties.retain(|pitch| *pitch != midi);
        } else {
            ties.push(midi);
            ties.sort_unstable();
        }
        if let Err(error) = entry_style::assign(&mut self.score, style) {
            self.status = error;
        }
    }
    pub(super) fn entry_style_window(&mut self, ctx: &egui::Context) {
        if !self.show_entry_style {
            return;
        }
        let mut open = true;
        egui::Window::new("Entry engraving")
            .open(&mut open)
            .default_width(410.0)
            .max_width((ctx.content_rect().width()-40.0).max(260.0))
            .max_height((ctx.content_rect().height()-48.0).max(240.0))
            .vscroll(true)
            .show(ctx,|ui|{
   let Some(selection)=self.selected else{ui.label("Select a note or rest to edit its engraving.");return};let Some(anchor)=Anchor::from_selection(&self.score,selection)else{return};
   let mut style=entry_style::at(&self.score,&anchor).cloned().unwrap_or_else(||EntryStyle::new(anchor));let original=style.clone();
   ui.label(format!("Staff {} · measure {} · layer {}",anchor.part+1,anchor.measure+1,anchor.layer+1));
   egui::ComboBox::from_label("Stem").selected_text(style.stem.name()).show_ui(ui,|ui|{for value in StemDirection::ALL{ui.selectable_value(&mut style.stem,value,value.name());}});
   egui::ComboBox::from_label("Beam").selected_text(style.beam.name()).show_ui(ui,|ui|{for value in BeamStyle::ALL{ui.selectable_value(&mut style.beam,value,value.name());}});
   egui::ComboBox::from_label("Notehead").selected_text(style.notehead.name()).show_ui(ui,|ui|{for value in Notehead::ALL{ui.selectable_value(&mut style.notehead,value,value.name());}});
   ui.checkbox(&mut style.visible,"Visible in score and print");ui.add(egui::Slider::new(&mut style.scale,0.4..=2.0).text("Notehead scale"));
   ui.small("Hidden entries keep their sounding pitches and timing. Use a smaller head for cue notation.");
   let pitches=self.selected_entry().map(|note|note.pitches.clone()).unwrap_or_default();
   for pitch in pitches{
    let mut display=style.accidental(pitch.midi());let old=display;ui.horizontal(|ui|{ui.label(self.entry_written_pitch(pitch).unwrap_or(pitch).name());egui::ComboBox::from_id_salt(("pitch-accidental",pitch.midi())).selected_text(display.name()).show_ui(ui,|ui|{for value in AccidentalDisplay::ALL{ui.selectable_value(&mut display,value,value.name());}});let mut tied=style.tie_pitches.as_ref().map_or_else(||self.selected_entry().is_some_and(|note|note.tied),|ties|ties.contains(&pitch.midi()));if ui.checkbox(&mut tied,"Tie").changed(){let ties=style.tie_pitches.get_or_insert_with(||if self.selected_entry().is_some_and(|note|note.tied){self.selected_entry().unwrap().pitches.iter().map(|pitch|pitch.midi()).collect()}else{Vec::new()});ties.retain(|own|*own!=pitch.midi());if tied{ties.push(pitch.midi());}}});if old!=display{style.accidentals.retain(|mark|mark.pitch!=pitch.midi());if display!=AccidentalDisplay::Auto{style.accidentals.push(AccidentalOverride{pitch:pitch.midi(),display});}}
   }
   egui::ComboBox::from_label("Tie direction").selected_text(style.tie_direction.name()).show_ui(ui,|ui|{for value in SlurDirection::ALL{ui.selectable_value(&mut style.tie_direction,value,value.name());}});
   if ui.button("Restore automatic engraving").clicked(){style=EntryStyle::new(anchor);}
   if style!=original&&let Err(error)=entry_style::assign(&mut self.score,style){self.status=error;}
   ui.horizontal(|ui| { if ui.button("Show/hide accidental (*)").clicked() { self.apply_style_command(StyleCommand::ToggleAccidental); }
 if ui.button("Automatic accidental").clicked() { self.apply_style_command(StyleCommand::DefaultAccidental); } });
   ui.separator();ui.label("Articulations");if let Some(note)=self.score.parts.get_mut(selection.part).and_then(|part|part.measures.get_mut(selection.measure)).and_then(|bar|bar.notes.get_mut(selection.note)){ui.checkbox(&mut note.staccato,"Staccato");for &mark in Articulation::ALL{let mut enabled=note.articulations.contains(&mark);if ui.checkbox(&mut enabled,mark.name()).changed(){if enabled{note.articulations.push(mark);}else{note.articulations.retain(|prior|*prior!=mark);}}}}
  });
        self.show_entry_style = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chord() -> EncoreApp {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.enter_note(0, 0, 0, Pitch::from_midi(61), false);
        app.enter_note(0, 0, 0, Pitch::from_midi(65), true);
        app.entry_pitch = Pitch::from_midi(61);
        app
    }
    #[test]
    fn partial_tie_and_accidental_choices_follow_inspector_transposition() {
        let mut app = chord();
        app.toggle_selected_pitch_tie();
        app.apply_style_command(StyleCommand::ToggleParentheses);
        let anchor = Anchor::from_selection(&app.score, app.selected.unwrap()).unwrap();
        assert!(entry_style::ties_forward(&app.score, &anchor, 61));
        assert!(!entry_style::ties_forward(&app.score, &anchor, 65));
        app.transpose(2);
        app.score.validate().unwrap();
        let style = entry_style::at(&app.score, &anchor).unwrap();
        assert_eq!(style.tie_pitches, Some(vec![63]));
        assert_eq!(style.accidental(63), AccidentalDisplay::Parenthesized);
    }
    #[test]
    fn replacing_a_styled_note_keeps_valid_display_keys_and_tie_subset() {
        let mut app = chord();
        app.toggle_selected_pitch_tie();
        app.apply_style_command(StyleCommand::ToggleCourtesy);
        app.enter_note(0, 0, 0, Pitch::from_midi(67), false);
        app.score.validate().unwrap();
        let style = &app.score.notation.entries[0];
        assert_eq!(style.tie_pitches, Some(vec![67]));
        assert_eq!(style.accidental(67), AccidentalDisplay::Courtesy);
    }
    #[test]
    fn respelling_a_transposed_linked_part_changes_spelling_without_pitch_or_style_loss() {
        let mut app = chord();
        app.score.parts[0].instrument = Instrument::Clarinet;
        app.score.parts[0].name = "Clarinet".into();
        app.part_view = true;
        app.toggle_selected_pitch_tie();
        app.apply_style_command(StyleCommand::ToggleParentheses);
        let old = app.selected_entry().unwrap().pitches[0];
        app.apply_style_command(StyleCommand::Respell);
        let now = app.selected_entry().unwrap().pitches[0];
        assert_eq!(old.midi(), now.midi());
        assert_ne!(old, now);
        let projection = app.selected_part_projection().unwrap().unwrap();
        projection.score.validate().unwrap();
        app.score.validate().unwrap();
        assert_eq!(
            projection.score.notation.entries[0].tie_pitches,
            Some(vec![63])
        );
    }
}
