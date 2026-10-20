//! Shared bounded underlay/spanner collection, before source voices are mapped
//! into Encore staff indices and layers.
use super::{Warnings, child, number, text, value};
use crate::document::Anchor;
use crate::lyrics::{Lyric, LyricKind, Syllabic};
use crate::model::Score;
use crate::slurs::{Slur, SlurDirection};
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
#[derive(Default)]
pub(super) struct Annotations {
    styles: super::entry_style::Styles,
    lyrics: Vec<RawLyric>,
    slurs: Vec<RawSlur>,
    lyric_keys: BTreeSet<(SourceAnchor, LyricKind, u16)>,
    open_slurs: BTreeMap<String, (SourceAnchor, Slur)>,
    extensions: BTreeMap<(u32, String, LyricKind, u16), usize>,
}
impl Annotations {
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
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
        for element in node.descendants().filter(|n| n.has_tag_name("slur")) {
            let number = element.attribute("number").unwrap_or("1");
            if number.len() > 64 {
                return Err("MusicXML slur number is too long.".into());
            }
            let key = format!("{}:{number}", at.staff);
            match element.attribute("type"){
                Some("start")=>{
                    let mut slur=Slur::new(at.dummy(),at.dummy());
                    slur.direction=direction(element.attribute("placement").or(element.attribute("orientation")));
                    slur.start_offset=xml_offset(element)?;
                    if let Some(y)=xml_float(element,"bezier-y")?{slur.height=y.abs().clamp(4.0,100.0);}
                    if self.open_slurs.insert(key,(at.clone(),slur)).is_some(){return Err("A MusicXML slur starts again before its previous endpoint.".into());}
                }
                Some("stop")=>{
                    if let Some((start,mut slur))=self.open_slurs.remove(&key){slur.end_offset=xml_offset(element)?;self.slurs.push(RawSlur{start,end:at.clone(),value:slur});}else{warnings.add("A slur stop without a matching start was omitted.");}
                }
                Some("continue")=>warnings.add("Slurs retain their endpoints; additional cross-system control points are re-engraved."),
                _=>return Err("MusicXML slur requires a start, stop, or continue type.".into()),
            }
            if [
                "bezier-x",
                "bezier-x2",
                "bezier-y2",
                "default-x",
                "default-y",
            ]
            .iter()
            .any(|name| element.attribute(*name).is_some())
            {
                warnings.add("Custom slur control points and absolute placement are re-engraved from the preserved endpoints and direction.");
            }
            if element.attribute("line-type").is_some_and(|s| s != "solid") {
                warnings.add("Dashed/dotted slurs are rendered as solid phrase curves.");
            }
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
        for (at, _) in self.open_slurs.values_mut() {
            if at.measure == measure {
                at.tick += ticks;
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
            self.open_slurs.insert(format!("muse:{id}"), (at, slur));
        } else if element.has_tag_name("endSpanner") {
            let id = element.attribute("id").unwrap_or("");
            if let Some((start, value)) = self.open_slurs.remove(&format!("muse:{id}")) {
                self.slurs.push(RawSlur {
                    start,
                    end: at,
                    value,
                });
            }
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
            if staff_delta != 0 || voice_delta != 0 {
                warnings.add("Cross-staff or cross-voice MuseScore slurs are not supported and were omitted.");
                return Ok(());
            }
            let end = SourceAnchor {
                measure,
                tick,
                ..at.clone()
            };
            let mut slur = Slur::new(at.dummy(), end.dummy());
            if let Some(properties) = child(element, "Slur") {
                slur.direction =
                    direction(value(properties, "up").or(value(properties, "direction")));
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
            || self.slurs.len() + self.open_slurs.len() > 20_000
        {
            return Err("Imported score exceeds Encore's lyric or slur limit.".into());
        }
        Ok(())
    }
    pub(super) fn finish(
        self,
        score: &mut Score,
        mapping: &BTreeMap<(u32, String), (usize, u8)>,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let mt = score.measure_ticks();
        let resolve = |at: &SourceAnchor| -> Result<Anchor, String> {
            let (part, layer) = mapping
                .get(&(at.staff, at.voice.clone()))
                .ok_or("Annotation refers to an unknown source voice.")?;
            Ok(Anchor {
                part: *part,
                layer: *layer,
                measure: at.measure + at.tick as usize / mt as usize,
                tick: at.tick % mt,
            })
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
            if slur.start.part != slur.end.part || slur.start.layer != slur.end.layer {
                warnings.add("Cross-staff or cross-voice slurs are omitted; same-layer phrase curves are retained.");
            } else {
                score.notation.slurs.push(slur);
            }
        }
        if !self.open_slurs.is_empty() {
            warnings.add("Slurs without matching end notes were omitted.");
        }
        crate::lyrics::validate(score)?;
        crate::slurs::validate(score)?;
        Ok(())
    }
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
                .map_err(|_| format!("Invalid slur {name}."))
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
    fn dangling_and_unsupported_slur_styles_report_loss() {
        let text = xml(&note("C", "<slur type='start' line-type='dashed'/>", ""));
        let doc = roxmltree::Document::parse(&text).unwrap();
        let result = super::super::musicxml::parse(&doc).unwrap();
        assert!(result.score.notation.slurs.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("matching end")));
        assert!(result.warnings.iter().any(|w| w.contains("solid phrase")));
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
