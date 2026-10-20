//! Public XML staff-bracket descriptions, independent of instrument identity.
use super::{Warnings, child, text, value};
use crate::groups::{GroupBarlines, GroupSymbol, MAX_GROUPS, StaffGroup};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};

type PartStaves = BTreeMap<String, Vec<usize>>;

fn symbol(value: &str) -> Result<GroupSymbol, String> {
    match value {
        "none" => Ok(GroupSymbol::None),
        "bracket" => Ok(GroupSymbol::Bracket),
        "brace" => Ok(GroupSymbol::Brace),
        "line" => Ok(GroupSymbol::Line),
        "square" => Ok(GroupSymbol::Square),
        _ => Err(format!("Unsupported MusicXML group symbol '{value}'.")),
    }
}
fn coordinate(node: Node<'_, '_>, name: &str) -> Result<f32, String> {
    node.attribute(name)
        .unwrap_or("0")
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| "Invalid MusicXML group position.".into())
}
fn append_members(
    result: &mut Vec<StaffGroup>,
    group: StaffGroup,
    mut members: Vec<usize>,
    warnings: &mut Warnings,
) -> Result<(), String> {
    members.sort_unstable();
    members.dedup();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for member in members {
        if let Some(run) = runs.last_mut().filter(|run| run.1 + 1 == member) {
            run.1 = member;
        } else {
            runs.push((member, member));
        }
    }
    if runs.len() > 1 {
        warnings.add("A visual group became noncontiguous after staff import and was split into separate brackets.");
    }
    for (index, (first, last)) in runs.into_iter().enumerate() {
        if result.len() >= MAX_GROUPS {
            return Err(format!(
                "Import exceeds the {MAX_GROUPS}-group layout limit."
            ));
        }
        let mut g = group.clone();
        g.first_staff = first;
        g.last_staff = last;
        if index > 0 || result.iter().any(|existing| existing.id == g.id) {
            let prefix = group.id.chars().take(48).collect::<String>();
            let mut suffix = index + 1;
            loop {
                g.id = format!("{prefix}-split-{suffix}");
                if !result.iter().any(|existing| existing.id == g.id) {
                    break;
                }
                suffix += 1;
            }
        }
        result.push(g);
    }
    Ok(())
}

