//! Voice-local ornament editor.
use super::*;
use encore::{
    document::Anchor,
    ornaments::{Ornament, OrnamentKind},
    slurs::SlurDirection,
};
pub(super) struct Editor {
    pub open: bool,
    mark: Ornament,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            mark: Ornament::new(
                Anchor {
                    part: 0,
                    measure: 0,
                    tick: 0,
                    layer: 0,
                    voice: 0,
                },
                OrnamentKind::Turn,
            ),
        }
    }
}
impl EncoreApp {
    pub(super) fn open_ornaments(&mut self) {
        if self.active_percussion().is_some() {
            self.status="Pitch ornaments and chord rolls are unavailable on unpitched percussion. Use grace notes for flams.".into();
            return;
        }
        if let Some(anchor) = self
            .selected
            .and_then(|s| Anchor::from_selection(&self.score, s))
        {
            self.ornaments_editor.mark = self
                .score
                .notation
                .ornaments
                .iter()
                .find(|m| m.anchor == anchor)
                .cloned()
                .unwrap_or_else(|| Ornament::new(anchor, OrnamentKind::Turn));
        }
        self.ornaments_editor.open = true;
        self.write_mode = false;
    }
    fn edit_ornament(&mut self, mark: &Ornament) {
        let a = mark.anchor;
        self.selected = a.selection(&self.score);
        self.active_part = a.part;
        self.active_layer = a.layer;
        self.active_voice = a.voice;
        self.cursor_measure = a.measure;
        self.cursor_tick = a.tick;
        self.active_voice_launch = encore::document::note_at(&self.score, &a)
            .and_then(|n| n.voice_launch)
            .map(|tick| Anchor {
                tick,
                voice: 0,
                ..a
            });
        self.ornaments_editor.mark = mark.clone();
    }
    pub(super) fn ornaments_window(&mut self, ctx: &egui::Context) {
        if !self.ornaments_editor.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Ornaments and chord rolls").open(&mut open).default_width(480.).max_width((ctx.content_rect().width()-48.).max(280.)).max_height((ctx.content_rect().height()-48.).max(240.)).vscroll(true).show(ctx,|ui|{
            let anchor=self.selected.and_then(|s|Anchor::from_selection(&self.score,s));
            if let Some(a)=anchor {ui.label(format!("Measure {} · beat {:.2} · layer {} · voice {}",a.measure+1,a.tick as f32/PPQ as f32+1.,a.layer+1,a.voice+1));} else {ui.label("Select a pitched entry to assign an ornament.");}
            let mark=&mut self.ornaments_editor.mark;
            egui::ComboBox::from_id_salt("ornament-kind").selected_text(mark.kind.name()).show_ui(ui,|ui|{for kind in OrnamentKind::ALL{ui.selectable_value(&mut mark.kind,kind,kind.name());}});
            ui.checkbox(&mut mark.playback,"Play ornament");
            if mark.kind!=OrnamentKind::NonArpeggiate{
                ui.horizontal_wrapped(|ui|{ui.add(egui::DragValue::new(&mut mark.window_ticks).range(1..=960).prefix("Window ").suffix(" ticks"));ui.add(egui::DragValue::new(&mut mark.delay_percent).range(0.0..=75.0).prefix("Delay ").suffix("%"));});
                ui.label("480 ticks = quarter note. The ornament borrows from the principal; written rhythm stays unchanged.");
            }
            if !mark.kind.chord(){
                ui.horizontal_wrapped(|ui|{let mut enabled=mark.upper.is_some();if ui.checkbox(&mut enabled,"Upper interval").changed(){mark.upper=enabled.then_some(2);}
            if let Some(v)=&mut mark.upper{ui.add(egui::DragValue::new(v).range(1..=4).suffix(" semitones"));}});
                ui.horizontal_wrapped(|ui|{let mut enabled=mark.lower.is_some();if ui.checkbox(&mut enabled,"Lower interval").changed(){mark.lower=enabled.then_some(-2);}
            if let Some(v)=&mut mark.lower{ui.add(egui::DragValue::new(v).range(-4..=-1).suffix(" semitones"));}});
                ui.label("Without overrides, neighbors follow the key signature.");
            }
            egui::ComboBox::from_id_salt("ornament-placement").selected_text(format!("{:?}",mark.placement)).show_ui(ui,|ui|{for p in [SlurDirection::Auto,SlurDirection::Above,SlurDirection::Below]{ui.selectable_value(&mut mark.placement,p,format!("{p:?}"));}});
            ui.horizontal_wrapped(|ui|{ui.add(egui::DragValue::new(&mut mark.offset[0]).range(-160.0..=160.0).prefix("X "));ui.add(egui::DragValue::new(&mut mark.offset[1]).range(-160.0..=160.0).prefix("Y "));});
            if ui.add_enabled(anchor.is_some(),egui::Button::new("Assign to selected entry")).clicked(){let mut mark=self.ornaments_editor.mark.clone();mark.anchor=anchor.unwrap();let mut candidate=self.score.clone();match encore::ornaments::assign(&mut candidate,mark).and_then(|()|candidate.validate()){Ok(())=>{self.score=candidate;self.status="Ornament assigned.".into();},Err(error)=>self.status=error}}
            ui.separator();
            for(index,mark)in self.score.notation.ornaments.clone().iter().enumerate(){ui.horizontal_wrapped(|ui|{ui.label(format!("{} · M{} beat {:.2} · L{} V{}",mark.kind.name(),mark.anchor.measure+1,mark.anchor.tick as f32/PPQ as f32+1.,mark.anchor.layer+1,mark.anchor.voice+1));if ui.small_button("Edit").clicked(){self.edit_ornament(mark);}
            if ui.small_button("Remove").clicked(){self.score.notation.ornaments.remove(index);}});}
        });
        self.ornaments_editor.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_existing_secondary_ornament_selects_its_principal() {
        let mut s = Score::blank();
        let n = Note {
            layer: 2,
            voice: 1,
            voice_launch: None,
            start: 480,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        };
        s.parts[0].measures[0].notes.push(n);
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 480,
            layer: 2,
            voice: 1,
        };
        let mark = Ornament::new(a, OrnamentKind::Turn);
        s.notation.ornaments.push(mark.clone());
        let mut app = EncoreApp::from_score(s, None, "Ready".into());
        app.edit_ornament(&mark);
        assert_eq!(
            Anchor::from_selection(&app.score, app.selected.unwrap()),
            Some(a)
        );
        assert_eq!(
            (
                app.active_part,
                app.active_layer,
                app.active_voice,
                app.cursor_measure,
                app.cursor_tick
            ),
            (0, 2, 1, 0, 480)
        );
        assert_eq!(app.ornaments_editor.mark, mark);
    }
}
