use super::*;
use encore::layout::{DocumentLayout, LockedSystem, MeasureWidth, PageSize, SystemBreak};

impl EncoreApp {
    pub(super) fn layout_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_layout {
            return;
        }
        let mut definitions = encore::parts::definitions(&self.score);
        let index = self
            .selected_linked_part
            .min(definitions.len().saturating_sub(1));
        let part_target = self.part_view && !definitions.is_empty();
        let mut layout = if part_target {
            definitions[index].layout.clone()
        } else {
            self.score.notation.layout.clone()
        };
        let original = layout.clone();
        let mut open = true;
        let measure = self
            .cursor_measure
            .min(self.score.measure_count().saturating_sub(1));
        let scope = if part_target {
            format!("Linked part: {}", definitions[index].name)
        } else {
            "Full score".into()
        };
        let mut clear = false;
        egui::Window::new("Page and system layout").open(&mut open).default_width(650.0).default_height((ctx.content_rect().height()-80.0).clamp(300.0,650.0)).max_height((ctx.content_rect().height()-40.0).max(280.0)).show(ctx,|ui|{
            ui.strong(&scope);ui.small("Formatting is saved independently for the full score and each linked part. Notes and playback remain shared.");ui.separator();
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-190.0).clamp(220.0,650.0)).show(ui,|ui|{
                ui.horizontal(|ui|{egui::ComboBox::from_label("Paper").selected_text(layout.page_size.name()).show_ui(ui,|ui|{for &size in PageSize::ALL{ui.selectable_value(&mut layout.page_size,size,size.name());}});ui.checkbox(&mut layout.landscape,"Landscape");});
                egui::Grid::new("layout-format").num_columns(4).show(ui,|ui|{
                    ui.label("Left margin");ui.add(egui::DragValue::new(&mut layout.margins.left).range(20.0..=250.0));ui.label("Right margin");ui.add(egui::DragValue::new(&mut layout.margins.right).range(20.0..=250.0));ui.end_row();
                    ui.label("Top margin");ui.add(egui::DragValue::new(&mut layout.margins.top).range(20.0..=250.0));ui.label("Bottom margin");ui.add(egui::DragValue::new(&mut layout.margins.bottom).range(20.0..=250.0));ui.end_row();
                    ui.label("Staff size");let mut percent=layout.staff_scale*100.0;if ui.add(egui::DragValue::new(&mut percent).range(50.0..=150.0).suffix("%")).changed(){layout.staff_scale=percent/100.0;}ui.label("Extra staff gap");ui.add(egui::DragValue::new(&mut layout.staff_gap).range(0.0..=80.0));ui.end_row();
                    ui.label("Measures/system");ui.add(egui::DragValue::new(&mut layout.measures_per_system).range(1..=8));ui.label("First page");ui.add(egui::DragValue::new(&mut layout.first_page_number).range(1..=60_000));ui.end_row();
                });ui.small("Margins and spacing use 100 units per inch. Automatic systems may contain fewer measures when music is dense.");
                ui.separator();ui.strong(format!("System controls · measure {}",measure+1));
                if measure>0{
                    let existing=layout.breaks.iter().find(|b|b.before==measure).copied();let mut system=existing.is_some();let mut page=existing.is_some_and(|b|b.page);
                    ui.horizontal(|ui|{if ui.checkbox(&mut system,"Start new system").changed(){layout.breaks.retain(|b|b.before!=measure);if system{layout.breaks.push(SystemBreak{before:measure,page});}}
                    if ui.add_enabled(system,egui::Checkbox::new(&mut page,"Start new page")).changed() && let Some(b)=layout.breaks.iter_mut().find(|b|b.before==measure){b.page=page;}});
                }else{ui.small("The first measure already begins a new system.");}
                let mut weight=layout.weight(measure);
                ui.horizontal(|ui|{ui.label("Measure width weight");if ui.add(egui::DragValue::new(&mut weight).range(0.25..=4.0).speed(0.05)).changed(){layout.measure_widths.retain(|w|w.measure!=measure);if(weight-1.0).abs()>0.001{layout.measure_widths.push(MeasureWidth{measure,weight});}}
                    if ui.button("Reset width").clicked(){layout.measure_widths.retain(|w|w.measure!=measure);}});
                ui.horizontal(|ui|{
                    if ui.button("Lock visible system").clicked(){
                        let bounds=self.cached_pages.iter().find_map(|p|{let region=p.staves.iter().find(|r|r.part==self.active_part&&r.measure==measure)?;let measures=p.staves.iter().filter(|r|r.part==region.part&&(r.y-region.y).abs()<0.01).map(|r|r.measure).collect::<Vec<_>>();Some((*measures.iter().min()?,*measures.iter().max()?))});
                        if let Some((start,end))=bounds{layout.locked_systems.retain(|l|l.end<start||l.start>end);layout.breaks.retain(|b|b.before<=start||b.before>end);layout.locked_systems.push(LockedSystem{start,end});}
                    }
                    if ui.button("Unlock current system").clicked(){layout.locked_systems.retain(|l|measure<l.start||measure>l.end);}
                });
                ui.small("Locks preserve system membership. Width weights distribute available space within each system; very dense locked systems may still need manual simplification.");
                let mut remove_break=None;for b in &layout.breaks{ui.horizontal(|ui|{ui.label(format!("{} before measure {}",if b.page{"Page break"}else{"System break"},b.before+1));if ui.small_button("Remove").clicked(){remove_break=Some(b.before);}});}
                    if let Some(before)=remove_break{layout.breaks.retain(|b|b.before!=before);}
                let mut remove_lock=None;for l in &layout.locked_systems{ui.horizontal(|ui|{ui.label(format!("Locked measures {}–{}",l.start+1,l.end+1));if ui.small_button("Unlock").clicked(){remove_lock=Some(l.start);}});}
                    if let Some(start)=remove_lock{layout.locked_systems.retain(|l|l.start!=start);}
                ui.separator();ui.strong("Headers and footers");ui.small("Tokens: {title}, {composer}, {page}, {pages}. The first page shows the title/composer; its header begins on subsequent pages.");
                egui::Grid::new("page-text").num_columns(2).show(ui,|ui|{for(label,text)in[("Header left",&mut layout.header.left),("Header center",&mut layout.header.center),("Header right",&mut layout.header.right),("Footer left",&mut layout.footer.left),("Footer center",&mut layout.footer.center),("Footer right",&mut layout.footer.right),("Copyright",&mut layout.copyright)]{ui.label(label);ui.text_edit_singleline(text);ui.end_row();}});
                ui.separator();if ui.button("Reset this format").clicked(){clear=true;}
            });
        });
        if clear {
            layout = DocumentLayout::default();
        }
        if layout != original {
            match layout.validate_for_score(self.score.measure_count()) {
                Ok(()) => {
                    if part_target {
                        definitions[index].layout = layout;
                        self.score.notation.parts = definitions;
                    } else {
                        self.score.notation.layout = layout;
                    }
                    self.cached_score = None;
                    self.status = format!("Updated page layout for {scope}.");
                }
                Err(error) => self.status = error,
            }
        }
        self.show_layout = open;
    }
}

