//! Bounded performance line editor.
use super::*;
use encore::{
    document::Anchor,
    expressions::Placement,
    lines::{LineKind, PedalKind, PerformanceLine},
};
pub(super) struct Editor {
    pub open: bool,
    kind: LineKind,
    end_measure: usize,
    end_tick: u32,
    above: bool,
    offset: f32,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            kind: LineKind::Ottava { semitones: 12 },
            end_measure: 2,
            end_tick: 0,
            above: true,
            offset: 0.0,
        }
    }
}
impl EncoreApp {
    pub(super) fn open_lines(&mut self) {
        self.lines_editor.open = true;
        self.write_mode = false;
    }
    pub(super) fn lines_window(&mut self, ctx: &egui::Context) {
        if !self.lines_editor.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Performance lines").open(&mut open).default_width(510.0).max_width((ctx.content_rect().width()-48.0).max(280.0)).max_height((ctx.content_rect().height()-48.0).max(240.0)).vscroll(true).show(ctx,|ui|{
        let anchor=self.selected.and_then(|s|Anchor::from_selection(&self.score,s)).unwrap_or(Anchor{ voice: self.active_voice,part:self.active_part,measure:self.cursor_measure,tick:self.cursor_tick.min(self.score.measure_ticks()-1),layer:self.active_layer});
        ui.label(format!("{} · measure {} · beat {:.2} · layer {}",self.score.parts[anchor.part].name,anchor.measure+1,anchor.tick as f32/PPQ as f32+1.0,anchor.layer+1));
        egui::ComboBox::from_id_salt("performance-line-kind").selected_text(self.lines_editor.kind.name()).show_ui(ui,|ui|{for kind in LineKind::ALL{if ui.selectable_value(&mut self.lines_editor.kind,kind,kind.name()).changed(){self.lines_editor.above = !matches!(kind,LineKind::Pedal{..});}}});
        match &mut self.lines_editor.kind{
            LineKind::Ottava{semitones}=>{egui::ComboBox::from_id_salt("octave-shift").selected_text(format!("{semitones:+} semitones")).show_ui(ui,|ui|{for(n,label)in[(12,"8va"),(-12,"8vb"),(24,"15ma"),(-24,"15mb"),(36,"22ma"),(-36,"22mb")]{ui.selectable_value(semitones,n,label);}});ui.label("Stored notes keep their sounding pitches. An 8va line prints them one octave lower; transpose notes separately to change the sound.");},
            LineKind::Pedal{kind}=>{egui::ComboBox::from_id_salt("pedal-kind").selected_text(kind.name()).show_ui(ui,|ui|{for p in PedalKind::ALL{ui.selectable_value(kind,p,p.name());}});ui.label("Pedal affects both voices of every layer on this staff. Sostenuto holds only notes sounding at its start.");},
            LineKind::Trill{interval,rate}=>{ui.horizontal_wrapped(|ui|{ui.add(egui::DragValue::new(interval).range(1..=12).prefix("Upper interval ").suffix(" semitones"));ui.add(egui::DragValue::new(rate).range(30..=480).prefix("Alternation ").suffix(" ticks"));});},
            LineKind::Glissando=>{ui.label("Choose consecutive single-pitch entries in one layer and voice. Playback steps chromatically between them.");}
        }
        ui.horizontal_wrapped(|ui|{ui.add(egui::DragValue::new(&mut self.lines_editor.end_measure).range(1..=self.score.measure_count()).prefix("End measure "));let mut beat=self.lines_editor.end_tick as f32/PPQ as f32+1.0;if ui.add(egui::DragValue::new(&mut beat).range(1.0..=self.score.measure_ticks()as f32/PPQ as f32+1.0).speed(0.25).prefix("End beat ")).changed(){self.lines_editor.end_tick=((beat-1.0)*PPQ as f32).round()as u32;}});
        ui.horizontal_wrapped(|ui|{ui.checkbox(&mut self.lines_editor.above,"Above staff");ui.add(egui::DragValue::new(&mut self.lines_editor.offset).range(-160.0..=160.0).prefix("Vertical offset "));});
        if ui.button("Add line at selected beat").clicked(){let e=&self.lines_editor;let mut start=anchor;if matches!(e.kind,LineKind::Ottava{..}|LineKind::Pedal{..}){start.voice=0;}let mut line=PerformanceLine::new(start,Anchor{measure:e.end_measure.saturating_sub(1),tick:e.end_tick,..start},e.kind);line.placement=if e.above{Placement::Above}else{Placement::Below};line.offset=e.offset;let mut candidate=self.score.clone();candidate.notation.lines.push(line);match candidate.validate(){Ok(())=>{self.score=candidate;self.status="Performance line added.".into();},Err(error)=>self.status=error}}
        ui.separator();for(index,line)in self.score.notation.lines.clone().iter().enumerate().filter(|(_,line)|line.start.part==anchor.part){ui.horizontal_wrapped(|ui|{ui.label(format!("{} · M{} beat {:.2} – M{} beat {:.2}",line.kind.name(),line.start.measure+1,line.start.tick as f32/PPQ as f32+1.0,line.end.measure+1,line.end.tick as f32/PPQ as f32+1.0));if ui.small_button("Remove").clicked(){self.score.notation.lines.remove(index);}});}
    });
        self.lines_editor.open = open;
    }
}
