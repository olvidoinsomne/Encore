//! Visual staff groups. These never merge instruments, playback, or voices.
use crate::layout::DocumentLayout;
use crate::model::{Clef, Instrument, Score};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod musicxml;
pub(crate) use musicxml::NativeGroups;
pub use musicxml::{musicxml_start, musicxml_stop, native_metadata};

pub const MAX_GROUPS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupSymbol {
    None,
    #[default]
    Bracket,
    Brace,
    Line,
    Square,
}
impl GroupSymbol {
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Bracket,
        Self::Brace,
        Self::Line,
        Self::Square,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bracket => "Bracket",
            Self::Brace => "Brace",
            Self::Line => "Line",
            Self::Square => "Square",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupBarlines {
    #[default]
    Separate,
    Through,
    Between,
}
impl GroupBarlines {
    pub const ALL: &'static [Self] = &[Self::Separate, Self::Through, Self::Between];
    pub fn name(self) -> &'static str {
        match self {
            Self::Separate => "Only on staves",
            Self::Through => "Through staves",
            Self::Between => "Only between staves",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaffGroup {
    pub id: String,
    /// Inclusive source staff indices, also in linked-part layouts.
    pub first_staff: usize,
    pub last_staff: usize,
    pub name: String,
    pub abbreviation: String,
    pub show_name: bool,
    pub symbol: GroupSymbol,
    pub barlines: GroupBarlines,
    pub show_single: bool,
    /// Page units: positive left adjustment moves left; positive top/bottom
    /// adjustments move the corresponding endpoint downward on the page.
    pub offset: [f32; 3],
}
impl StaffGroup {
    pub fn new(id: String, first_staff: usize, last_staff: usize) -> Self {
        Self {
            id,
            first_staff,
            last_staff,
            name: String::new(),
            abbreviation: String::new(),
            show_name: true,
            symbol: GroupSymbol::Bracket,
            barlines: GroupBarlines::Separate,
            show_single: false,
            offset: [0.0; 3],
        }
    }
    pub fn contains(&self, staff: usize) -> bool {
        (self.first_staff..=self.last_staff).contains(&staff)
    }
}

pub fn validate(groups: &[StaffGroup], staff_count: usize) -> Result<(), String> {
    if groups.len() > MAX_GROUPS {
        return Err(format!(
            "A layout may contain at most {MAX_GROUPS} staff groups."
        ));
    }
    let mut ids = BTreeSet::new();
    for group in groups {
        if group.id.is_empty()
            || group.id.len() > 64
            || !group
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || !ids.insert(&group.id)
        {
            return Err("Staff groups need unique IDs of 1–64 letters, digits, hyphens, underscores, or periods.".into());
        }
        if group.first_staff > group.last_staff || group.last_staff >= staff_count {
            return Err("A staff group refers to an invalid source staff range.".into());
        }
        if [&group.name, &group.abbreviation]
            .iter()
            .any(|s| s.len() > 1024 || s.chars().any(char::is_control))
        {
            return Err("Group names must be printable single lines of at most 1024 bytes.".into());
        }
        if group
            .offset
            .iter()
            .any(|v| !v.is_finite() || !(-200.0..=200.0).contains(v))
        {
            return Err("Group adjustments must be finite and within −200–200 page units.".into());
        }
    }
    Ok(())
}

pub fn validate_layout(layout: &DocumentLayout, staff_count: usize) -> Result<(), String> {
    layout
        .groups
        .as_deref()
        .map_or(Ok(()), |g| validate(g, staff_count))
}

/// Most-specific span wins; equal spans use the last definition. Apply the
/// winner to each staff/connection independently, including Separate overrides.
pub fn barline_group(groups: &[StaffGroup], first: usize, last: usize) -> Option<&StaffGroup> {
    groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.first_staff <= first && g.last_staff >= last)
        .min_by_key(|(i, g)| (g.last_staff - g.first_staff, std::cmp::Reverse(*i)))
        .map(|(_, g)| g)
}

/// Materialize legacy defaults for an editor. Rendering None keeps its exact
/// legacy pagination heuristic; merely opening this list does not save groups.
pub fn automatic(score: &Score, staves: &[usize]) -> Vec<StaffGroup> {
    let mut groups = Vec::new();
    for chunk in staves.chunks(8) {
        if chunk.len() < 2 {
            continue;
        }
        let first = *chunk.iter().min().unwrap();
        let last = *chunk.iter().max().unwrap();
        if staves
            .iter()
            .filter(|&&s| (first..=last).contains(&s))
            .count()
            != chunk.len()
        {
            continue;
        }
        let mut group = StaffGroup::new(format!("auto-{}", groups.len() + 1), first, last);
        group.show_name = false;
        group.barlines = GroupBarlines::Through;
        let piano_pair = |a: usize, b: usize| {
            score
                .parts
                .get(a)
                .zip(score.parts.get(b))
                .is_some_and(|(a, b)| {
                    a.percussion.is_none()
                        && b.percussion.is_none()
                        && a.instrument == Instrument::Piano
                        && b.instrument == Instrument::Piano
                })
        };
        if chunk.len() == 2 && piano_pair(chunk[0], chunk[1]) {
            group.symbol = GroupSymbol::Brace;
        }
        groups.push(group);
        if chunk.len() > 2 {
            for pair in chunk.windows(2) {
                if piano_pair(pair[0], pair[1])
                    && score.parts[pair[0]].clef == Clef::Treble
                    && score.parts[pair[1]].clef == Clef::Bass
                    && pair[0].abs_diff(pair[1]) == 1
                {
                    let mut brace = StaffGroup::new(
                        format!("auto-{}", groups.len() + 1),
                        pair[0].min(pair[1]),
                        pair[0].max(pair[1]),
                    );
                    brace.symbol = GroupSymbol::Brace;
                    brace.show_name = false;
                    brace.barlines = GroupBarlines::Through;
                    groups.push(brace);
                }
            }
        }
    }
    groups
}

pub fn effective(score: &Score, layout: &DocumentLayout, staves: &[usize]) -> Vec<StaffGroup> {
    layout
        .groups
        .clone()
        .unwrap_or_else(|| automatic(score, staves))
}

/// Reindex from old source IDs to new IDs. Deleted groups are dropped; an
/// explicit empty layout remains explicit, never returning to legacy mode.
pub fn remap(layout: &mut DocumentLayout, mapping: &[Option<usize>]) {
    let Some(groups) = &mut layout.groups else {
        return;
    };
    groups.retain_mut(|group| {
        let retained = (group.first_staff..=group.last_staff)
            .filter_map(|old| mapping.get(old).copied().flatten())
            .collect::<Vec<_>>();
        let Some(first) = retained.iter().min().copied() else {
            return false;
        };
        group.first_staff = first;
        group.last_staff = *retained.iter().max().unwrap();
        true
    });
}

pub fn remove_staff(layout: &mut DocumentLayout, removed: usize) {
    let count = layout
        .groups
        .as_ref()
        .and_then(|g| g.iter().map(|g| g.last_staff + 1).max())
        .unwrap_or(0)
        .max(removed + 1);
    let map = (0..count)
        .map(|i| {
            if i == removed {
                None
            } else {
                Some(i - usize::from(i > removed))
            }
        })
        .collect::<Vec<_>>();
    remap(layout, &map);
}

/// Project source ranges into actual displayed order. Noncontiguous membership
/// becomes separate groups, never brackets unrelated intervening staves.
pub fn project(layout: &mut DocumentLayout, source_staves: &[usize]) -> Vec<String> {
    let Some(groups) = layout.groups.take() else {
        return vec![];
    };
    let mut result = Vec::new();
    let mut warnings = Vec::new();
    let mut used = BTreeSet::new();
    let mut omitted = 0;
    for group in groups {
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (display, &source) in source_staves
            .iter()
            .enumerate()
            .filter(|(_, s)| group.contains(**s))
        {
            let _ = source;
            if let Some(run) = runs.last_mut().filter(|run| run.1 + 1 == display) {
                run.1 = display;
            } else {
                runs.push((display, display));
            }
        }
        if runs.len() > 1 {
            warnings.push(format!(
                "Staff group '{}' was split because its members are noncontiguous in this part.",
                if group.name.is_empty() {
                    &group.id
                } else {
                    &group.name
                }
            ));
        }
        for (index, (first, last)) in runs.into_iter().enumerate() {
            if result.len() == MAX_GROUPS {
                omitted += 1;
                continue;
            }
            let mut projected = group.clone();
            projected.first_staff = first;
            projected.last_staff = last;
            if index > 0 || used.contains(&projected.id) {
                let prefix = group.id.chars().take(48).collect::<String>();
                let mut suffix = index + 1;
                loop {
                    projected.id = format!("{prefix}-split-{suffix}");
                    if !used.contains(&projected.id) {
                        break;
                    }
                    suffix += 1;
                }
            }
            used.insert(projected.id.clone());
            result.push(projected);
        }
    }
    if omitted > 0 {
        warnings.push(format!("{omitted} split staff-group fragments exceeded the {MAX_GROUPS}-group display limit and were omitted."));
    }
    layout.groups = Some(result);
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts::{self, LinkedPart, Transposition};

    fn definition(staves: Vec<usize>) -> LinkedPart {
        LinkedPart {
            id: "group-test".into(),
            name: "Group study".into(),
            staves,
            voicing: vec![],
            transposition: Transposition::CONCERT,
            layout: DocumentLayout::default(),
        }
    }

    #[test]
    fn legacy_and_explicit_empty_groups_are_distinct_and_persistent() {
        let score = Score::blank();
        let original = score.clone();
        assert!(score.notation.layout.groups.is_none());
        let defaults = effective(&score, &score.notation.layout, &[0, 1]);
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults[0].symbol, GroupSymbol::Brace);
        assert_eq!(score, original);
        let mut explicit = score.clone();
        explicit.notation.layout.groups = Some(vec![]);
        assert!(effective(&explicit, &explicit.notation.layout, &[0, 1]).is_empty());
        let reopened: Score =
            serde_json::from_str(&serde_json::to_string(&explicit).unwrap()).unwrap();
        assert_eq!(reopened, explicit);
        let legacy: DocumentLayout = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.groups, None);
    }