impl EncoreApp {
    fn edit_current_layout(
        &mut self,
        edit: impl FnOnce(&mut DocumentLayout),
    ) -> Result<(), String> {
        let mut definitions = encore::parts::definitions(&self.score);
        let index = self
            .selected_linked_part
            .min(definitions.len().saturating_sub(1));
        let part = self.part_view && !definitions.is_empty();
        let mut layout = if part {
            definitions[index].layout.clone()
        } else {
            self.score.notation.layout.clone()
        };
        edit(&mut layout);
        layout.validate_for_score(self.score.measure_count())?;
        if part {
            definitions[index].layout = layout;
            self.score.notation.parts = definitions;
        } else {
            self.score.notation.layout = layout;
        }
        self.cached_score = None;
        Ok(())
    }
    pub(super) fn lock_selected_systems(&mut self) {
        let (start, end) = self
            .region_selection
            .as_ref()
            .map_or((self.cursor_measure, self.cursor_measure), |r| {
                (r.start, r.end)
            });
        let mut systems = std::collections::BTreeSet::new();
        for page in &self.cached_pages {
            let mut rows = std::collections::BTreeMap::<u32, Vec<usize>>::new();
            for staff in page.staves.iter().filter(|r| r.part == self.active_part) {
                rows.entry(staff.y.to_bits())
                    .or_default()
                    .push(staff.measure);
            }
            for measures in rows.into_values() {
                let first = *measures.iter().min().unwrap();
                let last = *measures.iter().max().unwrap();
                if first <= end && last >= start {
                    systems.insert((first, last));
                }
            }
        }
        if systems.is_empty() {
            for first in (start..=end).step_by(4) {
                systems.insert((first, (first + 3).min(end)));
            }
        }
        self.status = match self.edit_current_layout(|layout| {
            for (start, end) in systems {
                layout
                    .locked_systems
                    .retain(|l| l.end < start || l.start > end);
                layout
                    .breaks
                    .retain(|b| b.before <= start || b.before > end);
                layout.locked_systems.push(LockedSystem { start, end });
            }
        }) {
            Ok(()) => "Locked the selected systems.".into(),
            Err(error) => error,
        };
    }
    pub(super) fn unlock_selected_systems(&mut self) {
        let (start, end) = self
            .region_selection
            .as_ref()
            .map_or((self.cursor_measure, self.cursor_measure), |r| {
                (r.start, r.end)
            });
        self.status = match self.edit_current_layout(|layout| {
            layout
                .locked_systems
                .retain(|l| l.end < start || l.start > end)
        }) {
            Ok(()) => "Unlocked the selected systems.".into(),
            Err(error) => error,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(frame: u32) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 680.0))),
            time: Some(f64::from(frame) / 60.0),
            ..Default::default()
        }
    }
    #[test]
    fn minimum_window_layout_dialog_is_read_only_and_primary_controls_fit() {
        let ctx = egui::Context::default();
        configure_context(&ctx);
        let score = Score::demo();
        let mut app = EncoreApp::from_score(score.clone(), None, "Ready".into());
        app.show_layout = true;
        let mut controls = std::collections::BTreeSet::new();
        let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 680.0));
        for frame in 0..3 {
            let output = ctx.run(input(frame), |ctx| app.layout_dialog(ctx));
            for shape in output.shapes {
                if let egui::Shape::Text(text) = shape.shape {
                    let label = text.galley.text();
                    if [
                        "Page and system layout",
                        "Paper",
                        "Left margin",
                        "Staff size",
                    ]
                    .contains(&label)
                    {
                        let bounds = text.galley.rect.translate(text.pos.to_vec2());
                        assert!(
                            viewport.contains_rect(bounds),
                            "{label} falls outside minimum window: {bounds:?}"
                        );
                        controls.insert(label.to_owned());
                    }
                }
            }
        }
        assert!(controls.contains("Page and system layout"));
        assert!(controls.contains("Staff size"));
        assert_eq!(app.score, score);
        assert!(app.undo.is_empty());
    }
    #[test]
    fn contextual_locks_keep_full_score_and_linked_part_formats_independent() {
        let mut app = EncoreApp::from_score(Score::demo(), None, "Ready".into());
        app.cached_pages = engraving::engrave(&app.score, &EngravingOptions::default());
        app.cursor_measure = 0;
        app.lock_selected_systems();
        assert!(!app.score.notation.layout.locked_systems.is_empty());
        assert!(app.score.notation.parts.is_empty());
        let score_format = app.score.notation.layout.clone();
        app.part_view = true;
        app.selected_linked_part = 0;
        let projection = app.selected_part_projection().unwrap().unwrap();
        app.cached_pages = engraving::engrave(
            &projection.score,
            &EngravingOptions {
                layout: Some(projection.layout),
                ..Default::default()
            },
        );
        app.active_part = 0;
        app.lock_selected_systems();
        assert!(!app.score.notation.parts[0].layout.locked_systems.is_empty());
        assert_eq!(app.score.notation.layout, score_format);
        app.unlock_selected_systems();
        assert!(app.score.notation.parts[0].layout.locked_systems.is_empty());
        assert_eq!(app.score.notation.layout, score_format);
        app.score.validate().unwrap();
    }
}
