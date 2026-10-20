use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub const PPQ: u32 = 480;
pub const MAX_PARTS: usize = 64;
pub const MAX_MEASURES: usize = 1024;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub fn dynamic_name(velocity: u8) -> &'static str {
    match velocity {
        0..40 => "pp",
        40..56 => "p",
        56..72 => "mp",
        72..92 => "mf",
        92..110 => "f",
        _ => "ff",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    Piano,
    Violin,
    Viola,
    Cello,
    DoubleBass,
    Flute,
    Piccolo,
    Oboe,
    EnglishHorn,
    Clarinet,
    Bassoon,
    FrenchHorn,
    Trumpet,
    Trombone,
    Tuba,
    Harp,
    Timpani,
    StringEnsemble,
    /// An additional zero-based General MIDI program. Legacy named variants
    /// retain their original serialized representation for existing scores.
    GeneralMidi(u8),
}

// The General MIDI program map is zero-based internally. Bank selection and
// drum kits are separate concepts; these 128 entries address melodic bank 0.
const GM_NAMES: [&str; 128] = [
    "Piano",
    "Bright piano",
    "Electric grand piano",
    "Honky-tonk piano",
    "Electric piano 1",
    "Electric piano 2",
    "Harpsichord",
    "Clavinet",
    "Celesta",
    "Glockenspiel",
    "Music box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular bells",
    "Dulcimer",
    "Drawbar organ",
    "Percussive organ",
    "Rock organ",
    "Church organ",
    "Reed organ",
    "Accordion",
    "Harmonica",
    "Tango accordion",
    "Nylon-string guitar",
    "Steel-string guitar",
    "Jazz guitar",
    "Clean electric guitar",
    "Muted electric guitar",
    "Overdriven guitar",
    "Distortion guitar",
    "Guitar harmonics",
    "Acoustic bass",
    "Fingered electric bass",
    "Picked electric bass",
    "Fretless bass",
    "Slap bass 1",
    "Slap bass 2",
    "Synth bass 1",
    "Synth bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Double bass",
    "Tremolo strings",
    "Pizzicato strings",
    "Harp",
    "Timpani",
    "String ensemble",
    "Slow strings",
    "Synth strings 1",
    "Synth strings 2",
    "Choir aahs",
    "Voice oohs",
    "Synth voice",
    "Orchestra hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted trumpet",
    "French horn",
    "Brass section",
    "Synth brass 1",
    "Synth brass 2",
    "Soprano saxophone",
    "Alto saxophone",
    "Tenor saxophone",
    "Baritone saxophone",
    "Oboe",
    "English horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan flute",
    "Blown bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Square lead",
    "Sawtooth lead",
    "Calliope lead",
    "Chiff lead",
    "Charang lead",
    "Voice lead",
    "Fifths lead",
    "Bass and lead",
    "New age pad",
    "Warm pad",
    "Polysynth pad",
    "Choir pad",
    "Bowed pad",
    "Metallic pad",
    "Halo pad",
    "Sweep pad",
    "Rain effect",
    "Soundtrack effect",
    "Crystal effect",
    "Atmosphere effect",
    "Brightness effect",
    "Goblins effect",
    "Echoes effect",
    "Sci-fi effect",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bagpipe",
    "Fiddle",
    "Shanai",
    "Tinkle bell",
    "Agogo",
    "Steel drums",
    "Woodblock",
    "Taiko drum",
    "Melodic tom",
    "Synth drum",
    "Reverse cymbal",
    "Guitar fret noise",
    "Breath noise",
    "Seashore",
    "Bird tweet",
    "Telephone ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

const GM_FAMILIES: [&str; 16] = [
    "Piano",
    "Chromatic percussion",
    "Organ",
    "Guitar",
    "Bass",
    "Strings",
    "Ensemble",
    "Brass",
    "Reeds",
    "Pipes",
    "Synth leads",
    "Synth pads",
    "Synth effects",
    "World",
    "Percussion",
    "Sound effects",
];

impl Instrument {
    /// The original native score variants, in their original order.
    pub const LEGACY: &'static [Self] = &[
        Self::Piano,
        Self::Violin,
        Self::Viola,
        Self::Cello,
        Self::DoubleBass,
        Self::Flute,
        Self::Piccolo,
        Self::Oboe,
        Self::EnglishHorn,
        Self::Clarinet,
        Self::Bassoon,
        Self::FrenchHorn,
        Self::Trumpet,
        Self::Trombone,
        Self::Tuba,
        Self::Harp,
        Self::Timpani,
        Self::StringEnsemble,
    ];

    /// Every melodic General MIDI program, in bank order, exactly once.
    pub const ALL: &'static [Self] = &[
        Self::Piano,
        Self::GeneralMidi(1),
        Self::GeneralMidi(2),
        Self::GeneralMidi(3),
        Self::GeneralMidi(4),
        Self::GeneralMidi(5),
        Self::GeneralMidi(6),
        Self::GeneralMidi(7),
        Self::GeneralMidi(8),
        Self::GeneralMidi(9),
        Self::GeneralMidi(10),
        Self::GeneralMidi(11),
        Self::GeneralMidi(12),
        Self::GeneralMidi(13),
        Self::GeneralMidi(14),
        Self::GeneralMidi(15),
        Self::GeneralMidi(16),
        Self::GeneralMidi(17),
        Self::GeneralMidi(18),
        Self::GeneralMidi(19),
        Self::GeneralMidi(20),
        Self::GeneralMidi(21),
        Self::GeneralMidi(22),
        Self::GeneralMidi(23),
        Self::GeneralMidi(24),
        Self::GeneralMidi(25),
        Self::GeneralMidi(26),
        Self::GeneralMidi(27),
        Self::GeneralMidi(28),
        Self::GeneralMidi(29),
        Self::GeneralMidi(30),
        Self::GeneralMidi(31),
        Self::GeneralMidi(32),
        Self::GeneralMidi(33),
        Self::GeneralMidi(34),
        Self::GeneralMidi(35),
        Self::GeneralMidi(36),
        Self::GeneralMidi(37),
        Self::GeneralMidi(38),
        Self::GeneralMidi(39),
        Self::Violin,
        Self::Viola,
        Self::Cello,
        Self::DoubleBass,
        Self::GeneralMidi(44),
        Self::GeneralMidi(45),
        Self::Harp,
        Self::Timpani,
        Self::StringEnsemble,
        Self::GeneralMidi(49),
        Self::GeneralMidi(50),
        Self::GeneralMidi(51),
        Self::GeneralMidi(52),
        Self::GeneralMidi(53),
        Self::GeneralMidi(54),
        Self::GeneralMidi(55),
        Self::Trumpet,
        Self::Trombone,
        Self::Tuba,
        Self::GeneralMidi(59),
        Self::FrenchHorn,
        Self::GeneralMidi(61),
        Self::GeneralMidi(62),
        Self::GeneralMidi(63),
        Self::GeneralMidi(64),
        Self::GeneralMidi(65),
        Self::GeneralMidi(66),
        Self::GeneralMidi(67),
        Self::Oboe,
        Self::EnglishHorn,
        Self::Bassoon,
        Self::Clarinet,
        Self::Piccolo,
        Self::Flute,
        Self::GeneralMidi(74),
        Self::GeneralMidi(75),
        Self::GeneralMidi(76),
        Self::GeneralMidi(77),
        Self::GeneralMidi(78),
        Self::GeneralMidi(79),
        Self::GeneralMidi(80),
        Self::GeneralMidi(81),
        Self::GeneralMidi(82),
        Self::GeneralMidi(83),
        Self::GeneralMidi(84),
        Self::GeneralMidi(85),
        Self::GeneralMidi(86),
        Self::GeneralMidi(87),
        Self::GeneralMidi(88),
        Self::GeneralMidi(89),
        Self::GeneralMidi(90),
        Self::GeneralMidi(91),
        Self::GeneralMidi(92),
        Self::GeneralMidi(93),
        Self::GeneralMidi(94),
        Self::GeneralMidi(95),
        Self::GeneralMidi(96),
        Self::GeneralMidi(97),
        Self::GeneralMidi(98),
        Self::GeneralMidi(99),
        Self::GeneralMidi(100),
        Self::GeneralMidi(101),
        Self::GeneralMidi(102),
        Self::GeneralMidi(103),
        Self::GeneralMidi(104),
        Self::GeneralMidi(105),
        Self::GeneralMidi(106),
        Self::GeneralMidi(107),
        Self::GeneralMidi(108),
        Self::GeneralMidi(109),
        Self::GeneralMidi(110),
        Self::GeneralMidi(111),
        Self::GeneralMidi(112),
        Self::GeneralMidi(113),
        Self::GeneralMidi(114),
        Self::GeneralMidi(115),
        Self::GeneralMidi(116),
        Self::GeneralMidi(117),
        Self::GeneralMidi(118),
        Self::GeneralMidi(119),
        Self::GeneralMidi(120),
        Self::GeneralMidi(121),
        Self::GeneralMidi(122),
        Self::GeneralMidi(123),
        Self::GeneralMidi(124),
        Self::GeneralMidi(125),
        Self::GeneralMidi(126),
        Self::GeneralMidi(127),
    ];

    pub fn name(self) -> &'static str {
        GM_NAMES
            .get(usize::from(self.gm_program()))
            .copied()
            .unwrap_or("Unknown instrument")
    }

    pub fn family(self) -> &'static str {
        // GM places timpani beside strings; the browser uses its musical family.
        if self.gm_program() == 47 {
            return "Percussion";
        }
        GM_FAMILIES
            .get(usize::from(self.gm_program() / 8))
            .copied()
            .unwrap_or("Unknown")
    }

    pub fn from_gm_program(program: u8) -> Option<Self> {
        Self::ALL.get(usize::from(program)).copied()
    }

    pub fn gm_program(self) -> u8 {
        match self {
            Self::Piano => 0,
            Self::Violin => 40,
            Self::Viola => 41,
            Self::Cello => 42,
            Self::DoubleBass => 43,
            Self::Flute => 73,
            Self::Piccolo => 72,
            Self::Oboe => 68,
            Self::EnglishHorn => 69,
            Self::Clarinet => 71,
            Self::Bassoon => 70,
            Self::FrenchHorn => 60,
            Self::Trumpet => 56,
            Self::Trombone => 57,
            Self::Tuba => 58,
            Self::Harp => 46,
            Self::Timpani => 47,
            Self::StringEnsemble => 48,
            Self::GeneralMidi(program) => program,
        }
    }

    pub fn default_clef(self) -> Clef {
        match self.gm_program() {
            32..=39 | 42..=43 | 47 | 57..=58 | 66..=67 | 70 => Clef::Bass,
            41 => Clef::Alto,
            _ => Clef::Treble,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clef {
    Treble,
    Bass,
    Alto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duration {
    DoubleWhole,
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
    OneTwentyEighth,
}
impl Duration {
    pub const ALL: &'static [Self] = &[
        Self::DoubleWhole,
        Self::Whole,
        Self::Half,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::SixtyFourth,
        Self::OneTwentyEighth,
    ];
    pub fn ticks(self) -> u32 {
        match self {
            Self::DoubleWhole => PPQ * 8,
            Self::Whole => PPQ * 4,
            Self::Half => PPQ * 2,
            Self::Quarter => PPQ,
            Self::Eighth => PPQ / 2,
            Self::Sixteenth => PPQ / 4,
            Self::ThirtySecond => PPQ / 8,
            Self::SixtyFourth => PPQ / 16,
            Self::OneTwentyEighth => PPQ / 32,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::DoubleWhole => "Double whole",
            Self::Whole => "Whole",
            Self::Half => "Half",
            Self::Quarter => "Quarter",
            Self::Eighth => "Eighth",
            Self::Sixteenth => "Sixteenth",
            Self::ThirtySecond => "32nd",
            Self::SixtyFourth => "64th",
            Self::OneTwentyEighth => "128th",
        }
    }
    pub fn flags(self) -> usize {
        match self {
            Self::Eighth => 1,
            Self::Sixteenth => 2,
            Self::ThirtySecond => 3,
            Self::SixtyFourth => 4,
            Self::OneTwentyEighth => 5,
            _ => 0,
        }
    }
    pub fn has_stem(self) -> bool {
        !matches!(self, Self::DoubleWhole | Self::Whole)
    }
    pub fn musicxml_type(self) -> &'static str {
        match self {
            Self::DoubleWhole => "breve",
            Self::Whole => "whole",
            Self::Half => "half",
            Self::Quarter => "quarter",
            Self::Eighth => "eighth",
            Self::Sixteenth => "16th",
            Self::ThirtySecond => "32nd",
            Self::SixtyFourth => "64th",
            Self::OneTwentyEighth => "128th",
        }
    }
    pub fn from_musicxml_type(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|duration| duration.musicxml_type() == value)
    }
}

