//! Note-anchored ornament collection before source voice mapping.
use super::{VoiceMapping, Warnings, annotations::SourceAnchor, child, number, value};
use crate::{
    document::Anchor,
    model::{Pitch, Score, Step},
    ornaments::{MAX_ORNAMENTS, Ornament, OrnamentKind},
    slurs::SlurDirection,
};
use roxmltree::Node;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Ornaments {
    raw: BTreeMap<SourceAnchor, (Ornament, bool)>,
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
fn attr_float(node: Node<'_, '_>, name: &str) -> Result<Option<f32>, String> {
    node.attribute(name)
        .map(|v| {
            v.parse::<f32>()
                .map_err(|_| format!("Invalid ornament {name}."))
        })
        .transpose()
}
impl Ornaments {
    fn insert(
        &mut self,
        at: SourceAnchor,
        mark: Ornament,
        native: bool,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        mark.validate()?;
        if let Some((prior, was_native)) = self.raw.get(&at) {
            if *was_native {
                return Ok(());
            }
            if !native && prior.kind != mark.kind {
                warnings.add(
                    "Multiple ornaments on one entry are reduced to the first supported ornament.",
                );
                return Ok(());
            }
        }
        if self.raw.len() >= MAX_ORNAMENTS && !self.raw.contains_key(&at) {
            return Err("Import exceeds the 20000 ornament limit.".into());
        }
        self.raw.insert(at, (mark, native));
        Ok(())
    }
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if let Some(json) = node
            .descendants()
            .filter(|n| n.has_tag_name("other-notation"))
            .filter_map(|n| n.text())
            .find_map(|s| s.strip_prefix("encore-ornament:"))
        {
            let mut mark: Ornament =
                serde_json::from_str(json).map_err(|_| "Invalid Encore ornament metadata.")?;
            mark.anchor = dummy(&at);
            return self.insert(at, mark, true, warnings);
        }
        for element in node.descendants() {
            let kind = match element.tag_name().name() {
                "turn" | "delayed-turn" => OrnamentKind::Turn,
                "inverted-turn" | "delayed-inverted-turn" => OrnamentKind::InvertedTurn,
                "mordent" => OrnamentKind::Mordent,
                "inverted-mordent" => OrnamentKind::UpperMordent,
                "arpeggiate" => {
                    if element.attribute("direction") == Some("down") {
                        OrnamentKind::ArpeggioDown
                    } else {
                        OrnamentKind::ArpeggioUp
                    }
                }
                "non-arpeggiate" => OrnamentKind::NonArpeggiate,
                _ => continue,
            };
            let mut mark = Ornament::new(dummy(&at), kind);
            if element.tag_name().name().starts_with("delayed-") {
                mark.delay_percent = 75.;
            }
            mark.offset = [
                attr_float(element, "relative-x")?
                    .unwrap_or(0.)
                    .clamp(-160., 160.),
                -attr_float(element, "relative-y")?
                    .unwrap_or(0.)
                    .clamp(-160., 160.),
            ];
            mark.placement = match element.attribute("placement") {
                Some("above") => SlurDirection::Above,
                Some("below") => SlurDirection::Below,
                _ => SlurDirection::Auto,
            };
            if let Some(step) = element.attribute("trill-step") {
                let interval = match step {
                    "half" => Some(1),
                    "whole" => Some(2),
                    _ => None,
                };
                if let Some(i) = interval {
                    if kind == OrnamentKind::Mordent {
                        mark.lower = Some(-i);
                    } else {
                        mark.upper = Some(i);
                    }
                } else {
                    warnings.add("Unison/custom ornament playback patterns are not retained.");
                }
            }
            if !kind.chord()
                && let Some(p) = child(node, "pitch")
            {
                let step = match value(p, "step") {
                    Some("C") => Step::C,
                    Some("D") => Step::D,
                    Some("E") => Step::E,
                    Some("F") => Step::F,
                    Some("G") => Step::G,
                    Some("A") => Step::A,
                    Some("B") => Step::B,
                    _ => return Err("Invalid ornament principal pitch.".into()),
                };
                let main = Pitch {
                    step,
                    octave: number::<i8>(p, "octave")?.ok_or("Missing ornament pitch octave.")?,
                    alter: super::pitch_alter(p)?,
                };
                if let Some(ornaments) = element.parent() {
                    for accidental in ornaments
                        .children()
                        .filter(|n| n.has_tag_name("accidental-mark"))
                    {
                        let alter = match accidental.text().unwrap_or("") {
                            "flat-flat" => -2,
                            "flat" => -1,
                            "natural" => 0,
                            "sharp" => 1,
                            "double-sharp" | "sharp-sharp" => 2,
                            _ => {
                                warnings.add("Unsupported ornament accidental is not retained.");
                                continue;
                            }
                        };
                        let up = accidental.attribute("placement") != Some("below");
                        let position = i32::from(main.octave) * 7
                            + main.step.index()
                            + if up { 1 } else { -1 };
                        let neighbor = Pitch {
                            step: Step::from_index(position),
                            octave: position.div_euclid(7) as i8,
                            alter,
                        };
                        let delta = crate::ornaments::pitch_number(neighbor)
                            - crate::ornaments::pitch_number(main);
                        if (up && (1..=4).contains(&delta)) || (!up && (-4..=-1).contains(&delta)) {
                            if up {
                                mark.upper = Some(delta as i8)
                            } else {
                                mark.lower = Some(delta as i8)
                            }
                        } else {
                            warnings.add("An out-of-range ornament accidental is not retained.");
                        }
                    }
                }
            }
            if [
                "accelerate",
                "beats",
                "second-beat",
                "last-beat",
                "long",
                "start-note",
            ]
            .iter()
            .any(|a| element.attribute(*a).is_some())
            {
                warnings.add(
                    "Custom ornament timing is approximated by Encore's bounded ornament window.",
                );
            }
            self.insert(at.clone(), mark, false, warnings)?;
        }
        Ok(())
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        legacy: bool,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        for element in node.children().filter(|n| {
            matches!(
                n.tag_name().name(),
                "Articulation" | "Ornament" | "Arpeggio"
            )
        }) {
            let subtype = value(element, "subtype").unwrap_or("");
            let kind = if element.has_tag_name("Arpeggio") {
                match subtype {
                    "" | "0" | "normal" | "1" | "up" | "4" => Some(OrnamentKind::ArpeggioUp),
                    "2" | "down" | "5" => Some(OrnamentKind::ArpeggioDown),
                    "3" | "bracket" => Some(OrnamentKind::NonArpeggiate),
                    _ => None,
                }
            } else {
                muse_kind(subtype, legacy)
            };
            let Some(kind) = kind else {
                if element.has_tag_name("Ornament") || element.has_tag_name("Arpeggio") {
                    warnings.add(format!("MuseScore ornament '{subtype}' is not retained."));
                }
                continue;
            };
            let mut mark = Ornament::new(dummy(&at), kind);
            mark.playback = value(element, "play") != Some("0");
            if element.has_tag_name("Arpeggio") {
                if number::<u8>(element, "span")?.is_some_and(|span| span != 1) {
                    warnings.add("A cross-staff MuseScore arpeggio is reduced to its own chord.");
                }
                if matches!(subtype, "4" | "5") {
                    warnings.add(
                        "A straight MuseScore arpeggio is drawn with Encore's wavy roll sign.",
                    );
                }
                if let Some(stretch) = number::<f32>(element, "timeStretch")? {
                    if !stretch.is_finite() || stretch <= 0. {
                        return Err("Invalid MuseScore arpeggio stretch.".into());
                    }
                    mark.window_ticks = (120. * stretch).round().clamp(1., 960.) as u16;
                }
            }
            mark.placement = match value(element, "direction") {
                Some("up" | "1") => SlurDirection::Above,
                Some("down" | "2") => SlurDirection::Below,
                _ => SlurDirection::Auto,
            };
            self.insert(at.clone(), mark, false, warnings)?;
        }
        Ok(())
    }
    pub(super) fn shift(&mut self, measure: usize, amount: u32) {
        let old = std::mem::take(&mut self.raw);
        self.raw = old
            .into_iter()
            .map(|(mut a, (mut m, n))| {
                if a.measure == measure {
                    a.tick += amount;
                    m.anchor.tick = a.tick;
                }
                (a, (m, n))
            })
            .collect();
    }
    pub(super) fn finish(
        self,
        score: &mut Score,
        mapping: &VoiceMapping,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let notes = crate::ornaments::principal_index(score);
        let mut marks = Vec::new();
        for (at, (mut mark, native)) in self.raw {
            let Some(target) = mapping.get(&(at.staff, at.voice)) else {
                continue;
            };
            mark.anchor = target.anchor(at.measure, at.tick);
            if !notes
                .get(&mark.anchor)
                .is_some_and(|n| crate::ornaments::compatible(mark.kind, n))
            {
                if native {
                    return Err("Encore ornament principal is incompatible with its mark.".into());
                }
                warnings.add("An ornament on an incompatible chord/rest was omitted.");
                continue;
            }
            if !mark.kind.chord() {
                let note = notes.get(&mark.anchor).unwrap();
                let (upper, lower) =
                    crate::ornaments::auxiliary_pitches_for_note(score, &mark, note)?;
                let pitches = match mark.kind {
                    OrnamentKind::Mordent => vec![lower],
                    OrnamentKind::UpperMordent => vec![upper],
                    _ => vec![upper, lower],
                };
                if pitches.iter().any(|p| {
                    !(-2..=2).contains(&p.alter)
                        || !(0..=127).contains(&crate::ornaments::pitch_number(*p))
                        || p.midi() == note.pitches[0].midi()
                }) {
                    if native {
                        return Err(
                            "Encore ornament neighbor is outside the supported pitch range.".into(),
                        );
                    }
                    warnings.add("An ornament with an unsupported neighbor was omitted.");
                    continue;
                }
            }
            marks.push(mark);
        }
        score.notation.ornaments.extend(marks);
        crate::ornaments::validate(score)
    }
}
pub(super) fn muse_kind(s: &str, legacy: bool) -> Option<OrnamentKind> {
    Some(match s {
        "ornamentTurn" | "turn" => OrnamentKind::Turn,
        "ornamentTurnInverted" | "reverseturn" => OrnamentKind::InvertedTurn,
        "ornamentMordentInverted" => OrnamentKind::Mordent,
        "ornamentMordent" => {
            if legacy {
                OrnamentKind::UpperMordent
            } else {
                OrnamentKind::Mordent
            }
        }
        "ornamentShortTrill" | "mordent" => OrnamentKind::UpperMordent,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml(notes: &str) -> String {
        format!(
            "<score-partwise version='4.0'><part-list><score-part id='P1'><part-name>Piano</part-name></score-part></part-list><part id='P1'><measure number='1'><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>{notes}</measure></part></score-partwise>"
        )
    }
    fn import(source: &str) -> Score {
        super::super::musicxml::parse(&super::super::parse_xml(source).unwrap())
            .unwrap()
            .score
    }
    #[test]
    fn standard_xml_mordent_meaning_and_accidental_are_independent_of_native_metadata() {
        let s = import(&xml(
            "<note><pitch><step>E</step><alter>0.0</alter><octave>4</octave></pitch><duration>480</duration><type>quarter</type><notations><ornaments><mordent/><accidental-mark placement='below'>sharp</accidental-mark></ornaments></notations></note>",
        ));
        let mark = &s.notation.ornaments[0];
        assert_eq!(mark.kind, OrnamentKind::Mordent);
        assert_eq!(mark.lower, Some(-1));
        assert_eq!(
            crate::performance::Timeline::new(&s).unwrap().notes[1].midi,
            63
        );
    }
    #[test]
    fn native_options_and_voice_two_roundtrip_exactly() {
        let mut s = import(&xml(
            "<note><pitch><step>C</step><octave>4</octave></pitch><duration>480</duration><voice>5</voice><type>quarter</type></note>",
        ));
        let note = &s.parts[0].measures[0].notes[0];
        assert_eq!(note.voice, 1);
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: note.start,
            layer: 0,
            voice: 1,
        };
        let mut mark = Ornament::new(anchor, OrnamentKind::InvertedTurn);
        mark.delay_percent = 37.5;
        mark.window_ticks = 77;
        mark.upper = Some(1);
        mark.lower = Some(-1);
        mark.offset = [3., -7.];
        mark.placement = SlurDirection::Below;
        s.notation.ornaments.push(mark.clone());
        let parsed = import(&crate::export::musicxml(&s));
        assert_eq!(parsed.notation.ornaments, vec![mark]);
    }
    #[test]
    fn standard_chord_roll_deduplicates_and_nonarpeggiate_endpoints_are_portable() {
        let notes=[("C",false,"bottom"),("E",true,""),("G",true,"top")].map(|(step,chord,endpoint)|format!("<note>{}<pitch><step>{step}</step><octave>4</octave></pitch><duration>480</duration><type>quarter</type>{}</note>",if chord{"<chord/>"}else{""},if endpoint.is_empty(){String::new()}else{format!("<notations><non-arpeggiate type='{endpoint}'/></notations>")})).join("");
        let s = import(&xml(&notes));
        assert_eq!(s.notation.ornaments.len(), 1);
        assert_eq!(s.notation.ornaments[0].kind, OrnamentKind::NonArpeggiate);
        let out = crate::export::musicxml(&s);
        assert_eq!(out.matches("<non-arpeggiate").count(), 2);
    }
    #[test]
    fn muse_modern_and_pre36_mordent_names_preserve_their_different_meanings() {
        assert_eq!(
            muse_kind("ornamentMordent", false),
            Some(OrnamentKind::Mordent)
        );
        assert_eq!(
            muse_kind("ornamentMordent", true),
            Some(OrnamentKind::UpperMordent)
        );
        assert_eq!(
            muse_kind("ornamentMordentInverted", false),
            Some(OrnamentKind::Mordent)
        );
    }
    #[test]
    fn muse_pickup_mordent_is_shifted_once_and_chord_roll_imports() {
        let source = "<museScore version='3.02'><programVersion>3.6.2</programVersion><Score><Staff id='1'><Measure len='1/4'><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Articulation><subtype>ornamentMordent</subtype></Articulation><Note><pitch>60</pitch></Note></Chord></voice></Measure><Measure><voice><Chord><durationType>whole</durationType><Arpeggio><subtype>2</subtype><play>0</play></Arpeggio><Note><pitch>60</pitch></Note><Note><pitch>64</pitch></Note><Note><pitch>67</pitch></Note></Chord></voice></Measure></Staff></Score></museScore>";
        let parsed =
            super::super::musescore::parse(&super::super::parse_xml(source).unwrap()).unwrap();
        assert_eq!(parsed.score.notation.ornaments.len(), 2);
        assert_eq!(parsed.score.notation.ornaments[0].anchor.tick, 1440);
        assert_eq!(
            parsed.score.notation.ornaments[0].kind,
            OrnamentKind::Mordent
        );
        assert_eq!(
            parsed.score.notation.ornaments[1].kind,
            OrnamentKind::ArpeggioDown
        );
        assert!(!parsed.score.notation.ornaments[1].playback);
        assert!(
            !parsed
                .warnings
                .iter()
                .any(|w| w.contains("ornamentMordent") || w.contains("incompatible"))
        );
    }
    #[test]
    fn decimal_semitones_are_consistent_across_pitch_style_and_ornament_import() {
        for (alter, expected) in [
            ("-2.0", 58),
            ("-1.0", 59),
            ("0.0", 60),
            ("1.0", 61),
            ("2.000", 62),
        ] {
            let source = xml(&format!(
                "<note><pitch><step>C</step><alter>{alter}</alter><octave>4</octave></pitch><duration>480</duration><type>quarter</type><notehead>diamond</notehead></note>"
            ));
            let score = import(&source);
            assert_eq!(
                score.parts[0].measures[0].notes[0].pitches[0].midi(),
                expected
            );
            assert_eq!(
                score.notation.entries[0].notehead,
                crate::entry_style::Notehead::Diamond
            );
        }
        for alter in ["0.5", "1.00000001", "NaN", "inf", "3.0"] {
            let source = xml(&format!(
                "<note><pitch><step>C</step><alter>{alter}</alter><octave>4</octave></pitch><duration>480</duration></note>"
            ));
            assert!(
                super::super::musicxml::parse(&super::super::parse_xml(&source).unwrap())
                    .unwrap_err()
                    .contains("Microtonal")
            );
        }
    }
}
