//! Playback view controls use performed bars, so repeats have unambiguous bounds.
use super::*;
#[derive(Default)]
pub(super) struct Editor {
    pub open: bool,
    pub repeat: bool,
    first: usize,
    last: usize,
    from_cursor: bool,
    pub pending_start: Option<f32>,
    pub bounds: Option<(f32, f32)>,
    pub clock: Option<std::sync::Arc<encore::expressions::TempoMap>>,
}
impl EncoreApp {
    pub(super) fn cached_score_seconds(&self) -> f32 {
        if self.cached_score.as_ref() == Some(&self.score) {
            self.playback_editor
                .clock
                .as_ref()
                .map_or(f32::INFINITY, |map| map.duration_seconds() as f32)
        } else {
            synth::duration_seconds(&self.score)
        }
    }

    fn playback_bounds(&self) -> Result<(f32, f32), String> {
        let order = self.score.playback_order()?;
        let count = order.len();
        let first = self.playback_editor.first.max(1);
        let last = if self.playback_editor.last == 0 {
            count
        } else {
            self.playback_editor.last
        };
        if first > last || last > count {
            return Err("Choose an ordered playback-bar range within the performed score.".into());
        }
        let map = encore::expressions::TempoMap::new(&self.score)?;
        let ticks = f64::from(self.score.measure_ticks());
        Ok((
            map.seconds_at_tick((first - 1) as f64 * ticks) as f32,
            map.seconds_at_tick(last as f64 * ticks) as f32,
        ))
    }
    pub(super) fn prepare_playback_start(&mut self) -> Result<(), String> {
        let start = if self.playback_editor.repeat {
            let bounds = self.playback_bounds()?;
            self.playback_editor.bounds = Some(bounds);
            bounds.0
        } else if self.playback_editor.from_cursor {
            let order = self.score.playback_order()?;
            let index = order
                .iter()
                .position(|measure| *measure == self.cursor_measure)
                .ok_or("The entry cursor is not visited by this score's playback route.")?;
            let map = encore::expressions::TempoMap::new(&self.score)?;
            map.seconds_at_tick(
                index as f64 * f64::from(self.score.measure_ticks()) + f64::from(self.cursor_tick),
            ) as f32
        } else {
            0.0
        };
        self.playback_editor.pending_start = (start > 0.0).then_some(start);
        Ok(())
    }
    pub(super) fn poll_playback_range(&mut self) {
        if !self.playback_editor.repeat {
            return;
        }
        if !self
            .playback
            .as_ref()
            .is_some_and(|player| player.is_playing())
        {
            return;
        }
        let bounds = if let Some(bounds) = self.playback_editor.bounds {
            bounds
        } else {
            let Ok(bounds) = self.playback_bounds() else {
                return;
            };
            self.playback_editor.bounds = Some(bounds);
            bounds
        };
        let (start, end) = bounds;
        if let Some(player) = self.playback.as_mut()
            && player.is_playing()
            && player.position_seconds() >= end
            && let Err(error) = player.seek_seconds(start)
        {
            self.playback_editor.repeat = false;
            self.status = format!("Loop stopped: {error}");
        }
    }
    pub(super) fn playback_seek_control(&mut self, ui: &mut egui::Ui) {
        let duration = self.cached_score_seconds();
        let Some(player) = self.playback.as_mut() else {
            return;
        };
        if !duration.is_finite() || duration <= 0.0 {
            return;
        }
        let enabled = player.is_playing() || player.is_paused();
        let mut seconds = player.position_seconds().min(duration);
        let response = ui.add_enabled(
            enabled,
            egui::Slider::new(&mut seconds, 0.0..=duration)
                .show_value(false)
                .text("Position"),
        );
        if response.changed()
            && let Err(error) = player.seek_seconds(seconds)
        {
            self.status = format!("Cannot seek: {error}");
        }
    }
    fn use_selected_playback_range(&mut self) -> Result<(), String> {
        let region = self
            .region_selection
            .as_ref()
            .ok_or("Select whole measures first.")?;
        let order = self.score.playback_order()?;
        let first = order
            .iter()
            .position(|measure| region.contains_measure(*measure))
            .ok_or("The selected region is not visited in playback.")?;
        let last = order
            .iter()
            .rposition(|measure| region.contains_measure(*measure))
            .unwrap();
        self.playback_editor.first = first + 1;
        self.playback_editor.last = last + 1;
        Ok(())
    }
    pub(super) fn playback_dialog(&mut self, ctx: &egui::Context) {
        if !self.playback_editor.open {
            return;
        }
        let previous = (
            self.playback_editor.first,
            self.playback_editor.last,
            self.playback_editor.repeat,
        );
        let mut open = true;
        let order = self.score.playback_order().unwrap_or_default();
        let count = order.len().max(1);
        egui::Window::new("Playback controls").open(&mut open).default_width(430.0).resizable(false).show(ctx,|ui|{
            ui.checkbox(&mut self.playback_editor.from_cursor,"Start a fresh playback at the entry cursor");
            ui.checkbox(&mut self.playback_editor.repeat,"Loop playback range");
            ui.small("Playback bars include repeat and navigation visits. Range bounds use the first through last selected visit.");
            if self.playback_editor.first==0{self.playback_editor.first=1;}
            if self.playback_editor.last==0{self.playback_editor.last=count;}
            ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut self.playback_editor.first).range(1..=count).prefix("First playback bar "));ui.add(egui::DragValue::new(&mut self.playback_editor.last).range(1..=count).prefix("Last "));});
            if let Ok((start,end))=self.playback_bounds(){ui.label(format!("{start:.2}–{end:.2} seconds · written measures {}–{}",order.get(self.playback_editor.first.saturating_sub(1)).map_or(0,|bar|bar+1),order.get(self.playback_editor.last.saturating_sub(1)).map_or(0,|bar|bar+1)));}
            ui.horizontal(|ui|{if ui.button("Use selected measures").clicked()&&let Err(error)=self.use_selected_playback_range(){self.status=error;}
                if ui.button("Whole performed score").clicked(){self.playback_editor.first=1;self.playback_editor.last=count;}
            });
            self.playback_seek_control(ui);
            ui.small("Seeking uses the rendered audio, including notes already sounding. Score edits stop the old render. These playback-view options do not modify the score or exported files.");
        });
        if previous
            != (
                self.playback_editor.first,
                self.playback_editor.last,
                self.playback_editor.repeat,
            )
        {
            self.playback_editor.bounds = None;
        }
        self.playback_editor.open = open;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loop_bounds_use_performed_repeats_and_integrated_tempo() {
        let mut score = Score::blank();
        for staff in &mut score.parts {
            staff.measures.truncate(2);
        }
        score.notation.repeats.push(encore::repeats::RepeatRegion {
            start: 0,
            end: 1,
            times: 2,
            endings: vec![],
        });
        score.notation.tempo.push(encore::expressions::TempoChange {
            at: encore::expressions::TimePoint {
                measure: 0,
                tick: 0,
            },
            bpm: 60,
            ramp: None,
            text: String::new(),
        });
        let mut app = EncoreApp::from_score(score.clone(), None, "Ready".into());
        app.playback_editor.first = 3;
        app.playback_editor.last = 4;
        let bounds = app.playback_bounds().unwrap();
        assert_eq!(bounds, (8.0, 16.0));
        app.playback_editor.repeat = true;
        app.prepare_playback_start().unwrap();
        assert_eq!(app.playback_editor.pending_start, Some(8.0));
        assert_eq!(app.score, score);
        app.playback_editor.first = 5;
        assert!(app.playback_bounds().is_err());
    }
    #[test]
    fn selected_loop_range_includes_all_visits_without_editing_repeat_structure() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.region_selection = Some(encore::editing::Region::rectangle(0, 0, 1, 2));
        app.use_selected_playback_range().unwrap();
        assert_eq!(
            (app.playback_editor.first, app.playback_editor.last),
            (2, 3)
        );
        assert!(!app.dirty());
    }
}
