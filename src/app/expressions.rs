//! Expressive notation editor;
//! Every apply operation validates before mutation.
use super::*;

use encore::{
    document::Anchor,
    expressions::{
        Expression, ExpressionKind, Hairpin, HairpinKind, Placement, TempoChange, TempoRampEnd,
        TimePoint,
    },
};

pub(super) struct Editor {
    pub open: bool,
    tab: u8,
    kind: ExpressionKind,
    text: String,
    tempo_text: String,
    velocity: u8,
    playback: bool,
    above: bool,
    system: bool,
    end_measure: usize,
    end_tick: u32,
    diminuendo: bool,
    start_velocity: u8,
    end_velocity: u8,
    bpm: u16,
    end_bpm: u16,
    ramp: bool,
    offset: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            open: false,
            tab: 0,
            kind: ExpressionKind::Dynamic,
            text: "mf".into(),
            tempo_text: String::new(),
            velocity: 80,
            playback: true,
            above: false,
            system: false,
            end_measure: 2,
            end_tick: 0,
            diminuendo: false,
            start_velocity: 64,
            end_velocity: 96,
            bpm: 120,
            end_bpm: 90,
            ramp: false,
            offset: 0.0,
        }
    }
}

impl EncoreApp {
    pub(super) fn open_expressions(&mut self) {
        self.expressions_editor.open = true;

        self.write_mode = false;
    }

    fn expression_anchor(&self) -> Anchor {
        let mut anchor = self
            .selected
            .and_then(|selection| Anchor::from_selection(&self.score, selection))
            .unwrap_or(Anchor {
                voice: 0,
                part: self.active_part,
                measure: self.cursor_measure,
                tick: self.cursor_tick.min(self.score.measure_ticks() - 1),
                layer: self.active_layer,
            });
        anchor.voice = 0;
        anchor
    }

    fn apply_expressive_score(&mut self, score: Score) -> bool {
        match score.validate() {
            Ok(()) => {
                self.score = score;

                self.status = "Expressive notation updated.".into();

                true
            }

            Err(error) => {
                self.status = error;

                false
            }
        }
    }

