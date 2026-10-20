//! Linked parts are definitions over the concert-pitch score, never detached copies.
//! A projection can be regenerated after every source edit without changing the
//! document, its undo history, or its sounding pitches.
use crate::model::{Clef, Instrument, Pitch, Score, Selection, Step};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub use crate::layout::{DocumentLayout as PartLayout, PageSize};

/// Interval from a sounding/concert pitch to the written pitch. The diatonic
/// component preserves note names and key signatures instead of respelling every
/// black key as a sharp. A B-flat clarinet uses +2 semitones, +1 diatonic step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transposition {
    pub semitones: i8,
    pub diatonic_steps: i8,
}

impl Transposition {
    pub const CONCERT: Self = Self {
        semitones: 0,
        diatonic_steps: 0,
    };

    pub fn for_instrument(instrument: Instrument) -> Self {
        let (semitones, diatonic_steps) = match instrument.gm_program() {
            8 => (-12, -7),                   // Celesta sounds an octave above its notation.
            9 => (-24, -14),                  // Glockenspiel sounds two octaves above.
            24..=39 | 43 => (12, 7),          // Guitars and basses sound one octave below.
            56 | 59 | 64 | 66 | 71 => (2, 1), // B-flat trumpet/clarinet/saxophones.
            60 | 69 => (7, 4),                // F horn and English horn.
            65 => (9, 5),                     // E-flat alto saxophone.
            67 => (21, 12),                   // E-flat baritone saxophone.
            72 => (-12, -7),                  // Piccolo sounds an octave above.
            _ => (0, 0),
        };
        // Tenor saxophone sounds a ninth below its written pitch.
        if instrument.gm_program() == 66 {
            Self {
                semitones: 14,
                diatonic_steps: 8,
            }
        } else {
            Self {
                semitones,
                diatonic_steps,
            }
        }
    }

    pub fn validate(self) -> Result<(), String> {
        if !(-36..=36).contains(&self.semitones) || !(-21..=21).contains(&self.diatonic_steps) {
            return Err("Part transposition must be within three octaves.".into());
        }
        Ok(())
    }

    pub fn written_pitch(self, pitch: Pitch) -> Result<Pitch, String> {
        self.apply(pitch, 1)
    }

    pub fn concert_pitch(self, pitch: Pitch) -> Result<Pitch, String> {
        self.apply(pitch, -1)
    }

    fn apply(self, pitch: Pitch, direction: i32) -> Result<Pitch, String> {
        self.validate()?;
        let staff_step = pitch.staff_step() + direction * i32::from(self.diatonic_steps);
        let octave = staff_step.div_euclid(7);
        let step = Step::from_index(staff_step);
        let sounding = (i32::from(pitch.octave) + 1) * 12
            + pitch.step.semitone()
            + i32::from(pitch.alter)
            + direction * i32::from(self.semitones);
        let alter = sounding - ((octave + 1) * 12 + step.semitone());
        if !(0..=127).contains(&sounding) || !(-2..=2).contains(&alter) {
            return Err(format!(
                "{} cannot be represented at this written transposition (MIDI range or accidental limit).",
                pitch.name()
            ));
        }
        Ok(Pitch {
            step,
            octave: octave as i8,
            alter: alter as i8,
        })
    }

    pub fn written_key(self, fifths: i8) -> Result<i8, String> {
        self.validate()?;
        let letter = (i32::from(fifths) * 4 + i32::from(self.diatonic_steps)).rem_euclid(7);
        let pitch_class = (i32::from(fifths) * 7 + i32::from(self.semitones)).rem_euclid(12);
        (-7i8..=7).find(|candidate| {
            (i32::from(*candidate) * 4).rem_euclid(7) == letter
                && (i32::from(*candidate) * 7).rem_euclid(12) == pitch_class
        }).ok_or_else(|| "This written key requires more than seven sharps or flats. Choose concert pitch or another spelled transposition.".into())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkedPart {
    /// Stable identity independent of the display name and ordering.
    pub id: String,
    pub name: String,
    pub staves: Vec<usize>,
    #[serde(default)]
    pub transposition: Transposition,
    #[serde(default)]
    pub layout: PartLayout,
}

impl LinkedPart {
    pub fn validate(&self, staff_count: usize) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err(
                "Linked part identity must contain 1–128 letters, digits, underscores or hyphens."
                    .into(),
            );
        }
        if self.name.trim().is_empty()
            || self.name.len() > 1024
            || self.name.chars().any(char::is_control)
        {
            return Err(
                "Linked part names must contain 1–1024 bytes without control characters.".into(),
            );
        }
        if self.staves.is_empty() || self.staves.len() > crate::model::MAX_PARTS {
            return Err("A linked part must contain at least one staff.".into());
        }
        let mut seen = BTreeSet::new();
        for &staff in &self.staves {
            if staff >= staff_count || !seen.insert(staff) {
                return Err(
                    "Linked parts must reference existing staves without duplicates.".into(),
                );
            }
        }
        self.transposition.validate()?;
        self.layout.validate()
    }
}

