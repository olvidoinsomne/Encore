//! Letter-name chord entry and custom string diagrams over source beat anchors.
use super::*;
use encore::{
    chords::{self, Barre, BassStyle, ChordSymbol, Fretboard},
    document::Anchor,
};

pub(super) struct Editor {
    pub open: bool,
    pub active: bool,
    pub slot: u8,
    draft: String,
    error: String,
    frame: Option<Fretboard>,
    show_frame: bool,
    visible: bool,
    play: bool,
    bass_style: BassStyle,
    offset: [f32; 2],
    tuning: String,
    barre_from: usize,
    barre_to: usize,
    barre_fret: u8,
    suggestions: Vec<Fretboard>,
    suggestion: usize,
    replace_on_type: bool,
    original: Option<(String, chords::ChordSpec)>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            active: false,
            slot: 0,
            draft: String::new(),
            error: String::new(),
            frame: None,
            show_frame: true,
            visible: true,
            play: true,
            bass_style: BassStyle::Slash,
            offset: [0.; 2],
            tuning: "40 45 50 55 59 64".into(),
            barre_from: 0,
            barre_to: 5,
            barre_fret: 1,
            suggestions: vec![],
            suggestion: 0,
            replace_on_type: true,
            original: None,
        }
    }
}
impl EncoreApp {
    fn chord_anchor(&self) -> Anchor {
        let mut anchor = self
            .selected
            .and_then(|selection| Anchor::from_selection(&self.score, selection))
            .unwrap_or(Anchor {
                voice: 0,
                part: self.active_part,
                measure: self.cursor_measure,
                tick: self.cursor_tick.min(self.score.measure_ticks() - 1),
                layer: 0,
            });
        anchor.layer = 0;
        anchor.voice = 0;
        anchor
    }
    fn load_chord_draft(&mut self) {
        let anchor = self.chord_anchor();
        let found = self
            .score
            .notation
            .chords
            .iter()
            .find(|symbol| symbol.anchor == anchor && symbol.slot == self.chord_editor.slot)
            .cloned();
        let mut editor = Editor {
            active: self.chord_editor.active,
            open: self.chord_editor.open,
            slot: self.chord_editor.slot,
            ..Default::default()
        };
        if self.score.parts[anchor.part].percussion.is_some() {
            editor.play = false;
        }
        if let Some(mut symbol) = found {
            if let Err(error) = chords::transpose_symbol(&mut symbol, self.entry_transposition()) {
                editor.error = error;
            }
            editor.draft = symbol.chord.label();
            editor.original = Some((editor.draft.clone(), symbol.chord.clone()));
            editor.frame = symbol.fretboard;
            editor.show_frame = symbol.show_fretboard;
            editor.visible = symbol.visible;
            editor.play = symbol.play;
            editor.offset = symbol.offset;
            editor.bass_style = symbol.bass_style;
            if let Some(tuning) = editor
                .frame
                .as_ref()
                .and_then(|frame| frame.tuning.as_ref())
            {
                editor.tuning = tuning
                    .iter()
                    .map(u8::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
        self.chord_editor = editor;
    }
    pub(super) fn open_chords(&mut self) {
        self.grace_editor.open = false;
        self.lines_editor.open = false;
        self.chord_editor.open = true;
        self.chord_editor.active = true;
        self.write_mode = false;
        self.notation_editor.tool = notation::Tool::Select;
        self.load_chord_draft();
    }
    pub(super) fn chord_click(&mut self, part: usize, measure: usize, tick: u32) -> bool {
        if !self.chord_editor.active {
            return false;
        }
        if part >= self.score.parts.len() || measure >= self.score.measure_count() {
            return true;
        }
        self.active_part = part;
        self.cursor_measure = measure;
        self.cursor_tick = tick.min(self.score.measure_ticks() - 1);
        self.selected = None;
        self.load_chord_draft();
        true
    }
    fn apply_chord_draft(&mut self) -> bool {
        let result = (|| {
            let anchor = self.chord_anchor();
            let text = self.chord_editor.draft.trim();
            if text.is_empty() {
                return Err("Type a chord such as Cmaj7, Am, Bb7/D, or N.C.".into());
            }
            let spec = if let Some((_, spec)) = self
                .chord_editor
                .original
                .as_ref()
                .filter(|(original, _)| original == text)
            {
                spec.clone()
            } else {
                chords::parse(text)?
            };
            let mut symbol = ChordSymbol::new(anchor, spec);
            symbol.slot = self.chord_editor.slot;
            symbol.visible = self.chord_editor.visible;
            symbol.play = self.chord_editor.play;
            symbol.offset = self.chord_editor.offset;
            symbol.bass_style = self.chord_editor.bass_style;
            if text.contains('_') {
                symbol.bass_style = BassStyle::Under;
            } else if text.contains('|') {
                symbol.bass_style = BassStyle::LowerRight;
            }
            symbol.fretboard = self.chord_editor.frame.clone();
            symbol.show_fretboard = self.chord_editor.show_frame;
            let interval = self.entry_transposition();
            chords::transpose_symbol(
                &mut symbol,
                encore::parts::Transposition {
                    semitones: -interval.semitones,
                    diatonic_steps: -interval.diatonic_steps,
                },
            )?;
            let mut candidate = self.score.clone();
            candidate
                .notation
                .chords
                .retain(|existing| existing.anchor != anchor || existing.slot != symbol.slot);
            candidate.notation.chords.push(symbol);
            candidate.validate()?;
            Ok::<Score, String>(candidate)
        })();
        match result {
            Ok(score) => {
                self.score = score;
                self.chord_editor.error.clear();
                self.status="Chord symbol updated. Space advances; Shift+Space goes back; Tab changes measure.".into();
                true
            }
            Err(error) => {
                self.chord_editor.error = error.clone();
                self.status = error;
                false
            }
        }
    }
    fn delete_chord_at_cursor(&mut self) {
        let anchor = self.chord_anchor();
        let slot = self.chord_editor.slot;
        self.score
            .notation
            .chords
            .retain(|symbol| symbol.anchor != anchor || symbol.slot != slot);
        self.load_chord_draft();
        self.status = "Chord symbol removed.".into();
    }
    fn advance_chord(&mut self, backward: bool, measure: bool) {
        let anchor = self.chord_anchor();
        let ticks = self.score.measure_ticks();
        let mut positions = (0..ticks)
            .step_by(self.score.beat_ticks().max(1) as usize)
            .collect::<Vec<_>>();
        positions.extend(
            self.score.parts[anchor.part].measures[anchor.measure]
                .notes
                .iter()
                .map(|note| note.start),
        );
        positions.extend(
            self.score
                .notation
                .chords
                .iter()
                .filter(|s| s.anchor.part == anchor.part && s.anchor.measure == anchor.measure)
                .map(|s| s.anchor.tick),
        );
        positions.sort_unstable();
        positions.dedup();
        let next = if measure {
            None
        } else if backward {
            positions
                .iter()
                .rev()
                .copied()
                .find(|tick| *tick < anchor.tick)
        } else {
            positions.iter().copied().find(|tick| *tick > anchor.tick)
        };
        let (bar, tick) = if let Some(tick) = next {
            (anchor.measure, tick)
        } else if backward {
            let Some(bar) = anchor.measure.checked_sub(1) else {
                return;
            };
            (
                bar,
                if measure {
                    0
                } else {
                    ticks.saturating_sub(self.score.beat_ticks())
                },
            )
        } else {
            let bar = anchor.measure + 1;
            if bar >= self.score.measure_count() {
                self.status = "End of score. Add a measure to continue chord entry.".into();
                return;
            }
            (bar, 0)
        };
        self.active_part = anchor.part;
        self.cursor_measure = bar;
        self.cursor_tick = tick;
        self.selected = None;
        self.load_chord_draft();
    }
    /// Live chord mode consumes text only while its properties window is closed.
    /// The root still handles global Save/Undo before this method.
    pub(super) fn chord_keyboard(&mut self, ctx: &egui::Context) -> bool {
        if !self.chord_editor.active || self.chord_editor.open || ctx.wants_keyboard_input() {
            return false;
        }
        let events = ctx.input(|input| input.events.clone());
        if events.iter().any(|event| matches!(event, egui::Event::Key { pressed:true,modifiers,.. } if modifiers.command||modifiers.ctrl||modifiers.alt)) {
            return false;
        }
        for event in events {
            match event {
                egui::Event::Text(text) => {
                    if text == " " {
                        continue;
                    }
                    if self.chord_editor.replace_on_type {
                        self.chord_editor.draft.clear();
                        self.chord_editor.replace_on_type = false;
                    }
                    self.chord_editor.draft.push_str(&text);
                    self.chord_editor.frame = None;
                    self.status = format!(
                        "Chord: {} · Enter accepts; Space advances.",
                        self.chord_editor.draft
                    );
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.command && !modifiers.ctrl && !modifiers.alt => match key {
                    egui::Key::Space
                    | egui::Key::ArrowRight
                    | egui::Key::ArrowLeft
                    | egui::Key::Tab => {
                        if !self.chord_editor.draft.trim().is_empty() && !self.apply_chord_draft() {
                            return true;
                        }
                        self.advance_chord(
                            modifiers.shift || key == egui::Key::ArrowLeft,
                            key == egui::Key::Tab,
                        );
                    }
                    egui::Key::Enter => {
                        self.apply_chord_draft();
                    }
                    egui::Key::ArrowUp => {
                        self.chord_editor.slot = (self.chord_editor.slot + 1).min(7);
                        self.load_chord_draft();
                    }
                    egui::Key::ArrowDown => {
                        self.chord_editor.slot = self.chord_editor.slot.saturating_sub(1);
                        self.load_chord_draft();
                    }
                    egui::Key::Backspace => {
                        if self.chord_editor.replace_on_type {
                            self.chord_editor.draft.clear();
                            self.chord_editor.replace_on_type = false;
                        } else {
                            self.chord_editor.draft.pop();
                        }
                        self.chord_editor.frame = None;
                        self.status = format!("Chord: {}", self.chord_editor.draft);
                    }
                    egui::Key::Delete => self.delete_chord_at_cursor(),
                    _ => {}
                },
                _ => {}
            }
        }
        true
    }
    pub(super) fn chord_dialog(&mut self, ctx: &egui::Context) {
        if !self.chord_editor.open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut next = false;
        let mut previous = false;
        let mut delete = false;
        egui::Window::new("Chord symbols and fretboards").open(&mut open).default_width(550.).max_width((ctx.content_rect().width()-48.).max(280.)).max_height((ctx.content_rect().height()-48.).max(240.)).vscroll(true).show(ctx,|ui|{
        let anchor=self.chord_anchor();ui.label(format!("{} · measure {} · beat {:.2} · stack {}",self.score.parts[anchor.part].name,anchor.measure+1,anchor.tick as f32/self.score.beat_ticks() as f32+1.,self.chord_editor.slot+1));ui.label("Enter written chord names in a transposed part. The source keeps concert pitches.");
        let response=ui.add(egui::TextEdit::singleline(&mut self.chord_editor.draft).hint_text("Cmaj7, Am7, Bb7(b9)/D, C6/9, N.C."));
if response.changed(){self.chord_editor.frame=None;}
if response.lost_focus()&&ui.input(|input|input.key_pressed(egui::Key::Enter)){apply=true;}
        ui.horizontal(|ui|{for label in["C","Am","Fmaj7","G7","Dm7","N.C."]{if ui.small_button(label).clicked(){self.chord_editor.draft=label.into();self.chord_editor.frame=None;}}});
        ui.horizontal(|ui|{ui.checkbox(&mut self.chord_editor.visible,"Print symbol");ui.add_enabled(self.score.parts[anchor.part].percussion.is_none(),egui::Checkbox::new(&mut self.chord_editor.play,"Play harmony"));ui.checkbox(&mut self.chord_editor.show_frame,"Print fretboard");});
        ui.horizontal(|ui|{ui.label("Bass position");ui.selectable_value(&mut self.chord_editor.bass_style,BassStyle::Slash,"Slash");ui.selectable_value(&mut self.chord_editor.bass_style,BassStyle::Under,"Under");ui.selectable_value(&mut self.chord_editor.bass_style,BassStyle::LowerRight,"Lower right");});
        ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut self.chord_editor.offset[0]).range(-200.0..=200.0).prefix("Horizontal "));ui.add(egui::DragValue::new(&mut self.chord_editor.offset[1]).range(-200.0..=200.0).prefix("Vertical "));});
        ui.separator();ui.horizontal(|ui|{if ui.button("Suggest guitar shapes").clicked(){match chords::parse(&self.chord_editor.draft){Ok(chord)=>{self.chord_editor.suggestions=chords::suggest_fretboards(&chord);self.chord_editor.suggestion=0;self.chord_editor.frame=self.chord_editor.suggestions.first().cloned();
if self.chord_editor.frame.is_none(){self.chord_editor.error="No playable standard-guitar shape found; create a custom diagram below.".into();}},Err(error)=>self.chord_editor.error=error}}
if !self.chord_editor.suggestions.is_empty()&&ui.button("Next shape").clicked(){self.chord_editor.suggestion=(self.chord_editor.suggestion+1)%self.chord_editor.suggestions.len();self.chord_editor.frame=Some(self.chord_editor.suggestions[self.chord_editor.suggestion].clone());}
if ui.button("Custom diagram").clicked(){self.chord_editor.frame=Some(Fretboard::guitar(vec![None;6]));}
if ui.button("Remove diagram").clicked(){self.chord_editor.frame=None;}});
        if let Some(frame)=&mut self.chord_editor.frame{
            let mut count=frame.frets.len();
if ui.add(egui::DragValue::new(&mut count).range(4..=12).prefix("Strings ")).changed(){frame.frets.resize(count,None);frame.fingers.resize(count,None);frame.tuning=None;frame.barres.clear();self.chord_editor.barre_to=count-1;}
            ui.label("Strings run low to high. −1 mutes, 0 opens, positive numbers stop a fret.");ui.horizontal(|ui|{for (index,fret)in frame.frets.iter_mut().enumerate(){let mut value=fret.map_or(-1,i16::from);ui.vertical(|ui|{ui.label(format!("{}",index+1));
if ui.add(egui::DragValue::new(&mut value).range(-1..=36)).changed(){*fret=(value>=0).then_some(value as u8);}
if !frame.fingers.is_empty(){let mut finger=frame.fingers[index].map_or(0,i16::from);
if ui.add(egui::DragValue::new(&mut finger).range(0..=5).prefix("f")).changed(){frame.fingers[index]=(finger>0).then_some(finger as u8);}}});}});
            ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut frame.first_fret).range(1..=36).prefix("First fret "));ui.add(egui::DragValue::new(&mut frame.displayed_frets).range(1..=8).prefix("Frets shown "));});
            ui.horizontal(|ui|{ui.label("Tuning MIDI notes");ui.text_edit_singleline(&mut self.chord_editor.tuning);
if ui.button("Set tuning").clicked(){match self.chord_editor.tuning.split_whitespace().map(str::parse::<u8>).collect::<Result<Vec<_>,_>>(){Ok(tuning)if tuning.len()==frame.frets.len()=>frame.tuning=Some(tuning),_=>self.chord_editor.error="Enter one MIDI note per string, low to high.".into()}}
if ui.button("Unknown").clicked(){frame.tuning=None;}});
            ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut self.chord_editor.barre_from).range(0..=frame.frets.len()-1).prefix("Barre from "));ui.add(egui::DragValue::new(&mut self.chord_editor.barre_to).range(0..=frame.frets.len()-1).prefix("to "));ui.add(egui::DragValue::new(&mut self.chord_editor.barre_fret).range(1..=36).prefix("fret "));
