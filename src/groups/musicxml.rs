//! Standard visual part groups plus a bounded native metadata supplement.
use super::{GroupBarlines, GroupSymbol, StaffGroup};
use crate::model::Score;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeGroups {
    pub parts: Vec<String>,
    pub groups: Vec<StaffGroup>,
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn native_metadata(score: &Score) -> String {
    let Some(groups) = &score.notation.layout.groups else {
        return String::new();
    };
    let metadata = NativeGroups {
        parts: (1..=score.parts.len()).map(|i| format!("P{i}")).collect(),
        groups: groups.clone(),
    };
    format!(
        "<miscellaneous><miscellaneous-field name=\"encore-staff-groups\">{}</miscellaneous-field></miscellaneous>",
        xml(&serde_json::to_string(&metadata).unwrap_or_default())
    )
}
pub fn musicxml_start(score: &Score, staff: usize) -> String {
    let mut out = String::new();
    let Some(groups) = &score.notation.layout.groups else {
        return out;
    };
    for (index, group) in groups
        .iter()
        .enumerate()
        .filter(|(_, g)| g.first_staff == staff)
    {
        let _ = writeln!(
            out,
            "    <part-group type=\"start\" number=\"{}\">",
            index + 1
        );
        if group.show_name {
            let _ = write!(
                out,
                "<group-name>{}</group-name><group-abbreviation>{}</group-abbreviation>",
                xml(&group.name),
                xml(&group.abbreviation)
            );
        }
        let symbol = if group.first_staff == group.last_staff && !group.show_single {
            "none"
        } else {
            match group.symbol {
                GroupSymbol::None => "none",
                GroupSymbol::Bracket => "bracket",
                GroupSymbol::Brace => "brace",
                GroupSymbol::Line => "line",
                GroupSymbol::Square => "square",
            }
        };
        let bars = match group.barlines {
            GroupBarlines::Separate => "no",
            GroupBarlines::Through => "yes",
            GroupBarlines::Between => "Mensurstrich",
        };
        let _ = writeln!(
            out,
            "<group-symbol relative-x=\"{}\" relative-y=\"{}\">{symbol}</group-symbol><group-barline>{bars}</group-barline></part-group>",
            -group.offset[0] * (40.0 * 0.254 / 7.0),
            -group.offset[1] * (40.0 * 0.254 / 7.0)
        );
    }
    out
}
pub fn musicxml_stop(score: &Score, staff: usize) -> String {
    let mut out = String::new();
    if let Some(groups) = &score.notation.layout.groups {
        for (index, _) in groups
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, g)| g.last_staff == staff)
        {
            let _ = writeln!(
                out,
                "    <part-group type=\"stop\" number=\"{}\"/>",
                index + 1
            );
        }
    }
    out
}
