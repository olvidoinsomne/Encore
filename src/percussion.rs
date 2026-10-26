//! Unpitched percussion sound identities, independent notation and playback maps.
use crate::{
    document::Anchor,
    entry_style::Notehead,
    model::{Note, Pitch, Score, Step},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sound {
    pub id: u8,
    pub name: String,
    pub midi: Option<u8>,
    /// Diatonic half-spaces above E4, the bottom (or sole) staff line.
    pub position: i8,
    pub notehead: Notehead,
}
impl Sound {
    pub fn display_pitch(&self) -> Pitch {
        let step = 30 + i32::from(self.position); // E4 in diatonic staff steps.
        Pitch {
            step: Step::from_index(step),
            octave: step.div_euclid(7) as i8,
            alter: 0,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Staff {
    pub kit: u8,
    pub lines: u8,
    pub sounds: Vec<Sound>,
}
impl Staff {
    pub fn sound(&self, id: u8) -> Option<&Sound> {
        self.sounds.iter().find(|sound| sound.id == id)
    }
    pub fn output_key(&self, id: u8) -> Result<u8, String> {
        let sound = self
            .sound(id)
            .ok_or_else(|| format!("Percussion sound ID {id} is absent from the staff map."))?;
        sound.midi.ok_or_else(|| {
            format!(
                "Percussion sound {} has no playback MIDI mapping.",
                sound.name
            )
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.kit > 127
            || ![1, 5].contains(&self.lines)
            || self.sounds.is_empty()
            || self.sounds.len() > 128
        {
            return Err(
                "A percussion staff needs kit 0–127, one or five staff lines, and 1–128 sounds."
                    .into(),
            );
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for sound in &self.sounds {
            if sound.id > 127
                || sound.midi.is_some_and(|midi| midi > 127)
                || !(-16..=24).contains(&sound.position)
                || sound.name.trim().is_empty()
                || sound.name.len() > 256
                || sound.name.chars().any(char::is_control)
                || !ids.insert(sound.id)
                || !names.insert(sound.name.trim().to_lowercase())
            {
                return Err("Percussion sounds need unique IDs 0–127 and names, MIDI keys 0–127 or unmapped, and staff positions −16..24.".into());
            }
        }
        Ok(())
    }
    pub fn general_midi() -> Self {
        Self {
            kit: 0,
            lines: 5,
            sounds: GM_NAMES
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let id = index as u8 + 35;
                    let (position, notehead) = default_notation(id);
                    Sound {
                        id,
                        name: (*name).into(),
                        midi: Some(id),
                        position,
                        notehead,
                    }
                })
                .collect(),
        }
    }
}
impl Default for Staff {
    fn default() -> Self {
        Self::general_midi()
    }
}
pub fn token(id: u8) -> Pitch {
    Pitch::from_midi(id)
}
pub fn token_id(pitch: Pitch) -> Result<u8, String> {
    let id = pitch.midi();
    if pitch != token(id) {
        return Err("Unpitched entries must use canonical percussion sound-ID tokens.".into());
    }
    Ok(id)
}
fn validate_note(staff: &Staff, note: &Note) -> Result<(), String> {
    for pitch in &note.pitches {
        let id = token_id(*pitch)?;
        if staff.sound(id).is_none() {
            return Err(format!(
                "Percussion sound ID {id} is not in this staff's map."
            ));
        }
    }
    Ok(())
}
pub fn validate(score: &Score) -> Result<(), String> {
    for (part, source) in score.parts.iter().enumerate() {
        let Some(staff) = &source.percussion else {
            continue;
        };
        staff.validate()?;
        for measure in &source.measures {
            for note in &measure.notes {
                validate_note(staff, note)?;
            }
        }
        for group in score
            .notation
            .grace
            .iter()
            .filter(|group| group.anchor.part == part)
        {
            for note in &group.notes {
                for pitch in &note.pitches {
                    let id = token_id(*pitch)?;
                    if staff.sound(id).is_none() {
                        return Err(format!(
                            "Grace percussion sound ID {id} is absent from its staff map."
                        ));
                    }
                }
            }
        }
        if score
            .notation
            .ornaments
            .iter()
            .any(|mark| mark.anchor.part == part)
            || score
                .notation
                .lines
                .iter()
                .any(|line| line.start.part == part)
            || score
                .notation
                .chords
                .iter()
                .any(|chord| chord.anchor.part == part && chord.play)
        {
            return Err("Pitch ornaments, octave/glissando/trill/pedal lines and playable harmony are unavailable on unpitched percussion staves.".into());
        }
        for definition in &score.notation.parts {
            if definition
                .voicing
                .iter()
                .any(|rule| rule.staff == part && rule.chord != crate::parts::ChordFilter::All)
            {
                return Err("Unpitched percussion supports layer/voice voicing; chord pitch ranking does not apply to sound IDs.".into());
            }
        }
    }
    Ok(())
}
/// Check the used sound identities across a clipboard boundary. Unused map
/// differences and staff/kit formatting never change a copied sound's meaning.
pub fn compatible(
    source: Option<&Staff>,
    destination: Option<&Staff>,
    ids: impl IntoIterator<Item = u8>,
) -> Result<(), String> {
    match (source, destination) {
        (None, None) => Ok(()),
        (Some(source), Some(destination)) => {
            for id in ids {
                let a = source
                    .sound(id)
                    .ok_or("Clipboard refers to an unknown percussion sound.")?;
                let b = destination
                    .sound(id)
                    .ok_or("Destination percussion map lacks a copied sound ID.")?;
                if a.name.trim().to_lowercase() != b.name.trim().to_lowercase() || a.midi != b.midi
                {
                    return Err("Copied percussion sound IDs have different names or playback mappings in the destination.".into());
                }
            }
            Ok(())
        }
        _ => Err(
            "Pitched and unpitched percussion music cannot be pasted into each other's staves."
                .into(),
        ),
    }
}
pub fn anchor_is_percussion(score: &Score, anchor: Anchor) -> bool {
    score
        .parts
        .get(anchor.part)
        .is_some_and(|part| part.percussion.is_some())
}
fn default_notation(id: u8) -> (i8, Notehead) {
    match id {
        35 | 36 => (1, Notehead::Normal),
        37 => (5, Notehead::Cross),
        38 | 40 => (5, Notehead::Normal),
        39 => (6, Notehead::Cross),
        41 => (2, Notehead::Normal),
        43 => (3, Notehead::Normal),
        42 | 46 => (9, Notehead::Cross),
        44 => (-1, Notehead::Cross),
        45 => (4, Notehead::Normal),
        47 => (6, Notehead::Normal),
        48 => (7, Notehead::Normal),
        50 => (8, Notehead::Normal),
        49 | 52 | 55 | 57 => (10, Notehead::Cross),
        51 | 53 | 59 => (8, Notehead::Cross),
        54 => (7, Notehead::Cross),
        56 => (6, Notehead::Triangle),
        _ => (4, Notehead::Normal),
    }
}
const GM_NAMES: [&str; 47] = [
    "Acoustic bass drum",
    "Bass drum 1",
    "Side stick",
    "Acoustic snare",
    "Hand clap",
    "Electric snare",
    "Low floor tom",
    "Closed hi-hat",
    "High floor tom",
    "Pedal hi-hat",
    "Low tom",
    "Open hi-hat",
    "Low-mid tom",
    "Hi-mid tom",
    "Crash cymbal 1",
    "High tom",
    "Ride cymbal 1",
    "Chinese cymbal",
    "Ride bell",
    "Tambourine",
    "Splash cymbal",
    "Cowbell",
    "Crash cymbal 2",
    "Vibraslap",
    "Ride cymbal 2",
    "Hi bongo",
    "Low bongo",
    "Mute hi conga",
    "Open hi conga",
    "Low conga",
    "High timbale",
    "Low timbale",
    "High agogo",
    "Low agogo",
    "Cabasa",
    "Maracas",
    "Short whistle",
    "Long whistle",
    "Short guiro",
    "Long guiro",
    "Claves",
    "Hi wood block",
    "Low wood block",
    "Mute cuica",
    "Open cuica",
    "Mute triangle",
    "Open triangle",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entry_style::{AccidentalDisplay, AccidentalOverride, EntryStyle},
        grace::{GraceGroup, GraceNote},
        model::{Duration, Note},
        parts::{self, ChordFilter, StaffVoicing, Transposition},
    };
    fn hit(start: u32, ids: &[u8]) -> Note {
        Note {
            layer: 0,
            voice: 0,
            voice_launch: None,
            start,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: ids.iter().copied().map(token).collect(),
            velocity: 90,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        }
    }
    fn score() -> Score {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].percussion = Some(Staff::general_midi());
        score.parts[0].measures[0].notes = vec![hit(0, &[38, 42])];
        score
    }
    #[test]
    fn standard_map_canonical_tokens_display_and_unmapped_audio_are_distinct() {
        let mut staff = Staff::general_midi();
        staff.validate().unwrap();
        assert_eq!(staff.sounds.len(), 47);
        assert_eq!(staff.output_key(38).unwrap(), 38);
        assert_eq!(token_id(token(38)).unwrap(), 38);
        let sound = staff
            .sounds
            .iter_mut()
            .find(|sound| sound.id == 38)
            .unwrap();
        sound.midi = Some(40);
        sound.position = 0;
        assert_eq!(
            sound.display_pitch(),
            Pitch {
                step: Step::E,
                octave: 4,
                alter: 0
            }
        );
        assert_eq!(staff.output_key(38).unwrap(), 40);
        assert_eq!(token(38).midi(), 38);
        staff.sounds[0].midi = None;
        staff.validate().unwrap();
        assert!(staff.output_key(35).is_err());
        let mut enharmonic = token(38);
        enharmonic.step = Step::C;
        enharmonic.alter = 2;
        assert!(token_id(enharmonic).is_err());
    }
    #[test]
    fn pitched_legacy_documents_and_empty_drum_staff_native_roundtrip() {
        let blank = Score::blank();
        let json = serde_json::to_string(&blank).unwrap();
        assert!(!json.contains("percussion"));
        let restored: Score = serde_json::from_str(&json).unwrap();
        assert_eq!(blank, restored);
        let score = score();
        let json = serde_json::to_vec(&score).unwrap();
        let restored: Score = serde_json::from_slice(&json).unwrap();
        restored.validate().unwrap();
        assert_eq!(score, restored);
    }
    #[test]
    fn bad_maps_unknown_hits_and_melodic_attachments_fail_validation() {
        let original = score();
        original.validate().unwrap();
        let mut duplicate = original.clone();
        duplicate.parts[0].percussion.as_mut().unwrap().sounds[1].id = 35;
        assert!(duplicate.validate().is_err());
        let mut unknown = original.clone();
        unknown.parts[0].measures[0].notes[0].pitches.push(token(1));
        assert!(unknown.validate().is_err());
        let mut ornament = original.clone();
        ornament
            .notation
            .ornaments
            .push(crate::ornaments::Ornament::new(
                Anchor {
                    part: 0,
                    measure: 0,
                    tick: 0,
                    layer: 0,
                    voice: 0,
                },
                crate::ornaments::OrnamentKind::ArpeggioUp,
            ));
        assert!(ornament.validate().is_err());
        let mut ranked = original.clone();
        let mut def = parts::default_definitions(&ranked).remove(0);
        let mut rule = StaffVoicing::new(0);
        rule.chord = ChordFilter::Highest;
        def.voicing.push(rule);
        assert!(parts::project(&ranked, &def).is_err());
        ranked.notation.parts.push(def);
        assert!(ranked.validate().is_err());
    }
    #[test]
    fn part_and_region_transposition_preserve_principal_grace_and_style_tokens() {
        let mut source = score();
        source.parts.push(Score::blank().parts.remove(0));
        source.parts[1].measures[0].notes = vec![hit(0, &[60])];
        let at = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        source.notation.grace.push(GraceGroup::new(
            at,
            vec![GraceNote::new(token(38), Duration::Sixteenth)],
        ));
        let mut style = EntryStyle::new(at);
        style.accidentals.push(AccidentalOverride {
            pitch: 38,
            display: AccidentalDisplay::Hide,
        });
        style.tie_pitches = Some(vec![38]);
        source.notation.entries.push(style);
        let mut def = parts::default_definitions(&source).remove(0);
        def.staves = vec![0, 1];
        def.transposition = Transposition {
            semitones: 2,
            diatonic_steps: 1,
        };
        let projected = parts::project(&source, &def).unwrap().score;
        assert_eq!(
            projected.parts[0].measures[0].notes[0].pitches,
            vec![token(38), token(42)]
        );
        assert_eq!(
            projected.parts[1].measures[0].notes[0].pitches[0].midi(),
            62
        );
        assert_eq!(projected.notation.grace[0].notes[0].pitches[0], token(38));
        assert_eq!(projected.notation.entries[0].accidentals[0].pitch, 38);
        let region = crate::editing::Region::all(&source);
        crate::editing::transpose_region(
            &mut source,
            &region,
            Transposition {
                semitones: 12,
                diatonic_steps: 7,
            },
        )
        .unwrap();
        assert_eq!(
            source.parts[0].measures[0].notes[0].pitches,
            vec![token(38), token(42)]
        );
        assert_eq!(source.parts[1].measures[0].notes[0].pitches[0].midi(), 72);
        assert_eq!(source.notation.grace[0].notes[0].pitches[0], token(38));
    }
    #[test]
    fn region_clipboard_enforces_music_domain_and_used_map_compatibility_atomically() {
        let mut source = score();
        let region = crate::editing::Region {
            staves: vec![0],
            start: 0,
            end: 0,
        };
        let clip = crate::editing::copy_region(&source, &region).unwrap();
        assert_eq!(clip.version, 4);
        let clip = crate::editing::RegionClip::decode(&clip.encode().unwrap()).unwrap();
        assert!(clip.percussion[0].is_some());
        let mut pitched = Score::blank();
        let before = pitched.clone();
        assert!(
            crate::editing::paste_region(
                &mut pitched,
                &clip,
                &[0],
                0,
                crate::editing::PasteMode::Replace
            )
            .is_err()
        );
        assert_eq!(pitched, before);
        source.parts[0]
            .percussion
            .as_mut()
            .unwrap()
            .sounds
            .iter_mut()
            .find(|sound| sound.id == 38)
            .unwrap()
            .midi = Some(40);
        let before = source.clone();
        assert!(
            crate::editing::paste_region(
                &mut source,
                &clip,
                &[0],
                0,
                crate::editing::PasteMode::Replace
            )
            .is_err()
        );
        assert_eq!(source, before);
        let mut compatible = score();
        compatible.parts[0]
            .percussion
            .as_mut()
            .unwrap()
            .sounds
            .iter_mut()
            .find(|sound| sound.id == 38)
            .unwrap()
            .position = 9;
        crate::editing::paste_region(
            &mut compatible,
            &clip,
            &[0],
            1,
            crate::editing::PasteMode::Replace,
        )
        .unwrap();
        assert_eq!(
            compatible.parts[0].measures[1].notes[0].pitches,
            vec![token(38), token(42)]
        );
        let legacy = crate::editing::copy_region(&Score::blank(), &region).unwrap();
        let before = compatible.clone();
        assert!(
            crate::editing::paste_region(
                &mut compatible,
                &legacy,
                &[0],
                0,
                crate::editing::PasteMode::Replace
            )
            .is_err()
        );
        assert_eq!(compatible, before);
    }
    #[test]
    fn silent_harmony_transposes_while_percussion_tokens_and_keys_stay_fixed() {
        let mut score = score();
        score.key_fifths = 7;
        let mut chord = crate::chords::ChordSymbol::new(
            Anchor {
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
                voice: 0,
            },
            crate::chords::parse("C7").unwrap(),
        );
        chord.play = false;
        score.notation.chords.push(chord);
        let mut definition = parts::default_definitions(&score).remove(0);
        definition.transposition = Transposition {
            semitones: 2,
            diatonic_steps: 1,
        };
        let projected = parts::project(&score, &definition).unwrap().score;
        assert_eq!(projected.key_fifths, 7);
        assert_eq!(projected.notation.chords[0].chord.label(), "D7");
        assert_eq!(
            projected.parts[0].measures[0].notes[0].pitches,
            vec![token(38), token(42)]
        );
        let region = crate::editing::Region::all(&score);
        crate::editing::transpose_region(
            &mut score,
            &region,
            Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
        )
        .unwrap();
        assert_eq!(score.notation.chords[0].chord.label(), "D7");
        assert_eq!(
            score.parts[0].measures[0].notes[0].pitches,
            vec![token(38), token(42)]
        );
    }
}