/// An actual:normal time modification: three eighths in the time of two are 3:2.
/// The native 480-PPQ grid is retained so existing Encore scores keep their timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tuplet {
    pub actual: u8,
    pub normal: u8,
}
impl Tuplet {
    pub const TRIPLET: Self = Self {
        actual: 3,
        normal: 2,
    };
    pub fn validate(self) -> Result<(), String> {
        if !(1..=32).contains(&self.actual) || !(1..=32).contains(&self.normal) {
            return Err("Tuplet ratios require actual and normal counts between 1 and 32.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    C,
    D,
    E,
    F,
    G,
    A,
    B,
}
impl Step {
    pub fn index(self) -> i32 {
        match self {
            Self::C => 0,
            Self::D => 1,
            Self::E => 2,
            Self::F => 3,
            Self::G => 4,
            Self::A => 5,
            Self::B => 6,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::A => "A",
            Self::B => "B",
        }
    }
    pub fn semitone(self) -> i32 {
        [0, 2, 4, 5, 7, 9, 11][self.index() as usize]
    }
    pub fn from_index(index: i32) -> Self {
        [
            Self::C,
            Self::D,
            Self::E,
            Self::F,
            Self::G,
            Self::A,
            Self::B,
        ][index.rem_euclid(7) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pitch {
    pub step: Step,
    pub octave: i8,
    pub alter: i8,
}
impl Pitch {
    pub fn midi(self) -> u8 {
        self.midi_number().clamp(0, 127) as u8
    }
    fn midi_number(self) -> i32 {
        (i32::from(self.octave) + 1) * 12 + self.step.semitone() + i32::from(self.alter)
    }
    pub fn from_midi(midi: u8) -> Self {
        let midi = midi.min(127);
        let (step, alter) = [
            (Step::C, 0),
            (Step::C, 1),
            (Step::D, 0),
            (Step::D, 1),
            (Step::E, 0),
            (Step::F, 0),
            (Step::F, 1),
            (Step::G, 0),
            (Step::G, 1),
            (Step::A, 0),
            (Step::A, 1),
            (Step::B, 0),
        ][(midi % 12) as usize];
        Self {
            step,
            octave: (midi / 12) as i8 - 1,
            alter,
        }
    }
    pub fn staff_step(self) -> i32 {
        i32::from(self.octave) * 7 + self.step.index()
    }
    pub fn from_staff_step(staff_step: i32, key_fifths: i8) -> Self {
        // MIDI's playable range is a much smaller interval than an i32 staff coordinate.
        let staff_step = staff_step.clamp(-7, 69);
        let step = Step::from_index(staff_step);
        let alter = key_alter(step, key_fifths);
        Self {
            step,
            octave: staff_step.div_euclid(7) as i8,
            alter,
        }
    }
    pub fn name(self) -> String {
        let accidental = match self.alter {
            -2 => "𝄫",
            -1 => "♭",
            0 => "",
            1 => "♯",
            2 => "𝄪",
            _ => "?",
        };
        format!("{}{}{}", self.step.name(), accidental, self.octave)
    }
    pub fn transpose(self, semitones: i8) -> Self {
        Self::from_midi((self.midi_number() + i32::from(semitones)).clamp(0, 127) as u8)
    }
}

/// Written accidental supplied by a major/minor key signature (circle of fifths).
pub fn key_alter(step: Step, fifths: i8) -> i8 {
    const SHARPS: [Step; 7] = [
        Step::F,
        Step::C,
        Step::G,
        Step::D,
        Step::A,
        Step::E,
        Step::B,
    ];
    const FLATS: [Step; 7] = [
        Step::B,
        Step::E,
        Step::A,
        Step::D,
        Step::G,
        Step::C,
        Step::F,
    ];
    if fifths > 0 && SHARPS[..fifths.min(7) as usize].contains(&step) {
        1
    } else if fifths < 0 && FLATS[..(-i16::from(fifths)).min(7) as usize].contains(&step) {
        -1
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Articulation {
    Accent,
    Marcato,
    Tenuto,
    Staccatissimo,
}

impl Articulation {
    pub const ALL: &'static [Self] = &[
        Self::Accent,
        Self::Marcato,
        Self::Tenuto,
        Self::Staccatissimo,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Accent => "Accent",
            Self::Marcato => "Marcato",
            Self::Tenuto => "Tenuto",
            Self::Staccatissimo => "Staccatissimo",
        }
    }

    pub fn musicxml_name(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Marcato => "strong-accent",
            Self::Tenuto => "tenuto",
            Self::Staccatissimo => "staccatissimo",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Finale-compatible independent rhythmic layer, numbered 0–3 internally.
    #[serde(default, skip_serializing_if = "is_layer_zero")]
    pub layer: u8,
    pub start: u32,
    pub duration: Duration,
    pub dotted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuplet: Option<Tuplet>,
    pub pitches: Vec<Pitch>,
    pub velocity: u8,
    pub staccato: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub articulations: Vec<Articulation>,
    /// Tie forward to matching pitches in the next contiguous event.
    pub tied: bool,
}
fn is_layer_zero(layer: &u8) -> bool {
    *layer == 0
}
impl Note {
    /// Ordinary articulation performance is deliberately modest and deterministic.
    /// Chord-wide accents share one bounded boost; silent entries stay silent.
    pub fn playback_velocity(&self) -> u8 {
        if self.velocity == 0 {
            return 0;
        }
        let boost = self
            .articulations
            .iter()
            .map(|articulation| match articulation {
                Articulation::Accent => 12,
                Articulation::Marcato => 18,
                Articulation::Tenuto => 3,
                Articulation::Staccatissimo => 0,
            })
            .max()
            .unwrap_or(0);
        self.velocity.saturating_add(boost).min(127)
    }

    /// Fraction removed from the final tied segment for audio playback.
    /// The legacy 0.52 staccato calculation is retained exactly.
    pub fn playback_shortening(&self) -> f64 {
        let mut shortening: f64 = if self.staccato { 0.52 } else { 0.0 };
        for articulation in &self.articulations {
            shortening = shortening.max(match articulation {
                Articulation::Marcato => 0.28,
                Articulation::Staccatissimo => 0.70,
                Articulation::Accent | Articulation::Tenuto => 0.0,
            });
        }
        shortening
    }

    /// MIDI historically used integer half-length staccato, while audio used
    /// its 0.48 gate. Preserve that behavior and share all new articulation rules.
    pub fn playback_ticks(&self) -> u32 {
        if self.articulations.is_empty() {
            return if self.staccato {
                self.ticks() / 2
            } else {
                self.ticks()
            };
        }
        let mut gate: f64 = if self.staccato { 0.5 } else { 1.0 };
        for articulation in &self.articulations {
            gate = gate.min(match articulation {
                Articulation::Marcato => 0.72,
                Articulation::Staccatissimo => 0.30,
                Articulation::Accent | Articulation::Tenuto => 1.0,
            });
        }
        (f64::from(self.ticks()) * gate).floor() as u32
    }

    /// Exact score time, including dots and tuplets. Invalid fractional-grid
    /// durations are rejected by `checked_ticks` and score validation.
    pub fn ticks(&self) -> u32 {
        self.checked_ticks().unwrap_or(0)
    }
    pub fn checked_ticks(&self) -> Result<u32, String> {
        let ratio = self.tuplet.unwrap_or(Tuplet {
            actual: 1,
            normal: 1,
        });
        ratio.validate()?;
        let numerator =
            self.duration.ticks() * if self.dotted { 3 } else { 2 } * u32::from(ratio.normal);
        let denominator = 2 * u32::from(ratio.actual);
        if !numerator.is_multiple_of(denominator) {
            return Err("Rhythm cannot be represented exactly on Encore's 480-tick grid; choose a different note value or tuplet ratio.".into());
        }
        let ticks = numerator / denominator;
        if ticks == 0 {
            return Err("A note or rest must have positive duration.".into());
        }
        Ok(ticks)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub name: String,
    pub instrument: Instrument,
    pub clef: Clef,
    pub measures: Vec<Measure>,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
}
impl Part {
    fn new(instrument: Instrument, measures: usize) -> Self {
        Self {
            name: instrument.name().into(),
            instrument,
            clef: instrument.default_clef(),
            measures: vec![Measure::default(); measures],
            volume: 0.8,
            pan: 0.0,
            muted: false,
            solo: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub title: String,
    pub composer: String,
    pub tempo: u16,
    pub time_numerator: u8,
    pub time_denominator: u8,
    pub key_fifths: i8,
    pub parts: Vec<Part>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub part: usize,
    pub measure: usize,
    pub note: usize,
}

impl Score {
    pub fn carol_demo() -> Self {
        crate::demos::carol_of_the_bells()
    }

    pub fn blank() -> Self {
        let mut right = Part::new(Instrument::Piano, 4);
        right.name = "Piano · right hand".into();
        let mut left = Part::new(Instrument::Piano, 4);
        left.name = "Piano · left hand".into();
        left.clef = Clef::Bass;
        Self {
            title: "Untitled score".into(),
            composer: String::new(),
            tempo: 100,
            time_numerator: 4,
            time_denominator: 4,
            key_fifths: 0,
            parts: vec![right, left],
        }
    }
    pub fn demo() -> Self {
        let mut score = Self::blank();
        score.title = "First Light".into();
        score.composer = "An original Encore miniature".into();
        score.tempo = 108;
        score.key_fifths = 2;
        score.parts[0].measures = vec![Measure::default(); 8];
        score.parts[1].measures = vec![Measure::default(); 8];
        for instrument in [
            Instrument::Violin,
            Instrument::Cello,
            Instrument::Flute,
            Instrument::FrenchHorn,
        ] {
            score.add_part(instrument);
        }
        score.parts[0].volume = 0.68;
        score.parts[0].pan = -0.12;
        score.parts[1].volume = 0.64;
        score.parts[1].pan = -0.15;
        score.parts[2].volume = 0.79;
        score.parts[2].pan = -0.36;
        score.parts[3].volume = 0.75;
        score.parts[3].pan = 0.28;
        score.parts[4].volume = 0.62;
        score.parts[4].pan = 0.4;
        score.parts[5].volume = 0.57;
        score.parts[5].pan = 0.16;
        // D, B minor, G, A, D/F#, G, A, D. Everything here is composed for Encore.
        let harmony = [
            [62, 66, 69],
            [59, 62, 66],
            [55, 59, 62],
            [57, 61, 64],
            [54, 57, 62],
            [55, 59, 62],
            [57, 61, 64],
            [50, 54, 57],
        ];
        let bass = [38, 35, 31, 33, 30, 31, 33, 38];
        for bar in 0..8 {
            let chord = harmony[bar];
            for eighth in 0..8 {
                let midi = chord[[0, 1, 2, 1, 0, 1, 2, 1][eighth]] + if bar == 7 { 12 } else { 0 };
                score.parts[0].measures[bar].notes.push(demo_note(
                    eighth as u32 * 240,
                    Duration::Eighth,
                    &[midi],
                    // Keep the accompaniment in mp while accenting each group;
                    // subtle performance accents need no repeated dynamic marks.
                    if eighth % 4 == 0 { 69 } else { 61 },
                ));
            }
            for half in 0..2 {
                score.parts[1].measures[bar].notes.push(demo_note(
                    half * 960,
                    Duration::Half,
                    &[bass[bar], bass[bar] + 12],
                    65,
                ));
            }
            score.parts[3].measures[bar].notes.push(demo_note(
                0,
                Duration::Half,
                &[bass[bar] + 12],
                73,
            ));
            score.parts[3].measures[bar].notes.push(demo_note(
                960,
                Duration::Half,
                &[if bar == 7 { 50 } else { bass[bar] + 19 }],
                66,
            ));
            score.parts[5].measures[bar].notes.push(demo_note(
                0,
                Duration::Whole,
                &[chord[1], chord[2]],
                55,
            ));
        }
        let melody: [[u8; 5]; 8] = [
            [74, 78, 81, 78, 76],
            [74, 73, 71, 73, 74],
            [79, 78, 76, 74, 71],
            [73, 76, 81, 79, 76],
            [78, 81, 86, 81, 78],
            [79, 78, 76, 74, 71],
            [73, 76, 81, 79, 76],
            [78, 76, 74, 74, 74],
        ];
        for (bar, pitches) in melody.iter().enumerate() {
            for (index, (start, duration)) in [
                (0, Duration::Quarter),
                (480, Duration::Eighth),
                (720, Duration::Eighth),
                (960, Duration::Quarter),
                (1440, Duration::Quarter),
            ]
            .into_iter()
            .enumerate()
            {
                let mut note = demo_note(
                    start,
                    duration,
                    &[pitches[index]],
                    if index == 0 { 87 } else { 79 },
                );
                note.staccato = (bar == 1 || bar == 3) && (index == 1 || index == 2);
                score.parts[2].measures[bar].notes.push(note);
            }
            if bar % 2 == 0 {
                score.parts[4].measures[bar]
                    .notes
                    .push(demo_note(0, Duration::Half, &[], 64));
                score.parts[4].measures[bar].notes.push(demo_note(
                    960,
                    Duration::Quarter,
                    &[pitches[3] + 12],
                    63,
                ));
                score.parts[4].measures[bar].notes.push(demo_note(
                    1440,
                    Duration::Quarter,
                    &[pitches[4] + 12],
                    59,
                ));
            } else {
                score.parts[4].measures[bar].notes.push(demo_note(
                    0,
                    Duration::Half,
                    &[pitches[0] + 12],
                    65,
                ));
                score.parts[4].measures[bar]
                    .notes
                    .push(demo_note(960, Duration::Half, &[], 64));
            }
        }
        // A held dominant links bars six and seven, and the ensemble resolves together.
        score.parts[5].measures[5].notes[0] = demo_note(0, Duration::Whole, &[64, 69], 55);
        score.parts[5].measures[5].notes[0].tied = true;
        score.parts[5].measures[6].notes[0] = demo_note(0, Duration::Whole, &[64, 69], 55);
        score.parts[2].measures[7].notes = vec![
            demo_note(0, Duration::Half, &[78], 84),
            demo_note(960, Duration::Half, &[74], 76),
        ];
        score.parts[4].measures[7].notes = vec![demo_note(0, Duration::Whole, &[86], 58)];
        score
    }
    pub fn measure_ticks(&self) -> u32 {
        u32::from(self.time_numerator) * PPQ * 4 / u32::from(self.time_denominator.max(1))
    }
    pub fn measure_count(&self) -> usize {
        self.parts
            .iter()
            .map(|p| p.measures.len())
            .max()
            .unwrap_or(0)
    }
    pub fn add_measures(&mut self, count: usize) {
        let length = self
            .measure_count()
            .saturating_add(count)
            .clamp(1, MAX_MEASURES);
        for part in &mut self.parts {
            part.measures.resize_with(length, Measure::default);
        }
    }
    pub fn add_part(&mut self, instrument: Instrument) {
        if self.parts.len() < MAX_PARTS {
            self.parts.push(Part::new(
                instrument,
                if self.measure_count() == 0 {
                    4
                } else {
                    self.measure_count()
                },
            ));
        }
    }
    /// Align staff lengths and event order after structural editing. Invalid musical
    /// content is left visible for the caller to fix; this never silently deletes notes.
    pub fn normalize(&mut self) {
        let count = self.measure_count().max(1);
        for part in &mut self.parts {
            part.measures.resize_with(count, Measure::default);
            for measure in &mut part.measures {
                measure.notes.sort_by_key(|note| (note.start, note.layer));
            }
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.title.len() > 4096 || self.composer.len() > 4096 {
            return Err("Title or composer is too long (maximum 4096 bytes).".into());
        }
        if !(20..=400).contains(&self.tempo) {
            return Err("Tempo must be between 20 and 400 BPM.".into());
        }
        if !(1..=32).contains(&self.time_numerator)
            || ![1, 2, 4, 8, 16, 32, 64, 128].contains(&self.time_denominator)
        {
            return Err(
                "Time signature requires 1–32 beats and denominator 1, 2, 4, 8, 16, 32, 64, or 128.".into(),
            );
        }
        if !(-7..=7).contains(&self.key_fifths) {
            return Err("Key signature must have between seven flats and seven sharps.".into());
        }
        if self.parts.is_empty() || self.parts.len() > MAX_PARTS {
            return Err(format!("Score must contain 1–{MAX_PARTS} staves."));
        }
        let count = self.measure_count();
        if count == 0 || count > MAX_MEASURES {
            return Err(format!("Score must contain 1–{MAX_MEASURES} measures."));
        }
        let length = self.measure_ticks();
        let mut total_notes = 0usize;
        for (pi, part) in self.parts.iter().enumerate() {
            let context = format!("Staff {} ({})", pi + 1, part.name);
            if part.instrument.gm_program() > 127 {
                return Err(format!("{context}: instrument program must be 0–127."));
            }
            if part.name.len() > 1024 {
                return Err(format!("Staff {} name is too long.", pi + 1));
            }
            if part.measures.len() != count {
                return Err(format!(
                    "{context}: all staves must have the same number of measures."
                ));
            }
            if !part.volume.is_finite()
                || !(0.0..=1.0).contains(&part.volume)
                || !part.pan.is_finite()
                || !(-1.0..=1.0).contains(&part.pan)
            {
                return Err(format!("{context}: volume must be 0–1 and pan −1–1."));
            }
            for (mi, measure) in part.measures.iter().enumerate() {
                total_notes += measure.notes.len();
                if total_notes > 200_000 {
                    return Err("Score exceeds the 200,000-event limit.".into());
                }
                let mut ends = [0; 4];
                for (ni, note) in measure.notes.iter().enumerate() {
                    let context = format!("{context}, measure {}, event {}", mi + 1, ni + 1);
                    if note.layer > 3 {
                        return Err(format!("{context}: layer must be 1–4 (0–3 internally)."));
                    }
                    if note.articulations.len() > Articulation::ALL.len()
                        || note
                            .articulations
                            .iter()
                            .enumerate()
                            .any(|(index, articulation)| {
                                note.articulations[..index].contains(articulation)
                            })
                    {
                        return Err(format!(
                            "{context}: articulations must be unique, with at most four marks."
                        ));
                    }
                    let end = &mut ends[usize::from(note.layer)];
                    if note.start < *end {
                        return Err(format!(
                            "{context}: events overlap or are not in chronological order."
                        ));
                    }
                    let ticks = note
                        .checked_ticks()
                        .map_err(|error| format!("{context}: {error}"))?;
                    *end = note
                        .start
                        .checked_add(ticks)
                        .ok_or_else(|| format!("{context}: invalid event position."))?;
                    if *end > length {
                        return Err(format!("{context}: event extends past the bar line."));
                    }
                    if note.velocity > 127 {
                        return Err(format!("{context}: velocity must be 0–127."));
                    }
                    if note.pitches.len() > 16 {
                        return Err(format!(
                            "{context}: a chord can contain at most 16 pitches."
                        ));
                    }
                    if note.tied && note.pitches.is_empty() {
                        return Err(format!("{context}: a rest cannot be tied."));
                    }
                    let mut seen = [false; 128];
                    for pitch in &note.pitches {
                        let midi = pitch.midi_number();
                        if !(-2..=2).contains(&pitch.alter) || !(0..=127).contains(&midi) {
                            return Err(format!(
                                "{context}: pitch is outside MIDI range or has an unsupported accidental."
                            ));
                        }
                        if seen[midi as usize] {
                            return Err(format!(
                                "{context}: chord contains a duplicate sounding pitch."
                            ));
                        }
                        seen[midi as usize] = true;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|error| format!("Cannot open score: {error}"))?;
        if file.metadata().map_err(|error| error.to_string())?.len() > MAX_FILE_BYTES {
            return Err("Score file exceeds 16 MiB.".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Cannot read score: {error}"))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Score file exceeds 16 MiB.".into());
        }
        let score: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid Encore score: {error}"))?;
        score.validate()?;
        Ok(score)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Score file exceeds 16 MiB.".into());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let name = path
            .file_name()
            .ok_or_else(|| "Choose a score filename.".to_string())?
            .to_string_lossy();
        let temporary =
            path.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), stamp));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| format!("Cannot create score: {error}"))?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| format!("Cannot write score: {error}"))?;
            fs::rename(&temporary, path).map_err(|error| format!("Cannot replace score: {error}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

fn demo_note(start: u32, duration: Duration, midis: &[u8], velocity: u8) -> Note {
    Note {
        articulations: Vec::new(),
        start,
        duration,
        dotted: false,
        tuplet: None,
        layer: 0,
        pitches: midis.iter().map(|m| Pitch::from_midi(*m)).collect(),
        velocity,
        staccato: false,
        tied: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instrument_catalog_covers_each_gm_program_once() {
        assert_eq!(Instrument::ALL.len(), 128);
        assert_eq!(Instrument::LEGACY.len(), 18);
        let mut seen = [false; 128];
        for (program, &instrument) in Instrument::ALL.iter().enumerate() {
            assert_eq!(usize::from(instrument.gm_program()), program);
            assert!(!seen[program]);
            seen[program] = true;
            assert_eq!(Instrument::from_gm_program(program as u8), Some(instrument));
            assert!(!instrument.name().is_empty());
            assert!(!instrument.family().is_empty());
        }
        assert!(seen.into_iter().all(|present| present));
        for program in 128..=255 {
            assert_eq!(Instrument::from_gm_program(program), None);
        }
        for &instrument in Instrument::LEGACY {
            assert_eq!(
                Instrument::from_gm_program(instrument.gm_program()),
                Some(instrument)
            );
        }
        assert_eq!(Instrument::Timpani.family(), "Percussion");
        assert_eq!(Instrument::GeneralMidi(47).family(), "Percussion");
        assert_eq!(Instrument::GeneralMidi(116).family(), "Percussion");
        assert_eq!(Instrument::Violin.family(), "Strings");
        let families: std::collections::HashSet<_> = Instrument::ALL
            .iter()
            .map(|instrument| instrument.family())
            .collect();
        assert_eq!(families.len(), 16);
        assert_eq!(Instrument::GeneralMidi(127).name(), "Gunshot");
        assert_eq!(Instrument::GeneralMidi(255).name(), "Unknown instrument");
    }

    #[test]
    fn native_instruments_preserve_legacy_names_and_new_programs() {
        assert_eq!(
            serde_json::to_string(&Instrument::Piano).unwrap(),
            "\"Piano\""
        );
        assert_eq!(
            serde_json::to_string(&Instrument::DoubleBass).unwrap(),
            "\"DoubleBass\""
        );
        assert_eq!(
            serde_json::to_string(&Instrument::GeneralMidi(19)).unwrap(),
            "{\"GeneralMidi\":19}"
        );
        let mut score = Score::blank();
        for &instrument in Instrument::ALL {
            score.parts[0].instrument = instrument;
            let json = serde_json::to_string(&score).unwrap();
            let restored: Score = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, score);
            restored.validate().unwrap();
        }
        // Explicit aliases are accepted and retain the selected program.
        let alias: Instrument = serde_json::from_str("{\"GeneralMidi\":43}").unwrap();
        assert_eq!(alias.gm_program(), Instrument::DoubleBass.gm_program());
        assert_eq!(alias.default_clef(), Clef::Bass);
    }

    #[test]
    fn invalid_gm_programs_are_rejected_before_playback_or_export() {
        let mut score = Score::blank();
        for program in 128..=255 {
            score.parts[0].instrument = Instrument::GeneralMidi(program);
            assert!(score.validate().unwrap_err().contains("instrument program"));
        }
    }

    #[test]
    fn demo_and_blank_are_valid() {
        Score::blank().validate().unwrap();
        let score = Score::demo();
        score.validate().unwrap();
        assert_eq!(score.measure_count(), 8);
        assert_eq!(score.parts.len(), 6);
    }
    #[test]
    fn pitch_roundtrips_every_midi_note() {
        for midi in 0..=127 {
            assert_eq!(Pitch::from_midi(midi).midi(), midi);
        }
        assert_eq!(Pitch::from_staff_step(28, 0).name(), "C4");
        assert_eq!(Pitch::from_staff_step(31, 2).alter, 1);
        assert_eq!(Pitch::from_staff_step(34, -1).alter, -1);
    }
    #[test]
    fn validates_overlap_and_bar_bounds() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![
            demo_note(0, Duration::Half, &[60], 80),
            demo_note(480, Duration::Quarter, &[64], 80),
        ];
        assert!(score.validate().unwrap_err().contains("overlap"));
        score.parts[0].measures[0].notes = vec![demo_note(u32::MAX, Duration::Whole, &[60], 80)];
        assert!(score.validate().is_err());
    }
    #[test]
    fn validates_ranges_and_structure() {
        let mut score = Score::blank();
        score.parts[0].pan = f32::NAN;
        assert!(score.validate().is_err());
        score.parts[0].pan = 0.0;
        score.time_denominator = 0;
        assert!(score.validate().is_err());
        score.time_denominator = 4;
        score.parts[1].measures.pop();
        assert!(score.validate().is_err());
        score.normalize();
        score.validate().unwrap();
    }
    #[test]
    fn added_parts_preserve_short_score_length() {
        let mut score = Score::blank();
        for part in &mut score.parts {
            part.measures.truncate(1);
        }
        score.add_part(Instrument::Flute);
        score.validate().unwrap();
        assert_eq!(score.parts[2].measures.len(), 1);
    }
    #[test]
    fn invalid_pitches_and_duplicate_chords_are_rejected() {
        let mut score = Score::blank();
        let mut n = demo_note(0, Duration::Quarter, &[60], 80);
        n.pitches[0].octave = 127;
        score.parts[0].measures[0].notes = vec![n];
        assert!(score.validate().is_err());
        score.parts[0].measures[0].notes = vec![demo_note(0, Duration::Quarter, &[60, 60], 80)];
        assert!(score.validate().unwrap_err().contains("duplicate"));
    }
    #[test]
    fn native_roundtrip_and_rejected_corruption() {
        let path =
            std::env::temp_dir().join(format!("encore-model-test-{}.encore", std::process::id()));
        let score = Score::demo();
        score.save(&path).unwrap();
        assert_eq!(Score::load(&path).unwrap(), score);
        let mut bad = score;
        bad.tempo = 0;
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(Score::load(&path).is_err());
        let _ = fs::remove_file(path);
    }
    #[test]
    fn extended_rhythm_values_and_tuplets_keep_exact_native_time() {
        for (duration, ticks) in [
            (Duration::DoubleWhole, 3840),
            (Duration::ThirtySecond, 60),
            (Duration::SixtyFourth, 30),
            (Duration::OneTwentyEighth, 15),
        ] {
            assert_eq!(duration.ticks(), ticks);
            assert_eq!(
                Duration::from_musicxml_type(duration.musicxml_type()),
                Some(duration)
            );
        }
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = (0..3)
            .map(|index| {
                let mut note = demo_note(index * 160, Duration::Eighth, &[60], 80);
                note.tuplet = Some(Tuplet::TRIPLET);
                assert_eq!(note.checked_ticks().unwrap(), 160);
                note
            })
            .collect();
        score.validate().unwrap();
        let restored: Score =
            serde_json::from_str(&serde_json::to_string(&score).unwrap()).unwrap();
        assert_eq!(restored, score);
        let mut quintuplet = demo_note(0, Duration::Eighth, &[60], 80);
        quintuplet.tuplet = Some(Tuplet {
            actual: 5,
            normal: 4,
        });
        assert_eq!(quintuplet.checked_ticks().unwrap(), 192);
        quintuplet.tuplet = Some(Tuplet {
            actual: 7,
            normal: 4,
        });
        assert!(quintuplet.checked_ticks().unwrap_err().contains("480-tick"));
        quintuplet.tuplet = Some(Tuplet {
            actual: 0,
            normal: 2,
        });
        assert!(quintuplet.checked_ticks().is_err());
        let mut shortest = demo_note(0, Duration::OneTwentyEighth, &[60], 80);
        shortest.dotted = true;
        assert!(shortest.checked_ticks().is_err());
        // Combined rational arithmetic preserves an exact dotted tuplet even
        // when its intermediate unmodified dotted value would be fractional.
        shortest.tuplet = Some(Tuplet::TRIPLET);
        assert_eq!(shortest.checked_ticks().unwrap(), 15);
    }

    #[test]
    fn optional_tuplet_metadata_keeps_existing_native_scores_compatible() {
        let note = demo_note(0, Duration::Quarter, &[60], 80);
        let json = serde_json::to_string(&note).unwrap();
        assert!(!json.contains("tuplet"));
        assert_eq!(serde_json::from_str::<Note>(&json).unwrap(), note);
    }

    #[test]
    fn four_layers_overlap_independently_and_preserve_native_compatibility() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = (0..4)
            .map(|layer| {
                let mut note = demo_note(0, Duration::Whole, &[60 + layer], 80);
                note.layer = layer;
                note
            })
            .collect();
        score.validate().unwrap();
        let json = serde_json::to_string(&score).unwrap();
        let restored: Score = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, score);
        assert!(
            !serde_json::to_string(&score.parts[0].measures[0].notes[0])
                .unwrap()
                .contains("layer")
        );
        let mut collision = demo_note(480, Duration::Quarter, &[65], 80);
        collision.layer = 1;
        score.parts[0].measures[0].notes.push(collision);
        score.normalize();
        assert!(score.validate().unwrap_err().contains("overlap"));
        score.parts[0].measures[0]
            .notes
            .retain(|note| note.start == 0);
        score.parts[0].measures[0].notes[0].layer = 4;
        assert!(score.validate().unwrap_err().contains("layer must be"));
    }

    #[test]
    fn articulations_preserve_old_native_notes_and_reject_duplicates() {
        let old = r#"{"start":0,"duration":"Quarter","dotted":false,"pitches":[{"step":"C","octave":4,"alter":0}],"velocity":80,"staccato":true,"tied":false}"#;
        let restored: Note = serde_json::from_str(old).unwrap();
        assert!(restored.articulations.is_empty());
        assert!(
            !serde_json::to_string(&restored)
                .unwrap()
                .contains("articulations")
        );
        let mut score = Score::blank();
        let mut note = restored;
        note.articulations = Articulation::ALL.to_vec();
        score.parts[0].measures[0].notes.push(note);
        score.validate().unwrap();
        let saved = serde_json::to_string(&score).unwrap();
        assert_eq!(serde_json::from_str::<Score>(&saved).unwrap(), score);
        score.parts[0].measures[0].notes[0]
            .articulations
            .push(Articulation::Accent);
        assert!(
            score
                .validate()
                .unwrap_err()
                .contains("articulations must be unique")
        );
        score.parts[0].measures[0].notes[0].articulations = vec![Articulation::Tenuto; 2];
        assert!(score.validate().is_err());
    }

    #[test]
    fn ordinary_articulation_performance_is_bounded_and_legacy_timing_is_exact() {
        let mut note = demo_note(0, Duration::Quarter, &[60], 80);
        assert_eq!((note.playback_velocity(), note.playback_ticks()), (80, 480));
        assert_eq!(note.playback_shortening().to_bits(), 0.0_f64.to_bits());
        note.staccato = true;
        assert_eq!(note.playback_ticks(), 240);
        assert_eq!(note.playback_shortening().to_bits(), 0.52_f64.to_bits());
        note.staccato = false;
        for (articulation, velocity, ticks) in [
            (Articulation::Accent, 92, 480),
            (Articulation::Marcato, 98, 345),
            (Articulation::Tenuto, 83, 480),
            (Articulation::Staccatissimo, 80, 144),
        ] {
            note.articulations = vec![articulation];
            assert_eq!(
                (note.playback_velocity(), note.playback_ticks()),
                (velocity, ticks)
            );
        }
        note.articulations = Articulation::ALL.to_vec();
        assert_eq!((note.playback_velocity(), note.playback_ticks()), (98, 144));
        note.velocity = 126;
        assert_eq!(note.playback_velocity(), 127);
        note.velocity = 0;
        assert_eq!(note.playback_velocity(), 0);
    }
}
