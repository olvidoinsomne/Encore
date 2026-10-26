//! Shared bounded underlay/spanner collection, before source voices are mapped
//! into Encore staff indices and layers.
use super::{Warnings, child, number, text, value};
use crate::document::Anchor;
use crate::lyrics::{Lyric, LyricKind, Syllabic};
use crate::model::Score;
use crate::slurs::{Slur, SlurDirection, SlurShape, SlurStyle};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SourceAnchor {
    pub staff: u32,
    pub voice: String,
    pub measure: usize,
    pub tick: u32,
}
impl SourceAnchor {
    fn dummy(&self) -> Anchor {
        Anchor {
            voice: 0,
            part: 0,
            measure: self.measure,
            tick: self.tick,
            layer: 0,
        }
    }
}
struct RawLyric {
    at: SourceAnchor,
    value: Lyric,
    extend: Option<SourceAnchor>,
}
struct RawSlur {
    start: SourceAnchor,
    end: SourceAnchor,
    value: Slur,
}
struct SlurMark {
    at: SourceAnchor,
    number: String,
    start: bool,
    value: Slur,
    control: Option<f32>,
}
#[derive(Default)]
pub(super) struct Annotations {
    styles: super::entry_style::Styles,
    lyrics: Vec<RawLyric>,
    slurs: Vec<RawSlur>,
    lyric_keys: BTreeSet<(SourceAnchor, LyricKind, u16)>,
    slur_marks: Vec<SlurMark>,
    extensions: BTreeMap<(u32, String, LyricKind, u16), usize>,
}
impl Annotations {
    pub(super) fn percussion_note(
        &mut self,
        at: &SourceAnchor,
        pitch: crate::model::Pitch,
        tied: bool,
    ) {
        self.styles.percussion_note(at, pitch, tied);
    }
    pub(super) fn xml_excluding_slurs(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
        excluded: &BTreeSet<String>,
    ) -> Result<(), String> {
        self.styles.xml(node, at.clone(), warnings)?;
        if child(node, "rest").is_some() {
            if node
                .descendants()
                .any(|n| n.has_tag_name("lyric") || n.has_tag_name("slur"))
            {
                warnings.add("Lyrics and slur endpoints on rests are omitted; attach them to pitched notes to retain them.");
            }
            return Ok(());
        }
        let lyrics = node
            .children()
            .filter(|n| n.has_tag_name("lyric"))
            .collect::<Vec<_>>();
        // New syllables terminate a preceding implicit extension before this note.
        for l in &lyrics {
            if child(*l, "text").is_some() {
                let (kind, verse) = xml_line(*l, warnings)?;
                self.extensions
                    .remove(&(at.staff, at.voice.clone(), kind, verse));
            }
        }
        self.advance(&at);
        for l in lyrics {
            if l.attribute("print-object") == Some("no") {
                warnings.add("Hidden lyric syllables are imported as visible text.");
            }
            if l.attribute("placement") == Some("above") {
                warnings.add(
                    "Lyrics above a staff are placed on Encore's below-staff lyric baselines.",
                );
            }
            let (kind, verse) = xml_line(l, warnings)?;
            let key = (at.staff, at.voice.clone(), kind, verse);
            let extend = child(l, "extend");
            let mut syllable = String::new();
            for element in l.children().filter(|n| n.is_element()) {
                if element.has_tag_name("text") {
                    syllable.push_str(&text(element));
                } else if element.has_tag_name("elision") {
                    syllable.push('‿');
                }
            }
            if !syllable.trim().is_empty() {
                let syllabic = value(l, "syllabic")
                    .map(|s| Syllabic::parse(s).ok_or("Unknown MusicXML lyric syllabic value."))
                    .transpose()?
                    .unwrap_or_default();
                let baseline = l
                    .attribute("relative-y")
                    .map(|s| {
                        s.parse::<f32>()
                            .map_err(|_| "Invalid lyric baseline offset.")
                    })
                    .transpose()?
                    .map_or(0.0, |v| -v);
                let lyric = Lyric {
                    kind,
                    verse,
                    syllabic,
                    baseline,
                    ..Lyric::new(at.dummy(), syllable)
                };
                if !self.lyric_keys.insert((at.clone(), kind, verse)) {
                    warnings.add("Duplicate chord-tone lyrics on one line use the first syllable.");
                    continue;
                }
                self.lyrics.push(RawLyric {
                    at: at.clone(),
                    value: lyric,
                    extend: None,
                });
                if extend.is_some_and(|e| e.attribute("type") != Some("stop")) {
                    self.extensions.insert(key.clone(), self.lyrics.len() - 1);
                }
            } else if extend.is_none() {
                warnings.add("MusicXML humming/laughing lyrics and empty underlay instructions are not retained.");
            }
            if extend.is_some_and(|e| e.attribute("type") == Some("stop")) {
                self.extensions.remove(&key);
            }
            if l.attribute("time-only").is_some() {
                warnings.add(
                    "Verse text is retained, but per-repeat lyric visibility is not retained.",
                );
            }
            if l.descendants().any(|n| {
                n.attribute("font-family").is_some() || n.attribute("font-style").is_some()
            }) {
                warnings.add("Lyric font styling is re-engraved using Encore's lyric font.");
            }
        }
        let mut geometry = BTreeMap::new();
        for element in node
            .descendants()
            .filter(|n| n.has_tag_name("other-notation"))
        {
            if let Some(json) = element
                .text()
                .and_then(|s| s.strip_prefix(crate::slurs::XML_METADATA_PREFIX))
            {
                if json.len() > 2048 {
                    return Err("Encore slur metadata is too long.".into());
                }
                let value: crate::slurs::XmlGeometry = serde_json::from_str(json)
                    .map_err(|_| "Invalid Encore slur geometry metadata.")?;
                if geometry.insert(value.number.to_string(), value).is_some() {
                    return Err("Duplicate Encore slur geometry metadata.".into());
                }
            }
        }
        for element in node.descendants().filter(|n| n.has_tag_name("slur")) {
            let number = element.attribute("number").unwrap_or("1");
            if excluded.contains(number) {
                continue;
            }
            if number
                .parse::<u8>()
                .ok()
                .is_none_or(|n| !(1..=16).contains(&n))
            {
                return Err("MusicXML slur number must be 1–16.".into());
            }
            let start = match element.attribute("type") {
                Some("start") => true,
                Some("stop") => false,
                Some("continue") => {
                    warnings.add("Slur continuation control points are re-engraved from the preserved endpoint shape.");
                    continue;
                }
                _ => return Err("MusicXML slur requires a start, stop, or continue type.".into()),
            };
            let mut slur = Slur::new(at.dummy(), at.dummy());
            slur.direction = direction(
                element
                    .attribute("placement")
                    .or(element.attribute("orientation")),
            );
            let offset = xml_offset(element)?;
            if start {
                slur.start_offset = offset;
            } else {
                slur.end_offset = offset;
            }
            slur.style = match element.attribute("line-type").unwrap_or("solid") {
                "solid" => SlurStyle::Solid,
                "dashed" => SlurStyle::Dashed,
                "dotted" => SlurStyle::Dotted,
                _ => {
                    warnings.add("Wavy slurs are imported as solid curves; solid, dashed, and dotted styles are retained.");
                    SlurStyle::Solid
                }
            };
            slur.dash_length = xml_float(element, "dash-length")?.unwrap_or(6.0);
            slur.gap = xml_float(element, "space-length")?.unwrap_or(4.0);
            let control = xml_float(element, "bezier-y")?;
            if let Some(y) = control {
                slur.height = y.abs().clamp(4.0, 100.0);
            }
            if let Some(native) = geometry.remove(number) {
                if !start {
                    return Err("Encore slur geometry must attach to the start entry.".into());
                }
                slur.height = native.height;
                slur.shape = native.shape;
            } else if [
                "bezier-x",
                "bezier-x2",
                "bezier-y2",
                "default-x",
                "default-y",
            ]
            .iter()
            .any(|name| element.attribute(*name).is_some())
            {
                warnings.add("Absolute MusicXML slur coordinates and horizontal control distances are re-engraved; endpoint offsets and vertical controls are retained.");
            }
            self.slur_marks.push(SlurMark {
                at: at.clone(),
                number: format!("xml:{number}"),
                start,
                value: slur,
                control,
            });
        }
        self.check_size()
    }
    fn advance(&mut self, at: &SourceAnchor) {
        let start = (at.staff, at.voice.clone(), LyricKind::Verse, 0);
        let end = (at.staff, at.voice.clone(), LyricKind::Section, u16::MAX);
        for (_, index) in self.extensions.range(start..=end) {
            if self.lyrics[*index].at != *at {
                self.lyrics[*index].extend = Some(at.clone());
            }
        }
    }