    #[test]
    fn nested_and_overlapping_barlines_have_a_shared_deterministic_policy() {
        let mut outer = StaffGroup::new("outer".into(), 0, 5);
        outer.barlines = GroupBarlines::Through;
        let mut inner = StaffGroup::new("inner".into(), 1, 2);
        inner.barlines = GroupBarlines::Between;
        let mut override_group = inner.clone();
        override_group.id = "override".into();
        override_group.barlines = GroupBarlines::Separate;
        let overlap = StaffGroup::new("overlap".into(), 2, 4);
        let groups = vec![outer, inner, override_group, overlap];
        validate(&groups, 6).unwrap();
        assert_eq!(barline_group(&groups, 0, 1).unwrap().id, "outer");
        assert_eq!(barline_group(&groups, 1, 2).unwrap().id, "override");
        assert_eq!(barline_group(&groups, 2, 3).unwrap().id, "overlap");
    }

    #[test]
    fn malformed_definitions_fail_validation_before_source_changes() {
        let score = Score::blank();
        let before = score.clone();
        for group in [
            StaffGroup::new("bad".into(), 0, 2),
            StaffGroup::new("bad".into(), 1, 0),
            StaffGroup::new("space id".into(), 0, 1),
        ] {
            let mut candidate = score.clone();
            candidate.notation.layout.groups = Some(vec![group]);
            assert!(candidate.validate().is_err());
        }
        let mut group = StaffGroup::new("finite".into(), 0, 1);
        group.offset[0] = f32::NAN;
        assert!(validate(&[group], 2).is_err());
        assert_eq!(score, before);
    }