pub fn validate_definitions(parts: &[LinkedPart], staff_count: usize) -> Result<(), String> {
    if parts.len() > 128 {
        return Err("A score can contain at most 128 linked part definitions.".into());
    }
    let mut ids = BTreeSet::new();
    for part in parts {
        part.validate(staff_count)?;
        if !ids.insert(&part.id) {
            return Err("Linked part identities must be unique.".into());
        }
    }
    Ok(())
}

/// Propose parts without modifying the score. Consecutive piano treble/bass
/// staves form one grand-staff part; independent instruments keep their identity.
pub fn default_definitions(score: &Score) -> Vec<LinkedPart> {
    let mut result = Vec::new();
    let mut staff = 0;
    while staff < score.parts.len() {
        let first = &score.parts[staff];
        let piano_pair = first.instrument.gm_program() == 0
            && first.clef == Clef::Treble
            && score.parts.get(staff + 1).is_some_and(|second| {
                second.instrument.gm_program() == 0 && second.clef == Clef::Bass
            });
        let staves = if piano_pair {
            vec![staff, staff + 1]
        } else {
            vec![staff]
        };
        result.push(LinkedPart {
            id: format!("part-{}", staff + 1),
            name: if piano_pair {
                "Piano".into()
            } else if first.name.trim().is_empty() {
                first.instrument.name().into()
            } else {
                first.name.clone()
            },
            staves,
            transposition: Transposition::for_instrument(first.instrument),
            layout: PartLayout::default(),
        });
        staff += if piano_pair { 2 } else { 1 };
    }
    result
}

