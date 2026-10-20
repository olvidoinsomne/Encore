//! Contextual lyric, phrase and repeat editing over the shared score document.
use super::*;
use encore::{
    document::Anchor,
    lyrics::{self, Lyric, LyricKind, Syllabic},
    repeats::{Ending, NavigationKind, NavigationMark, RepeatRegion},
    slurs::{self, Slur, SlurDirection},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Select,
    Lyrics,
    Slurs,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Lyrics,
    Slurs,
    Repeats,
}
#[derive(Clone)]
struct EndingDraft {
    start: usize,
    end: usize,
    passes: String,
}
pub(super) struct Editor {
    pub tool: Tool,
    pub open: bool,
    tab: Tab,
    kind: LyricKind,
    verse: u16,
    batch: String,
    slur_start: Option<Anchor>,
    slur_index: usize,
    repeat_start: usize,
    repeat_end: usize,
    repeat_times: u8,
    endings: Vec<EndingDraft>,
    editing_repeat: Option<usize>,
    navigation_measure: usize,
    navigation_kind: NavigationKind,
    navigation_repeats: bool,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            open: false,
            tab: Tab::Lyrics,
            kind: LyricKind::Verse,
            verse: 1,
            batch: String::new(),
            slur_start: None,
            slur_index: 0,
            repeat_start: 1,
            repeat_end: 2,
            repeat_times: 2,
            endings: Vec::new(),
            editing_repeat: None,
            navigation_measure: 1,
            navigation_kind: NavigationKind::Segno,
            navigation_repeats: false,
        }
    }
}
impl EncoreApp {
    fn annotation_anchor(&self) -> Option<Anchor> {
        Anchor::from_selection(&self.score, self.selected?)
    }
    fn select_anchor(&mut self, anchor: Anchor) {
        self.selected = anchor.selection(&self.score);
        self.active_part = anchor.part;
        self.active_layer = anchor.layer;
        self.cursor_measure = anchor.measure;
        self.cursor_tick = anchor.tick;
    }
    pub(super) fn open_lyrics(&mut self) {
        self.exit_chord_entry();
        self.notation_editor.open = true;
        self.notation_editor.tab = Tab::Lyrics;
        self.notation_editor.tool = Tool::Lyrics;
        self.write_mode = false;
    }
    pub(super) fn open_slurs(&mut self) {
        self.exit_chord_entry();
        self.notation_editor.open = true;
        self.notation_editor.tab = Tab::Slurs;
        self.notation_editor.tool = Tool::Slurs;
        self.write_mode = false;
    }
    pub(super) fn open_repeats(&mut self) {
        self.exit_chord_entry();
        self.notation_editor.open = true;
        self.notation_editor.tab = Tab::Repeats;
        self.notation_editor.tool = Tool::Select;
        self.write_mode = false;
    }
    /// Return true when this tool consumed a click, including incomplete spans.
    pub(super) fn begin_slur_at(&mut self, selection: Selection) {
        if let Some(anchor) = Anchor::from_selection(&self.score, selection)
            && encore::document::note_at(&self.score, &anchor)
                .is_some_and(|note| !note.pitches.is_empty())
        {
            self.notation_editor.slur_start = Some(anchor);
            self.select_anchor(anchor);
        }
    }
    pub(super) fn annotation_click(&mut self, selection: Selection) -> bool {
        if self.notation_editor.tool != Tool::Slurs {
            return false;
        }
        let Some(anchor) = Anchor::from_selection(&self.score, selection) else {
            return true;
        };
        self.select_anchor(anchor);
        if let Some(start) = self.notation_editor.slur_start {
            match slurs::add(&mut self.score, Slur::new(start, anchor)) {
                Ok(()) => {
                    self.notation_editor.slur_start = None;
                    self.notation_editor.slur_index =
                        self.score.notation.slurs.len().saturating_sub(1);
                    self.status="Slur added. Choose another starting note, or adjust its curve in Notation.".into();
                }
                Err(error) => self.status = error,
            }
        } else if encore::document::note_at(&self.score, &anchor)
            .is_some_and(|note| !note.pitches.is_empty())
        {
            self.notation_editor.slur_start = Some(anchor);
            self.status =
                "Slur start set. Click its final note on the same staff and layer.".into();
        } else {
            self.status = "A slur must start on a pitched note.".into();
        }
        true
    }
    fn move_lyric(&mut self, backward: bool) {
        let anchor = self.annotation_anchor().unwrap_or(Anchor {
            part: self.active_part,
            measure: self.cursor_measure,
            tick: self.cursor_tick.saturating_add(u32::from(backward)),
            layer: self.active_layer,
        });
        if let Some(next) = lyrics::next_anchor(&self.score, &anchor, backward) {
            self.select_anchor(next);
        }
    }
    fn type_lyric(&mut self, text: &str) {
        let before = self.score.clone();
        let location = (
            self.selected,
            self.active_part,
            self.active_layer,
            self.cursor_measure,
            self.cursor_tick,
        );
        if let Err(error) = self.try_type_lyric(text) {
            self.score = before;
            (
                self.selected,
                self.active_part,
                self.active_layer,
                self.cursor_measure,
                self.cursor_tick,
            ) = location;
            self.status = error;
        }
    }
    fn advance_lyric(&mut self) {
        if let Some(anchor) = self.annotation_anchor() {
            if let Some(next) = lyrics::next_anchor(&self.score, &anchor, false) {
                self.select_anchor(next);
            } else {
                self.selected = None;
                self.status = "End of melody. Select another note to continue lyrics.".into();
            }
        }
    }
    fn try_type_lyric(&mut self, text: &str) -> Result<(), String> {
        let kind = self.notation_editor.kind;
        let verse = self.notation_editor.verse;
        for character in text.chars() {
            let Some(anchor) = self.annotation_anchor() else {
                return Err(
                    "Select a melody note before typing lyrics; the input event was not applied."
                        .into(),
                );
            };
            if encore::document::note_at(&self.score, &anchor)
                .is_none_or(|note| note.pitches.is_empty())
            {
                return Err("Choose a pitched melody note for lyrics.".into());
            }
            if character == '_' {
                let mut candidates = self
                    .score
                    .notation
                    .lyrics
                    .iter_mut()
                    .filter(|lyric| {
                        lyric.kind == kind
                            && lyric.verse == verse
                            && lyric.anchor.part == anchor.part
                            && lyric.anchor.layer == anchor.layer
                            && lyrics::position(&lyric.anchor) < lyrics::position(&anchor)
                    })
                    .collect::<Vec<_>>();
                candidates.sort_by_key(|lyric| lyrics::position(&lyric.anchor));
                if let Some(lyric) = candidates.last_mut() {
                    lyric.extend_to = Some(anchor);
                } else {
                    return Err("Type a syllable before extending a word.".into());
                }
                self.advance_lyric();
            } else if character == '-' || character == ' ' || character == '\n' {
                if character == ' '
                    && !self.score.notation.lyrics.iter().any(|lyric| {
                        lyric.anchor == anchor && lyric.kind == kind && lyric.verse == verse
                    })
                    && let Some(lyric) = self
                        .score
                        .notation
                        .lyrics
                        .iter_mut()
                        .filter(|lyric| {
                            lyric.kind == kind
                                && lyric.verse == verse
                                && lyric.anchor.part == anchor.part
                                && lyric.anchor.layer == anchor.layer
                                && lyrics::position(&lyric.anchor) < lyrics::position(&anchor)
                        })
                        .max_by_key(|lyric| lyrics::position(&lyric.anchor))
                {
                    lyric.extend_to = Some(anchor);
                }
                if character == '-'
                    && let Some(lyric) = self.score.notation.lyrics.iter_mut().find(|lyric| {
                        lyric.anchor == anchor && lyric.kind == kind && lyric.verse == verse
                    })
                {
                    lyric.syllabic = if lyric.syllabic == Syllabic::End {
                        Syllabic::Middle
                    } else {
                        Syllabic::Begin
                    };
                }
                self.advance_lyric();
            } else if !character.is_control() {
                let mut lyric = self
                    .score
                    .notation
                    .lyrics
                    .iter()
                    .find(|lyric| {
                        lyric.anchor == anchor && lyric.kind == kind && lyric.verse == verse
                    })
                    .cloned()
                    .unwrap_or_else(|| Lyric {
                        kind,
                        verse,
                        ..Lyric::new(anchor, "")
                    });
                if lyric.text.is_empty()
                    && let Some(previous) = lyrics::next_anchor(&self.score, &anchor, true)
                    && self.score.notation.lyrics.iter().any(|lyric| {
                        lyric.anchor == previous
                            && lyric.kind == kind
                            && lyric.verse == verse
                            && matches!(lyric.syllabic, Syllabic::Begin | Syllabic::Middle)
                    })
                {
                    lyric.syllabic = Syllabic::End;
                }
                lyric.text.push(character);
                lyrics::assign(&mut self.score, lyric)?;
            }
        }
        Ok(())
    }
    /// Finale's lyric keys act only while the Lyrics tool owns score input.
    pub(super) fn annotation_keyboard(&mut self, ctx: &egui::Context) -> bool {
        if self.notation_editor.tool == Tool::Slurs
            && ctx.input(|input| !input.modifiers.command && input.key_pressed(egui::Key::Delete))
        {
            if self.notation_editor.slur_index < self.score.notation.slurs.len() {
                self.score
                    .notation
                    .slurs
                    .remove(self.notation_editor.slur_index);
            }
            return true;
        }
        if self.notation_editor.tool == Tool::Slurs
            && ctx
                .input(|input| !input.modifiers.command && input.key_pressed(egui::Key::Backspace))
        {
            if let Some(slur) = self
                .score
                .notation
                .slurs
                .get_mut(self.notation_editor.slur_index)
            {
                *slur = Slur::new(slur.start, slur.end);
            }
            return true;
        }
        if self.notation_editor.tool == Tool::Slurs
            && ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::F))
        {
            if let Some(slur) = self
                .score
                .notation
                .slurs
                .get_mut(self.notation_editor.slur_index)
            {
                slur.direction = if slur.direction == SlurDirection::Below {
                    SlurDirection::Above
                } else {
                    SlurDirection::Below
                };
            }
            return true;
        }
        if self.notation_editor.tool != Tool::Lyrics {
            return false;
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::L)) {
            self.open_lyrics();
            return true;
        }
        if ctx.input(|input| input.modifiers.command || input.modifiers.alt) {
            return false;
        }
        let events = ctx.input(|input| input.events.clone());
        for event in events {
            match event {
                egui::Event::Text(text) | egui::Event::Paste(text) => self.type_lyric(&text),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.command && !modifiers.alt => {
                    match key {
                        egui::Key::Space=>{ if !ctx.input(|input|input.events.iter().any(|event|matches!(event,egui::Event::Text(text) if text.contains(' ')))){self.type_lyric(" ");} }
                        egui::Key::ArrowLeft=>self.move_lyric(true),
                        egui::Key::ArrowRight=>self.move_lyric(false),
                        egui::Key::ArrowUp=>self.notation_editor.verse=self.notation_editor.verse.saturating_sub(1).max(1),
                        egui::Key::ArrowDown=>self.notation_editor.verse=(self.notation_editor.verse+1).min(512),
                        egui::Key::Backspace=>{if let Some(anchor)=self.annotation_anchor(){let kind=self.notation_editor.kind;let verse=self.notation_editor.verse;let text=self.score.notation.lyrics.iter().find(|lyric|lyric.anchor==anchor&&lyric.kind==kind&&lyric.verse==verse).map(|lyric|lyric.text.clone());if let Some(mut text)=text{ text.pop();let _=lyrics::set_text(&mut self.score,&anchor,kind,verse,&text);}else{self.move_lyric(true);}}}
                        egui::Key::Delete=>{if let Some(anchor)=self.annotation_anchor(){lyrics::remove(&mut self.score,&anchor,self.notation_editor.kind,self.notation_editor.verse);}}
                        _=>{}
                    }
                    ctx.input_mut(|input| {
                        input.consume_key(modifiers, key);
                    });
                    ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                }
                _ => {}
            }
        }
        true
    }
    pub(super) fn notation_dialog(&mut self, ctx: &egui::Context) {
        if !self.notation_editor.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Notation")
            .open(&mut open)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.notation_editor.tab, Tab::Lyrics, "Lyrics");
                    ui.selectable_value(&mut self.notation_editor.tab, Tab::Slurs, "Slurs");
                    ui.selectable_value(&mut self.notation_editor.tab, Tab::Repeats, "Repeats");
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("notation-dialog-scroll")
                    .max_height(560.0)
                    .show(ui, |ui| match self.notation_editor.tab {
                        Tab::Lyrics => self.lyrics_editor(ui),
                        Tab::Slurs => self.slurs_editor(ui),
                        Tab::Repeats => self.repeats_editor(ui),
                    });
            });
        self.notation_editor.open = open;
    }
    fn lyrics_editor(&mut self, ui: &mut egui::Ui) {
        ui.label("Select a melody note, then type on the score. Space and hyphen advance; underscore extends a word. Arrow keys change notes or verses.");
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("lyric-kind")
                .selected_text(self.notation_editor.kind.name())
                .show_ui(ui, |ui| {
                    for kind in LyricKind::ALL {
                        ui.selectable_value(&mut self.notation_editor.kind, kind, kind.name());
                    }
                });
            ui.add(
                egui::DragValue::new(&mut self.notation_editor.verse)
                    .range(1..=512)
                    .prefix("No. "),
            );
            if ui
                .selectable_label(self.notation_editor.tool == Tool::Lyrics, "Type into score")
                .clicked()
            {
                self.notation_editor.tool = Tool::Lyrics;
                self.write_mode = false;
                ui.memory_mut(|memory| {
                    if let Some(focused) = memory.focused() {
                        memory.surrender_focus(focused);
                    }
                });
            }
        });
        if let Some(anchor) = self.annotation_anchor() {
            ui.label(format!(
                "Staff {} · measure {} · layer {}",
                anchor.part + 1,
                anchor.measure + 1,
                anchor.layer + 1
            ));
            let kind = self.notation_editor.kind;
            let verse = self.notation_editor.verse;
            let mut lyric = self
                .score
                .notation
                .lyrics
                .iter()
                .find(|lyric| lyric.anchor == anchor && lyric.kind == kind && lyric.verse == verse)
                .cloned()
                .unwrap_or_else(|| Lyric {
                    kind,
                    verse,
                    ..Lyric::new(anchor, "")
                });
            let mut changed = ui.text_edit_singleline(&mut lyric.text).changed();
            ui.horizontal(|ui| {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut lyric.baseline)
                            .range(-24.0..=120.0)
                            .prefix("Baseline "),
                    )
                    .changed();
                egui::ComboBox::from_id_salt("lyric-syllabic")
                    .selected_text(lyric.syllabic.name())
                    .show_ui(ui, |ui| {
                        for syllabic in Syllabic::ALL {
                            changed |= ui
                                .selectable_value(&mut lyric.syllabic, syllabic, syllabic.name())
                                .changed();
                        }
                    });
            });
            if changed {
                if lyric.text.is_empty() {
                    lyrics::remove(&mut self.score, &anchor, kind, verse);
                } else if let Err(error) = lyrics::assign(&mut self.score, lyric) {
                    self.status = error;
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Previous note").clicked() {
                    self.move_lyric(true);
                }
                if ui.button("Next note").clicked() {
                    self.move_lyric(false);
                }
                if ui.button("Remove syllable").clicked() {
                    lyrics::remove(&mut self.score, &anchor, kind, verse);
                }
            });
            ui.collapsing("Assign a lyric passage",|ui|{
                ui.label("Hyphens divide syllables; underscores reserve notes for a word extension. Assignment starts at the selected note.");ui.text_edit_multiline(&mut self.notation_editor.batch);
                if ui.button("Assign passage").clicked(){match lyrics::assign_text(&mut self.score,anchor,kind,verse,&self.notation_editor.batch){Ok(end)=>{self.select_anchor(end);self.status="Lyric passage assigned".into();}Err(error)=>self.status=error}}
            });
        } else {
            ui.label("Select a pitched note to assign a syllable.");
        }
    }
    fn slurs_editor(&mut self, ui: &mut egui::Ui) {
        ui.label("Choose Draw slurs, then click the beginning and final notes. Slurs may span measures, systems and pages.");
        if ui
            .selectable_label(self.notation_editor.tool == Tool::Slurs, "Draw slurs")
            .clicked()
        {
            self.notation_editor.tool = Tool::Slurs;
            self.write_mode = false;
        }
        if let Some(start) = self.notation_editor.slur_start {
            ui.label(format!(
                "Start: staff {}, measure {}. Choose the final note.",
                start.part + 1,
                start.measure + 1
            ));
            if ui.button("Cancel span").clicked() {
                self.notation_editor.slur_start = None;
            }
        }
        if self.score.notation.slurs.is_empty() {
            ui.label("No phrase slurs in this score.");
            return;
        }
        self.notation_editor.slur_index = self
            .notation_editor
            .slur_index
            .min(self.score.notation.slurs.len() - 1);
        egui::ComboBox::from_id_salt("slur-index")
            .selected_text(format!("Slur {}", self.notation_editor.slur_index + 1))
            .show_ui(ui, |ui| {
                for (index, slur) in self.score.notation.slurs.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.notation_editor.slur_index,
                        index,
                        format!(
                            "{} · staff {} · measures {}–{}",
                            index + 1,
                            slur.start.part + 1,
                            slur.start.measure + 1,
                            slur.end.measure + 1
                        ),
                    );
                }
            });
        let index = self.notation_editor.slur_index;
        let mut slur = self.score.notation.slurs[index].clone();
        let original = slur.clone();
        egui::ComboBox::from_id_salt("slur-direction")
            .selected_text(slur.direction.name())
            .show_ui(ui, |ui| {
                for direction in SlurDirection::ALL {
                    ui.selectable_value(&mut slur.direction, direction, direction.name());
                }
            });
        ui.add(egui::Slider::new(&mut slur.height, 4.0..=100.0).text("Curve height"));
        for (label, offset) in [
            ("Start", &mut slur.start_offset),
            ("End", &mut slur.end_offset),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut offset[0])
                        .range(-80.0..=80.0)
                        .prefix("x "),
                );
                ui.add(
                    egui::DragValue::new(&mut offset[1])
                        .range(-80.0..=80.0)
                        .prefix("y "),
                );
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Use selected note as start").clicked()
                && let Some(anchor) = self.annotation_anchor()
            {
                slur.start = anchor;
            }
            if ui.button("Use selected note as end").clicked()
                && let Some(anchor) = self.annotation_anchor()
            {
                slur.end = anchor;
            }
        });
        if ui.button("Reset curve").clicked() {
            slur = Slur::new(slur.start, slur.end);
        }
        if slur != original {
            let mut edited = self.score.clone();
            edited.notation.slurs[index] = slur;
            match slurs::validate(&edited) {
                Ok(()) => self.score = edited,
                Err(error) => self.status = error,
            }
        }
        if ui.button("Delete slur").clicked() {
            self.score.notation.slurs.remove(index);
        }
        ui.label("Ctrl/Cmd+F flips the active slur while Draw slurs is selected.");
    }
    fn repeats_editor(&mut self, ui: &mut egui::Ui) {
        ui.label("Repeats affect playback and MIDI. Measures below include the entire region through its final alternate ending.");
        let mut load = None;
        let mut remove = None;
        for (index, region) in self.score.notation.repeats.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(
                        self.notation_editor.editing_repeat == Some(index),
                        format!(
                            "Measures {}–{} · {} passes",
                            region.start + 1,
                            region.end + 1,
                            region.times
                        ),
                    )
                    .clicked()
                {
                    load = Some(index);
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = load {
            let region = &self.score.notation.repeats[index];
            self.notation_editor.repeat_start = region.start + 1;
            self.notation_editor.repeat_end = region.end + 1;
            self.notation_editor.repeat_times = region.times;
            self.notation_editor.endings = region
                .endings
                .iter()
                .map(|ending| EndingDraft {
                    start: ending.start + 1,
                    end: ending.end + 1,
                    passes: encore::repeats::pass_text(&ending.passes),
                })
                .collect();
            self.notation_editor.editing_repeat = Some(index);
        }
        if let Some(index) = remove {
            self.score.notation.repeats.remove(index);
            self.notation_editor.editing_repeat = None;
        }
        ui.separator();
        let count = self.score.measure_count();
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.notation_editor.repeat_start)
                    .range(1..=count)
                    .prefix("First "),
            );
            ui.add(
                egui::DragValue::new(&mut self.notation_editor.repeat_end)
                    .range(1..=count)
                    .prefix("Last "),
            );
            ui.add(
                egui::DragValue::new(&mut self.notation_editor.repeat_times)
                    .range(2..=16)
                    .prefix("Passes "),
            );
        });
        for ending in &mut self.notation_editor.endings {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut ending.start)
                        .range(1..=count)
                        .prefix("Ending from "),
                );
                ui.add(
                    egui::DragValue::new(&mut ending.end)
                        .range(1..=count)
                        .prefix("to "),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut ending.passes)
                        .desired_width(65.0)
                        .hint_text("1,2"),
                );
            });
        }
        ui.horizontal(|ui| {
            if ui.button("Add ending").clicked() {
                self.notation_editor.endings.push(EndingDraft {
                    start: self.notation_editor.repeat_end,
                    end: self.notation_editor.repeat_end,
                    passes: (self.notation_editor.endings.len() + 1).to_string(),
                });
            }
            if ui.button("Clear endings").clicked() {
                self.notation_editor.endings.clear();
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button(if self.notation_editor.editing_repeat.is_some() {
                    "Update repeat"
                } else {
                    "Add repeat"
                })
                .clicked()
            {
                self.apply_repeat_draft();
            }
            if ui.button("New region").clicked() {
                self.notation_editor.editing_repeat = None;
                self.notation_editor.endings.clear();
                self.notation_editor.repeat_start = self.cursor_measure + 1;
                self.notation_editor.repeat_end = (self.cursor_measure + 2).min(count);
            }
        });
        ui.separator();
        ui.label(RichText::new("D.C., D.S., coda and Fine").strong());
        ui.small("Add segno, coda, To Coda and Fine targets before assigning a jump. Marks act at bar boundaries; return passages use final endings unless repeat replay is selected.");
        let mut remove_mark = None;
        for (index, mark) in self.score.notation.navigation.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Measure {} · {}",
                    mark.measure + 1,
                    mark.kind.name()
                ));
                if ui.small_button("Remove mark").clicked() {
                    remove_mark = Some(index);
                }
            });
        }
        if let Some(index) = remove_mark {
            let mut edited = self.score.clone();
            edited.notation.navigation.remove(index);
            match encore::repeats::validate(&edited) {
                Ok(()) => self.score = edited,
                Err(error) => self.status = error,
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.notation_editor.navigation_measure)
                    .range(1..=count)
                    .prefix("Measure "),
            );
            egui::ComboBox::from_id_salt("navigation-kind")
                .selected_text(self.notation_editor.navigation_kind.name())
                .show_ui(ui, |ui| {
                    for kind in NavigationKind::ALL {
                        ui.selectable_value(
                            &mut self.notation_editor.navigation_kind,
                            kind,
                            kind.name(),
                        );
                    }
                });
        });
        if self.notation_editor.navigation_kind.is_jump() {
            ui.checkbox(
                &mut self.notation_editor.navigation_repeats,
                "Play repeats after jump",
            );
        }
        if ui.button("Assign navigation mark").clicked() {
            self.apply_navigation_mark();
        }
        if let Ok(order) = self.score.playback_order() {
            let preview = order
                .iter()
                .take(48)
                .map(|measure| (measure + 1).to_string())
                .collect::<Vec<_>>()
                .join(" → ");
            ui.label(format!(
                "Performed order: {preview}{}",
                if order.len() > 48 { " …" } else { "" }
            ));
        }
    }
    fn apply_repeat_draft(&mut self) {
        let editor = &self.notation_editor;
        let endings = editor
            .endings
            .iter()
            .map(|ending| {
                Ok(Ending {
                    start: ending.start.saturating_sub(1),
                    end: ending.end.saturating_sub(1),
                    passes: encore::repeats::parse_passes(&ending.passes)?,
                })
            })
            .collect::<Result<Vec<_>, String>>();
        let endings = match endings {
            Ok(endings) => endings,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let region = RepeatRegion {
            start: editor.repeat_start.saturating_sub(1),
            end: editor.repeat_end.saturating_sub(1),
            times: editor.repeat_times,
            endings,
        };
        let mut edited = self.score.clone();
        if let Some(index) = editor
            .editing_repeat
            .filter(|index| *index < edited.notation.repeats.len())
        {
            edited.notation.repeats[index] = region;
        } else {
            edited.notation.repeats.push(region);
        }
        match encore::repeats::validate(&edited) {
            Ok(()) => {
                self.score = edited;
                if self.notation_editor.editing_repeat.is_none() {
                    self.notation_editor.editing_repeat =
                        Some(self.score.notation.repeats.len() - 1);
                }
                self.status = "Repeat region updated".into();
            }
            Err(error) => self.status = error,
        }
    }
    fn apply_navigation_mark(&mut self) {
        let mark = NavigationMark {
            measure: self.notation_editor.navigation_measure.saturating_sub(1),
            kind: self.notation_editor.navigation_kind,
            play_repeats_after_jump: self.notation_editor.navigation_repeats,
        };
        let mut edited = self.score.clone();
        if let Some(old) = edited
            .notation
            .navigation
            .iter_mut()
            .find(|old| old.measure == mark.measure && old.kind == mark.kind)
        {
            *old = mark;
        } else {
            edited.notation.navigation.push(mark);
        }
        match encore::repeats::validate(&edited) {
            Ok(()) => {
                self.score = edited;
                self.status = "Navigation mark assigned".into();
            }
            Err(error) => self.status = error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> EncoreApp {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = (0..4)
            .map(|index| Note {
                start: index * PPQ,
                layer: 0,
                duration: Duration::Quarter,
                dotted: false,
                tuplet: None,
                pitches: vec![Pitch::from_midi(60 + index as u8)],
                velocity: 88,
                staccato: false,
                articulations: vec![],
                tied: false,
            })
            .collect();
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.selected = Some(Selection {
            part: 0,
            measure: 0,
            note: 0,
        });
        app.notation_editor.tool = Tool::Lyrics;
        app
    }

    fn input(frame: u32, events: Vec<egui::Event>, modifiers: egui::Modifiers) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 1000.0))),
            time: Some(f64::from(frame) / 60.0),
            events,
            modifiers,
            focused: true,
            ..Default::default()
        }
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn live_lyric_keys_advance_once_and_keep_syllabic_and_extensions() {
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let mut app = app();
        for (frame, events) in [
            vec![],
            vec![egui::Event::Text("Hal-le".into())],
            vec![
                key(egui::Key::Space, egui::Modifiers::NONE),
                egui::Event::Text(" ".into()),
            ],
            vec![egui::Event::Text("lu ".into())],
            vec![egui::Event::Text("_".into())],
        ]
        .into_iter()
        .enumerate()
        {
            let _ = ctx.run(input(frame as u32, events, egui::Modifiers::NONE), |ctx| {
                app.ui(ctx)
            });
        }
        let lyrics = &app.score.notation.lyrics;
        assert_eq!(lyrics.len(), 3);
        assert_eq!(
            (lyrics[0].text.as_str(), lyrics[0].syllabic),
            ("Hal", Syllabic::Begin)
        );
        assert_eq!(
            (lyrics[1].text.as_str(), lyrics[1].syllabic),
            ("le", Syllabic::End)
        );
        assert_eq!(lyrics[2].anchor.tick, 960);
        assert_eq!(lyrics[2].extend_to.unwrap().tick, 1440);
        assert!(
            app.selected.is_none(),
            "Consuming the last melody note ends lyric entry instead of merging the next word"
        );
        assert!(
            app.worker.is_none(),
            "Space in lyric entry must not start playback"
        );
        app.score.validate().unwrap();
    }

    #[test]
    fn live_lyric_typing_uses_global_score_undo_and_redo() {
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let mut app = app();
        let _ = ctx.run(input(0, vec![], egui::Modifiers::NONE), |ctx| app.ui(ctx));
        let _ = ctx.run(
            input(
                1,
                vec![egui::Event::Text("Sing".into())],
                egui::Modifiers::NONE,
            ),
            |ctx| app.ui(ctx),
        );
        assert_eq!(app.score.notation.lyrics[0].text, "Sing");
        let command = egui::Modifiers {
            ctrl: true,
            command: true,
            ..egui::Modifiers::NONE
        };
        let _ = ctx.run(input(2, vec![key(egui::Key::Z, command)], command), |ctx| {
            app.ui(ctx)
        });
        assert!(app.score.notation.lyrics.is_empty());
        assert!(app.undo.is_empty());
        let (redo_key, redo_modifiers) = if cfg!(target_os = "macos") {
            (
                egui::Key::Z,
                egui::Modifiers {
                    shift: true,
                    ..command
                },
            )
        } else {
            (egui::Key::Y, command)
        };
        let _ = ctx.run(
            input(3, vec![key(redo_key, redo_modifiers)], redo_modifiers),
            |ctx| app.ui(ctx),
        );
        assert_eq!(app.score.notation.lyrics[0].text, "Sing");
    }

    #[test]
    fn focused_dialog_text_input_does_not_leak_into_live_lyrics() {
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let mut app = app();
        let mut text = String::new();
        let _ = ctx.run(input(0, vec![], egui::Modifiers::NONE), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.text_edit_singleline(&mut text).request_focus();
            });
        });
        let _ = ctx.run(
            input(
                1,
                vec![egui::Event::Text("dialog text".into())],
                egui::Modifiers::NONE,
            ),
            |ctx| {
                app.keyboard(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.text_edit_singleline(&mut text);
                });
            },
        );
        assert_eq!(text, "dialog text");
        assert!(app.score.notation.lyrics.is_empty());
    }

    #[test]
    fn repeat_draft_changes_are_atomic_and_edit_existing_regions() {
        let mut app = app();
        app.notation_editor.repeat_start = 1;
        app.notation_editor.repeat_end = 4;
        app.notation_editor.repeat_times = 2;
        app.notation_editor.endings = vec![
            EndingDraft {
                start: 3,
                end: 3,
                passes: "1".into(),
            },
            EndingDraft {
                start: 4,
                end: 4,
                passes: "2".into(),
            },
        ];
        app.apply_repeat_draft();
        assert_eq!(app.score.playback_order().unwrap(), vec![0, 1, 2, 0, 1, 3]);
        let before = app.score.clone();
        app.notation_editor.editing_repeat = Some(0);
        app.notation_editor.endings[1].passes = "invalid".into();
        app.apply_repeat_draft();
        assert_eq!(app.score, before);
        app.notation_editor.endings.clear();
        app.notation_editor.repeat_times = 3;
        app.apply_repeat_draft();
        assert_eq!(app.score.notation.repeats.len(), 1);
        assert_eq!(app.score.playback_order().unwrap().len(), 12);
    }

    #[test]
    fn live_paste_at_the_last_note_never_merges_two_words_or_partially_applies() {
        let mut app = app();
        let last = Anchor {
            part: 0,
            measure: 0,
            tick: 1440,
            layer: 0,
        };
        app.select_anchor(last);
        let before = app.score.clone();
        app.type_lyric("one two");
        assert_eq!(app.score, before);
        assert_eq!(app.annotation_anchor(), Some(last));
        app.type_lyric("last ");
        assert!(app.selected.is_none());
        assert_eq!(app.score.notation.lyrics[0].text, "last");
        let completed = app.score.clone();
        app.type_lyric("extra");
        assert_eq!(app.score, completed);
    }

    #[test]
    fn deleting_selected_note_removes_its_underlay_and_phrase_anchors() {
        let mut app = app();
        let first = app.annotation_anchor().unwrap();
        let last = Anchor { tick: PPQ, ..first };
        lyrics::assign(&mut app.score, Lyric::new(first, "Sing")).unwrap();
        slurs::add(&mut app.score, Slur::new(first, last)).unwrap();
        app.notation_editor.tool = Tool::Select;
        app.delete_selected();
        assert!(app.score.notation.lyrics.is_empty());
        assert!(app.score.notation.slurs.is_empty());
        app.score.validate().unwrap();
    }
}
