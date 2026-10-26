//! Independent score/part staff-group formatting, edited as an atomic draft.
use super::*;
use encore::groups::{GroupBarlines, GroupSymbol, StaffGroup};

#[derive(Default)]
pub(super) struct Editor {
    pub open: bool,
    target: Option<String>,
    scope: String,
    staves: Vec<usize>,
    staff_names: Vec<String>,
    original: Option<Vec<StaffGroup>>,
    draft: Option<Vec<StaffGroup>>,
    selected: usize,
    error: Option<String>,
}

impl EncoreApp {
    pub(super) fn open_groups(&mut self) {
        let definitions = encore::parts::definitions(&self.score);
        let definition = self
            .part_view
            .then(|| {
                definitions
                    .get(self.selected_linked_part)
                    .or_else(|| definitions.first())
            })
            .flatten();
        let (target, scope, staves, groups) = if let Some(part) = definition {
            (
                Some(part.id.clone()),
                format!("Linked part: {}", part.name),
                part.staves.clone(),
                part.layout.groups.clone(),
            )
        } else {
            (
                None,
                "Full score".into(),
                (0..self.score.parts.len()).collect(),
                self.score.notation.layout.groups.clone(),
            )
        };
        self.groups_editor = Editor {
            open: true,
            target,
            scope,
            staves,
            staff_names: self
                .score
                .parts
                .iter()
                .map(|part| part.name.clone())
                .collect(),
            original: groups.clone(),
            draft: groups,
            selected: 0,
            error: None,
        };
    }

    fn apply_groups_draft(&mut self) -> bool {
        let result = (|| {
            let editor = &self.groups_editor;
            if self
                .score
                .parts
                .iter()
                .map(|part| part.name.clone())
                .collect::<Vec<_>>()
                != editor.staff_names
            {
                return Err(
                    "The staff list changed. Reopen Staff groups before saving this draft."
                        .to_owned(),
                );
            }
            let mut candidate = self.score.clone();
            if let Some(id) = &editor.target {
                let mut definitions = encore::parts::definitions(&candidate);
                let part = definitions
                    .iter_mut()
                    .find(|part| &part.id == id)
                    .ok_or("The linked part was removed. Reopen Staff groups.")?;
                if part.staves != editor.staves || part.layout.groups != editor.original {
                    return Err(
                        "The linked part or its groups changed. Reopen Staff groups.".into(),
                    );
                }
                part.layout.groups = editor.draft.clone();
                candidate.notation.parts = definitions;
            } else {
                if candidate.notation.layout.groups != editor.original {
                    return Err("The score groups changed. Reopen Staff groups.".into());
                }
                candidate.notation.layout.groups = editor.draft.clone();
            }
            candidate.validate()?;
            Ok(candidate)
        })();
        match result {
            Ok(score) => {
                self.score = score;
                self.groups_editor.open = false;
                self.cached_score = None;
                self.status = "Staff groups saved for this score or part.".into();
                true
            }
            Err(error) => {
                self.status = error.clone();
                self.groups_editor.error = Some(error);
                false
            }
        }
    }