pub fn definitions(score: &Score) -> Vec<LinkedPart> {
    if score.notation.parts.is_empty() {
        default_definitions(score)
    } else {
        score.notation.parts.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Projection {
    pub score: Score,
    /// Displayed staff index -> source score staff index.
    pub source_staves: Vec<usize>,
    pub layout: PartLayout,
    pub transposition: Transposition,
}

impl Projection {
    pub fn source_selection(&self, selection: Selection) -> Option<Selection> {
        Some(Selection {
            part: *self.source_staves.get(selection.part)?,
            ..selection
        })
    }

    pub fn project_selection(&self, selection: Selection) -> Option<Selection> {
        Some(Selection {
            part: self
                .source_staves
                .iter()
                .position(|staff| *staff == selection.part)?,
            ..selection
        })
    }
}

pub fn project(score: &Score, part: &LinkedPart) -> Result<Projection, String> {
    part.validate(score.parts.len())?;
    part.layout.validate_for_score(score.measure_count())?;
    let mut projected = score.clone();
    projected.title = format!("{} — {}", score.title, part.name);
    projected.key_fifths = part.transposition.written_key(score.key_fifths)?;
    projected.parts = part
        .staves
        .iter()
        .map(|&staff| score.parts[staff].clone())
        .collect();
    for staff in &mut projected.parts {
        for measure in &mut staff.measures {
            for note in &mut measure.notes {
                for pitch in &mut note.pitches {
                    *pitch = part.transposition.written_pitch(*pitch)?;
                }
            }
        }
    }
    project_notation(&mut projected, &part.staves, part.transposition)?;
    projected.notation.layout = part.layout.clone();
    projected.validate()?;
    Ok(Projection {
        score: projected,
        source_staves: part.staves.clone(),
        layout: part.layout.clone(),
        transposition: part.transposition,
    })
}

fn project_notation(
    score: &mut Score,
    source_staves: &[usize],
    transposition: Transposition,
) -> Result<(), String> {
    let map = |anchor: &mut crate::document::Anchor| {
        if let Some(index) = source_staves.iter().position(|staff| *staff == anchor.part) {
            anchor.part = index;
            true
        } else {
            false
        }
    };
    score
        .notation
        .lyrics
        .retain_mut(|lyric| map(&mut lyric.anchor) && lyric.extend_to.as_mut().is_none_or(&map));
    score
        .notation
        .slurs
        .retain_mut(|slur| map(&mut slur.start) && map(&mut slur.end));
    score.notation.entries.retain_mut(|entry| {
        if !map(&mut entry.anchor) {
            return false;
        }
        for accidental in &mut entry.accidentals {
            accidental.pitch =
                (i16::from(accidental.pitch) + i16::from(transposition.semitones)) as u8;
        }
        if let Some(ties) = &mut entry.tie_pitches {
            for pitch in ties {
                *pitch = (i16::from(*pitch) + i16::from(transposition.semitones)) as u8;
            }
        }
        true
    });
    score.notation.expressions.retain_mut(|mark| {
        if mark.system {
            mark.anchor.part = 0;
            true
        } else {
            map(&mut mark.anchor)
        }
    });
    score
        .notation
        .spanners
        .retain_mut(|line| map(&mut line.start) && map(&mut line.end));
    score
        .notation
        .chords
        .retain_mut(|symbol| map(&mut symbol.anchor));
    for symbol in &mut score.notation.chords {
        crate::chords::transpose_symbol(symbol, transposition)?;
    }
    score
        .notation
        .grace
        .retain_mut(|group| map(&mut group.anchor));
    for group in &mut score.notation.grace {
        for note in &mut group.notes {
            for pitch in &mut note.pitches {
                *pitch = transposition.written_pitch(*pitch)?;
            }
        }
    }
    score
        .notation
        .lines
        .retain_mut(|line| map(&mut line.start) && map(&mut line.end));
    score.notation.parts.clear();
    Ok(())
}

/// Write one PDF per linked part. Numbered file prefixes prevent collisions
/// when two players share a display name; all projections are prepared before any
/// output is written, so an unsupported transposition cannot leave a partial set.
pub fn export_pdf_parts(score: &Score, directory: &Path) -> Result<Vec<PathBuf>, String> {
    score.validate()?;
    let definitions = definitions(score);
    // Preflight without retaining copies of every part: 128 overlapping custom
    // definitions can each contain the entire source score.
    for part in &definitions {
        let _ = project(score, part)?;
    }
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Cannot create parts directory: {error}"))?;
    let mut paths = Vec::new();
    for (index, definition) in definitions.iter().enumerate() {
        let projection = project(score, definition)?;
        let name = safe_name(&definition.name);
        let path = directory.join(format!("{:02}-{name}.pdf", index + 1));
        let pages = crate::engraving::engrave(
            &projection.score,
            &crate::engraving::EngravingOptions {
                layout: Some(projection.layout),
                ..Default::default()
            },
        );
        std::fs::write(&path, crate::engraving::pdf(&pages))
            .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
        paths.push(path);
    }
    Ok(paths)
}

fn safe_name(name: &str) -> String {
    let name = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .scan(0usize, |bytes, character| {
            *bytes += character.len_utf8();
            (*bytes <= 180).then_some(character)
        })
        .take(96)
        .collect::<String>();
    let name = name.trim().trim_matches('.');
    if name.is_empty() {
        "Part".into()
    } else {
        name.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Anchor;
    use crate::lyrics::Lyric;
    use crate::slurs::Slur;

    #[test]
    fn generates_grand_staff_and_individual_instruments_without_edits() {
        let score = Score::demo();
        let before = score.clone();
        let definitions = definitions(&score);
        assert_eq!(definitions.len(), score.parts.len() - 1);
        assert_eq!(definitions[0].staves, vec![0, 1]);
        assert_eq!(definitions[0].name, "Piano");
        assert_eq!(definitions[1].staves, vec![2]);
        assert_eq!(score, before);
    }

    #[test]
    fn independent_piano_treble_staves_do_not_form_grand_staff() {
        let mut score = Score::blank();
        score.parts[1].clef = Clef::Treble;
        assert_eq!(definitions(&score).len(), 2);
    }

    #[test]
    fn native_roundtrip_keeps_part_ids_transposition_and_independent_page_settings() {
        let mut score = Score::demo();
        score.notation.parts = default_definitions(&score);
        score.notation.parts[0].id = "keyboard-performer".into();
        score.notation.parts[0].layout.page_size = PageSize::Legal;
        score.notation.parts[0].layout.measures_per_system = 3;
        let serialized = serde_json::to_vec(&score).unwrap();
        let restored: Score = serde_json::from_slice(&serialized).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.notation.parts, score.notation.parts);
        assert_eq!(
            project(&restored, &restored.notation.parts[0])
                .unwrap()
                .layout
                .dimensions(),
            (850.0, 1400.0)
        );
        assert_eq!(
            restored.notation.parts.last().unwrap().transposition,
            Transposition::for_instrument(Instrument::FrenchHorn)
        );
    }

    #[test]
    fn written_clarinet_spells_notes_and_key_and_reverses_exactly() {
        let mut score = Score::demo();
        score.parts[2].instrument = Instrument::Clarinet;
        score.key_fifths = -3;
        let mut part = default_definitions(&score)[1].clone();
        part.transposition = Transposition::for_instrument(Instrument::Clarinet);
        let projection = project(&score, &part).unwrap();
        assert_eq!(projection.score.key_fifths, -1);
        for (concert, written) in score.parts[2]
            .measures
            .iter()
            .zip(&projection.score.parts[0].measures)
        {
            for (concert, written) in concert.notes.iter().zip(&written.notes) {
                for (&concert, &written) in concert.pitches.iter().zip(&written.pitches) {
                    assert_eq!(written.midi(), concert.midi() + 2);
                    assert_eq!(part.transposition.concert_pitch(written).unwrap(), concert);
                }
            }
        }
        let flat = Pitch {
            step: Step::E,
            alter: -1,
            octave: 4,
        };
        assert_eq!(
            part.transposition.written_pitch(flat).unwrap(),
            Pitch {
                step: Step::F,
                alter: 0,
                octave: 4
            }
        );
    }

    #[test]
    fn projection_reads_current_source_and_never_changes_source() {
        let mut score = Score::demo();
        let part = default_definitions(&score)[1].clone();
        let before = score.clone();
        let first = project(&score, &part).unwrap();
        assert_eq!(score, before);
        score.parts[2].measures[0].notes[0].velocity = 111;
        let second = project(&score, &part).unwrap();
        assert_ne!(first.score.parts[0].measures[0].notes[0].velocity, 111);
        assert_eq!(second.score.parts[0].measures[0].notes[0].velocity, 111);
    }

    #[test]
    fn remaps_lyrics_extensions_slurs_and_selections_for_reordered_staves() {
        let mut score = Score::demo();
        let first = &score.parts[2].measures[0].notes[0];
        let second = &score.parts[2].measures[1].notes[0];
        let a = Anchor {
            part: 2,
            measure: 0,
            tick: first.start,
            layer: first.layer,
        };
        let b = Anchor {
            part: 2,
            measure: 1,
            tick: second.start,
            layer: second.layer,
        };
        let mut lyric = Lyric::new(a, "Sing");
        lyric.extend_to = Some(b);
        score.notation.lyrics.push(lyric);
        score.notation.slurs.push(Slur::new(a, b));
        let other = Anchor { part: 0, ..a };
        score.notation.lyrics.push(Lyric::new(other, "Excluded"));
        let mut definition = default_definitions(&score)[1].clone();
        definition.staves = vec![3, 2];
        let projection = project(&score, &definition).unwrap();
        assert_eq!(projection.score.notation.lyrics.len(), 1);
        assert_eq!(projection.score.notation.lyrics[0].anchor.part, 1);
        assert_eq!(
            projection.score.notation.lyrics[0].extend_to.unwrap().part,
            1
        );
        assert_eq!(projection.score.notation.slurs[0].start.part, 1);
        assert_eq!(projection.score.notation.slurs[0].end.part, 1);
        let selection = Selection {
            part: 2,
            measure: 1,
            note: 0,
        };
        assert_eq!(projection.project_selection(selection).unwrap().part, 1);
        assert_eq!(
            projection.source_selection(projection.project_selection(selection).unwrap()),
            Some(selection)
        );
        assert!(
            projection
                .project_selection(Selection {
                    part: 0,
                    ..selection
                })
                .is_none()
        );
    }

    #[test]
    fn explicit_parts_keep_identity_and_independent_layout() {
        let mut score = Score::demo();
        score.notation.parts = default_definitions(&score);
        let identity = score.notation.parts[0].id.clone();
        score.notation.parts[0].name = "Keyboard player".into();
        score.notation.parts[0].layout.page_size = PageSize::A4;
        score.notation.parts[0].layout.landscape = true;
        score.notation.parts[0].layout.measures_per_system = 2;
        let parts = definitions(&score);
        assert_eq!(parts[0].id, identity);
        assert_eq!(parts[1].layout, PartLayout::default());
        let projection = project(&score, &parts[0]).unwrap();
        assert_eq!(projection.layout.dimensions(), (1169.0, 827.0));
        assert!(projection.score.notation.parts.is_empty());
        let pages = crate::engraving::engrave(
            &projection.score,
            &crate::engraving::EngravingOptions {
                layout: Some(projection.layout),
                ..Default::default()
            },
        );
        assert_eq!((pages[0].width, pages[0].height), (1169.0, 827.0));
        let pdf = crate::engraving::pdf(&pages);
        assert!(String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 841.680 595.440]"));
        let measures_in_first_system = pages[0]
            .staves
            .iter()
            .filter(|staff| staff.part == 0 && staff.y == pages[0].staves[0].y)
            .count();
        assert!(measures_in_first_system <= 2);
    }

    #[test]
    fn rejects_invalid_definitions_and_nonfinite_layout() {
        let score = Score::demo();
        let mut parts = default_definitions(&score);
        parts[0].staves = vec![0, 0];
        assert!(validate_definitions(&parts, score.parts.len()).is_err());
        parts[0].staves = vec![score.parts.len()];
        assert!(validate_definitions(&parts, score.parts.len()).is_err());
        parts[0].staves = vec![0];
        parts[0].layout.staff_gap = f32::NAN;
        assert!(validate_definitions(&parts, score.parts.len()).is_err());
        parts[0].layout.staff_gap = 0.0;
        parts[1].id = parts[0].id.clone();
        assert!(validate_definitions(&parts, score.parts.len()).is_err());
    }

    #[test]
    fn rejects_unrepresentable_pitch_and_key_instead_of_clamping() {
        let clarinet = Transposition::for_instrument(Instrument::Clarinet);
        assert!(clarinet.written_pitch(Pitch::from_midi(127)).is_err());
        assert!(clarinet.written_key(7).is_err());
        assert_eq!(
            Transposition::for_instrument(Instrument::DoubleBass)
                .written_pitch(Pitch::from_midi(28))
                .unwrap()
                .midi(),
            40
        );
        assert_eq!(
            Transposition::for_instrument(Instrument::Piccolo)
                .written_pitch(Pitch::from_midi(84))
                .unwrap()
                .midi(),
            72
        );
        assert_eq!(
            Transposition::for_instrument(Instrument::GeneralMidi(66))
                .written_pitch(Pitch::from_midi(48))
                .unwrap()
                .midi(),
            62
        );
    }

    #[test]
    fn invalid_batch_transposition_does_not_create_partial_outputs() {
        let mut score = Score::demo();
        score.notation.parts = default_definitions(&score);
        score.notation.parts[1].transposition = Transposition {
            semitones: 1,
            diatonic_steps: 0,
        };
        let directory =
            std::env::temp_dir().join(format!("encore-parts-invalid-{}", std::process::id()));
        assert!(export_pdf_parts(&score, &directory).is_err());
        assert!(!directory.exists());
    }

    #[test]
    fn batch_pdf_names_are_distinct_and_do_not_escape_directory() {
        let mut score = Score::demo();
        score.notation.parts = default_definitions(&score);
        score.notation.parts[0].name = "../Same / name".into();
        score.notation.parts[1].name = "../Same / name".into();
        score.notation.parts[2].name = "弦".repeat(100);
        let directory = std::env::temp_dir().join(format!("encore-parts-{}", std::process::id()));
        let paths = export_pdf_parts(&score, &directory).unwrap();
        assert_eq!(paths.len(), score.notation.parts.len());
        assert_ne!(paths[0], paths[1]);
        for path in paths {
            assert_eq!(path.parent(), Some(directory.as_path()));
            assert!(path.file_name().unwrap().to_string_lossy().len() < 255);
            assert!(std::fs::read(path).unwrap().starts_with(b"%PDF-"));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
mod notation_projection_tests {
    use super::*;
    use crate::{
        document::Anchor,
        entry_style::{AccidentalDisplay, AccidentalOverride, EntryStyle},
        expressions::{Expression, Hairpin},
    };
    #[test]
    fn new_anchored_notation_and_pitch_overrides_follow_written_parts() {
        let mut score = Score::demo();
        let start = Anchor {
            part: 2,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor {
            part: 2,
            measure: 1,
            tick: 0,
            layer: 0,
        };
        let pitch = score.parts[2].measures[0].notes[0].pitches[0].midi();
        let mut style = EntryStyle::new(start);
        style.accidentals.push(AccidentalOverride {
            pitch,
            display: AccidentalDisplay::Parenthesized,
        });
        style.tie_pitches = Some(vec![pitch]);
        score.notation.entries.push(style);
        score
            .notation
            .expressions
            .push(Expression::text(start, "cantabile"));
        score.notation.spanners.push(Hairpin::new(start, end));
        let part = LinkedPart {
            id: "test".into(),
            name: "Test".into(),
            staves: vec![2],
            transposition: Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
            layout: PartLayout {
                staff_scale: 0.8,
                ..Default::default()
            },
        };
        let projected = project(&score, &part).unwrap();
        assert_eq!(projected.score.notation.entries[0].anchor.part, 0);
        assert_eq!(
            projected.score.notation.entries[0].accidentals[0].pitch,
            pitch + 2
        );
        assert_eq!(
            projected.score.notation.entries[0].tie_pitches,
            Some(vec![pitch + 2])
        );
        assert_eq!(projected.score.notation.expressions[0].anchor.part, 0);
        assert_eq!(projected.score.notation.spanners[0].end.part, 0);
        assert_eq!(projected.score.notation.layout.staff_scale, 0.8);
        assert_eq!(score.notation.layout.staff_scale, 1.0);
    }
}

#[cfg(test)]
mod new_metadata_tests {
    use super::*;
    use crate::{
        chords::{self, ChordSymbol},
        document::Anchor,
        grace::{GraceGroup, GraceNote},
        lines::{LineKind, PerformanceLine},
        model::Duration,
    };
    #[test]
    fn chord_grace_and_line_projection_transposes_only_pitched_metadata() {
        let mut score = Score::demo();
        let at = Anchor {
            part: 2,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let mut chord =
            ChordSymbol::new(Anchor { tick: 240, ..at }, chords::parse("C7/E").unwrap());
        chord.fretboard = Some(chords::Fretboard::guitar(vec![
            None,
            Some(3),
            Some(2),
            Some(3),
            Some(1),
            Some(0),
        ]));
        score.notation.chords.push(chord);
        score.notation.grace.push(GraceGroup::new(
            at,
            vec![GraceNote::new(Pitch::from_midi(60), Duration::Sixteenth)],
        ));
        score.notation.lines.push(PerformanceLine::new(
            at,
            Anchor { measure: 1, ..at },
            LineKind::Ottava { semitones: 12 },
        ));
        score.validate().unwrap();
        let before = score.clone();
        let part = LinkedPart {
            id: "solo".into(),
            name: "Clarinet".into(),
            staves: vec![2],
            transposition: Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
            layout: Default::default(),
        };
        let projection = project(&score, &part).unwrap();
        let projected = &projection.score.notation;
        assert_eq!(projected.chords[0].chord.label(), "D7/F#");
        assert_eq!(projected.chords[0].anchor.part, 0);
        assert_eq!(
            projected.chords[0].fretboard.as_ref().unwrap().frets[1],
            Some(5)
        );
        assert_eq!(projected.grace[0].notes[0].pitches[0].midi(), 62);
        assert_eq!(projected.lines[0].start.part, 0);
        assert_eq!(projected.lines[0].kind, LineKind::Ottava { semitones: 12 });
        assert_eq!(score, before);
    }
}