pub(super) fn xml(
    root: Node<'_, '_>,
    mapping: &PartStaves,
    warnings: &mut Warnings,
) -> Result<Option<Vec<StaffGroup>>, String> {
    let native = child(root, "identification")
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("miscellaneous"))
        .flat_map(|n| n.children())
        .filter(|n| {
            n.has_tag_name("miscellaneous-field")
                && n.attribute("name") == Some("encore-staff-groups")
        })
        .collect::<Vec<_>>();
    if native.len() > 1 {
        return Err("Duplicate Encore staff-group metadata.".into());
    }
    if let Some(native) = native.first() {
        let data = text(*native);
        if data.len() > 2 * 1024 * 1024 {
            return Err("Encore staff-group metadata exceeds 2 MiB.".into());
        }
        let native: crate::groups::NativeGroups = serde_json::from_str(&data)
            .map_err(|e| format!("Invalid Encore staff-group metadata: {e}"))?;
        if native.parts.len() != mapping.len()
            || native.parts.iter().collect::<BTreeSet<_>>().len() != native.parts.len()
            || native.parts.iter().any(|id| !mapping.contains_key(id))
        {
            return Err("Encore staff-group metadata does not match the imported parts.".into());
        }
        crate::groups::validate(&native.groups, native.parts.len())?;
        let mut result = Vec::new();
        for group in native.groups {
            let members = native.parts[group.first_staff..=group.last_staff]
                .iter()
                .flat_map(|id| mapping[id].iter().copied())
                .collect();
            append_members(&mut result, group, members, warnings)?;
        }
        crate::groups::validate(&result, mapping.values().map(Vec::len).sum())?;
        return Ok(Some(result));
    }
    let mut result = Vec::new();
    let mut ids = Vec::new();
    let mut active = BTreeMap::new();
    let mut found = false;
    let scaling = child(root, "defaults").and_then(|n| child(n, "scaling"));
    let units = |name, fallback| {
        scaling
            .map(|n| super::number::<f32>(n, name))
            .transpose()
            .map(|n| n.flatten().unwrap_or(fallback))
    };
    let millimeters = units("millimeters", 7.0)?;
    let tenths = units("tenths", 40.0)?;
    if !millimeters.is_finite() || !tenths.is_finite() || millimeters <= 0.0 || tenths <= 0.0 {
        return Err("Invalid MusicXML page scaling for staff groups.".into());
    }
    let page_units = millimeters / tenths / 0.254;
    if let Some(list) = child(root, "part-list") {
        for node in list.children().filter(Node::is_element) {
            if node.has_tag_name("score-part") {
                ids.push(
                    node.attribute("id")
                        .ok_or("MusicXML score-part has no id.")?
                        .to_owned(),
                );
                continue;
            }
            if !node.has_tag_name("part-group") {
                continue;
            }
            found = true;
            let number = node.attribute("number").unwrap_or("1");
            if number.len() > 128 {
                return Err("MusicXML group number exceeds 128 bytes.".into());
            }
            match node.attribute("type") {
                Some("start") => {
                    if active.len() + result.len() >= MAX_GROUPS {
                        return Err("Too many MusicXML staff groups.".into());
                    }
                    if active
                        .insert(number.to_owned(), (ids.len(), node))
                        .is_some()
                    {
                        return Err("Duplicate active MusicXML part-group number.".into());
                    }
                }
                Some("stop") => {
                    let (start, node) = active
                        .remove(number)
                        .ok_or("MusicXML part-group stop has no matching start.")?;
                    if ids.len() == start {
                        return Err("A MusicXML part-group contains no parts.".into());
                    }
                    let mut group =
                        StaffGroup::new(format!("xml-group-{}", result.len() + 1), 0, 0);
                    group.name = value(node, "group-name")
                        .map(str::to_owned)
                        .or_else(|| child(node, "group-name-display").map(text))
                        .unwrap_or_default();
                    group.abbreviation = value(node, "group-abbreviation")
                        .map(str::to_owned)
                        .or_else(|| child(node, "group-abbreviation-display").map(text))
                        .unwrap_or_default();
                    group.show_name = child(node, "group-name-display")
                        .and_then(|n| n.attribute("print-object"))
                        != Some("no");
                    group.symbol = symbol(value(node, "group-symbol").unwrap_or("none"))?;
                    group.barlines = match value(node, "group-barline").unwrap_or("no") {
                        "no" => GroupBarlines::Separate,
                        "yes" => GroupBarlines::Through,
                        "Mensurstrich" => GroupBarlines::Between,
                        _ => return Err("Invalid MusicXML group-barline value.".into()),
                    };
                    if let Some(s) = child(node, "group-symbol") {
                        group.offset[0] = -coordinate(s, "relative-x")? * page_units;
                        group.offset[1] = -coordinate(s, "relative-y")? * page_units;
                        if s.attribute("default-x").is_some() || s.attribute("default-y").is_some()
                        {
                            warnings.add("Absolute MusicXML group positioning is re-engraved; relative bracket adjustments are retained.");
                        }
                    }
                    let mut members = Vec::new();
                    for id in &ids[start..] {
                        members.extend(
                            mapping
                                .get(id)
                                .ok_or("A MusicXML group references a missing score part.")?
                                .iter()
                                .copied(),
                        );
                    }
                    append_members(&mut result, group, members, warnings)?;
                }
                _ => return Err("MusicXML part-group needs a start or stop type.".into()),
            }
        }
    }
    if !active.is_empty() {
        return Err("A MusicXML part-group has no matching stop.".into());
    }
    // A source multi-staff part has a brace by default. This remains a visual
    // association after flattening; it does not merge Encore instruments.
    for part in root.children().filter(|n| n.has_tag_name("part")) {
        let Some(staves) = part.attribute("id").and_then(|id| mapping.get(id)) else {
            continue;
        };
        if staves.len() < 2 {
            continue;
        }
        let symbols = part
            .children()
            .filter(|n| n.has_tag_name("measure"))
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("attributes"))
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("part-symbol"))
            .collect::<Vec<_>>();
        if symbols.len() > 1 {
            warnings.add(
                "Changing MusicXML part symbols use their initial appearance for all systems.",
            );
        }
        let mut group = StaffGroup::new(format!("xml-group-{}", result.len() + 1), 0, 0);
        group.symbol = symbols
            .first()
            .map(|n| symbol(n.text().unwrap_or("brace").trim()))
            .transpose()?
            .unwrap_or(GroupSymbol::Brace);
        group.barlines = GroupBarlines::Through;
        group.show_name = false;
        let bound = |name: &str, fallback| {
            symbols
                .first()
                .and_then(|n| n.attribute(name))
                .map(|v| {
                    v.parse::<usize>()
                        .map_err(|_| "Invalid MusicXML part-symbol staff bound.")
                })
                .transpose()
                .map(|v| v.unwrap_or(fallback))
        };
        let top = bound("top-staff", 1)?;
        let bottom = bound("bottom-staff", staves.len())?;
        if top == 0 || top > bottom || bottom > staves.len() {
            return Err("MusicXML part-symbol refers to an invalid staff range.".into());
        }
        append_members(
            &mut result,
            group,
            staves[top - 1..bottom].to_vec(),
            warnings,
        )?;
        found = true;
    }
    if !found {
        return Ok(None);
    }
    crate::groups::validate(&result, mapping.values().map(Vec::len).sum())?;
    Ok(Some(result))
}