if ui.button("Add barre").clicked(){frame.barres.push(Barre{from:self.chord_editor.barre_from.min(self.chord_editor.barre_to),to:self.chord_editor.barre_from.max(self.chord_editor.barre_to),fret:self.chord_editor.barre_fret});}
if ui.button("Clear barres").clicked(){frame.barres.clear();}});
            if let Err(error)=frame.validate(){ui.label(error);}
        }
        if !self.chord_editor.error.is_empty(){ui.separator();ui.label(&self.chord_editor.error);}
        ui.separator();ui.horizontal(|ui|{apply|=ui.button("Apply").clicked();previous=ui.button("Apply & previous").clicked();next=ui.button("Apply & next").clicked();delete=ui.button("Delete chord").clicked();
if ui.button("Type on score").clicked(){self.chord_editor.open=false;}});ui.small("On-score keys: Enter accepts; Space / Shift+Space moves; Tab / Shift+Tab changes measure; Up / Down selects a stack. Escape exits.");
    });
        if delete {
            self.delete_chord_at_cursor();
        }
        if (apply || next || previous) && self.apply_chord_draft() && (next || previous) {
            self.advance_chord(previous, false);
        }
        self.chord_editor.open &= open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silent_harmony_over_drums_uses_written_part_pitch_while_drum_tokens_stay_fixed() {
        let mut score = Score::blank();
        score.parts[0].percussion = Some(encore::percussion::Staff::general_midi());
        let mut definition = encore::parts::default_definitions(&score)[0].clone();
        definition.staves = vec![0, 1];
        definition.transposition = encore::parts::Transposition {
            semitones: 2,
            diatonic_steps: 1,
        };
        score.notation.parts = vec![definition];
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.part_view = true;
        app.active_part = 0;
        app.entry_pitch = encore::percussion::token(38);
        app.apply_entry_command(EntryCommand::Enter);
        app.open_chords();
        app.chord_editor.draft = "D7/F#".into();
        assert!(!app.chord_editor.play);
        assert!(app.apply_chord_draft());
        assert_eq!(app.score.notation.chords[0].chord.label(), "C7/E");
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches,
            vec![encore::percussion::token(38)]
        );
        app.load_chord_draft();
        assert_eq!(app.chord_editor.draft, "D7/F#");
        app.score.validate().unwrap();
    }
    #[test]
    fn written_chord_entry_keeps_source_concert_and_undo_snapshots() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.active_part = 1;
        app.score.parts[1].instrument = Instrument::Clarinet;
        app.score.notation.parts = encore::parts::default_definitions(&app.score);
        app.part_view = true;
        app.selected_linked_part = 1;
        app.open_chords();
        app.chord_editor.draft = "D7/F#".into();
        assert!(app.apply_chord_draft());
        assert_eq!(app.score.notation.chords[0].chord.label(), "C7/E");
        let source = app.score.clone();
        app.load_chord_draft();
        assert_eq!(app.chord_editor.draft, "D7/F#");
        assert_eq!(app.score, source);
    }
    #[test]
    fn invalid_chord_edit_does_not_remove_previous_symbol() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.open_chords();
        app.chord_editor.draft = "Cmaj7".into();
        assert!(app.apply_chord_draft());
        let old = app.score.clone();
        app.chord_editor.draft = "Cbananas".into();
        assert!(!app.apply_chord_draft());
        assert_eq!(app.score, old);
    }
    #[test]
    fn editing_imported_custom_suffix_keeps_its_semantics() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        let mut chord = chords::parse("C7b9").unwrap();
        chord.suffix_text = Some("custom seventh".into());
        let symbol = ChordSymbol::new(
            Anchor {
                voice: 0,
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
            },
            chord.clone(),
        );
        app.score.notation.chords.push(symbol);
        app.open_chords();
        app.chord_editor.play = false;
        assert!(app.apply_chord_draft());
        assert_eq!(app.score.notation.chords[0].chord, chord);
        assert!(!app.score.notation.chords[0].play);
    }
    #[test]
    fn live_entry_replaces_loaded_text_and_passes_global_shortcuts_through() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.chord_editor.active = true;
        app.chord_editor.draft = "C".into();
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Text("D7".into())],
                ..Default::default()
            },
            |ctx| assert!(app.chord_keyboard(ctx)),
        );
        assert_eq!(app.chord_editor.draft, "D7");
        let command = egui::Modifiers {
            command: true,
            ctrl: true,
            ..Default::default()
        };
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Z,
                    pressed: true,
                    repeat: false,
                    physical_key: None,
                    modifiers: command,
                }],
                ..Default::default()
            },
            |ctx| assert!(!app.chord_keyboard(ctx)),
        );
    }
}
