//! Portable MusicXML/MuseScore performance-line collection before voice mapping.
use super::{Warnings, annotations::SourceAnchor, child, number, text, value};
use crate::{
    document::Anchor,
    expressions::Placement,
    lines::{LineKind, PedalKind, PerformanceLine},
    model::Score,
};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};
fn scaled(ticks: u32, division: u32) -> Result<u32, String> {
    let ticks = u64::from(ticks) * 480;
    if division == 0 || ticks % u64::from(division) != 0 {
        return Err("Performance line timing is not representable at480PPQ.".into());
    }
    u32::try_from(ticks / u64::from(division)).map_err(|_| "Performance line tick overflow.".into())
}
#[derive(Default)]
pub(super) struct Lines {
    raw: Vec<(SourceAnchor, SourceAnchor, PerformanceLine)>,
    open: BTreeMap<String, (SourceAnchor, PerformanceLine)>,
    native: BTreeSet<String>,
}
fn dummy(a: &SourceAnchor) -> Anchor {
    Anchor {
        voice: 0,
        part: 0,
        layer: 0,
        measure: a.measure,
        tick: a.tick,
    }
}
fn key(at: &SourceAnchor, family: &str, number: &str) -> String {
    if matches!(family, "wavy-line" | "glissando" | "slide") {
        format!("{}:{}:{family}:{number}", at.staff, at.voice)
    } else {
        format!("{}:{family}:{number}", at.staff)
    }
}
impl Lines {
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if let Some(json) = node
            .descendants()
            .filter(|n| n.has_tag_name("other-direction"))
            .filter_map(|n| n.text())
            .find_map(|s| s.strip_prefix("encore-line:"))
        {
            let line: PerformanceLine = serde_json::from_str(json)
                .map_err(|_| "Invalid Encore performance-line metadata.")?;
            let mut end = at.clone();
            end.measure = at
                .measure
                .checked_add(
                    line.end
                        .measure
                        .checked_sub(line.start.measure)
                        .ok_or("Reversed native performance line.")?,
                )
                .ok_or("Performance-line endpoint overflow.")?;
            end.tick = line.end.tick;
            let family = match line.kind {
                LineKind::Ottava { .. } => "octave-shift",
                LineKind::Pedal { .. } => "pedal",
                LineKind::Trill { .. } => "wavy-line",
                LineKind::Glissando => "glissando",
            };
            let number = node
                .descendants()
                .find(|n| n.tag_name().name() == family)
                .and_then(|n| n.attribute("number"))
                .map(str::to_owned)
                .unwrap_or_else(|| (line.start.layer + 1).to_string());
            self.native.insert(key(&at, family, &number));
            self.raw.push((at, end, line));
            return Ok(());
        }
        for element in node
            .descendants()
            .filter(|n| n.has_tag_name("octave-shift") || n.has_tag_name("pedal"))
        {
            let family = element.tag_name().name();
            let key = key(&at, family, element.attribute("number").unwrap_or("1"));
            if self.native.contains(&key) {
                if element.attribute("type") == Some("stop") {
                    self.native.remove(&key);
                }
                continue;
            }
            let kind = if family == "octave-shift" {
                let size = element
                    .attribute("size")
                    .unwrap_or("8")
                    .parse::<i8>()
                    .map_err(|_| "Invalid octave-line size.")?;
                let octave = match size {
                    8 => 12,
                    15 => 24,
                    22 => 36,
                    _ => return Err("Octave lines support size8,15or22.".into()),
                };
                LineKind::Ottava {
                    semitones: if element.attribute("type") == Some("up") {
                        -octave
                    } else {
                        octave
                    },
                }
            } else {
                // The pedal type chooses the pedal; abbreviated changes only
                // its printed sign. A normal abbreviated start remains damper.
                let sostenuto = element.attribute("type") == Some("sostenuto");
                LineKind::Pedal {
                    kind: if sostenuto {
                        PedalKind::Sostenuto
                    } else {
                        PedalKind::Damper
                    },
                }
            };
            match element.attribute("type") {
                Some("down" | "up" | "start" | "sostenuto") => {
                    if self
                        .open
                        .insert(
                            key,
                            (
                                at.clone(),
                                PerformanceLine::new(dummy(&at), dummy(&at), kind),
                            ),
                        )
                        .is_some()
                    {
                        return Err("A performance line restarts before its prior endpoint.".into());
                    }
                }
                Some("stop" | "discontinue") => {
                    if let Some((start, mut line)) = self.open.remove(&key) {
                        line.placement = if node.attribute("placement") == Some("below") {
                            Placement::Below
                        } else {
                            line.placement
                        };
                        self.raw.push((start, at.clone(), line));
                    } else {
                        warnings.add("An unmatched performance-line stop is omitted.");
                    }
                }
                Some("change") => {
                    if let Some((start, line)) = self.open.remove(&key) {
                        self.raw.push((start, at.clone(), line.clone()));
                        self.open.insert(key, (at.clone(), line));
                    } else {
                        return Err("A pedal change requires an open pedal line.".into());
                    }
                }
                Some("continue") => {}
                _ => warnings.add("An unsupported pedal/octave-line state is omitted."),
            }
        }
        Ok(())
    }
    pub(super) fn xml_note(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        division: f64,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        for element in node.descendants().filter(|n| {
            n.has_tag_name("wavy-line") || n.has_tag_name("glissando") || n.has_tag_name("slide")
        }) {
            let family = element.tag_name().name();
            if family == "slide" {
                warnings
                    .add("Continuous pitch slides are not approximated as chromatic glissandi.");
                continue;
            }
            let key = key(&at, family, element.attribute("number").unwrap_or("1"));
            if self.native.contains(&key) {
                if element.attribute("type") == Some("stop") {
                    self.native.remove(&key);
                }
                continue;
            }
            match element.attribute("type") {
                Some("start") => {
                    let kind = if family == "glissando" {
                        LineKind::Glissando
                    } else {
                        LineKind::Trill {
                            interval: 2,
                            rate: 60,
                        }
                    };
                    if self
                        .open
                        .insert(
                            key,
                            (
                                at.clone(),
                                PerformanceLine::new(dummy(&at), dummy(&at), kind),
                            ),
                        )
                        .is_some()
                    {
                        return Err("An ornament line restarts before its endpoint.".into());
                    }
                }
                Some("stop") => {
                    if let Some((start, line)) = self.open.remove(&key) {
                        self.raw.push((start, at.clone(), line));
                    } else {
                        warnings.add("An unmatched ornament-line stop is omitted.");
                    }
                }
                Some("continue") => {}
                _ => {}
            }
        }
        if node.descendants().any(|n| n.has_tag_name("trill-mark"))
            && !node.descendants().any(|n| n.has_tag_name("wavy-line"))
        {
            // Ordinary single-note trills last for the principal written duration.
            let duration = child(node, "duration")
                .map(text)
                .and_then(|v| v.parse::<u32>().ok())
                .map(|n| {
                    let value = f64::from(n) * 480.0 / division;
                    if !value.is_finite()
                        || value < 0.0
                        || value > f64::from(u32::MAX)
                        || (value - value.round()).abs() > 1e-7
                    {
                        Err("Trill duration is not representable at480PPQ.".to_string())
                    } else {
                        Ok(value.round() as u32)
                    }
                })
                .transpose()?;
            if let Some(duration) = duration {
                let mut end = at.clone();
                end.tick = end
                    .tick
                    .checked_add(duration)
                    .ok_or("Trill endpoint overflow.")?;
                self.raw.push((
                    at.clone(),
                    end,
                    PerformanceLine::new(
                        dummy(&at),
                        dummy(&at),
                        LineKind::Trill {
                            interval: 2,
                            rate: 60,
                        },
                    ),
                ));
            }
        }
        Ok(())
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        division: u32,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let name = node.tag_name().name();
        let kind_name = if name == "Spanner" {
            node.attribute("type").unwrap_or("")
        } else {
            name
        };
        let kind = match kind_name {
            "Ottava" => {
                let subtype = value(node, "subtype")
                    .map(str::to_owned)
                    .or_else(|| {
                        node.descendants()
                            .find(|n| n.has_tag_name("subtype"))
                            .map(text)
                    })
                    .unwrap_or_else(|| "8va".into());
                let semitones = match subtype.as_str() {
                    "0" | "8va" => 12,
                    "1" | "8vb" => -12,
                    "2" | "15ma" => 24,
                    "3" | "15mb" => -24,
                    "4" | "22ma" => 36,
                    "5" | "22mb" => -36,
                    _ => return Err("Unsupported MuseScore octave-line type.".into()),
                };
                LineKind::Ottava { semitones }
            }
            "Pedal" => LineKind::Pedal {
                kind: PedalKind::Damper,
            },
            "Trill" => LineKind::Trill {
                interval: 2,
                rate: 60,
            },
            "Glissando" => LineKind::Glissando,
            _ => return Ok(()),
        };
        if let Some(next) = child(node, "next") {
            let location = child(next, "location").unwrap_or(next);
            let measures = number::<usize>(location, "measures")?.unwrap_or(0);
            let mut end = at.clone();
            end.measure = end
                .measure
                .checked_add(measures)
                .ok_or("Performance-line measure overflow.")?;
            if let Some(frac) = value(location, "fractions") {
                let (n, d) = frac
                    .split_once('/')
                    .ok_or("Invalid performance-line fraction.")?;
                let n = n
                    .parse::<i64>()
                    .map_err(|_| "Invalid performance-line fraction.")?;
                let d = d
                    .parse::<i64>()
                    .map_err(|_| "Invalid performance-line fraction.")?;
                if d <= 0 || n * 1920 % d != 0 {
                    return Err("Unquantized MuseScore performance-line endpoint.".into());
                }
                end.tick = u32::try_from(i64::from(at.tick) + n * 1920 / d)
                    .map_err(|_| "Performance-line endpoint precedes its measure.")?;
            }
            self.raw.push((
                at.clone(),
                end,
                PerformanceLine::new(dummy(&at), dummy(&at), kind),
            ));
        } else if let Some(ticks) = number::<u32>(node, "ticks")? {
            let duration = scaled(ticks, division)?;
            let mut end = at.clone();
            end.tick = end
                .tick
                .checked_add(duration)
                .ok_or("Performance-line endpoint overflow.")?;
            self.raw.push((
                at.clone(),
                end,
                PerformanceLine::new(dummy(&at), dummy(&at), kind),
            ));
        } else if child(node, "prev").is_none() {
            warnings.add("A MuseScore performance line without a supported endpoint is omitted.");
        }
        Ok(())
    }
    pub(super) fn shift(&mut self, measure: usize, amount: u32) {
        for (a, b, _) in &mut self.raw {
            if a.measure == measure {
                a.tick += amount;
            }
            if b.measure == measure {
                b.tick += amount;
            }
        }
        for (a, _) in self.open.values_mut() {
            if a.measure == measure {
                a.tick += amount;
            }
        }
    }
    pub(super) fn resolve(
        self,
        score: &mut Score,
        mapping: &super::VoiceMapping,
        _warnings: &mut Warnings,
    ) -> Result<(), String> {
        if !self.open.is_empty() {
            return Err("A performance line is missing its closing endpoint.".into());
        }
        let width = u64::from(score.measure_ticks());
        let count = score.measure_count();
        let anchor = |source: &SourceAnchor| -> Result<Anchor, String> {
            let target = mapping
                .get(&(source.staff, source.voice.clone()))
                .or_else(|| {
                    mapping
                        .iter()
                        .find(|((staff, _), _)| *staff == source.staff)
                        .map(|(_, v)| v)
                })
                .ok_or("Performance line references a missing staff/voice.")?;
            let total = source.measure as u64 * width + u64::from(source.tick);
            let (measure, tick) = if total == count as u64 * width {
                (count - 1, width as u32)
            } else {
                ((total / width) as usize, (total % width) as u32)
            };
            Ok(Anchor {
                voice: target.voice,
                part: target.part,
                layer: target.layer,
                measure,
                tick,
            })
        };
        for (a, b, mut line) in self.raw {
            line.start = anchor(&a)?;
            line.end = anchor(&b)?;
            if matches!(line.kind, LineKind::Pedal { .. } | LineKind::Ottava { .. }) {
                line.start.voice = 0;
                line.end.voice = 0;
            }
            if !score.notation.lines.contains(&line) {
                score.notation.lines.push(line);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Note, Pitch};
    fn n(start: u32, midi: u8) -> Note {
        Note {
            voice: 0,
            voice_launch: None,
            start,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: vec![Pitch::from_midi(midi)],
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        }
    }
    #[test]
    fn all_four_line_families_roundtrip_musicxml_with_native_options() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(2);
        for m in &mut score.parts[0].measures {
            m.notes.clear();
        }
        score.parts[0].measures[0].notes = vec![n(0, 72), n(480, 76), n(960, 79)];
        let a = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let b = Anchor { tick: 480, ..a };
        score.notation.lines = vec![
            PerformanceLine::new(a, b, LineKind::Ottava { semitones: 24 }),
            PerformanceLine::new(
                a,
                Anchor { measure: 1, ..a },
                LineKind::Pedal {
                    kind: PedalKind::Sostenuto,
                },
            ),
            PerformanceLine::new(
                a,
                b,
                LineKind::Trill {
                    interval: 1,
                    rate: 90,
                },
            ),
            PerformanceLine::new(b, Anchor { tick: 960, ..a }, LineKind::Glissando),
        ];
        score.notation.lines[0].offset = -7.;
        score.validate().unwrap();
        let xml = crate::export::musicxml(&score);
        let doc = super::super::parse_xml(&xml).unwrap();
        let imported = super::super::musicxml::parse(&doc).unwrap();
        assert_eq!(imported.score.notation.lines, score.notation.lines);
        assert_eq!(
            imported.score.parts[0].measures[0]
                .notes
                .iter()
                .filter(|n| !n.pitches.is_empty())
                .map(|n| n.pitches[0].midi())
                .collect::<Vec<_>>(),
            vec![72, 76, 79]
        );
        assert!(xml.contains("sostenuto-pedal=\"yes\""));
    }
    #[test]
    fn foreign_sostenuto_and_abbreviated_damper_use_the_standard_pedal_type() {
        for (kind, expected) in [
            ("sostenuto", PedalKind::Sostenuto),
            ("start", PedalKind::Damper),
        ] {
            let xml = format!(
                "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>Piano</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\"><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes><direction placement=\"below\"><direction-type><pedal type=\"{kind}\" number=\"1\" abbreviated=\"yes\" line=\"yes\" sign=\"yes\"/></direction-type></direction><note><pitch><step>C</step><octave>4</octave></pitch><duration>480</duration><voice>1</voice><type>quarter</type></note><direction><direction-type><pedal type=\"stop\" number=\"1\"/></direction-type></direction></measure></part></score-partwise>"
            );
            assert!(!xml.contains("encore-line:") && !xml.contains("<sound"));
            let doc = super::super::parse_xml(&xml).unwrap();
            let imported = super::super::musicxml::parse(&doc).unwrap().score;
            assert_eq!(imported.notation.lines.len(), 1);
            assert_eq!(
                imported.notation.lines[0].kind,
                LineKind::Pedal { kind: expected }
            );
            let exported = crate::export::musicxml(&imported);
            assert!(exported.contains(&format!("<pedal type=\"{kind}\"")));
            assert!(!exported.contains("abbreviated=\"yes\""));
        }
    }
    #[test]
    fn decimal_divisions_trill_duration_is_scaled_exactly() {
        let xml = "<note><duration>60</duration><notations><ornaments><trill-mark/></ornaments></notations></note>";
        let doc = roxmltree::Document::parse(xml).unwrap();
        let mut lines = Lines::default();
        let mut warnings = Warnings::default();
        lines
            .xml_note(
                doc.root_element(),
                SourceAnchor {
                    staff: 1,
                    voice: "1".into(),
                    measure: 0,
                    tick: 0,
                },
                120.0,
                &mut warnings,
            )
            .unwrap();
        assert_eq!(lines.raw[0].1.tick, 240);
        assert!(
            lines
                .xml_note(
                    doc.root_element(),
                    SourceAnchor {
                        staff: 1,
                        voice: "1".into(),
                        measure: 0,
                        tick: 0
                    },
                    121.0,
                    &mut warnings
                )
                .is_err()
        );
    }
}