    pub(super) fn shift(&mut self, measure: usize, ticks: u32) {
        self.styles.shift(measure, ticks);
        for l in &mut self.lyrics {
            if l.at.measure == measure {
                l.at.tick += ticks;
            }
            if let Some(end) = &mut l.extend
                && end.measure == measure
            {
                end.tick += ticks;
            }
        }
        for s in &mut self.slurs {
            if s.start.measure == measure {
                s.start.tick += ticks;
            }
            if s.end.measure == measure {
                s.end.tick += ticks;
            }
        }
        for mark in &mut self.slur_marks {
            if mark.at.measure == measure {
                mark.at.tick += ticks;
            }
        }
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        division: u32,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if node.has_tag_name("Rest") {
            return Ok(());
        }
        for l in node.children().filter(|n| n.has_tag_name("Lyrics")) {
            let verse = number::<u16>(l, "no")?
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("MuseScore lyric number overflowed.")?;
            self.extensions
                .remove(&(at.staff, at.voice.clone(), LyricKind::Verse, verse));
        }
        self.advance(&at);
        for l in node.children().filter(|n| n.has_tag_name("Lyrics")) {
            let verse = number::<u16>(l, "no")?
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("MuseScore lyric number overflowed.")?;
            let syllabic = value(l, "syllabic")
                .map(|s| Syllabic::parse(s).ok_or("Unknown MuseScore lyric syllabic value."))
                .transpose()?
                .unwrap_or_default();
            let syllable = child(l, "text").map(text).unwrap_or_default();
            if syllable.trim().is_empty() {
                warnings.add("Empty MuseScore lyric elements are omitted.");
                continue;
            }
            let extend = if let Some(fraction) = value(l, "ticks_f") {
                Some(
                    super::musescore::fraction_ticks(fraction)?
                        .try_into()
                        .map_err(|_| "Invalid MuseScore lyric extension.")?,
                )
            } else if let Some(raw) = value(l, "ticks") {
                if raw.contains('/') {
                    Some(
                        super::musescore::fraction_ticks(raw)?
                            .try_into()
                            .map_err(|_| "Invalid MuseScore lyric extension.")?,
                    )
                } else {
                    let ticks = raw
                        .parse::<u32>()
                        .map_err(|_| "Invalid MuseScore lyric extension.")?;
                    let numerator = ticks
                        .checked_mul(crate::model::PPQ)
                        .ok_or("MuseScore lyric extension overflowed.")?;
                    if !numerator.is_multiple_of(division) {
                        return Err(
                            "MuseScore lyric extension is fractional on Encore's grid.".into()
                        );
                    }
                    Some(numerator / division)
                }
            } else {
                None
            };
            if child(l, "text").is_some_and(|t| t.descendants().any(|n| n.is_element() && n != t)) {
                warnings.add(
                    "MuseScore rich lyric text retains its words, with font/style markup removed.",
                );
            }
            let lyric = Lyric {
                verse,
                syllabic,
                ..Lyric::new(at.dummy(), syllable)
            };
            // A tick-based target is resolved against actual notes after measure
            // lengths and pickup placement have been established.
            let end = extend.filter(|n| *n > 0).map(|ticks| SourceAnchor {
                tick: at.tick.saturating_add(ticks),
                ..at.clone()
            });
            self.lyrics.push(RawLyric {
                at: at.clone(),
                value: lyric,
                extend: end,
            });
            if l.children()
                .any(|n| matches!(n.tag_name().name(), "offset" | "pos" | "autoplace"))
            {
                warnings
                    .add("MuseScore lyric placement is recalculated on Encore's lyric baselines.");
            }
        }
        for element in node.children().filter(|n| {
            n.has_tag_name("Slur") || n.has_tag_name("endSpanner") || n.has_tag_name("Spanner")
        }) {
            self.muse_spanner(element, at.clone(), warnings)?;
        }
        self.check_size()
    }
    pub(super) fn muse_spanner(
        &mut self,
        element: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if element.has_tag_name("Slur") {
            let id = element.attribute("id").ok_or("MuseScore slur has no id.")?;
            let mut slur = Slur::new(at.dummy(), at.dummy());
            slur.direction = direction(value(element, "up").or(value(element, "direction")));
            muse_style(element, &mut slur, warnings)?;
            self.slur_marks.push(SlurMark {
                at,
                number: format!("muse:{id}"),
                start: true,
                value: slur,
                control: None,
            });
        } else if element.has_tag_name("endSpanner") {
            let id = element.attribute("id").unwrap_or("");
            self.slur_marks.push(SlurMark {
                value: Slur::new(at.dummy(), at.dummy()),
                at,
                number: format!("muse:{id}"),
                start: false,
                control: None,
            });
        } else if element.attribute("type") == Some("Slur")
            && let Some(next) = child(element, "next").and_then(|n| child(n, "location"))
        {
            let measures = number::<i32>(next, "measures")?.unwrap_or(0);
            let fractions = value(next, "fractions")
                .map(super::musescore::fraction_ticks)
                .transpose()?
                .unwrap_or(0);
            let measure = (at.measure as i64)
                .checked_add(i64::from(measures))
                .filter(|m| *m >= 0)
                .ok_or("MuseScore slur endpoint lies before the score.")?
                as usize;
            let tick = i64::from(at.tick)
                .checked_add(fractions)
                .filter(|t| *t >= 0 && *t <= i64::from(u32::MAX))
                .ok_or("MuseScore slur endpoint tick is invalid.")? as u32;
            let staff_delta = number::<i32>(next, "staves")?.unwrap_or(0);
            let voice_delta = number::<i32>(next, "voices")?.unwrap_or(0);
            if staff_delta != 0 {
                warnings.add("Cross-staff MuseScore slurs require grouped staff export and are not supported yet; the slur was omitted.");
                return Ok(());
            }
            let voice = at
                .voice
                .parse::<i32>()
                .map_err(|_| "Invalid MuseScore slur source voice.")?
                .checked_add(voice_delta)
                .filter(|v| (0..4).contains(v))
                .ok_or("MuseScore slur endpoint voice lies outside 0–3.")?;
            let end = SourceAnchor {
                measure,
                tick,
                voice: voice.to_string(),
                ..at.clone()
            };
            let mut slur = Slur::new(at.dummy(), end.dummy());
            if let Some(properties) = child(element, "Slur") {
                slur.direction =
                    direction(value(properties, "up").or(value(properties, "direction")));
                muse_style(properties, &mut slur, warnings)?;
            }
            self.slurs.push(RawSlur {
                start: at,
                end,
                value: slur,
            });
        }
        self.check_size()
    }
    fn check_size(&self) -> Result<(), String> {
        if self.lyrics.len() > crate::lyrics::MAX_LYRICS
            || self.slurs.len() + self.slur_marks.len().div_ceil(2) > 20_000
        {
            return Err("Imported score exceeds Encore's lyric or slur limit.".into());
        }
        Ok(())
    }
    pub(super) fn finish(
        mut self,
        score: &mut Score,
        mapping: &super::VoiceMapping,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        // Match number levels in source document order, without assuming that
        // a start precedes its stop in a serialized independent voice. A level
        // cannot start twice before its pair: that would be ambiguous XML.
        let mut pending = BTreeMap::<String, SlurMark>::new();
        for mark in std::mem::take(&mut self.slur_marks) {
            if let Some(prior) = pending.remove(&mark.number) {
                if prior.start == mark.start {
                    return Err(
                        "Ambiguous slur number: two starts or stops share one overlapping level."
                            .into(),
                    );
                }
                let (start, stop) = if prior.start {
                    (prior, mark)
                } else {
                    (mark, prior)
                };
                let mut slur = start.value;
                slur.end_offset = stop.value.end_offset;
                if slur.shape.is_none()
                    && let Some(height) = stop.control
                {
                    if slur.direction == SlurDirection::Auto {
                        slur.direction = if start.control.unwrap_or(height) < 0.0 {
                            SlurDirection::Below
                        } else {
                            SlurDirection::Above
                        };
                    }
                    let sign = if slur.direction == SlurDirection::Below {
                        -1.0
                    } else {
                        1.0
                    };
                    slur.shape = Some(SlurShape {
                        inset: [1.0 / 3.0; 2],
                        heights: [
                            start.control.map_or(slur.height, |y| y * sign),
                            height * sign,
                        ],
                    });
                }
                self.slurs.push(RawSlur {
                    start: start.at,
                    end: stop.at,
                    value: slur,
                });
            } else {
                pending.insert(mark.number.clone(), mark);
            }
        }
        if !pending.is_empty() {
            warnings.add("Slurs without matching end notes were omitted.");
        }
        let mt = score.measure_ticks();
        let resolve = |at: &SourceAnchor| -> Result<Anchor, String> {
            let target = mapping
                .get(&(at.staff, at.voice.clone()))
                .ok_or("Annotation refers to an unknown source voice.")?;
            Ok(target.anchor(at.measure + at.tick as usize / mt as usize, at.tick % mt))
        };
        self.styles.finish(score, mapping)?;
        for raw in self.lyrics {
            let mut lyric = raw.value;
            lyric.anchor = resolve(&raw.at)?;
            lyric.extend_to = raw.extend.as_ref().map(&resolve).transpose()?;
            score.notation.lyrics.push(lyric);
        }
        for raw in self.slurs {
            let mut slur = raw.value;
            slur.start = resolve(&raw.start)?;
            slur.end = resolve(&raw.end)?;
            if slur.start.part != slur.end.part {
                warnings.add("Cross-staff slurs require grouped staff export and are not supported yet; the slur was omitted.");
            } else {
                score.notation.slurs.push(slur);
            }
        }
        crate::lyrics::validate(score)?;
        crate::slurs::validate(score)?;
        Ok(())
    }
}
fn muse_style(node: Node<'_, '_>, slur: &mut Slur, warnings: &mut Warnings) -> Result<(), String> {
    if let Some(style) = value(node, "lineType").or(value(node, "lineStyle")) {
        slur.style = match style {
            "0" | "solid" => SlurStyle::Solid,
            "1" | "dotted" => SlurStyle::Dotted,
            "2" | "3" | "dashed" | "wideDashed" => SlurStyle::Dashed,
            _ => {
                warnings.add("An unknown MuseScore slur line style was imported as solid.");
                SlurStyle::Solid
            }
        };
        if style == "3" || style == "wideDashed" {
            slur.dash_length = 10.0;
            slur.gap = 6.0;
        }
    }
    if node.descendants().any(|n| {
        matches!(
            n.tag_name().name(),
            "SlurSegment" | "o1" | "o2" | "o3" | "o4"
        )
    }) {
        warnings.add("MuseScore per-system slur control points are re-engraved; direction, line style, and endpoints are retained.");
    }
    Ok(())
}
fn direction(value: Option<&str>) -> SlurDirection {
    match value {
        Some("above" | "over" | "1" | "up") => SlurDirection::Above,
        Some("below" | "under" | "2" | "down") => SlurDirection::Below,
        _ => SlurDirection::Auto,
    }
}
fn xml_float(node: Node<'_, '_>, name: &str) -> Result<Option<f32>, String> {
    node.attribute(name)
        .map(|s| {
            s.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("Invalid slur {name}."))
        })
        .transpose()
}
fn xml_offset(node: Node<'_, '_>) -> Result<[f32; 2], String> {
    Ok([
        xml_float(node, "relative-x")?.unwrap_or(0.0),
        -xml_float(node, "relative-y")?.unwrap_or(0.0),
    ])
}
fn xml_line(node: Node<'_, '_>, warnings: &mut Warnings) -> Result<(LyricKind, u16), String> {
    let kind = match node
        .attribute("name")
        .unwrap_or("verse")
        .to_ascii_lowercase()
        .as_str()
    {
        "chorus" => LyricKind::Chorus,
        "section" => LyricKind::Section,
        _ => LyricKind::Verse,
    };
    let number = node.attribute("number").unwrap_or("1");
    let number=number.parse::<u16>().unwrap_or_else(|_|{warnings.add("Non-numeric MusicXML lyric line labels use line 1; rename lyric numbers before importing if lines collide.");1});
    if !(1..=512).contains(&number) {
        return Err("MusicXML lyric line number must be 1–512.".into());
    }
    Ok((kind, number))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml(notes: &str) -> String {
        format!(
            "<score-partwise><part-list><score-part id='P1'><part-name>Piano</part-name><midi-instrument><midi-program>1</midi-program></midi-instrument></score-part></part-list><part id='P1'><measure><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>{notes}</measure></part></score-partwise>"
        )
    }
    fn note(step: &str, notations: &str, lyrics: &str) -> String {
        format!(
            "<note><pitch><step>{step}</step><octave>4</octave></pitch><duration>480</duration><voice>2</voice><type>quarter</type><notations>{notations}</notations>{lyrics}</note>"
        )
    }
    #[test]
    fn cross_voice_stop_before_start_and_line_styles_roundtrip() {
        let plain = note("C", "", "").replace("<voice>2</voice>", "<voice>1</voice>");
        let stop = note("D", "<slur type='stop' number='1'/>", "")
            .replace("<voice>2</voice>", "<voice>1</voice>");
        let start = note(
            "E",
            "<slur type='start' number='1' line-type='dotted' dash-length='5' space-length='7'/>",
            "",
        );
        let text = xml(&format!(
            "{plain}{stop}<backup><duration>960</duration></backup>{start}"
        ));
        let doc = roxmltree::Document::parse(&text).unwrap();
        let imported = super::super::musicxml::parse(&doc).unwrap();
        let slur = &imported.score.notation.slurs[0];
        assert_ne!(slur.start.lane(), slur.end.lane());
        assert_eq!(slur.style, SlurStyle::Dotted);
        assert_eq!(slur.gap, 7.0);
        let exported = crate::export::musicxml(&imported.score);
        let doc = super::super::parse_xml(&exported).unwrap();
        let round = super::super::musicxml::parse(&doc).unwrap();
        assert_eq!(round.score.notation.slurs, imported.score.notation.slurs);
    }
    #[test]
    fn custom_curve_metadata_and_asymmetric_controls_roundtrip() {
        let text = xml(&[
            note(
                "C",
                "<slur type='start' line-type='dashed' placement='below'/>",
                "",
            ),
            note("D", "<slur type='stop'/>", ""),
        ]
        .concat());
        let doc = roxmltree::Document::parse(&text).unwrap();
        let mut score = super::super::musicxml::parse(&doc).unwrap().score;
        let slur = &mut score.notation.slurs[0];
        slur.shape = Some(SlurShape {
            inset: [0.15, 0.55],
            heights: [60.0, -24.0],
        });
        slur.height = 27.0;
        slur.start_offset = [4.0, 3.0];
        slur.end_offset = [-2.0, 6.0];
        slur.dash_length = 11.0;
        slur.gap = 3.0;
        let exported = crate::export::musicxml(&score);
        assert!(exported.contains(crate::slurs::XML_METADATA_PREFIX));
        let doc = super::super::parse_xml(&exported).unwrap();
        let round = super::super::musicxml::parse(&doc).unwrap();
        assert_eq!(round.score.notation.slurs, score.notation.slurs);
    }
    #[test]
    fn ambiguous_numbered_slurs_fail_instead_of_joining_wrong_voices() {
        let notes = [
            note("C", "<slur type='start' number='2'/>", ""),
            note("D", "<slur type='start' number='2'/>", ""),
            note("E", "<slur type='stop' number='2'/>", ""),
        ]
        .concat();
        let text = xml(&notes);
        let doc = roxmltree::Document::parse(&text).unwrap();
        let err = super::super::musicxml::parse(&doc).unwrap_err();
        assert!(err.contains("Ambiguous slur number"));
    }
    #[test]
    fn musicxml_verses_elision_extensions_and_curves_roundtrip() {
        let notes=[note("C","<slur type='start' number='1' placement='below' relative-x='2' relative-y='-3' bezier-y='25'/>","<lyric number='1' name='verse' relative-y='-4'><syllabic>begin</syllabic><text>Sil</text></lyric><lyric number='2' name='chorus'><text>Shine</text><elision> </elision><text>on</text><extend type='start'/></lyric>"),note("D","","<lyric number='1'><syllabic>end</syllabic><text>ver</text></lyric>"),note("E","<slur type='stop' number='1' relative-y='2'/>","<lyric number='2' name='chorus'><extend type='stop'/></lyric>")].concat();
        let text = xml(&notes);
        let document = roxmltree::Document::parse(&text).unwrap();
        let imported = super::super::musicxml::parse(&document).unwrap();
        assert_eq!(imported.score.notation.lyrics.len(), 3);
        assert_eq!(imported.score.notation.slurs.len(), 1);
        let chorus = imported
            .score
            .notation
            .lyrics
            .iter()
            .find(|l| l.kind == LyricKind::Chorus)
            .unwrap();
        assert_eq!(chorus.text, "Shine‿on");
        assert_eq!(chorus.anchor.layer, 1);
        assert_eq!(chorus.extend_to.unwrap().tick, 960);
        let exported = crate::export::musicxml(&imported.score);
        let doc = super::super::parse_xml(&exported).unwrap();
        let round = super::super::musicxml::parse(&doc).unwrap();
        assert_eq!(round.score.notation, imported.score.notation);
    }
    fn muse(chords: &str, version: &str) -> String {
        format!(
            "<museScore version='{version}'><Score><Staff id='1'><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig>{chords}</voice></Measure></Staff></Score></museScore>"
        )
    }
    #[test]
    fn musescore_modern_slurs_and_multiple_lyric_verses() {
        let text = muse(
            "<Chord><durationType>quarter</durationType><Spanner type='Slur'><Slur><up>1</up></Slur><next><location><fractions>1/4</fractions></location></next></Spanner><Lyrics><no>0</no><syllabic>begin</syllabic><text>Glo</text><ticks_f>1/4</ticks_f></Lyrics><Lyrics><no>1</no><text>Sing</text></Lyrics><Note><pitch>60</pitch></Note></Chord><Chord><durationType>quarter</durationType><Note><pitch>62</pitch></Note></Chord>",
            "4.30",
        );
        let doc = roxmltree::Document::parse(&text).unwrap();
        let imported = super::super::musescore::parse(&doc).unwrap();
        assert_eq!(imported.score.notation.slurs.len(), 1);
        assert_eq!(
            imported.score.notation.slurs[0].direction,
            SlurDirection::Above
        );
        assert_eq!(imported.score.notation.slurs[0].end.tick, 480);
        assert_eq!(imported.score.notation.lyrics.len(), 2);
        assert_eq!(
            imported.score.notation.lyrics[0].extend_to.unwrap().tick,
            480
        );
    }
    #[test]
    fn musescore_legacy_numbered_slur_endpoints() {
        let text = muse(
            "<Chord><durationType>quarter</durationType><Slur id='7'><up>2</up></Slur><Note><pitch>60</pitch></Note></Chord><Chord><durationType>quarter</durationType><endSpanner id='7'/><Note><pitch>62</pitch></Note></Chord>",
            "2.06",
        );
        let doc = roxmltree::Document::parse(&text).unwrap();
        let imported = super::super::musescore::parse(&doc).unwrap();
        assert_eq!(imported.score.notation.slurs.len(), 1);
        assert_eq!(
            imported.score.notation.slurs[0].direction,
            SlurDirection::Below
        );
    }
    #[test]
    fn musescore_cross_voice_locations_and_patterns_are_retained() {
        let text = "<museScore version='4.30'><Score><Staff id='1'><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Spanner type='Slur'><Slur><up>1</up><lineType>3</lineType></Slur><next><location><voices>1</voices><fractions>1/4</fractions></location></next></Spanner><Note><pitch>60</pitch></Note></Chord></voice><voice><Rest><durationType>quarter</durationType></Rest><Chord><durationType>quarter</durationType><Note><pitch>64</pitch></Note></Chord></voice></Measure></Staff></Score></museScore>";
        let doc = roxmltree::Document::parse(text).unwrap();
        let imported = super::super::musescore::parse(&doc).unwrap();
        let slur = &imported.score.notation.slurs[0];
        assert_ne!(slur.start.lane(), slur.end.lane());
        assert_eq!(slur.style, SlurStyle::Dashed);
        assert_eq!(slur.dash_length, 10.0);
        assert_eq!(slur.end.tick, 480);
        let legacy = "<museScore version='2.06'><Score><Staff id='1'><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Slur id='5'><lineType>1</lineType></Slur><Note><pitch>60</pitch></Note></Chord></voice><voice><Rest><durationType>quarter</durationType></Rest><Chord><durationType>quarter</durationType><endSpanner id='5'/><Note><pitch>64</pitch></Note></Chord></voice></Measure></Staff></Score></museScore>";
        let doc = roxmltree::Document::parse(legacy).unwrap();
        let imported = super::super::musescore::parse(&doc).unwrap();
        assert_eq!(imported.score.notation.slurs[0].style, SlurStyle::Dotted);
        assert_ne!(
            imported.score.notation.slurs[0].start.lane(),
            imported.score.notation.slurs[0].end.lane()
        );
    }
    #[test]
    fn dangling_slurs_report_unmatched_endpoints() {
        let text = xml(&note("C", "<slur type='start' line-type='dashed'/>", ""));
        let doc = roxmltree::Document::parse(&text).unwrap();
        let result = super::super::musicxml::parse(&doc).unwrap();
        assert!(result.score.notation.slurs.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("matching end")));
        assert!(!result.warnings.iter().any(|w| w.contains("solid phrase")));
    }
    #[test]
    fn annotation_targets_outside_notes_fail_clearly() {
        let text = muse(
            "<Chord><durationType>quarter</durationType><Spanner type='Slur'><next><location><measures>500</measures></location></next></Spanner><Note><pitch>60</pitch></Note></Chord>",
            "4.30",
        );
        let doc = roxmltree::Document::parse(&text).unwrap();
        let err = super::super::musescore::parse(&doc).unwrap_err();
        assert!(err.contains("endpoints"));
    }
}