/// MuseScore 2–4 staff-definition bracket attributes. Type values are the
/// published NORMAL=0, BRACE=1, SQUARE=2, LINE=3, NO_BRACKET=-1 format.
pub(super) fn muse(
    source: Node<'_, '_>,
    ids: &[&str],
    warnings: &mut Warnings,
) -> Result<Option<Vec<StaffGroup>>, String> {
    let mut result = Vec::new();
    let mut found = false;
    for staff in source
        .children()
        .filter(|n| n.has_tag_name("Part"))
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("Staff"))
    {
        let Some(first) = staff
            .attribute("id")
            .and_then(|id| ids.iter().position(|s| *s == id))
        else {
            continue;
        };
        for bracket in staff.children().filter(|n| n.has_tag_name("bracket")) {
            found = true;
            let kind = bracket
                .attribute("type")
                .unwrap_or("-1")
                .parse::<i16>()
                .map_err(|_| "Invalid MuseScore bracket type.")?;
            let symbol = match kind {
                -1 => continue,
                0 => GroupSymbol::Bracket,
                1 => GroupSymbol::Brace,
                2 => GroupSymbol::Square,
                3 => GroupSymbol::Line,
                _ => {
                    warnings.add("An unsupported MuseScore bracket style was omitted.");
                    continue;
                }
            };
            let span = bracket
                .attribute("span")
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|_| "Invalid MuseScore bracket span.")?;
            if span == 0 {
                continue;
            }
            let end = first
                .checked_add(span - 1)
                .filter(|end| *end < ids.len())
                .ok_or("MuseScore bracket extends beyond its staves.")?;
            let mut group = StaffGroup::new(format!("muse-group-{}", result.len() + 1), first, end);
            group.symbol = symbol;
            group.show_name = false;
            if bracket.attribute("col").is_some_and(|v| v != "0") {
                warnings.add("MuseScore bracket columns are re-engraved as independent nested visual groups.");
            }
            result.push(group);
            if result.len() > MAX_GROUPS {
                return Err("Too many MuseScore staff brackets.".into());
            }
        }
        if value(staff, "barLineSpan").is_some_and(|v| v != "0") {
            warnings.add("MuseScore custom barline spans are not imported yet; brackets retain separate staff barlines.");
        }
    }
    if found {
        crate::groups::validate(&result, ids.len())?;
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Instrument, Score};

    fn import(xml: &str) -> Result<super::super::ImportResult, String> {
        let doc = crate::import::parse_xml(xml)?;
        crate::import::musicxml::parse(&doc)
    }
    fn part(id: &str) -> String {
        format!(
            "<part id='{id}'><measure number='1'><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes></measure></part>"
        )
    }

    #[test]
    fn native_and_standard_groups_roundtrip_without_merging_instruments() {
        let mut score = Score::blank();
        score.parts[1].instrument = Instrument::Cello;
        let mut group = StaffGroup::new("duo".into(), 0, 1);
        group.name = "Strings & keys <duo>".into();
        group.abbreviation = "S. & K.".into();
        group.symbol = GroupSymbol::Square;
        group.barlines = GroupBarlines::Between;
        group.offset = [12.5, -6.0, 8.0];
        group.show_single = true;
        group.show_name = false;
        score.notation.layout.groups = Some(vec![group]);
        score.validate().unwrap();
        let xml = crate::export::musicxml(&score);
        assert!(xml.contains("<group-symbol"));
        assert!(xml.contains("Mensurstrich"));
        let parsed = import(&xml).unwrap().score;
        assert_eq!(parsed.parts.len(), 2);
        assert_eq!(parsed.parts[0].instrument, Instrument::Piano);
        assert_eq!(parsed.parts[1].instrument, Instrument::Cello);
        assert_eq!(parsed.notation.layout.groups, score.notation.layout.groups);
        score.notation.layout.groups = Some(vec![]);
        assert_eq!(
            import(&crate::export::musicxml(&score))
                .unwrap()
                .score
                .notation
                .layout
                .groups,
            Some(vec![])
        );
    }

    #[test]
    fn foreign_nested_groups_resolve_part_ids_even_when_bodies_are_reordered() {
        let xml = format!(
            "<score-partwise><part-list><part-group type='start' number='outer'><group-name>Orchestra</group-name><group-symbol>bracket</group-symbol><group-barline>yes</group-barline></part-group><score-part id='A'><part-name>A</part-name></score-part><part-group type='start' number='inner'><group-symbol>brace</group-symbol><group-barline>Mensurstrich</group-barline></part-group><score-part id='B'><part-name>B</part-name></score-part><score-part id='C'><part-name>C</part-name></score-part><part-group type='stop' number='inner'/><part-group type='stop' number='outer'/></part-list>{}{}{}</score-partwise>",
            part("B"),
            part("A"),
            part("C")
        );
        let imported = import(&xml).unwrap();
        let groups = imported.score.notation.layout.groups.unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!((groups[0].first_staff, groups[0].last_staff), (0, 0));
        assert_eq!((groups[1].first_staff, groups[1].last_staff), (2, 2));
        assert_eq!((groups[2].first_staff, groups[2].last_staff), (0, 2));
        assert_eq!(groups[0].symbol, GroupSymbol::Brace);
        assert!(
            imported
                .warnings
                .iter()
                .any(|s| s.contains("noncontiguous"))
        );
    }

    #[test]
    fn multistaff_part_symbol_is_a_visual_group_after_import() {
        let xml = "<score-partwise><part-list><score-part id='O'><part-name>Organ</part-name></score-part></part-list><part id='O'><measure number='1'><attributes><divisions>480</divisions><staves>3</staves><part-symbol top-staff='1' bottom-staff='2'>brace</part-symbol></attributes></measure></part></score-partwise>";
        let score = import(xml).unwrap().score;
        assert_eq!(score.parts.len(), 3);
        let group = &score.notation.layout.groups.as_ref().unwrap()[0];
        assert_eq!((group.first_staff, group.last_staff), (0, 1));
        assert_eq!(group.symbol, GroupSymbol::Brace);
    }

    #[test]
    fn invalid_group_pairing_and_native_references_fail_clearly() {
        let xml = format!(
            "<score-partwise><part-list><part-group type='stop'/><score-part id='P'><part-name>P</part-name></score-part></part-list>{}</score-partwise>",
            part("P")
        );
        assert!(import(&xml).unwrap_err().contains("matching start"));
        let mut score = Score::blank();
        score.notation.layout.groups = Some(vec![]);
        let xml = crate::export::musicxml(&score).replace("&quot;P2&quot;", "&quot;missing&quot;");
        assert!(import(&xml).unwrap_err().contains("does not match"));
    }

    #[test]
    fn native_groups_survive_part_reordering_and_generated_id_collisions() {
        let mut score = Score::blank();
        let mut extra = score.parts[0].clone();
        extra.instrument = Instrument::Flute;
        score.parts.push(extra);
        score.notation.layout.groups = Some(vec![
            StaffGroup::new("outer".into(), 0, 1),
            StaffGroup::new("outer-split-2".into(), 2, 2),
        ]);
        let mut xml = crate::export::musicxml(&score);
        let doc = crate::import::parse_xml(&xml).unwrap();
        let parts = doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("part"))
            .collect::<Vec<_>>();
        let range = parts[0].range().start..parts[2].range().end;
        let ordered = format!(
            "{}{}{}",
            &xml[parts[0].range()],
            &xml[parts[2].range()],
            &xml[parts[1].range()]
        );
        drop(doc);
        xml.replace_range(range, &ordered);
        let score = import(&xml).unwrap().score;
        score.validate().unwrap();
        let groups = score.notation.layout.groups.unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!(
            groups.iter().map(|g| &g.id).collect::<BTreeSet<_>>().len(),
            3
        );
        assert_eq!((groups[0].first_staff, groups[0].last_staff), (0, 0));
        assert_eq!((groups[1].first_staff, groups[1].last_staff), (2, 2));
        assert_eq!((groups[2].first_staff, groups[2].last_staff), (1, 1));
    }

    #[test]
    fn foreign_relative_group_geometry_honors_musicxml_scaling() {
        let xml = format!(
            "<score-partwise><defaults><scaling><millimeters>14</millimeters><tenths>40</tenths></scaling></defaults><part-list><part-group type='start'><group-symbol relative-x='-10' relative-y='5'>brace</group-symbol></part-group><score-part id='A'><part-name>A</part-name></score-part><score-part id='B'><part-name>B</part-name></score-part><part-group type='stop'/></part-list>{}{}</score-partwise>",
            part("A"),
            part("B")
        );
        let score = import(&xml).unwrap().score;
        let offset = score.notation.layout.groups.unwrap()[0].offset;
        assert!((offset[0] - 10.0 * 14.0 / 40.0 / 0.254).abs() < 0.001);
        assert!((offset[1] + 5.0 * 14.0 / 40.0 / 0.254).abs() < 0.001);
    }

    #[test]
    fn muse_brackets_use_staff_definition_spans_without_instrument_merging() {
        let xml = "<museScore><Score><Part><Staff id='1'><bracket type='0' span='3' col='1'/><bracket type='1' span='2' col='0'/></Staff><Staff id='2'/><Staff id='3'/></Part></Score></museScore>";
        let doc = crate::import::parse_xml(xml).unwrap();
        let mut warnings = Warnings::default();
        let groups = muse(
            child(doc.root_element(), "Score").unwrap(),
            &["1", "2", "3"],
            &mut warnings,
        )
        .unwrap()
        .unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!((groups[0].first_staff, groups[0].last_staff), (0, 2));
        assert_eq!(groups[1].symbol, GroupSymbol::Brace);
        assert!(warnings.finish().iter().any(|w| w.contains("columns")));
    }
}
