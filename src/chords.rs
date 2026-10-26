//! Original, spelled chord symbols and editable fretboard geometry.
//! Chords attach to staff beats rather than notes and remain in concert pitch.
use crate::document::Anchor;
use crate::model::{Pitch, Score, Step};
use crate::parts::Transposition;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod frame;
pub mod musicxml;
pub use musicxml::musicxml;
mod parser;
pub use frame::{Barre, Fretboard, suggest_fretboards};
pub use parser::parse;

pub const MAX_CHORDS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PitchClass {
    pub step: Step,
    pub alter: i8,
}
impl PitchClass {
    pub fn pitch(self) -> Pitch {
        Pitch {
            step: self.step,
            octave: 4,
            alter: self.alter,
        }
    }
    pub fn from_pitch(pitch: Pitch) -> Self {
        Self {
            step: pitch.step,
            alter: pitch.alter,
        }
    }
    pub fn midi_class(self) -> u8 {
        (self.step.semitone() + i32::from(self.alter)).rem_euclid(12) as u8
    }
    pub fn label(self) -> String {
        format!(
            "{}{}",
            self.step.name(),
            match self.alter {
                -2 => "bb",
                -1 => "b",
                0 => "",
                1 => "#",
                2 => "##",
                _ => "?",
            }
        )
    }
    pub fn transposed(self, interval: Transposition) -> Result<Self, String> {
        interval.written_pitch(self.pitch()).map(Self::from_pitch)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChordKind {
    #[default]
    Major,
    Minor,
    Augmented,
    Diminished,
    Dominant,
    MajorSeventh,
    MinorSeventh,
    DiminishedSeventh,
    HalfDiminished,
    MajorMinor,
    AugmentedSeventh,
    MajorSixth,
    MinorSixth,
    DominantNinth,
    MajorNinth,
    MinorNinth,
    Dominant11th,
    Major11th,
    Minor11th,
    Dominant13th,
    Major13th,
    Minor13th,
    SuspendedSecond,
    SuspendedFourth,
    Power,
    Neapolitan,
    Italian,
    French,
    German,
    Tristan,
    Pedal,
    None,
    Other,
}
impl ChordKind {
    pub fn xml(self) -> &'static str {
        use ChordKind::*;
        match self {
            Major => "major",
            Minor => "minor",
            Augmented => "augmented",
            Diminished => "diminished",
            Dominant => "dominant",
            MajorSeventh => "major-seventh",
            MinorSeventh => "minor-seventh",
            DiminishedSeventh => "diminished-seventh",
            HalfDiminished => "half-diminished",
            MajorMinor => "major-minor",
            AugmentedSeventh => "augmented-seventh",
            MajorSixth => "major-sixth",
            MinorSixth => "minor-sixth",
            DominantNinth => "dominant-ninth",
            MajorNinth => "major-ninth",
            MinorNinth => "minor-ninth",
            Dominant11th => "dominant-11th",
            Major11th => "major-11th",
            Minor11th => "minor-11th",
            Dominant13th => "dominant-13th",
            Major13th => "major-13th",
            Minor13th => "minor-13th",
            SuspendedSecond => "suspended-second",
            SuspendedFourth => "suspended-fourth",
            Power => "power",
            Neapolitan => "Neapolitan",
            Italian => "Italian",
            French => "French",
            German => "German",
            Tristan => "Tristan",
            Pedal => "pedal",
            None => "none",
            Other => "other",
        }
    }
    pub fn from_xml(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.xml() == value)
    }
    pub const ALL: [Self; 33] = [
        Self::Major,
        Self::Minor,
        Self::Augmented,
        Self::Diminished,
        Self::Dominant,
        Self::MajorSeventh,
        Self::MinorSeventh,
        Self::DiminishedSeventh,
        Self::HalfDiminished,
        Self::MajorMinor,
        Self::AugmentedSeventh,
        Self::MajorSixth,
        Self::MinorSixth,
        Self::DominantNinth,
        Self::MajorNinth,
        Self::MinorNinth,
        Self::Dominant11th,
        Self::Major11th,
        Self::Minor11th,
        Self::Dominant13th,
        Self::Major13th,
        Self::Minor13th,
        Self::SuspendedSecond,
        Self::SuspendedFourth,
        Self::Power,
        Self::Neapolitan,
        Self::Italian,
        Self::French,
        Self::German,
        Self::Tristan,
        Self::Pedal,
        Self::None,
        Self::Other,
    ];
    pub fn suffix(self) -> &'static str {
        use ChordKind::*;
        match self {
            Major => "",
            Minor => "m",
            Augmented => "aug",
            Diminished => "dim",
            Dominant => "7",
            MajorSeventh => "maj7",
            MinorSeventh => "m7",
            DiminishedSeventh => "dim7",
            HalfDiminished => "m7b5",
            MajorMinor => "m(maj7)",
            AugmentedSeventh => "aug7",
            MajorSixth => "6",
            MinorSixth => "m6",
            DominantNinth => "9",
            MajorNinth => "maj9",
            MinorNinth => "m9",
            Dominant11th => "11",
            Major11th => "maj11",
            Minor11th => "m11",
            Dominant13th => "13",
            Major13th => "maj13",
            Minor13th => "m13",
            SuspendedSecond => "sus2",
            SuspendedFourth => "sus4",
            Power => "5",
            Neapolitan => "N6",
            Italian => "It6",
            French => "Fr6",
            German => "Ger6",
            Tristan => "Tristan",
            Pedal => "pedal",
            None => "N.C.",
            Other => "",
        }
    }
    pub fn tones(self) -> Vec<(u8, i8)> {
        use ChordKind::*;
        let mut tones = match self {
            Major | MajorSeventh | MajorSixth | MajorNinth | Major11th | Major13th | Dominant
            | DominantNinth | Dominant11th | Dominant13th => vec![(1, 0), (3, 4), (5, 7)],
            Minor | MinorSeventh | MinorSixth | MinorNinth | Minor11th | Minor13th | MajorMinor => {
                vec![(1, 0), (3, 3), (5, 7)]
            }
            Augmented | AugmentedSeventh => vec![(1, 0), (3, 4), (5, 8)],
            Diminished | DiminishedSeventh | HalfDiminished => vec![(1, 0), (3, 3), (5, 6)],
            SuspendedSecond => vec![(1, 0), (2, 2), (5, 7)],
            SuspendedFourth => vec![(1, 0), (4, 5), (5, 7)],
            Power => vec![(1, 0), (5, 7)],
            Pedal => vec![(1, 0)],
            Neapolitan => vec![(1, 0), (3, 4), (5, 7)],
            Italian => vec![(1, 0), (3, 4), (6, 10)],
            French => vec![(1, 0), (3, 4), (4, 6), (6, 10)],
            German => vec![(1, 0), (3, 4), (5, 7), (6, 10)],
            Tristan => vec![(1, 0), (4, 6), (6, 10), (9, 15)],
            None | Other => vec![],
        };
        match self {
            MajorSeventh | MajorNinth | Major11th | Major13th | MajorMinor => tones.push((7, 11)),
            Dominant | MinorSeventh | HalfDiminished | AugmentedSeventh | DominantNinth
            | MinorNinth | Dominant11th | Minor11th | Dominant13th | Minor13th => {
                tones.push((7, 10))
            }
            DiminishedSeventh => tones.push((7, 9)),
            MajorSixth | MinorSixth => tones.push((6, 9)),
            _ => {}
        }
        if matches!(
            self,
            DominantNinth
                | MajorNinth
                | MinorNinth
                | Dominant11th
                | Major11th
                | Minor11th
                | Dominant13th
                | Major13th
                | Minor13th
        ) {
            tones.push((9, 14));
        }
        if matches!(
            self,
            Dominant11th | Major11th | Minor11th | Dominant13th | Major13th | Minor13th
        ) {
            tones.push((11, 17));
        }
        if matches!(self, Dominant13th | Major13th | Minor13th) {
            tones.push((13, 21));
        }
        tones
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DegreeOperation {
    Add,
    Alter,
    Subtract,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordDegree {
    pub value: u8,
    pub alter: i8,
    pub operation: DegreeOperation,
}
impl ChordDegree {
    pub fn semitones(self) -> i8 {
        const SCALE: [i8; 7] = [0, 2, 4, 5, 7, 9, 11];
        let index = self.value.saturating_sub(1);
        SCALE[usize::from(index % 7)] + 12 * (index / 7) as i8 + self.alter
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordSpec {
    pub root: Option<PitchClass>,
    pub kind: ChordKind,
    #[serde(default)]
    pub bass: Option<PitchClass>,
    #[serde(default)]
    pub degrees: Vec<ChordDegree>,
    #[serde(default)]
    pub suffix_text: Option<String>,
}
impl ChordSpec {
    pub fn validate(&self) -> Result<(), String> {
        if self.root.is_none() != (self.kind == ChordKind::None) {
            return Err("Only N.C. may omit its chord root.".into());
        }
        if self
            .root
            .iter()
            .chain(self.bass.iter())
            .any(|p| !(-2..=2).contains(&p.alter))
        {
            return Err("Chord roots and basses support up to double accidentals.".into());
        }
        if self.kind == ChordKind::None && (self.bass.is_some() || !self.degrees.is_empty()) {
            return Err("N.C. cannot have a bass or chord degrees.".into());
        }
        if self.degrees.len() > 16
            || self.degrees.iter().any(|degree| {
                !(1..=13).contains(&degree.value) || !(-2..=2).contains(&degree.alter)
            })
        {
            return Err(
                "Chord degrees must be 1–13 with at most double accidentals (16 maximum).".into(),
            );
        }
        let mut seen = BTreeSet::new();
        for degree in &self.degrees {
            if !seen.insert((degree.value, degree.alter, degree.operation as u8)) {
                return Err("Duplicate chord degrees are not allowed.".into());
            }
        }
        if self
            .suffix_text
            .as_ref()
            .is_some_and(|text| text.len() > 128 || text.chars().any(char::is_control))
        {
            return Err("Chord suffix text must be a single line of at most 128 bytes.".into());
        }
        Ok(())
    }
    pub fn label(&self) -> String {
        if self.kind == ChordKind::None {
            return "N.C.".into();
        }
        format!(
            "{}{}{}",
            self.root.map_or_else(String::new, PitchClass::label),
            self.suffix(),
            self.bass
                .map_or_else(String::new, |bass| format!("/{}", bass.label()))
        )
    }
    pub fn suffix(&self) -> String {
        self.suffix_text.clone().unwrap_or_else(|| {
            let mut s = self.kind.suffix().to_owned();
            for d in &self.degrees {
                let op = match d.operation {
                    DegreeOperation::Add => "add",
                    DegreeOperation::Alter => "",
                    DegreeOperation::Subtract => "no",
                };
                let accidental = match d.alter {
                    -2 => "bb",
                    -1 => "b",
                    1 => "#",
                    2 => "##",
                    _ => "",
                };
                s.push_str(&format!("{op}{accidental}{}", d.value));
            }
            s
        })
    }
    pub fn tones(&self) -> Vec<(u8, i8)> {
        let mut tones = self.kind.tones();
        for degree in &self.degrees {
            if degree.operation != DegreeOperation::Add {
                tones.retain(|(value, _)| *value != degree.value);
            }
            if degree.operation != DegreeOperation::Subtract {
                tones.push((degree.value, degree.semitones()));
            }
        }
        tones.sort_unstable();
        tones.dedup();
        tones
    }
}
pub fn pitches(chord: &ChordSpec) -> Vec<u8> {
    let Some(root) = chord.root else {
        return vec![];
    };
    let mut tones = chord
        .tones()
        .into_iter()
        .map(|(_, interval)| {
            (i16::from(root.midi_class()) + i16::from(interval)).rem_euclid(12) as u8
        })
        .collect::<Vec<_>>();
    if let Some(bass) = chord.bass {
        tones.push(bass.midi_class());
    }
    tones.sort_unstable();
    tones.dedup();
    tones
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BassStyle {
    #[default]
    Slash,
    Under,
    LowerRight,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChordSymbol {
    pub anchor: Anchor,
    #[serde(default)]
    pub slot: u8,
    pub chord: ChordSpec,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default = "yes")]
    pub play: bool,
    #[serde(default)]
    pub offset: [f32; 2],
    #[serde(default)]
    pub bass_style: BassStyle,
    #[serde(default)]
    pub fretboard: Option<Fretboard>,
    #[serde(default = "yes")]
    pub show_fretboard: bool,
}
impl ChordSymbol {
    pub fn new(mut anchor: Anchor, chord: ChordSpec) -> Self {
        anchor.layer = 0;
        anchor.voice = 0;
        Self {
            anchor,
            slot: 0,
            chord,
            visible: true,
            play: true,
            offset: [0.; 2],
            bass_style: BassStyle::Slash,
            fretboard: None,
            show_fretboard: true,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.chord.validate()?;
        if self.slot > 7 || self.anchor.layer != 0 || self.anchor.voice != 0 {
            return Err(
                "Chord symbols support eight stacked slots on staff beats (layer 1).".into(),
            );
        }
        if self.offset.iter().any(|v| !v.is_finite() || v.abs() > 200.) {
            return Err("Chord offsets must be finite and within 200 points.".into());
        }
        if let Some(frame) = &self.fretboard {
            if self.chord.kind == ChordKind::None {
                return Err("N.C. cannot have a fretboard.".into());
            }
            frame.validate()?;
        }
        Ok(())
    }
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.chords.len() > MAX_CHORDS {
        return Err("A score supports at most 20,000 chord symbols.".into());
    }
    let mut seen = BTreeSet::new();
    for symbol in &score.notation.chords {
        symbol.validate()?;
        if symbol.anchor.part >= score.parts.len()
            || symbol.anchor.measure >= score.measure_count()
            || symbol.anchor.tick >= score.measure_ticks()
        {
            return Err("A chord symbol is outside its source staff or measure.".into());
        }
        if !seen.insert((
            symbol.anchor.part,
            symbol.anchor.measure,
            symbol.anchor.tick,
            symbol.slot,
        )) {
            return Err("Two chord symbols occupy the same staff beat and stack slot.".into());
        }
    }
    Ok(())
}
pub fn remove_part(score: &mut Score, part: usize) {
    score
        .notation
        .chords
        .retain(|symbol| symbol.anchor.part != part);
}
pub fn reindex_parts(score: &mut Score, part: usize) {
    for symbol in &mut score.notation.chords {
        if symbol.anchor.part > part {
            symbol.anchor.part -= 1;
        }
    }
}
pub fn prune(score: &mut Score) {
    let parts = score.parts.len();
    let measures = score.measure_count();
    let ticks = score.measure_ticks();
    score.notation.chords.retain(|symbol| {
        symbol.anchor.part < parts && symbol.anchor.measure < measures && symbol.anchor.tick < ticks
    });
}
pub fn transpose_symbol(symbol: &mut ChordSymbol, interval: Transposition) -> Result<(), String> {
    map_symbol_pitches(symbol, |pitch| interval.written_pitch(pitch))
}
pub fn map_symbol_pitches(
    symbol: &mut ChordSymbol,
    transform: impl Fn(Pitch) -> Result<Pitch, String>,
) -> Result<(), String> {
    let mut candidate = symbol.clone();
    let old_root = candidate.chord.root;
    candidate.chord.root = candidate
        .chord
        .root
        .map(|pitch| transform(pitch.pitch()).map(PitchClass::from_pitch))
        .transpose()?;
    candidate.chord.bass = candidate
        .chord
        .bass
        .map(|pitch| transform(pitch.pitch()).map(PitchClass::from_pitch))
        .transpose()?;
    if let (Some(old), Some(new), Some(frame)) =
        (old_root, candidate.chord.root, candidate.fretboard.as_mut())
    {
        frame.transpose(
            (i16::from(new.midi_class()) - i16::from(old.midi_class())).rem_euclid(12) as i8,
        )?;
    }
    candidate.validate()?;
    *symbol = candidate;
    Ok(())
}

/// Deterministic accompaniment events in source/concert pitch, independent of
/// notation visibility. Simultaneous stack slots remain separate voicings.
pub fn playback_notes(score: &Score) -> Vec<(usize, u64, u64, Vec<u8>)> {
    playback_events(score)
        .into_iter()
        .map(|(staff, _, start, end, pitches)| (staff, start, end, pitches))
        .collect()
}
pub fn playback_events(score: &Score) -> Vec<(usize, u8, u64, u64, Vec<u8>)> {
    let ticks = u64::from(score.measure_ticks());
    let mut by_staff: BTreeMap<(usize, u8), Vec<&ChordSymbol>> = BTreeMap::new();
    for symbol in &score.notation.chords {
        if symbol.play {
            by_staff
                .entry((symbol.anchor.part, symbol.slot))
                .or_default()
                .push(symbol);
        }
    }
    let mut result = Vec::new();
    for ((staff, slot), mut symbols) in by_staff {
        symbols.sort_by_key(|s| (s.anchor.measure, s.anchor.tick));
        for (index, symbol) in symbols.iter().enumerate() {
            let start = symbol.anchor.measure as u64 * ticks + u64::from(symbol.anchor.tick);
            let end = symbols
                .get(index + 1)
                .map_or((symbol.anchor.measure as u64 + 1) * ticks, |next| {
                    next.anchor.measure as u64 * ticks + u64::from(next.anchor.tick)
                })
                .min((symbol.anchor.measure as u64 + 1) * ticks);
            let Some(root) = symbol.chord.root else {
                continue;
            };
            let base = 48 + root.midi_class();
            let mut notes = symbol
                .chord
                .tones()
                .into_iter()
                .map(|(_, interval)| (i16::from(base) + i16::from(interval)).clamp(0, 127) as u8)
                .collect::<Vec<_>>();
            if let Some(bass) = symbol.chord.bass {
                notes.push(36 + bass.midi_class());
            }
            notes.sort_unstable();
            notes.dedup();
            if !notes.is_empty() && end > start {
                result.push((staff, slot, start, end, notes));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at() -> Anchor {
        Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        }
    }
    #[test]
    fn semantic_degrees_and_spelled_transposition() {
        let mut symbol = ChordSymbol::new(at(), parse("Bb7(b9,#11)/D").unwrap());
        assert_eq!(pitches(&symbol.chord), vec![2, 4, 5, 8, 10, 11]);
        transpose_symbol(
            &mut symbol,
            Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
        )
        .unwrap();
        assert_eq!(symbol.chord.label(), "C7(b9,#11)/E");
    }
    #[test]
    fn chords_are_free_beats_and_stacked_but_not_duplicate_layers() {
        let mut score = Score::blank();
        let mut symbol = ChordSymbol::new(at(), parse("Cmaj7").unwrap());
        symbol.anchor.tick = 240;
        score.notation.chords.push(symbol.clone());
        score.validate().unwrap();
        symbol.anchor.layer = 1;
        score.notation.chords.push(symbol);
        assert!(validate(&score).is_err());
    }
    #[test]
    fn no_chord_stops_previous_playback() {
        let mut score = Score::blank();
        let mut stop = ChordSymbol::new(at(), parse("N.C.").unwrap());
        stop.anchor.tick = 480;
        score.notation.chords = vec![ChordSymbol::new(at(), parse("C").unwrap()), stop];
        let notes = playback_notes(&score);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].2, 480);
        assert_eq!(notes[0].3, vec![48, 52, 55]);
    }
}
