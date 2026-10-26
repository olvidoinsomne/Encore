//! Page-view navigation and explicit measure location, independent of score edits.
use super::*;
#[derive(Default)]
pub(super) struct Editor {
    pub open: bool,
    measure: usize,
    beat: f32,
    pub scroll: Vec2,
    pub viewport: Vec2,
    pub content: Vec2,
    pub pages: Vec<Rect>,
    pub offset: Option<Vec2>,
    pub reveal: Option<(usize, usize, u32)>,
}
impl EncoreApp {
    pub(super) fn open_go_to(&mut self) {
        self.navigation.open = true;
        self.navigation.measure = self.cursor_measure + 1;
        self.navigation.beat = 1.0 + self.cursor_tick as f32 / self.score.beat_ticks() as f32;
    }
    pub(super) fn fit_view(&mut self, whole_page: bool) {
        let Some(page) = self.cached_pages.first() else {
            return;
        };
        let available = self.navigation.viewport;
        if available.x > 60.0 && available.y > 60.0 {
            let mut zoom = (available.x - 48.0) / page.width;
            if whole_page {
                zoom = zoom.min((available.y - 50.0) / page.height);
            }
            self.zoom = zoom.clamp(0.1, 2.0);
            self.navigation.offset = Some(Vec2::ZERO);
        }
    }
    pub(super) fn view_keyboard(
        &mut self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        mac: bool,
    ) -> bool {
        let primary = if mac {
            modifiers.mac_cmd
        } else {
            modifiers.ctrl
        };
        if (mac && modifiers.ctrl) || modifiers.shift {
            return false;
        }
        let nav = &mut self.navigation;
        let max = (nav.content - nav.viewport).max(Vec2::ZERO);
        let mut target = nav.scroll;
        match key {
            egui::Key::PageUp | egui::Key::PageDown => {
                let direction = if key == egui::Key::PageUp { -1.0 } else { 1.0 };
                if primary && !modifiers.alt {
                    let current = nav
                        .pages
                        .iter()
                        .rposition(|rect| rect.top() <= nav.scroll.y + 30.0)
                        .unwrap_or(0);
                    let index = if direction < 0.0 {
                        current.saturating_sub(1)
                    } else {
                        (current + 1).min(nav.pages.len().saturating_sub(1))
                    };
                    if let Some(page) = nav.pages.get(index) {
                        target.y = (page.top() - 25.0).max(0.0);
                    }
                } else if !primary {
                    target.y += direction
                        * if modifiers.alt {
                            24.0
                        } else {
                            nav.viewport.y * 0.9
                        };
                } else {
                    return false;
                }
            }
            egui::Key::Home | egui::Key::End => {
                let end = key == egui::Key::End;
                if primary && !modifiers.alt {
                    target = if end { max } else { Vec2::ZERO };
                } else if !primary {
                    // Finale's horizontal incremental Alt/Option direction differs
                    // from its full-screen Home/End movement.
                    target.x += if modifiers.alt {
                        if end { -24.0 } else { 24.0 }
                    } else if end {
                        nav.viewport.x * 0.9
                    } else {
                        -nav.viewport.x * 0.9
                    };
                } else {
                    return false;
                }
            }
            _ => return false,
        }
        nav.offset = Some(target.clamp(Vec2::ZERO, max));
        true
    }
    fn go_to_location(&mut self, measure: usize, beat: f32) -> Result<(), String> {
        if measure == 0
            || measure > self.score.measure_count()
            || !beat.is_finite()
            || beat < 1.0
            || beat >= f32::from(self.score.time_numerator) + 1.0
        {
            return Err("Choose an existing measure and a beat inside it.".into());
        }
        self.cursor_measure = measure - 1;
        self.cursor_tick = ((beat - 1.0) * self.score.beat_ticks() as f32).round() as u32;
        self.selected = None;
        self.clear_region_selection();
        if self.chord_editor.active {
            self.chord_click(self.active_part, self.cursor_measure, self.cursor_tick);
        }
        self.navigation.reveal = Some((self.active_part, self.cursor_measure, self.cursor_tick));
        self.status = format!("Measure {measure} · beat {beat:.2}");
        Ok(())
    }
    pub(super) fn go_to_dialog(&mut self, ctx: &egui::Context) {
        if !self.navigation.open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Go to measure")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Measure");
                    ui.add(
                        egui::DragValue::new(&mut self.navigation.measure)
                            .range(1..=self.score.measure_count()),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Beat");
                    ui.add(
                        egui::DragValue::new(&mut self.navigation.beat)
                            .range(1.0..=f32::from(self.score.time_numerator))
                            .speed(0.25),
                    );
                });
                ui.small(format!(
                    "Beats use the score's 1/{} note.",
                    self.score.time_denominator
                ));
                apply = ui.button("Go").clicked()
                    || ctx.input(|input| input.key_pressed(egui::Key::Enter));
            });
        if apply {
            match self.go_to_location(self.navigation.measure, self.navigation.beat) {
                Ok(()) => open = false,
                Err(error) => self.status = error,
            }
        }
        self.navigation.open = open;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn go_to_uses_meter_beats_and_never_changes_or_dirties_music() {
        let mut score = Score::blank();
        score.time_numerator = 6;
        score.time_denominator = 8;
        let mut app = EncoreApp::from_score(score.clone(), None, "Ready".into());
        app.go_to_location(2, 4.5).unwrap();
        assert_eq!((app.cursor_measure, app.cursor_tick), (1, 840));
        assert_eq!(app.score, score);
        assert!(!app.dirty());
        assert!(app.go_to_location(0, 1.0).is_err());
        assert!(app.go_to_location(1, f32::NAN).is_err());
        assert!(app.go_to_location(1, 7.0).is_err());
    }
    #[test]
    fn view_keys_clamp_scroll_and_preserve_selection_shortcuts() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.navigation.viewport = Vec2::new(500.0, 400.0);
        app.navigation.content = Vec2::new(1000.0, 2500.0);
        app.navigation.scroll = Vec2::new(0.0, 100.0);
        assert!(app.view_keyboard(egui::Key::PageDown, egui::Modifiers::NONE, false));
        assert_eq!(app.navigation.offset, Some(Vec2::new(0.0, 460.0)));
        assert!(!app.view_keyboard(
            egui::Key::PageDown,
            egui::Modifiers {
                shift: true,
                ..egui::Modifiers::NONE
            },
            false
        ));
        assert!(app.view_keyboard(
            egui::Key::End,
            egui::Modifiers {
                ctrl: true,
                command: true,
                ..egui::Modifiers::NONE
            },
            false
        ));
        assert_eq!(app.navigation.offset, Some(Vec2::new(500.0, 2100.0)));
    }
}