    #[test]
    fn score_and_linked_layouts_are_independent_and_reindexed_on_staff_removal() {
        let mut score = Score::demo();
        let mut part = definition(vec![1, 2, 3]);
        score.notation.layout.groups = Some(vec![StaffGroup::new("score".into(), 0, 3)]);
        part.layout.groups = Some(vec![StaffGroup::new("part".into(), 1, 3)]);
        score.notation.parts = vec![part];
        score.validate().unwrap();
        crate::document::remove_staff(&mut score, 1);
        score.parts.remove(1);
        score.validate().unwrap();
        let full = &score.notation.layout.groups.as_ref().unwrap()[0];
        let part = &score.notation.parts[0].layout.groups.as_ref().unwrap()[0];
        assert_eq!((full.first_staff, full.last_staff), (0, 2));
        assert_eq!((part.first_staff, part.last_staff), (1, 2));
        assert_eq!(part.id, "part");
        assert_eq!(full.id, "score");
    }

    #[test]
    fn reordered_parts_split_groups_without_enclosing_unrelated_staves() {
        let score = Score::demo();
        let before = score.clone();
        let mut part = definition(vec![2, 5, 0, 1]);
        part.layout.groups = Some(vec![StaffGroup::new("strings".into(), 0, 2)]);
        let projection = parts::project(&score, &part).unwrap();
        let groups = projection.layout.groups.as_ref().unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!((groups[0].first_staff, groups[0].last_staff), (0, 0));
        assert_eq!((groups[1].first_staff, groups[1].last_staff), (2, 3));
        assert_ne!(groups[0].id, groups[1].id);
        assert!(
            projection
                .warnings
                .iter()
                .any(|s| s.contains("noncontiguous"))
        );
        let extracted = parts::extract(&score, &part, false).unwrap();
        assert_eq!(extracted.notation.layout.groups, projection.layout.groups);
        assert_eq!(score, before);
        assert_eq!(part.layout.groups.as_ref().unwrap()[0].last_staff, 2);
    }

    #[test]
    fn removing_the_only_group_member_preserves_explicit_empty_setting() {
        let mut layout = DocumentLayout {
            groups: Some(vec![StaffGroup::new("solo".into(), 1, 1)]),
            ..Default::default()
        };
        remove_staff(&mut layout, 1);
        assert_eq!(layout.groups, Some(vec![]));
    }
}