    pub(super) fn groups_window(&mut self, ctx: &egui::Context) {
        if !self.groups_editor.open {
            return;
        }
        let mut open = true;
        let mut save = false;
        let viewport = ctx.content_rect();
        let automatic = encore::groups::automatic(&self.score, &self.groups_editor.staves);
        let editor = &mut self.groups_editor;
        let names = &editor.staff_names;
        egui::Window::new("Staff groups").open(&mut open).vscroll(true)
            .default_pos(egui::pos2(((viewport.width()-700.0)*0.5).max(16.0),24.0))
            .constrain_to(viewport.shrink(8.0)).default_width(650.0)
            .max_width((viewport.width()-32.0).max(280.0)).max_height((viewport.height()-110.0).max(240.0))
            .show(ctx,|ui| {
                ui.horizontal(|ui| {save=ui.button("Save staff groups").clicked();if ui.button("Cancel").clicked(){editor.open=false;}});
                ui.strong(&editor.scope);
                ui.label("Brackets, names and barline spans are saved independently for the full score and each linked part.");
                if let Some(error)=&editor.error {ui.label(error);}
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(editor.draft.is_none(),"Automatic defaults").clicked(){editor.draft=None;editor.selected=0;}
                    if ui.selectable_label(editor.draft.is_some(),"Custom groups").clicked() && editor.draft.is_none() {editor.draft=Some(automatic.clone());editor.selected=0;}
                    if ui.button("Add group").clicked() {
                        let groups=editor.draft.get_or_insert_with(||automatic.clone());
                        let mut n=1;
                        while groups.iter().any(|group|group.id==format!("group-{n}")){n+=1;}
                        let first=editor.staves.iter().copied().min().unwrap_or(0);
                        let last=editor.staves.iter().copied().max().unwrap_or(first);
                        groups.push(StaffGroup::new(format!("group-{n}"),first,last));editor.selected=groups.len()-1;
                    }
                });
                let Some(groups)=editor.draft.as_mut() else {ui.label("Automatic presentation is retained. Choose Custom groups to replace it, including a deliberately ungrouped staff layout.");return;};
                if groups.is_empty(){ui.label("No custom groups. Automatic brackets will stay hidden after saving.");return;}
                editor.selected=editor.selected.min(groups.len()-1);
                egui::ScrollArea::vertical().id_salt("staff-group-list").max_height(110.0).show(ui,|ui| {
                    for (index,group) in groups.iter().enumerate(){ui.selectable_value(&mut editor.selected,index,format!("{} · {}–{}",if group.name.is_empty(){&group.id}else{&group.name},group.first_staff+1,group.last_staff+1));}
                });
                let group=&mut groups[editor.selected];
                ui.separator();
                egui::Grid::new("staff-group-fields").num_columns(2).show(ui,|ui| {
                    ui.label("Name");ui.text_edit_singleline(&mut group.name);ui.end_row();
                    ui.label("Abbreviation");ui.text_edit_singleline(&mut group.abbreviation);ui.end_row();
                    ui.label("Show name");ui.checkbox(&mut group.show_name,"Full name on first system, abbreviation later");ui.end_row();
                    ui.label("First staff");egui::ComboBox::from_id_salt("group-first-staff").selected_text(names.get(group.first_staff).map_or("Removed staff",String::as_str)).show_ui(ui,|ui|{for &staff in &editor.staves{ui.selectable_value(&mut group.first_staff,staff,format!("{} · {}",staff+1,names[staff]));}});ui.end_row();
                    ui.label("Last staff");egui::ComboBox::from_id_salt("group-last-staff").selected_text(names.get(group.last_staff).map_or("Removed staff",String::as_str)).show_ui(ui,|ui|{for &staff in &editor.staves{ui.selectable_value(&mut group.last_staff,staff,format!("{} · {}",staff+1,names[staff]));}});ui.end_row();
                    ui.label("Bracket");egui::ComboBox::from_id_salt("group-symbol").selected_text(group.symbol.name()).show_ui(ui,|ui|{for &symbol in GroupSymbol::ALL{ui.selectable_value(&mut group.symbol,symbol,symbol.name());}});ui.end_row();
                    ui.label("Barlines");egui::ComboBox::from_id_salt("group-barlines").selected_text(group.barlines.name()).show_ui(ui,|ui|{for &barlines in GroupBarlines::ALL{ui.selectable_value(&mut group.barlines,barlines,barlines.name());}});ui.end_row();
                });
                ui.checkbox(&mut group.show_single,"Show bracket when only one member is visible");
                let [left,top,bottom]=&mut group.offset;
                ui.horizontal_wrapped(|ui|{for (label,value) in [("Left ",left),("Top ",top),("Bottom ",bottom)]{ui.add(egui::DragValue::new(value).range(-200.0..=200.0).prefix(label));}});
                ui.small("Membership includes every source staff between the first and last. Overlapping groups use the smallest span's barlines; later groups break equal-span ties.");
                let selected=editor.selected;
                ui.horizontal(|ui| {
                    if ui.add_enabled(selected>0,egui::Button::new("Move earlier")).clicked(){groups.swap(selected,selected-1);editor.selected-=1;}
                    if ui.add_enabled(selected+1<groups.len(),egui::Button::new("Move later")).clicked(){groups.swap(selected,selected+1);editor.selected+=1;}
                    if ui.button("Remove group").clicked(){groups.remove(selected);editor.selected=selected.saturating_sub(1);}
                });
            });
        if !open {
            self.groups_editor.open = false;
        }
        if save {
            self.apply_groups_draft();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> EncoreApp {
        EncoreApp::from_score(Score::blank(), None, "Ready".into())
    }
    #[test]
    fn opening_automatic_groups_does_not_materialize_or_dirty_them() {
        let mut app = app();
        let source = app.score.clone();
        app.open_groups();
        assert!(app.groups_editor.draft.is_none());
        assert_eq!(app.score, source);
        app.groups_editor.open = false;
        assert_eq!(app.score, source);
    }
    #[test]
    fn removing_the_final_custom_group_does_not_restore_automatic_braces() {
        let mut app = app();
        app.score.notation.layout.groups = Some(vec![StaffGroup::new("piano".into(), 0, 1)]);
        app.open_groups();
        app.groups_editor.draft = Some(Vec::new());
        assert!(app.apply_groups_draft());
        assert_eq!(app.score.notation.layout.groups, Some(Vec::new()));
        app.open_groups();
        assert_eq!(app.groups_editor.draft, Some(Vec::new()));
        app.groups_editor.draft = None;
        assert!(app.apply_groups_draft());
        assert!(app.score.notation.layout.groups.is_none());
    }
    #[test]
    fn linked_part_group_edits_leave_full_score_and_other_part_formats_independent() {
        let mut app = app();
        app.score.add_part(Instrument::Flute);
        let full = StaffGroup::new("ensemble".into(), 0, 2);
        app.score.notation.layout.groups = Some(vec![full.clone()]);
        app.score.notation.parts = encore::parts::default_definitions(&app.score);
        let other = app.score.notation.parts[1].layout.clone();
        app.part_view = true;
        app.selected_linked_part = 0;
        app.open_groups();
        let mut piano = StaffGroup::new("piano".into(), 0, 1);
        piano.symbol = GroupSymbol::Brace;
        app.groups_editor.draft = Some(vec![piano.clone()]);
        assert!(app.apply_groups_draft());
        assert_eq!(app.score.notation.layout.groups, Some(vec![full]));
        assert_eq!(app.score.notation.parts[0].layout.groups, Some(vec![piano]));
        assert_eq!(app.score.notation.parts[1].layout, other);
    }
    #[test]
    fn invalid_and_stale_group_drafts_cannot_overwrite_a_score() {
        let mut app = app();
        app.open_groups();
        let source = app.score.clone();
        app.groups_editor.draft = Some(vec![StaffGroup::new("invalid".into(), 1, 0)]);
        assert!(!app.apply_groups_draft());
        assert_eq!(app.score, source);
        app.open_groups();
        app.score.parts[0].name = "Renamed".into();
        let source = app.score.clone();
        assert!(!app.apply_groups_draft());
        assert_eq!(app.score, source);
    }
}
