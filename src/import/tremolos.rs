//! Measured tremolo collection, preserving public double-note duration semantics.
use super::{VoiceMapping, Warnings, annotations::SourceAnchor, child, number, value};
use crate::{
    document::Anchor,
    model::{Duration, Score, Tuplet},
    tremolos::{MAX_TREMOLOS, Tremolo},
};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Tremolos {
    raw: BTreeMap<SourceAnchor, (Tremolo, Option<SourceAnchor>)>,
    open: BTreeMap<(u32, String), (SourceAnchor, u8, [f32; 2])>,
    paired: BTreeSet<SourceAnchor>,
}
fn dummy(a: &SourceAnchor) -> Anchor {
    Anchor {
        part: 0,
        measure: a.measure,
        tick: a.tick,
        layer: 0,
        voice: 0,
    }
}
impl Tremolos {
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
    ) -> Result<bool, String> {
        if let Some(json) = node
            .descendants()
            .filter(|n| n.has_tag_name("other-notation"))
            .filter_map(|n| n.text())
            .find_map(|t| t.strip_prefix("encore-tremolo:"))
        {
            let mut mark: Tremolo =
                serde_json::from_str(json).map_err(|_| "Invalid Encore tremolo metadata.")?;
            mark.validate()?;
            let end = mark
                .end
                .map(|old| {
                    if old.measure != mark.anchor.measure || old.tick <= mark.anchor.tick {
                        return Err("Invalid Encore paired tremolo endpoint.");
                    }
                    let distance=old.tick - mark.anchor.tick;
                    if !matches!(distance,480|960) { return Err("Encore paired tremolo endpoints must be consecutive stored quarters/halves."); }
                    let tick = at
                        .tick
                        .checked_add(distance)
                        .ok_or("Tremolo position overflow.")?;
                    Ok(SourceAnchor { tick, ..at.clone() })
                })
                .transpose()?;
            mark.anchor = dummy(&at);
            mark.end = end.as_ref().map(dummy);
            if let Some(end) = &end {
                self.paired.insert(at.clone());
                self.paired.insert(end.clone());
            }
            self.insert(at.clone(), mark, end)?;
            return Ok(self.paired.contains(&at));
        }
        let mut marks = node.descendants().filter(|n| n.has_tag_name("tremolo"));
        let Some(element) = marks.next() else {
            return Ok(self.paired.contains(&at));
        };
        if marks.next().is_some() {
            return Err("An entry contains multiple tremolo marks.".into());
        }
        let strokes = element
            .text()
            .unwrap_or("0")
            .trim()
            .parse::<u8>()
            .map_err(|_| "Invalid tremolo stroke count.")?;
        if element.attribute("type") == Some("unmeasured") || strokes == 0 {
            warnings.add("Unmeasured/buzz tremolo is not converted to a measured roll; its notation is omitted.");
            return Ok(false);
        }
        if !(1..=8).contains(&strokes) {
            return Err("MusicXML tremolo strokes must be 1–8.".into());
        }
        let offset = [
            float(element, "relative-x")?,
            -float(element, "relative-y")?,
        ];
        match element.attribute("type").unwrap_or("single") {
            "single" => {
                let mut mark = Tremolo::new(dummy(&at));
                mark.strokes = strokes;
                mark.offset = offset;
                self.insert(at.clone(), mark, None)?;
            }
            "start" => {
                self.paired.insert(at.clone());
                if self
                    .open
                    .insert((at.staff, at.voice.clone()), (at.clone(), strokes, offset))
                    .is_some()
                {
                    return Err("Overlapping paired tremolos in one voice are unsupported.".into());
                }
            }
            "stop" => {
                if let Some((start, count, offset)) =
                    self.open.remove(&(at.staff, at.voice.clone()))
                {
                    if start.measure != at.measure || count != strokes {
                        return Err(
                            "Paired tremolos require matching strokes in one measure.".into()
                        );
                    }
                    let mut mark = Tremolo::new(dummy(&start));
                    mark.end = Some(dummy(&at));
                    mark.strokes = strokes;
                    mark.offset = offset;
                    self.paired.insert(at.clone());
                    self.insert(start, mark, Some(at.clone()))?;
                } else if !self.paired.contains(&at) {
                    return Err("A paired tremolo stop has no start in its voice.".into());
                }
            }
            _ => return Err("Unsupported MusicXML tremolo type.".into()),
        }
        Ok(self.paired.contains(&at))
    }
    fn insert(
        &mut self,
        at: SourceAnchor,
        mark: Tremolo,
        end: Option<SourceAnchor>,
    ) -> Result<(), String> {
        if self.raw.len() >= MAX_TREMOLOS && !self.raw.contains_key(&at) {
            return Err("Import exceeds 20000 tremolo marks.".into());
        }
        self.raw.insert(at, (mark, end));
        Ok(())
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        for element in node.children().filter(|n| {
            n.has_tag_name("Tremolo")
                || n.has_tag_name("TremoloSingleChord")
                || n.has_tag_name("TremoloTwoChord")
        }) {
            let kind = value(element, "subtype").unwrap_or("");
            let strokes = match kind {
                "r8" | "0" => 1,
                "r16" | "1" => 2,
                "r32" | "2" => 3,
                "r64" | "3" => 4,
                _ => {
                    warnings.add("Unmeasured or two-chord MuseScore tremolo is not imported; use MusicXML for supported paired tremolos.");
                    continue;
                }
            };
            if element.has_tag_name("TremoloTwoChord") {
                warnings.add("Two-chord MuseScore tremolo is not imported; use MusicXML for supported paired tremolos.");
                continue;
            }
            let mut mark = Tremolo::new(dummy(&at));
            mark.strokes = strokes;
            mark.playback = number::<u8>(element, "play")?.unwrap_or(1) != 0;
            self.insert(at.clone(), mark, None)?;
        }
        Ok(())
    }
    pub(super) fn shift(&mut self, measure: usize, amount: u32) {
        fn shift(a: &mut SourceAnchor, m: usize, n: u32) {
            if a.measure == m {
                a.tick += n;
            }
        }
        self.raw = std::mem::take(&mut self.raw)
            .into_iter()
            .map(|(mut at, (mut mark, mut end))| {
                shift(&mut at, measure, amount);
                if let Some(a) = &mut end {
                    shift(a, measure, amount);
                }
                mark.anchor = dummy(&at);
                mark.end = end.as_ref().map(dummy);
                (at, (mark, end))
            })
            .collect();
        for (at, _, _) in self.open.values_mut() {
            shift(at, measure, amount);
        }
        self.paired = std::mem::take(&mut self.paired)
            .into_iter()
            .map(|mut a| {
                shift(&mut a, measure, amount);
                a
            })
            .collect();
    }
    pub(super) fn finish(self, score: &mut Score, mapping: &VoiceMapping) -> Result<(), String> {
        if !self.open.is_empty() {
            return Err("An unfinished paired tremolo has no matching stop.".into());
        }
        for (at, (mut mark, end)) in self.raw {
            let target = mapping
                .get(&(at.staff, at.voice))
                .ok_or("Tremolo voice mapping is missing.")?;
            mark.anchor = target.anchor(at.measure, at.tick);
            mark.end = end.map(|a| target.anchor(a.measure, a.tick));
            score.notation.tremolos.push(mark);
        }
        crate::tremolos::validate(score)
    }
}
fn float(node: Node<'_, '_>, name: &str) -> Result<f32, String> {
    let n = node
        .attribute(name)
        .unwrap_or("0")
        .parse::<f32>()
        .map_err(|_| "Invalid tremolo offset.")?;
    if !n.is_finite() || n.abs() > 160. {
        return Err("Tremolo offset exceeds 160 points.".into());
    }
    Ok(n)
}
/// Double-note XML type is twice its actual stored duration, with a 2:1 ratio.
pub(super) fn paired_rhythm(
    node: Node<'_, '_>,
    ticks: u32,
) -> Result<Option<(Duration, bool, Option<Tuplet>)>, String> {
    let duration =
        match value(node, "type") {
            Some("half") => Duration::Quarter,
            Some("whole") => Duration::Half,
            _ => return Err(
                "Paired tremolo import supports displayed halves/wholes (stored quarters/halves)."
                    .into(),
            ),
        };
    let ratio = child(node, "time-modification")
        .ok_or("Paired MusicXML tremolo requires 2:1 time-modification.")?;
    if number::<u8>(ratio, "actual-notes")? != Some(2)
        || number::<u8>(ratio, "normal-notes")? != Some(1)
        || child(node, "dot").is_some()
        || node.descendants().any(|n| n.has_tag_name("tuplet"))
        || ticks != duration.ticks()
    {
        return Err("Paired tremolo rhythm must be equal undotted quarters/halves with only its required 2:1 time-modification.".into());
    }
    Ok(Some((duration, false, None)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml(notes: &str) -> String {
        format!(
            "<score-partwise version='4.0'><part-list><score-part id='P1'><part-name>Piano</part-name></score-part></part-list><part id='P1'><measure number='1'><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>{notes}</measure></part></score-partwise>"
        )
    }
    fn parse(source: &str) -> Result<super::super::ImportResult, String> {
        super::super::musicxml::parse(&super::super::parse_xml(source)?)
    }
    fn principal(pitch: &str, kind: &str, chord: bool) -> String {
        format!(
            "<note>{}<pitch><step>{pitch}</step><octave>4</octave></pitch><duration>480</duration><voice>5</voice><type>half</type><time-modification><actual-notes>2</actual-notes><normal-notes>1</normal-notes></time-modification>{}</note>",
            if chord { "<chord/>" } else { "" },
            if kind.is_empty() {
                String::new()
            } else {
                format!(
                    "<notations><ornaments><tremolo type='{kind}'>2</tremolo></ornaments></notations>"
                )
            }
        )
    }
    #[test]
    fn foreign_double_tremolo_normalizes_chord_types_without_native_metadata() {
        let notes = principal("C", "start", false)
            + &principal("E", "", true)
            + &principal("G", "stop", false)
            + &principal("B", "", true);
        let s = parse(&xml(&notes)).unwrap();
        assert!(
            !s.warnings
                .iter()
                .any(|s| s.contains("Unsupported ornaments"))
        );
        assert_eq!(s.score.notation.tremolos.len(), 1);
        let m = &s.score.notation.tremolos[0];
        assert_eq!(m.anchor.voice, 1);
        assert_eq!(m.end.unwrap().tick, 480);
        assert!(
            s.score.parts[0].measures[0]
                .notes
                .iter()
                .all(|n| n.duration == Duration::Quarter && n.tuplet.is_none())
        );
        assert_eq!(
            crate::performance::Timeline::new(&s.score)
                .unwrap()
                .notes
                .len(),
            16
        );
    }
    #[test]
    fn paired_and_single_exact_options_roundtrip_with_voice_and_chords() {
        let (mut s, a) = crate::tremolos::tests::fixture();
        s.parts[0].measures[0].notes = vec![
            crate::tremolos::tests::note(0, Duration::Half, &[60, 64]),
            crate::tremolos::tests::note(960, Duration::Half, &[67, 71]),
        ];
        let mut m = Tremolo::new(a);
        m.end = Some(Anchor { tick: 960, ..a });
        m.strokes = 4;
        m.offset = [7.5, -13.];
        m.playback = false;
        s.notation.tremolos.push(m);
        let written = crate::export::musicxml(&s);
        assert_eq!(written.matches("<tremolo type=").count(), 2);
        assert_eq!(written.matches("<type>whole</type>").count(), 4);
        assert_eq!(written.matches("<actual-notes>2</actual-notes>").count(), 4);
        let loaded = parse(&written).unwrap().score;
        assert_eq!(loaded.notation.tremolos, s.notation.tremolos);
        assert_eq!(
            loaded.parts[0].measures[0].notes,
            s.parts[0].measures[0].notes
        );
        s.notation.tremolos[0].end = None;
        s.notation.tremolos[0].anchor.tick = 960;
        s.notation.tremolos[0].playback = true;
        s.parts[0].measures[0].notes[1].voice = 1;
        s.notation.tremolos[0].anchor.voice = 1;
        let loaded = parse(&crate::export::musicxml(&s)).unwrap().score;
        assert_eq!(loaded.notation.tremolos, s.notation.tremolos);
    }
    #[test]
    fn public_single_tremolo_keeps_existing_tuplet_and_dots() {
        let source = xml(
            "<note><pitch><step>C</step><octave>4</octave></pitch><duration>240</duration><type>eighth</type><dot/><time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification><notations><ornaments><tremolo>2</tremolo></ornaments></notations></note>",
        );
        let s = parse(&source).unwrap().score;
        assert_eq!(s.notation.tremolos[0].strokes, 2);
        let n = &s.parts[0].measures[0].notes[0];
        assert!(n.dotted);
        assert_eq!(
            n.tuplet,
            Some(Tuplet {
                actual: 3,
                normal: 2
            })
        );
        assert_eq!(
            crate::performance::Timeline::new(&s).unwrap().notes.len(),
            6
        );
    }
    #[test]
    fn public_pair_requires_matching_end_strokes_voice_and_ratio() {
        for tail in [
            principal("G", "", false),
            principal("G", "stop", false).replace(">2</tremolo>", ">3</tremolo>"),
            principal("G", "stop", false).replace("<voice>5</voice>", "<voice>1</voice>"),
        ] {
            assert!(parse(&xml(&(principal("C", "start", false) + &tail))).is_err());
        }
        let source = xml(&(principal("C", "start", false) + &principal("G", "stop", false)))
            .replace(
                "<actual-notes>2</actual-notes>",
                "<actual-notes>3</actual-notes>",
            );
        assert!(parse(&source).unwrap_err().contains("2:1"));
    }
    #[test]
    fn pickup_endpoints_shift_once_and_preserve_the_actual_pair_duration() {
        let source = xml(&(principal("C", "start", false) + &principal("G", "stop", false)))
            .replace(
                "<measure number='1'>",
                "<measure number='0' implicit='yes'>",
            );
        let s = parse(&source).unwrap().score;
        assert_eq!(s.notation.tremolos[0].anchor.tick, 960);
        assert_eq!(s.notation.tremolos[0].end.unwrap().tick, 1440);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(t.notes.first().unwrap().start, 960.);
        assert_eq!(t.notes.last().unwrap().end, 1920.);
    }
    #[test]
    fn percussion_pair_xml_preserves_sound_ids_before_midi_mapping() {
        let mut s = crate::performance::percussion_tests::fixture();
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        s.parts[0].measures[0]
            .notes
            .push(crate::performance::percussion_tests::note(
                480,
                Duration::Quarter,
                7,
                0,
                0,
            ));
        let mut m = Tremolo::new(a);
        m.end = Some(Anchor { tick: 480, ..a });
        s.notation.tremolos.push(m);
        let loaded = parse(&crate::export::musicxml(&s)).unwrap().score;
        assert_eq!(loaded.notation.tremolos, s.notation.tremolos);
        assert_eq!(
            loaded.parts[0].measures[0].notes[0].pitches[0],
            crate::percussion::token(3)
        );
        assert_eq!(
            loaded.parts[0].measures[0].notes[1].pitches[0],
            crate::percussion::token(7)
        );
        assert!(
            crate::performance::Timeline::new(&loaded)
                .unwrap()
                .notes
                .iter()
                .all(|n| n.midi == 38)
        );
    }
    #[test]
    fn unmeasured_roll_is_reported_without_fabricated_measured_attacks() {
        let source = xml(
            "<note><pitch><step>C</step><octave>4</octave></pitch><duration>480</duration><type>quarter</type><notations><ornaments><tremolo type='unmeasured'>0</tremolo></ornaments></notations></note>",
        );
        let s = parse(&source).unwrap();
        assert!(s.score.notation.tremolos.is_empty());
        assert!(s.warnings.iter().any(|s| s.contains("Unmeasured/buzz")));
    }
    #[test]
    fn muse_single_roll_and_pickup_import_with_explicit_pair_warning() {
        let source = "<museScore version='3.02'><programVersion>3.6.2</programVersion><Score><Staff id='1'><Measure len='1/4'><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Tremolo><subtype>r16</subtype><play>0</play></Tremolo><Note><pitch>60</pitch></Note></Chord></voice></Measure><Measure><voice><Chord><durationType>whole</durationType><TremoloTwoChord><subtype>c32</subtype></TremoloTwoChord><Note><pitch>60</pitch></Note></Chord></voice></Measure></Staff></Score></museScore>";
        let parsed =
            super::super::musescore::parse(&super::super::parse_xml(source).unwrap()).unwrap();
        assert_eq!(parsed.score.notation.tremolos.len(), 1);
        let mark = &parsed.score.notation.tremolos[0];
        assert_eq!(mark.anchor.tick, 1440);
        assert_eq!(mark.strokes, 2);
        assert!(!mark.playback);
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.contains("two-chord") || w.contains("Two-chord"))
        );
    }
}
