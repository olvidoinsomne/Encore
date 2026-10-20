//! Saved page production settings shared by full scores and linked parts.
//! Units are 1/100 inch, matching Encore's 850×1100 Letter canvas.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageSize {
    #[default]
    Letter,
    A4,
    Legal,
    Tabloid,
}
impl PageSize {
    pub const ALL: &'static [Self] = &[Self::Letter, Self::A4, Self::Legal, Self::Tabloid];
    pub fn name(self) -> &'static str {
        match self {
            Self::Letter => "Letter",
            Self::A4 => "A4",
            Self::Legal => "Legal",
            Self::Tabloid => "Tabloid",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Margins {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}
impl Default for Margins {
    fn default() -> Self {
        Self {
            left: 48.0,
            right: 48.0,
            top: 45.0,
            bottom: 45.0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemBreak {
    /// Zero-based measure beginning the new system.
    pub before: usize,
    #[serde(default)]
    pub page: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedSystem {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasureWidth {
    pub measure: usize,
    pub weight: f32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageText {
    pub left: String,
    pub center: String,
    pub right: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentLayout {
    pub page_size: PageSize,
    pub landscape: bool,
    pub staff_gap: f32,
    pub measures_per_system: u8,
    pub margins: Margins,
    pub staff_scale: f32,
    pub breaks: Vec<SystemBreak>,
    pub locked_systems: Vec<LockedSystem>,
    pub measure_widths: Vec<MeasureWidth>,
    pub header: PageText,
    pub footer: PageText,
    pub copyright: String,
    pub first_page_number: u16,
}
impl Default for DocumentLayout {
    fn default() -> Self {
        Self {
            page_size: PageSize::Letter,
            landscape: false,
            staff_gap: 0.0,
            measures_per_system: 4,
            margins: Margins::default(),
            staff_scale: 1.0,
            breaks: vec![],
            locked_systems: vec![],
            measure_widths: vec![],
            header: PageText {
                left: "{title}".into(),
                ..Default::default()
            },
            footer: PageText {
                center: "{page} / {pages}".into(),
                ..Default::default()
            },
            copyright: String::new(),
            first_page_number: 1,
        }
    }
}
impl DocumentLayout {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn dimensions(&self) -> (f32, f32) {
        let (w, h) = match self.page_size {
            PageSize::Letter => (850.0, 1100.0),
            PageSize::A4 => (827.0, 1169.0),
            PageSize::Legal => (850.0, 1400.0),
            PageSize::Tabloid => (1100.0, 1700.0),
        };
        if self.landscape { (h, w) } else { (w, h) }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.staff_gap.is_finite() || !(0.0..=80.0).contains(&self.staff_gap) {
            return Err("Extra staff spacing must be 0–80 page units.".into());
        }
        if !(1..=8).contains(&self.measures_per_system) {
            return Err("A system must target 1–8 measures.".into());
        }
        if !self.staff_scale.is_finite() || !(0.5..=1.5).contains(&self.staff_scale) {
            return Err("Staff scaling must be 50–150%.".into());
        }
        if [
            self.margins.left,
            self.margins.right,
            self.margins.top,
            self.margins.bottom,
        ]
        .iter()
        .any(|m| !m.is_finite() || !(20.0..=250.0).contains(m))
        {
            return Err("Page margins must be 20–250 page units.".into());
        }
        let (w, h) = self.dimensions();
        if self.margins.left + self.margins.right > w - 260.0
            || self.margins.top + self.margins.bottom > h - 340.0
        {
            return Err("The margins leave too little space for music on this page.".into());
        }
        if self.first_page_number == 0 || self.first_page_number > 60_000 {
            return Err("The first page number must be 1–60,000.".into());
        }
        for text in [
            &self.header.left,
            &self.header.center,
            &self.header.right,
            &self.footer.left,
            &self.footer.center,
            &self.footer.right,
            &self.copyright,
        ] {
            if text.len() > 4096 || text.chars().any(char::is_control) {
                return Err(
                    "Page text must be a single printable line of at most 4096 bytes.".into(),
                );
            }
        }
        if self.breaks.len() > crate::model::MAX_MEASURES
            || self.locked_systems.len() > crate::model::MAX_MEASURES
            || self.measure_widths.len() > crate::model::MAX_MEASURES
        {
            return Err("Too many saved layout adjustments.".into());
        }
        let mut breaks = BTreeSet::new();
        for b in &self.breaks {
            if b.before == 0 || !breaks.insert(b.before) {
                return Err(
                    "System/page breaks must have distinct nonzero measure boundaries.".into(),
                );
            }
        }
        let mut locks = self.locked_systems.clone();
        locks.sort_by_key(|l| l.start);
        for (index, lock) in locks.iter().enumerate() {
            if lock.end < lock.start || lock.end - lock.start >= 8 {
                return Err("Locked systems must contain 1–8 measures.".into());
            }
            if index > 0 && locks[index - 1].end >= lock.start {
                return Err("Locked systems cannot overlap.".into());
            }
            if self
                .breaks
                .iter()
                .any(|b| b.before > lock.start && b.before <= lock.end)
            {
                return Err("A system/page break cannot split a locked system.".into());
            }
        }
        let mut widths = BTreeSet::new();
        for width in &self.measure_widths {
            if !width.weight.is_finite()
                || !(0.25..=4.0).contains(&width.weight)
                || !widths.insert(width.measure)
            {
                return Err(
                    "Measure width weights must be unique and range from 0.25 to 4.".into(),
                );
            }
        }
        Ok(())
    }
    pub fn validate_for_score(&self, count: usize) -> Result<(), String> {
        self.validate()?;
        if self.breaks.iter().any(|b| b.before >= count)
            || self.locked_systems.iter().any(|l| l.end >= count)
            || self.measure_widths.iter().any(|w| w.measure >= count)
        {
            return Err("A saved layout adjustment refers to a measure outside this score.".into());
        }
        Ok(())
    }
    /// A malformed direct Rust option falls back safely; validated documents
    /// report errors before reaching engraving rather than silently changing it.
    pub fn safe(&self, count: usize) -> Self {
        if self.validate_for_score(count).is_ok() {
            self.clone()
        } else {
            Self::default()
        }
    }
    pub fn weight(&self, measure: usize) -> f32 {
        self.measure_widths
            .iter()
            .find(|w| w.measure == measure)
            .map_or(1.0, |w| w.weight)
    }
    pub fn locked_at(&self, measure: usize) -> Option<LockedSystem> {
        self.locked_systems
            .iter()
            .copied()
            .find(|l| l.start == measure)
    }
    pub fn next_boundary(&self, measure: usize, end: usize) -> usize {
        self.breaks
            .iter()
            .map(|b| b.before)
            .chain(self.locked_systems.iter().map(|l| l.start))
            .filter(|b| *b > measure && *b < end)
            .min()
            .unwrap_or(end)
    }
}
/// Measure-relative formatting is deliberately separate from musical clipboard
/// contents. Structural editing remaps full-score and each linked-part layout.
pub fn remap_measures(layout: &mut DocumentLayout, mapping: &[Option<usize>]) {
    let mut breaks = std::collections::BTreeMap::new();
    for boundary in &layout.breaks {
        if let Some(before) = mapping
            .get(boundary.before..)
            .and_then(|tail| tail.iter().copied().flatten().next())
            .filter(|n| *n > 0)
        {
            breaks
                .entry(before)
                .and_modify(|page: &mut bool| *page |= boundary.page)
                .or_insert(boundary.page);
        }
    }
    layout.breaks = breaks
        .into_iter()
        .map(|(before, page)| SystemBreak { before, page })
        .collect();
    layout.measure_widths = layout
        .measure_widths
        .iter()
        .filter_map(|w| {
            mapping
                .get(w.measure)
                .copied()
                .flatten()
                .map(|measure| MeasureWidth {
                    measure,
                    weight: w.weight,
                })
        })
        .collect();
    layout.locked_systems = layout
        .locked_systems
        .iter()
        .filter_map(|l| {
            let mapped = (l.start..=l.end)
                .map(|m| mapping.get(m).copied().flatten())
                .collect::<Option<Vec<_>>>()?;
            if mapped.windows(2).any(|p| p[1] != p[0] + 1) {
                return None;
            }
            Some(LockedSystem {
                start: mapped[0],
                end: *mapped.last()?,
            })
        })
        .collect();
}
pub fn page_text(template: &str, title: &str, composer: &str, page: u32, pages: u32) -> String {
    let mut remaining = template;
    let mut output = String::new();
    while let Some(start) = remaining.find('{') {
        output.push_str(&remaining[..start]);
        let tail = &remaining[start..];
        let Some(end) = tail.find('}') else {
            output.push_str(tail);
            return output;
        };
        match &tail[..=end] {
            "{title}" => output.push_str(title),
            "{composer}" => output.push_str(composer),
            "{page}" => output.push_str(&page.to_string()),
            "{pages}" => output.push_str(&pages.to_string()),
            other => output.push_str(other),
        }
        remaining = &tail[end + 1..];
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_part_layout_defaults_and_full_native_roundtrip() {
        let old: DocumentLayout = serde_json::from_str(
            r#"{"page_size":"A4","landscape":true,"staff_gap":5.0,"measures_per_system":3}"#,
        )
        .unwrap();
        assert_eq!(old.staff_scale, 1.0);
        old.validate_for_score(8).unwrap();
        let mut layout = old;
        layout.breaks.push(SystemBreak {
            before: 4,
            page: true,
        });
        layout
            .locked_systems
            .push(LockedSystem { start: 0, end: 2 });
        layout.footer.right = "{composer}".into();
        let saved = serde_json::to_string(&layout).unwrap();
        assert_eq!(
            serde_json::from_str::<DocumentLayout>(&saved).unwrap(),
            layout
        );
    }
    #[test]
    fn layout_rejects_conflicting_locks_bounds_and_nonfinite_formats() {
        let mut l = DocumentLayout {
            staff_scale: f32::NAN,
            ..Default::default()
        };
        assert!(l.validate().is_err());
        l.staff_scale = 1.0;
        l.locked_systems = vec![LockedSystem { start: 0, end: 3 }];
        l.breaks = vec![SystemBreak {
            before: 2,
            page: false,
        }];
        assert!(l.validate().is_err());
        l.breaks.clear();
        l.measure_widths.push(MeasureWidth {
            measure: 10,
            weight: 2.0,
        });
        assert!(l.validate_for_score(8).is_err());
    }
    #[test]
    fn structural_remapping_drops_broken_locks_and_retains_independent_widths() {
        let mut l = DocumentLayout {
            breaks: vec![SystemBreak {
                before: 2,
                page: true,
            }],
            locked_systems: vec![LockedSystem { start: 0, end: 2 }],
            measure_widths: vec![MeasureWidth {
                measure: 3,
                weight: 2.0,
            }],
            ..Default::default()
        };
        remap_measures(&mut l, &[Some(0), None, Some(1), Some(2)]);
        assert!(l.locked_systems.is_empty());
        assert_eq!(l.breaks[0].before, 1);
        assert_eq!(l.measure_widths[0].measure, 2);
    }
    #[test]
    fn publication_tokens_do_not_reinterpret_inserted_document_text() {
        assert_eq!(
            page_text("{title} / {page}", "Literal {page}", "Composer", 3, 8),
            "Literal {page} / 3"
        );
    }
}
