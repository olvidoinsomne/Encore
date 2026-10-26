//! Readers for documented unpitched MusicXML and MuseScore map fields.
use super::{Warnings, child, number, value};
use crate::{
    entry_style::Notehead,
    model::{Clef, Pitch, Step},
    percussion::{Sound, Staff, token},
};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Default)]
pub(super) struct Instruments {
    pub sounds: BTreeMap<String, (String, Option<u8>)>,
    pub kit: u8,
    pub unpitched: bool,
}
impl Instruments {
    pub fn xml(node: Node<'_, '_>) -> Result<Self, String> {
        let mut result = Self::default();
        for instrument in node
            .children()
            .filter(|n| n.has_tag_name("score-instrument"))
        {
            let id = instrument
                .attribute("id")
                .ok_or("Percussion score-instrument has no ID.")?;
            if result.sounds.len() >= 128 {
                return Err("A percussion part can declare at most128 instrument types.".into());
            }
            let midi = node
                .children()
                .find(|n| n.has_tag_name("midi-instrument") && n.attribute("id") == Some(id));
            let key = midi
                .map(|n| number::<u16>(n, "midi-unpitched"))
                .transpose()?
                .flatten();
            if key.is_some_and(|key| !(1..=128).contains(&key)) {
                return Err("MusicXML midi-unpitched must be1–128.".into());
            }
            result.unpitched |=
                key.is_some() || midi.is_some_and(|n| value(n, "midi-channel") == Some("10"));
            if let Some(program) = midi
                .map(|n| number::<u16>(n, "midi-program"))
                .transpose()?
                .flatten()
            {
                if !(1..=128).contains(&program) {
                    return Err("MusicXML percussion program must be1–128.".into());
                }
                if result.sounds.is_empty() {
                    result.kit = (program - 1) as u8;
                } else if result.unpitched && result.kit != (program - 1) as u8 {
                    return Err(
                        "Different kits within one percussion part are not supported.".into(),
                    );
                }
            }
            let name = value(instrument, "instrument-name")
                .unwrap_or(id)
                .to_owned();
            if result
                .sounds
                .insert(id.to_owned(), (name, key.map(|key| (key - 1) as u8)))
                .is_some()
            {
                return Err("Duplicate MusicXML score-instrument identity.".into());
            }
        }
        Ok(result)
    }
}
pub(super) struct Xml {
    instruments: Instruments,
    pub staves: BTreeMap<u32, Staff>,
    pitched: BTreeSet<u32>,
    identities: BTreeMap<(u32, String, i8, NoteheadKey), u8>,
    native: Option<Staff>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct NoteheadKey(u8);
fn head_key(head: Notehead) -> NoteheadKey {
    NoteheadKey(match head {
        Notehead::Normal => 0,
        Notehead::Cross => 1,
        Notehead::Diamond => 2,
        Notehead::Triangle => 3,
        Notehead::Slash => 4,
        Notehead::Square => 5,
    })
}
pub(super) fn notehead(name: &str, warnings: &mut Warnings) -> Notehead {
    match name {
        "normal" | "HEAD_NORMAL" | "0" => Notehead::Normal,
        "x" | "cross" | "HEAD_CROSS" | "1" => Notehead::Cross,
        "diamond" | "HEAD_DIAMOND" | "2" => Notehead::Diamond,
        "triangle" | "HEAD_TRIANGLE_DOWN" | "HEAD_TRIANGLE_UP" => Notehead::Triangle,
        "slash" | "HEAD_SLASH" => Notehead::Slash,
        "square" | "HEAD_SQUARE" => Notehead::Square,
        _ => {
            warnings.add("An unsupported percussion notehead uses a normal head; playback identity is retained.");
            Notehead::Normal
        }
    }
}
impl Xml {
    pub fn new(instruments: Instruments) -> Self {
        Self {
            instruments,
            staves: BTreeMap::new(),
            pitched: BTreeSet::new(),
            identities: BTreeMap::new(),
            native: None,
        }
    }
    pub fn native(&mut self, source: Node<'_, '_>) -> Result<(), String> {
        let markers: Vec<_> = source
            .descendants()
            .filter(|n| n.has_tag_name("other-direction"))
            .filter_map(|n| n.text())
            .filter_map(|text| text.strip_prefix("encore-percussion:"))
            .collect();
        if markers.len() > 1 {
            return Err("Duplicate Encore percussion metadata.".into());
        }
        if let Some(json) = markers.first() {
            if json.len() > 128 * 1024 {
                return Err("Percussion interchange metadata exceeds128KiB.".into());
            }
            let staff: Staff = serde_json::from_str(json)
                .map_err(|e| format!("Invalid Encore percussion map: {e}"))?;
            staff.validate()?;
            self.staves.insert(1, staff.clone());
            self.native = Some(staff);
        }
        Ok(())
    }
    pub fn declare(&mut self, staff: u32, lines: u8) -> Result<(), String> {
        if !(1..=64).contains(&staff) {
            return Err("Percussion source staff number must be1–64.".into());
        }
        if ![1, 5].contains(&lines) {
            return Err("Percussion imports support one or five staff lines.".into());
        }
        let kit = self.instruments.kit;
        let layout = self.staves.entry(staff).or_insert_with(|| Staff {
            kit,
            lines,
            sounds: Vec::new(),
        });
        layout.lines = lines;
        Ok(())
    }
    pub fn pitched(&mut self, staff: u32) -> Result<(), String> {
        if self.staves.contains_key(&staff) {
            return Err("Pitched and unpitched notes cannot share one Encore staff.".into());
        }
        self.pitched.insert(staff);
        Ok(())
    }
    pub fn note(
        &mut self,
        node: Node<'_, '_>,
        staff: u32,
        clef: Clef,
        warnings: &mut Warnings,
    ) -> Result<Option<Pitch>, String> {
        let Some(unpitched) = child(node, "unpitched") else {
            return Ok(None);
        };
        if child(node, "pitch").is_some() {
            return Err(
                "A MusicXML note cannot contain both pitched and unpitched content.".into(),
            );
        }
        if self.pitched.contains(&staff) {
            return Err("Pitched and unpitched notes cannot share one Encore staff.".into());
        }
        let id = child(node, "instrument")
            .and_then(|n| n.attribute("id"))
            .map(str::to_owned)
            .or_else(|| {
                (self.instruments.sounds.len() == 1)
                    .then(|| self.instruments.sounds.keys().next().unwrap().clone())
            })
            .unwrap_or_else(|| "unmapped".into());
        let (name,midi)=self.instruments.sounds.get(&id).cloned().unwrap_or_else(||{
            warnings.add("A percussion instrument has no known MIDI mapping; choose its playback sound in the percussion map.");
            (format!("Unmapped {id}"),None)
        });
        if midi.is_none() {
            warnings.add("Some percussion sounds have no MIDI playback mapping; choose their map before audio or MIDI export.");
        }
        let step = match value(unpitched, "display-step").unwrap_or("B") {
            "C" => Step::C,
            "D" => Step::D,
            "E" => Step::E,
            "F" => Step::F,
            "G" => Step::G,
            "A" => Step::A,
            "B" => Step::B,
            _ => return Err("Invalid unpitched display step.".into()),
        };
        let octave = number::<i8>(unpitched, "display-octave")?.unwrap_or(4);
        let baseline = match clef {
            Clef::Treble => 30,
            Clef::Bass => 18,
            Clef::Alto => 24,
        };
        let position = i32::from(octave) * 7 + step.index() - baseline;
        if !(-16..=24).contains(&position) {
            return Err("Unpitched display position exceeds the supported staff range.".into());
        }
        let head = notehead(value(node, "notehead").unwrap_or("normal"), warnings);
        if let Some(native) = &self.native {
            if staff != 1 {
                return Err(
                    "Encore native percussion maps apply to a single MusicXML staff.".into(),
                );
            }
            let sound_id = id
                .rsplit_once("-S")
                .and_then(|(_, id)| id.parse::<u8>().ok())
                .ok_or("Encore percussion note references an invalid instrument ID.")?;
            let sound = native
                .sound(sound_id)
                .ok_or("Encore percussion note references an absent sound ID.")?;
            if sound.midi != midi || sound.position != position as i8 {
                return Err("Encore percussion metadata conflicts with its standard instrument or display position.".into());
            }
            return Ok(Some(token(sound_id)));
        }
        let key = (staff, id, position as i8, head_key(head));
        if let Some(&id) = self.identities.get(&key) {
            return Ok(Some(token(id)));
        }
        let kit = self.instruments.kit;
        let layout = self.staves.entry(staff).or_insert_with(|| Staff {
            kit,
            lines: 5,
            sounds: Vec::new(),
        });
        let sound_id = midi
            .filter(|id| layout.sound(*id).is_none())
            .or_else(|| (0..=127u8).find(|id| layout.sound(*id).is_none()))
            .ok_or("Percussion staff exceeds128 sound identities.")?;
        let mut label = name.clone();
        let mut suffix = 2;
        while layout
            .sounds
            .iter()
            .any(|sound| sound.name.eq_ignore_ascii_case(&label))
        {
            label = format!("{name} [{suffix}]");
            suffix += 1;
        }
        layout.sounds.push(Sound {
            id: sound_id,
            name: label,
            midi,
            position: position as i8,
            notehead: head,
        });
        self.identities.insert(key, sound_id);
        Ok(Some(token(sound_id)))
    }
    pub fn finish(&mut self, staff: u32) -> Option<Staff> {
        let mut result = self.staves.remove(&staff)?;
        if result.sounds.is_empty() {
            result.sounds.push(Sound {
                id: 0,
                name: "Unmapped percussion".into(),
                midi: None,
                position: 4,
                notehead: Notehead::Normal,
            });
        }
        Some(result)
    }
}
pub(super) fn muse(
    instrument: Node<'_, '_>,
    lines: u8,
    warnings: &mut Warnings,
) -> Result<Staff, String> {
    let program = child(instrument, "Channel")
        .and_then(|n| child(n, "program"))
        .and_then(|n| n.attribute("value"))
        .unwrap_or("0")
        .parse::<u8>()
        .map_err(|_| "Invalid MuseScore drum kit program.")?;
    let mut result = Staff {
        kit: program,
        lines,
        sounds: Vec::new(),
    };
    for drum in instrument.children().filter(|n| n.has_tag_name("Drum")) {
        let id = drum
            .attribute("pitch")
            .ok_or("MuseScore Drum needs a pitch key.")?
            .parse::<u8>()
            .map_err(|_| "Invalid MuseScore drum key.")?;
        let line = number::<i16>(drum, "line")?.unwrap_or(4);
        let position = (i16::from(lines) - 1) * 2 - line;
        if !(-16..=24).contains(&position) {
            return Err("MuseScore drum staff position exceeds the supported range.".into());
        }
        let head = notehead(value(drum, "head").unwrap_or("normal"), warnings);
        result.sounds.push(Sound {
            id,
            name: value(drum, "name")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Drum {id}")),
            midi: Some(id),
            position: position as i8,
            notehead: head,
        });
    }
    if result.sounds.is_empty() {
        result = Staff::general_midi();
        result.kit = program;
        result.lines = lines;
        warnings.add("MuseScore omitted its drum map; the standard General MIDI map was selected. Review custom kit assignments.");
    }
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use crate::{
        model::{Duration, Note, Score},
        percussion::{Staff, token},
    };
    fn load(xml: &str, extension: &str) -> super::super::ImportResult {
        let path = std::env::temp_dir().join(format!(
            "encore-percussion-import-{}-{extension}.{}",
            std::process::id(),
            extension
        ));
        std::fs::write(&path, xml).unwrap();
        let result = crate::import::load(&path);
        let _ = std::fs::remove_file(path);
        result.unwrap()
    }
    #[test]
    fn standard_unpitched_xml_retains_display_heads_chords_and_midi_one_based_keys() {
        let xml = r#"<score-partwise><part-list><score-part id="kit"><part-name>Kit</part-name><score-instrument id="snare"><instrument-name>Snare</instrument-name></score-instrument><score-instrument id="hat"><instrument-name>Hi hat</instrument-name></score-instrument><midi-instrument id="snare"><midi-channel>10</midi-channel><midi-program>1</midi-program><midi-unpitched>39</midi-unpitched></midi-instrument><midi-instrument id="hat"><midi-channel>10</midi-channel><midi-program>1</midi-program><midi-unpitched>43</midi-unpitched></midi-instrument></score-part></part-list><part id="kit"><measure number="1"><attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>percussion</sign></clef></attributes><note><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><duration>480</duration><instrument id="snare"/><voice>1</voice><type>quarter</type></note><note><chord/><unpitched><display-step>G</display-step><display-octave>5</display-octave></unpitched><duration>480</duration><instrument id="hat"/><voice>1</voice><type>quarter</type><notehead>x</notehead></note></measure></part></score-partwise>"#;
        let imported = load(xml, "musicxml").score;
        let staff = imported.parts[0].percussion.as_ref().unwrap();
        assert_eq!(staff.output_key(38).unwrap(), 38);
        assert_eq!(staff.output_key(42).unwrap(), 42);
        assert_eq!(staff.sound(38).unwrap().position, 5);
        assert_eq!(staff.sound(42).unwrap().position, 9);
        assert_eq!(
            staff.sound(42).unwrap().notehead,
            crate::entry_style::Notehead::Cross
        );
        assert_eq!(
            imported.parts[0].measures[0].notes[0].pitches,
            vec![token(38), token(42)]
        );
        assert!(
            imported
                .notation
                .entries
                .iter()
                .all(|style| style.notehead == crate::entry_style::Notehead::Normal)
        );
    }
    #[test]
    fn native_percussion_musicxml_restores_custom_ids_unmapped_sounds_and_grace() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        let mut staff = Staff::general_midi();
        staff.kit = 40;
        staff.lines = 1;
        staff.sounds.retain(|s| [38, 42].contains(&s.id));
        staff.sounds[0].midi = Some(40);
        staff.sounds[0].position = 0;
        staff.sounds[1].midi = None;
        staff.sounds[1].position = 2;
        score.parts[0].percussion = Some(staff.clone());
        score.parts[0].measures[0].notes = vec![Note {
            layer: 0,
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![token(38), token(42)],
            velocity: 80,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        }];
        score.notation.grace.push(crate::grace::GraceGroup::new(
            crate::document::Anchor {
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
                voice: 0,
            },
            vec![crate::grace::GraceNote::new(token(38), Duration::Sixteenth)],
        ));
        score.validate().unwrap();
        let imported = load(&crate::export::musicxml(&score), "xml").score;
        assert_eq!(imported.parts[0].percussion.as_ref().unwrap(), &staff);
        assert_eq!(
            imported.parts[0].measures[0].notes[0].pitches,
            score.parts[0].measures[0].notes[0].pitches
        );
        assert_eq!(imported.notation.grace[0].notes[0].pitches[0], token(38));
    }
    #[test]
    fn musescore_drum_map_keeps_canonical_keys_and_independent_voices() {
        let xml = r#"<museScore version="3.6"><Score><Division>480</Division><Part><Staff id="1"><StaffType group="percussion"><lines>5</lines></StaffType><defaultClef>PERC</defaultClef></Staff><Instrument><useDrumset>1</useDrumset><Channel><program value="0"/></Channel><Drum pitch="38"><head>normal</head><line>3</line><name>Snare</name></Drum><Drum pitch="42"><head>cross</head><line>-1</line><name>Hat</name></Drum></Instrument></Part><Staff id="1"><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><durationType>quarter</durationType><Note><pitch>38</pitch><tpc>16</tpc></Note></Chord></voice><voice><Chord><durationType>quarter</durationType><Note><pitch>42</pitch><tpc>20</tpc></Note></Chord></voice></Measure></Staff></Score></museScore>"#;
        let score = load(xml, "mscx").score;
        let staff = score.parts[0].percussion.as_ref().unwrap();
        assert_eq!(staff.sound(38).unwrap().position, 5);
        assert_eq!(staff.sound(42).unwrap().position, 9);
        let notes = &score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].pitches[0], token(38));
        assert_eq!(notes[1].pitches[0], token(42));
        assert_eq!(notes[1].layer, 1);
    }
}