    pub(super) fn expressions_window(&mut self, ctx: &egui::Context) {
        if !self.expressions_editor.open {
            return;
        }

        let mut open = true;

        egui::Window::new("Expressions and tempo").open(&mut open).default_width(510.0).max_width((ctx.content_rect().width()-48.0).max(280.0)).max_height((ctx.content_rect().height()-48.0).max(240.0)).vscroll(true).show(ctx,|ui|{

        let anchor=self.expression_anchor();
ui.label(format!("{} · measure {} · beat {:.2} · layer {}",self.score.parts[anchor.part].name,anchor.measure+1,anchor.tick as f32/PPQ as f32+1.0,anchor.layer+1));

        ui.horizontal(|ui|{
ui.selectable_value(&mut self.expressions_editor.tab,0,"Expressions");
ui.selectable_value(&mut self.expressions_editor.tab,1,"Hairpins");
ui.selectable_value(&mut self.expressions_editor.tab,2,"Tempo");
}
);
ui.separator();

        match self.expressions_editor.tab{
0=>{

            egui::ComboBox::from_id_salt("expression-kind").selected_text(self.expressions_editor.kind.name()).show_ui(ui,|ui|{
for kind in ExpressionKind::ALL{
if ui.selectable_value(&mut self.expressions_editor.kind,kind,kind.name()).changed(){
self.expressions_editor.above=kind!=ExpressionKind::Dynamic;
self.expressions_editor.playback=kind==ExpressionKind::Dynamic;
}
}
}
);

            ui.text_edit_singleline(&mut self.expressions_editor.text);
ui.horizontal_wrapped(|ui|{
for(label,velocity)in[("ppp",28),("pp",40),("p",52),("mp",64),("mf",80),("f",96),("ff",112),("fff",124)]{
if ui.small_button(label).clicked(){
self.expressions_editor.kind=ExpressionKind::Dynamic;
self.expressions_editor.text=label.into();
self.expressions_editor.velocity=velocity;
self.expressions_editor.playback=true;
self.expressions_editor.above=false;
}
}
}
);

            ui.horizontal(|ui|{
ui.checkbox(&mut self.expressions_editor.above,"Above staff");
if self.expressions_editor.kind!=ExpressionKind::Dynamic{
ui.checkbox(&mut self.expressions_editor.system,"System marking");
}
ui.add(egui::DragValue::new(&mut self.expressions_editor.offset).range(-160.0..=160.0).prefix("Vertical offset "));
}
);

            ui.horizontal(|ui|{
ui.checkbox(&mut self.expressions_editor.playback,"Set velocity");
ui.add(egui::DragValue::new(&mut self.expressions_editor.velocity).range(0..=127));
}
);

            if ui.button("Apply at selected beat").clicked(){
let editor=&self.expressions_editor;
let mark=Expression{
anchor,kind:editor.kind,text:editor.text.clone(),velocity:editor.playback.then_some(editor.velocity),placement:if editor.above{
Placement::Above}
else{
Placement::Below}
,system:editor.kind!=ExpressionKind::Dynamic&&editor.system,offset:[0.0,editor.offset]}
;
let mut score=self.score.clone();
if mark.kind==ExpressionKind::Dynamic{
score.notation.expressions.retain(|prior|prior.anchor!=anchor||prior.kind!=ExpressionKind::Dynamic);
}
score.notation.expressions.push(mark);
self.apply_expressive_score(score);
}

            ui.separator();
for(index,mark)in self.score.notation.expressions.clone().iter().enumerate(){
ui.horizontal(|ui|{
ui.label(format!("M{}: {} · {}",mark.anchor.measure+1,mark.kind.name(),mark.text));
if ui.small_button("Remove").clicked(){
self.score.notation.expressions.remove(index);
}
}
);
}

        }
,1=>{

            ui.label("Begin at the selected beat. Hairpins can end on notes, rests or barlines.");
ui.checkbox(&mut self.expressions_editor.diminuendo,"Diminuendo");
self.expression_end_controls(ui);

            ui.horizontal(|ui|{
ui.add(egui::DragValue::new(&mut self.expressions_editor.start_velocity).range(0..=127).prefix("From "));
ui.add(egui::DragValue::new(&mut self.expressions_editor.end_velocity).range(0..=127).prefix("To "));
ui.checkbox(&mut self.expressions_editor.above,"Above staff");
}
);

            if ui.button("Add hairpin").clicked(){
let editor=&self.expressions_editor;
let line=Hairpin{
start:anchor,end:Anchor{
measure:editor.end_measure.saturating_sub(1),tick:editor.end_tick,..anchor}
,kind:if editor.diminuendo{
HairpinKind::Diminuendo}
else{
HairpinKind::Crescendo}
,start_velocity:Some(editor.start_velocity),end_velocity:Some(editor.end_velocity),placement:if editor.above{
Placement::Above}
else{
Placement::Below}
,offset:editor.offset}
;
let mut score=self.score.clone();
score.notation.spanners.push(line);
self.apply_expressive_score(score);
}

            ui.separator();
for(index,line)in self.score.notation.spanners.clone().iter().enumerate(){
ui.horizontal(|ui|{
ui.label(format!("{} · M{}–{}",line.kind.name(),line.start.measure+1,line.end.measure+1));
if ui.small_button("Remove").clicked(){
self.score.notation.spanners.remove(index);
}
}
);
}

        }
,_=>{

            ui.label("Tempo applies to the full ensemble. Ramps change BPM linearly in written beats.");
ui.text_edit_singleline(&mut self.expressions_editor.tempo_text);
ui.add(egui::DragValue::new(&mut self.expressions_editor.bpm).range(20..=400).prefix("BPM "));
ui.checkbox(&mut self.expressions_editor.ramp,"Gradual change");
if self.expressions_editor.ramp{
self.expression_end_controls(ui);
ui.add(egui::DragValue::new(&mut self.expressions_editor.end_bpm).range(20..=400).prefix("Final BPM "));
}

            if ui.button("Apply tempo at selected beat").clicked(){
let editor=&self.expressions_editor;
let mark=TempoChange{
at:anchor.into(),bpm:editor.bpm,text:editor.tempo_text.clone(),ramp:editor.ramp.then_some(TempoRampEnd{
end:TimePoint{
measure:editor.end_measure.saturating_sub(1),tick:editor.end_tick}
,bpm:editor.end_bpm}
)}
;
let mut score=self.score.clone();
score.notation.tempo.retain(|prior|prior.at!=mark.at);
score.notation.tempo.push(mark);
self.apply_expressive_score(score);
}

            ui.separator();
for(index,mark)in self.score.notation.tempo.clone().iter().enumerate(){
ui.horizontal(|ui|{
ui.label(format!("M{} · {} BPM{} · {}",mark.at.measure+1,mark.bpm,mark.ramp.as_ref().map_or(String::new(),|ramp|format!(" → {}",ramp.bpm)),mark.text));
if ui.small_button("Remove").clicked(){
self.score.notation.tempo.remove(index);
}
}
);
}

        }
}

    }
);

        self.expressions_editor.open = open;
    }

    fn expression_end_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.expressions_editor.end_measure)
                    .range(1..=self.score.measure_count())
                    .prefix("End measure "),
            );

            let mut beat = self.expressions_editor.end_tick as f64 / f64::from(PPQ) + 1.0;

            if ui
                .add(
                    egui::DragValue::new(&mut beat)
                        .range(1.0..=f64::from(self.score.measure_ticks()) / f64::from(PPQ) + 1.0)
                        .speed(0.125)
                        .prefix("End beat "),
                )
                .changed()
            {
                self.expressions_editor.end_tick = ((beat - 1.0) * f64::from(PPQ)).round() as u32;
            }
        });
    }
}
