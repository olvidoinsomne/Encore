//! Collect per-entry engraving before source voices and concert pitches resolve.
use super::annotations::SourceAnchor;
use super::{Warnings, child, number, value};
use crate::{
    document::{Anchor, note_at},
    entry_style::{
        AccidentalDisplay, AccidentalOverride, BeamStyle, EntryStyle, Notehead, StemDirection,
    },
    model::{Pitch, Score, Step},
};
use roxmltree::Node;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Styles {
    entries: BTreeMap<SourceAnchor, RawStyle>,
}
struct RawStyle {
    style: EntryStyle,
    pitches: Vec<(u8, bool)>,
}
impl Styles {
    pub fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if self.entries.len() > crate::model::MAX_EVENTS {
            return Err("Too many imported entry overrides.".into());
        }
        let raw = self.entries.entry(at.clone()).or_insert_with(|| RawStyle {
            style: EntryStyle::new(Anchor {
                part: 0,
                measure: at.measure,
                tick: at.tick,
                layer: 0,
            }),
            pitches: Vec::new(),
        });
        if node.attribute("print-object") == Some("no") {
            raw.style.visible = false;
        }
        if let Some(stem) = value(node, "stem") {
            raw.style.stem = match stem {
                "up" => StemDirection::Up,
                "down" => StemDirection::Down,
                "none" => StemDirection::None,
                _ => StemDirection::Auto,
            };
        }
        if let Some(head) = child(node, "notehead") {
            raw.style.notehead = match head.text().unwrap_or("normal").trim() {
                "normal" => Notehead::Normal,
                "x" | "cross" => Notehead::Cross,
                "diamond" => Notehead::Diamond,
                "triangle" => Notehead::Triangle,
                "slash" => Notehead::Slash,
                "square" => Notehead::Square,
                _ => {
                    warnings.add("An unsupported MusicXML notehead uses the normal head.");
                    Notehead::Normal
                }
            };
            if let Some(size) = head.attribute("font-size") {
                match size.parse::<f32>() {
                    Ok(size) if size.is_finite() && (8.0..=40.0).contains(&size) => {
                        raw.style.scale = size / 20.0
                    }
                    _ => warnings
                        .add("Unsupported MusicXML notehead font size uses the default scale."),
                }
            }
        }
        if let Some(beam) = node.children().find(|element| {
            element.has_tag_name("beam") && element.attribute("number").unwrap_or("1") == "1"
        }) {
            raw.style.beam = match beam.text().unwrap_or("").trim() {
                "begin" => BeamStyle::Break,
                "continue" | "end" => BeamStyle::Join,
                _ => BeamStyle::Auto,
            };
        }
        if let Some(pitch) = child(node, "pitch") {
            let step = match value(pitch, "step").unwrap_or("") {
                "C" => Step::C,
                "D" => Step::D,
                "E" => Step::E,
                "F" => Step::F,
                "G" => Step::G,
                "A" => Step::A,
                "B" => Step::B,
                _ => return Err("Invalid MusicXML pitch for entry style.".into()),
            };
            let pitch = Pitch {
                step,
                octave: number::<i8>(pitch, "octave")?.ok_or("MusicXML pitch needs an octave.")?,
                alter: number::<i8>(pitch, "alter")?.unwrap_or(0),
            };
            let tied = node
                .children()
                .any(|n| n.has_tag_name("tie") && n.attribute("type") == Some("start"))
                || node
                    .descendants()
                    .any(|n| n.has_tag_name("tied") && n.attribute("type") == Some("start"));
            raw.pitches.push((pitch.midi(), tied));
            if let Some(accidental) = child(node, "accidental") {
                let display = if accidental.attribute("print-object") == Some("no") {
                    AccidentalDisplay::Hide
                } else if accidental.attribute("cautionary") == Some("yes") {
                    AccidentalDisplay::Courtesy
                } else if accidental.attribute("parentheses") == Some("yes") {
                    AccidentalDisplay::Parenthesized
                } else {
                    AccidentalDisplay::Show
                };
                raw.style
                    .accidentals
                    .retain(|mark| mark.pitch != pitch.midi());
                raw.style.accidentals.push(AccidentalOverride {
                    pitch: pitch.midi(),
                    display,
                });
            }
            if let Some(tied) = node.descendants().find(|element| {
                element.has_tag_name("tied") && element.attribute("type") == Some("start")
            }) {
                raw.style.tie_direction = match tied
                    .attribute("orientation")
                    .or(tied.attribute("placement"))
                {
                    Some("over" | "above") => crate::slurs::SlurDirection::Above,
                    Some("under" | "below") => crate::slurs::SlurDirection::Below,
                    _ => Default::default(),
                };
            }
        }
        for element in node
            .descendants()
            .filter(|element| element.has_tag_name("other-notation"))
        {
            if let Some(json) = element
                .text()
                .and_then(|text| text.strip_prefix("encore-entry-style:"))
            {
                if json.len() > 16 * 1024 {
                    return Err("Entry-style interchange metadata exceeds its limit.".into());
                }
                raw.style = serde_json::from_str(json)
                    .map_err(|error| format!("Invalid Encore entry style in MusicXML: {error}"))?;
            }
        }
        Ok(())
    }
    pub fn shift(&mut self, measure: usize, ticks: u32) {
        let entries = std::mem::take(&mut self.entries);
        self.entries = entries
            .into_iter()
            .map(|(mut at, raw)| {
                if at.measure == measure {
                    at.tick += ticks;
                }
                (at, raw)
            })
            .collect();
    }
    pub fn finish(
        self,
        score: &mut Score,
        mapping: &BTreeMap<(u32, String), (usize, u8)>,
    ) -> Result<(), String> {
        let mt = score.measure_ticks();
        for (at, raw) in self.entries {
            let Some((part, layer)) = mapping.get(&(at.staff, at.voice)) else {
                return Err("Entry engraving references an unknown source voice.".into());
            };
            let anchor = Anchor {
                part: *part,
                layer: *layer,
                measure: at.measure + at.tick as usize / mt as usize,
                tick: at.tick % mt,
            };
            let mut style = raw.style;
            style.anchor = anchor;
            let note = note_at(score, &anchor).ok_or("Entry engraving has no source note.")?;
            let remap = |midi: u8| {
                raw.pitches
                    .iter()
                    .position(|(pitch, _)| *pitch == midi)
                    .and_then(|index| note.pitches.get(index))
                    .map(|pitch| pitch.midi())
            };
            style.accidentals = style
                .accidentals
                .iter()
                .filter_map(|mark| {
                    remap(mark.pitch).map(|pitch| AccidentalOverride { pitch, ..*mark })
                })
                .collect();
            if let Some(ties) = &mut style.tie_pitches {
                *ties = ties.iter().filter_map(|midi| remap(*midi)).collect();
            } else if raw.pitches.len() > 1
                && raw.pitches.iter().any(|(_, tie)| *tie)
                && raw.pitches.iter().any(|(_, tie)| !*tie)
            {
                style.tie_pitches = Some(
                    raw.pitches
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, tie))| *tie)
                        .filter_map(|(index, _)| note.pitches.get(index).map(|pitch| pitch.midi()))
                        .collect(),
                );
            }
            crate::entry_style::assign(score, style)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{document::Anchor, entry_style::*, model::*};
    #[test]
    fn native_and_portable_manual_engraving_roundtrip_with_per_pitch_ties() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![Note {
            start: 0,
            layer: 2,
            duration: Duration::Eighth,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(61), Pitch::from_midi(65)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: false,
        }];
        let mut continuation = score.parts[0].measures[0].notes[0].clone();
        continuation.start = 240;
        score.parts[0].measures[0].notes.push(continuation);
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 2,
        };
        let style = EntryStyle {
            stem: StemDirection::Down,
            beam: BeamStyle::Flat,
            visible: false,
            scale: 0.65,
            notehead: Notehead::Diamond,
            accidentals: vec![
                AccidentalOverride {
                    pitch: 61,
                    display: AccidentalDisplay::Parenthesized,
                },
                AccidentalOverride {
                    pitch: 65,
                    display: AccidentalDisplay::Hide,
                },
            ],
            tie_pitches: Some(vec![65]),
            tie_direction: crate::slurs::SlurDirection::Above,
            ..EntryStyle::new(anchor)
        };
        assign(&mut score, style.clone()).unwrap();
        let xml = crate::export::musicxml(&score);
        let doc = super::super::parse_xml(&xml).unwrap();
        let notes: Vec<_> = doc
            .descendants()
            .filter(|node| {
                node.has_tag_name("note") && node.children().any(|node| node.has_tag_name("pitch"))
            })
            .collect();
        assert_eq!(notes[0].attribute("print-object"), Some("no"));
        assert_eq!(
            notes[0]
                .children()
                .find(|n| n.has_tag_name("stem"))
                .unwrap()
                .text(),
            Some("down")
        );
        assert!(
            notes[0]
                .descendants()
                .any(|n| n.has_tag_name("accidental") && n.attribute("parentheses") == Some("yes"))
        );
        assert!(!notes[0].children().any(|n| n.has_tag_name("tie")));
        assert!(
            notes[1]
                .children()
                .any(|n| n.has_tag_name("tie") && n.attribute("type") == Some("start"))
        );
        let imported = super::super::musicxml::parse(&doc).unwrap().score;
        assert_eq!(imported.notation.entries, vec![style]);
        imported.validate().unwrap();
    }
    #[test]
    fn foreign_transposing_chord_retains_only_the_tied_concert_pitch() {
        let xml = "<score-partwise><part-list><score-part id='P'><part-name>Clarinet</part-name></score-part></part-list><part id='P'><measure><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time><transpose><diatonic>-1</diatonic><chromatic>-2</chromatic></transpose></attributes><note><pitch><step>D</step><octave>4</octave></pitch><duration>480</duration><type>quarter</type><accidental parentheses='yes'>natural</accidental></note><note><chord/><pitch><step>F</step><alter>1</alter><octave>4</octave></pitch><duration>480</duration><tie type='start'/><type>quarter</type><notations><tied type='start' orientation='under'/></notations></note></measure></part></score-partwise>";
        let doc = roxmltree::Document::parse(xml).unwrap();
        let score = super::super::musicxml::parse(&doc).unwrap().score;
        score.validate().unwrap();
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        assert_eq!(at(&score, &anchor).unwrap().tie_pitches, Some(vec![64]));
        assert_eq!(
            at(&score, &anchor).unwrap().accidental(60),
            AccidentalDisplay::Parenthesized
        );
        assert!(!ties_forward(&score, &anchor, 60));
        assert!(ties_forward(&score, &anchor, 64));
    }
}
