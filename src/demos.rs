//! Original instrumental orchestration of the traditional Ukrainian Shchedryk
//! four-note cell, also known as Carol of the Bells. There are no lyrics and no
//! material from a modern arrangement. Concert pitch throughout; the deliberate
//! 4/4 expansion leaves room for the low drums and bowed bass to resonate.
//!
//! The 36-bar arc lasts exactly 80 seconds at quarter note = 108, before the
//! synthesizer's final release tail. Section bounds below are 1-based, inclusive.
use crate::model::{Clef, Duration, Instrument, Measure, Note, PPQ, Part, Pitch, Score};

pub const CAROL_SECTIONS: &[(&str, usize, usize)] = &[
    ("The bells awaken · solo low timpani", 1, 4),
    ("A bow in the dark · sustained double bass", 5, 5),
    ("Gathering shadows · the strings answer", 6, 12),
    ("The procession · winds and brass emerge", 13, 20),
    ("Snow and starlight · a quiet clearing", 21, 24),
    ("The ascent · climbing strings", 25, 28),
    ("Winter unbound · the orchestra opens", 29, 34),
    ("The gates of morning · a held final chord", 35, 36),
];

const BARS: usize = 36;
const TIMPANI: usize = 0;
const BASS: usize = 1;
const VIOLIN_I: usize = 2;
const VIOLIN_II: usize = 3;
const VIOLA: usize = 4;
const CELLO: usize = 5;
const FLUTE: usize = 6;
const OBOE: usize = 7;
const CLARINET: usize = 8;
const BASSOON: usize = 9;
const HORN: usize = 10;
const TRUMPET: usize = 11;
const TROMBONE: usize = 12;
const TUBA: usize = 13;
const HARP: usize = 14;
const PIANO: usize = 15;

#[derive(Clone, Copy)]
struct Harmony {
    bass: u8,
    // Compact concert-pitch voicings, starting near the third octave.
    triad: [u8; 3],
}
const E_MINOR: Harmony = Harmony {
    bass: 28,
    triad: [52, 55, 59],
};
const C_MAJOR: Harmony = Harmony {
    bass: 36,
    triad: [48, 52, 55],
};
const G_MAJOR: Harmony = Harmony {
    bass: 31,
    triad: [55, 59, 62],
};
const D_MAJOR: Harmony = Harmony {
    bass: 38,
    triad: [50, 54, 57],
};
const A_MINOR: Harmony = Harmony {
    bass: 33,
    triad: [57, 60, 64],
};
const B_MAJOR: Harmony = Harmony {
    bass: 35,
    triad: [47, 51, 54],
};

/// An epic, deliberately paced orchestral reimagining with an exposed timpani
/// introduction, bowed E1 bass entrance, quiet center, and a tied tutti ending.
pub fn carol_of_the_bells() -> Score {
    let specification = [
        (Instrument::Timpani, "Timpani · low bells", 0.72, 0.06),
        (Instrument::DoubleBass, "Double bass · arco", 0.66, 0.35),
        (Instrument::Violin, "Violin I", 0.50, -0.62),
        (Instrument::Violin, "Violin II", 0.40, -0.35),
        (Instrument::Viola, "Viola", 0.38, -0.12),
        (Instrument::Cello, "Cello", 0.45, 0.20),
        (Instrument::Flute, "Flute", 0.32, -0.15),
        (Instrument::Oboe, "Oboe", 0.28, 0.10),
        (Instrument::Clarinet, "Clarinet", 0.30, 0.25),
        (Instrument::Bassoon, "Bassoon", 0.33, -0.05),
        (Instrument::FrenchHorn, "Horns", 0.40, -0.40),
        (Instrument::Trumpet, "Trumpets", 0.29, 0.20),
        (Instrument::Trombone, "Trombones", 0.34, 0.35),
        (Instrument::Tuba, "Tuba", 0.32, 0.10),
        (Instrument::Harp, "Harp", 0.26, -0.70),
        (Instrument::Piano, "Piano · bell accents", 0.28, 0.65),
    ];
    let parts = specification
        .into_iter()
        .map(|(instrument, name, volume, pan)| Part {
            name: name.into(),
            instrument,
            percussion: None,
            clef: instrument.default_clef(),
            measures: vec![Measure::default(); BARS],
            volume,
            pan,
            muted: false,
            solo: false,
        })
        .collect();
    let mut score = Score {
        title: "Carol of the Bells — Winter Ascendant".into(),
        composer: "Traditional Ukrainian melody · Original Encore orchestration".into(),
        tempo: 108,
        time_numerator: 4,
        time_denominator: 4,
        key_fifths: 1,
        parts,
        notation: Default::default(),
    };
    score.parts[HORN].clef = Clef::Treble;

    // Bars 1–4: exactly the accepted exposed G2–F♯2–G2–E2 statement.
    // No string, piano, wind, or brass note is scheduled before bar five.
    for bar in 0..4 {
        motif(
            &mut score,
            TIMPANI,
            bar,
            [43, 42, 43, 40],
            78 + bar as u8 * 5,
        );
    }
    // Bar 5: one new attack only: bowed double bass E1, a full whole note.
    // The tie carries its bow into the first answering cello phrase in bar 6.
    held(&mut score, BASS, 4, &[28], 67, true);
    held(&mut score, BASS, 5, &[28], 67, false);

    // Bars 6–12: individual string voices accumulate before the first brass call.
    let gathering = [
        E_MINOR, E_MINOR, C_MAJOR, G_MAJOR, D_MAJOR, A_MINOR, B_MAJOR,
    ];
    for (i, harmony) in gathering.into_iter().enumerate() {
        let bar = i + 5;
        let velocity = 55 + i as u8 * 4;
        if bar > 5 {
            held(&mut score, BASS, bar, &[harmony.bass], velocity, false);
        }
        if bar == 5 {
            line(
                &mut score,
                CELLO,
                bar,
                &[
                    (0, Duration::Half, 52),
                    (960, Duration::Quarter, 55),
                    (1440, Duration::Quarter, 54),
                ],
                59,
            );
        } else {
            motif(
                &mut score,
                CELLO,
                bar,
                [
                    harmony.triad[0],
                    harmony.triad[1],
                    harmony.triad[2],
                    harmony.triad[0],
                ],
                velocity,
            );
        }
        if bar >= 6 {
            motif(&mut score, VIOLIN_II, bar, [67, 66, 67, 64], velocity + 3);
        }
        if bar >= 7 {
            held(
                &mut score,
                VIOLA,
                bar,
                &[harmony.triad[1], harmony.triad[2]],
                velocity - 7,
                false,
            );
            arpeggio(&mut score, HARP, bar, harmony, 12, velocity - 10, false);
        }
        if bar >= 8 {
            let phrases = [
                [
                    (0, Duration::Half, 76),
                    (960, Duration::Quarter, 74),
                    (1440, Duration::Quarter, 71),
                ],
                [
                    (0, Duration::Half, 78),
                    (960, Duration::Quarter, 76),
                    (1440, Duration::Quarter, 74),
                ],
                [
                    (0, Duration::Half, 81),
                    (960, Duration::Quarter, 79),
                    (1440, Duration::Quarter, 76),
                ],
                [
                    (0, Duration::Half, 78),
                    (960, Duration::Quarter, 75),
                    (1440, Duration::Quarter, 71),
                ],
            ];
            line(&mut score, VIOLIN_I, bar, &phrases[bar - 8], velocity + 8);
        }
        if bar == 9 || bar == 11 {
            line(
                &mut score,
                CLARINET,
                bar,
                &[
                    (0, Duration::Half, harmony.triad[2] + 12),
                    (960, Duration::Half, harmony.triad[1] + 12),
                ],
                velocity - 4,
            );
        }
        if bar >= 10 {
            held(
                &mut score,
                HORN,
                bar,
                &[harmony.triad[0], harmony.triad[2]],
                velocity - 7,
                false,
            );
        }
    }
    push(
        &mut score,
        PIANO,
        11,
        1440,
        Duration::Quarter,
        &[83],
        70,
        false,
    );

    // Bars 13–20: a processional pulse underneath a melody that rises, answers,
    // and changes with the harmony instead of looping one unchanged ostinato.
    let procession = [
        E_MINOR, C_MAJOR, G_MAJOR, D_MAJOR, E_MINOR, C_MAJOR, A_MINOR, B_MAJOR,
    ];
    let statements = [
        [79, 78, 79, 76],
        [79, 78, 79, 76],
        [83, 81, 83, 79],
        [81, 79, 81, 78],
        [79, 78, 79, 76],
        [76, 74, 76, 72],
        [81, 79, 81, 76],
        [78, 76, 78, 75],
    ];
    for (i, harmony) in procession.into_iter().enumerate() {
        let bar = i + 12;
        let velocity = 78 + i as u8 * 2;
        motif(&mut score, VIOLIN_I, bar, statements[i], velocity + 4);
        arpeggio(&mut score, VIOLIN_II, bar, harmony, 12, velocity - 5, true);
        quarters(
            &mut score,
            VIOLA,
            bar,
            [
                harmony.triad[1],
                harmony.triad[2],
                harmony.triad[1] + 12,
                harmony.triad[2],
            ],
            velocity - 10,
            false,
        );
        quarters(
            &mut score,
            CELLO,
            bar,
            [
                harmony.triad[0] - 12,
                harmony.triad[2] - 12,
                harmony.triad[0],
                harmony.triad[2] - 12,
            ],
            velocity - 4,
            true,
        );
        held(&mut score, BASS, bar, &[harmony.bass], velocity - 5, false);
        if i % 2 == 0 {
            line(
                &mut score,
                OBOE,
                bar,
                &[
                    (0, Duration::Half, statements[i][3]),
                    (960, Duration::Quarter, statements[i][0] - 2),
                    (1440, Duration::Quarter, statements[i][3] - 2),
                ],
                velocity - 8,
            );
            held(
                &mut score,
                BASSOON,
                bar,
                &[harmony.triad[0]],
                velocity - 10,
                false,
            );
        } else {
            quarters(
                &mut score,
                FLUTE,
                bar,
                [
                    statements[i][3] + 12,
                    statements[i][0] + 12,
                    statements[i][1] + 12,
                    statements[i][3] + 12,
                ],
                velocity - 11,
                false,
            );
            line(
                &mut score,
                CLARINET,
                bar,
                &[
                    (0, Duration::Half, harmony.triad[2] + 12),
                    (960, Duration::Half, harmony.triad[1] + 12),
                ],
                velocity - 9,
            );
        }
        horn_call(&mut score, HORN, bar, harmony, velocity - 6);
        if i >= 4 {
            horn_call(&mut score, TRUMPET, bar, harmony, velocity - 7);
            held(
                &mut score,
                TROMBONE,
                bar,
                &[harmony.triad[0] - 12, harmony.triad[2] - 12],
                velocity - 9,
                false,
            );
        }
        if i >= 6 {
            held(
                &mut score,
                TUBA,
                bar,
                &[harmony.triad[0] - 12],
                velocity - 6,
                false,
            );
        }
        let drum = harmony.triad[0] - 12;
        push(
            &mut score,
            TIMPANI,
            bar,
            0,
            Duration::Quarter,
            &[drum],
            velocity + 2,
            false,
        );
        push(
            &mut score,
            TIMPANI,
            bar,
            960,
            Duration::Quarter,
            &[drum],
            velocity - 6,
            false,
        );
        if i >= 6 {
            push(
                &mut score,
                TIMPANI,
                bar,
                1680,
                Duration::Eighth,
                &[drum + 7],
                velocity,
                false,
            );
        }
        arpeggio(&mut score, HARP, bar, harmony, 12, velocity - 20, false);
        if i % 2 == 0 {
            push(
                &mut score,
                PIANO,
                bar,
                0,
                Duration::Half,
                &shift(harmony.triad, 12),
                velocity - 6,
                false,
            );
        }
    }

    // Bars 21–24: withdraw the percussion, upper string pulse, and brass.
    // Woodwind phrases breathe over a soft bowed floor and harp reflections.
    let clearing = [E_MINOR, C_MAJOR, A_MINOR, B_MAJOR];
    for (i, harmony) in clearing.into_iter().enumerate() {
        let bar = i + 20;
        held(&mut score, BASS, bar, &[harmony.bass], 43, false);
        held(&mut score, CELLO, bar, &[harmony.triad[0]], 45, false);
        held(
            &mut score,
            VIOLIN_I,
            bar,
            &[if i % 2 == 0 { 83 } else { 79 }],
            42,
            false,
        );
        if i % 2 == 0 {
            line(
                &mut score,
                FLUTE,
                bar,
                &[
                    (0, Duration::Half, 83),
                    (960, Duration::Quarter, 79),
                    (1440, Duration::Quarter, 78),
                ],
                56,
            );
            line(
                &mut score,
                OBOE,
                bar,
                &[(0, Duration::Half, 76), (960, Duration::Half, 74)],
                48,
            );
        } else {
            line(
                &mut score,
                CLARINET,
                bar,
                &[
                    (0, Duration::Quarter, 71),
                    (480, Duration::Quarter, 72),
                    (960, Duration::Half, if i == 3 { 75 } else { 76 }),
                ],
                54,
            );
            line(
                &mut score,
                OBOE,
                bar,
                &[
                    (0, Duration::Half, 76),
                    (960, Duration::Quarter, 74),
                    (1440, Duration::Quarter, 71),
                ],
                53,
            );
        }
        arpeggio(&mut score, HARP, bar, harmony, 12, 46, false);
        if i == 3 {
            held(&mut score, BASSOON, bar, &[47], 48, false);
            push(
                &mut score,
                HORN,
                bar,
                960,
                Duration::Half,
                &[59, 63],
                46,
                false,
            );
            push(
                &mut score,
                PIANO,
                bar,
                1440,
                Duration::Quarter,
                &[75, 83],
                50,
                false,
            );
        }
    }

    // Bars 25–28: the ascending string lines span two octaves in successive
    // waves; the last bar becomes a dominant-seventh arpeggio into the tutti.
    let ascent = [E_MINOR, C_MAJOR, D_MAJOR, B_MAJOR];
    let scales = [
        [64, 66, 67, 69, 71, 72, 74, 76],
        [67, 69, 71, 72, 74, 76, 78, 79],
        [66, 67, 69, 71, 72, 74, 76, 78],
        [63, 66, 69, 71, 75, 78, 81, 83],
    ];
    for (i, harmony) in ascent.into_iter().enumerate() {
        let bar = i + 24;
        let velocity = 72 + i as u8 * 8;
        eighths(&mut score, VIOLIN_I, bar, scales[i], velocity + 4, false);
        eighths(
            &mut score,
            VIOLIN_II,
            bar,
            scales[i].map(|pitch| pitch - 12),
            velocity - 3,
            false,
        );
        arpeggio(&mut score, VIOLA, bar, harmony, 0, velocity - 7, true);
        arpeggio(&mut score, CELLO, bar, harmony, -12, velocity, true);
        push(
            &mut score,
            BASS,
            bar,
            0,
            Duration::Half,
            &[harmony.bass],
            velocity,
            false,
        );
        push(
            &mut score,
            BASS,
            bar,
            960,
            Duration::Half,
            &[harmony.bass + 12],
            velocity - 4,
            false,
        );
        horn_call(&mut score, HORN, bar, harmony, velocity - 3);
        line(
            &mut score,
            BASSOON,
            bar,
            &[
                (0, Duration::Half, harmony.triad[0]),
                (960, Duration::Half, harmony.triad[2]),
            ],
            velocity - 9,
        );
        if i >= 1 {
            line(
                &mut score,
                CLARINET,
                bar,
                &[
                    (0, Duration::Half, scales[i][2]),
                    (960, Duration::Half, scales[i][6]),
                ],
                velocity - 8,
            );
        }
        if i >= 2 {
            line(
                &mut score,
                FLUTE,
                bar,
                &[
                    (0, Duration::Half, scales[i][4] + 12),
                    (960, Duration::Half, scales[i][7] + 12),
                ],
                velocity - 8,
            );
            horn_call(&mut score, TRUMPET, bar, harmony, velocity - 3);
            quarters(
                &mut score,
                TROMBONE,
                bar,
                [
                    harmony.triad[0],
                    harmony.triad[2],
                    harmony.triad[1],
                    harmony.triad[2],
                ],
                velocity - 7,
                true,
            );
            held(
                &mut score,
                TUBA,
                bar,
                &[harmony.triad[0] - 12],
                velocity - 4,
                false,
            );
        }
        let drum = harmony.triad[0] - 12;
        push(
            &mut score,
            TIMPANI,
            bar,
            0,
            Duration::Half,
            &[drum],
            velocity + 4,
            false,
        );
        for pulse in 0..4 {
            push(
                &mut score,
                TIMPANI,
                bar,
                960 + pulse * 240,
                Duration::Eighth,
                &[if pulse % 2 == 0 { drum } else { drum + 7 }],
                velocity + 4 + pulse as u8 * 2,
                false,
            );
        }
        arpeggio(&mut score, HARP, bar, harmony, 24, velocity - 18, false);
        push(
            &mut score,
            PIANO,
            bar,
            0,
            Duration::Quarter,
            &shift(harmony.triad, 12),
            velocity,
            false,
        );
        push(
            &mut score,
            PIANO,
            bar,
            1440,
            Duration::Quarter,
            &shift(harmony.triad, 24),
            velocity + 3,
            false,
        );
    }

    // Bars 29–34: the cell returns high and broad. Brass answers in lower
    // octaves, winds move in contrary phrases, strings carry the forward pulse.
    let summit = [E_MINOR, C_MAJOR, G_MAJOR, B_MAJOR, E_MINOR, B_MAJOR];
    let summit_melody = [
        [79, 78, 79, 76],
        [79, 78, 79, 76],
        [83, 81, 83, 79],
        [78, 76, 78, 75],
        [91, 90, 91, 88],
        [90, 93, 90, 87],
    ];
    for (i, harmony) in summit.into_iter().enumerate() {
        let bar = i + 28;
        let velocity = if i < 4 { 105 } else { 114 };
        motif(&mut score, VIOLIN_I, bar, summit_melody[i], velocity);
        arpeggio(&mut score, VIOLIN_II, bar, harmony, 12, velocity - 8, true);
        arpeggio(&mut score, VIOLA, bar, harmony, 0, velocity - 12, true);
        quarters(
            &mut score,
            CELLO,
            bar,
            [
                harmony.triad[0] - 12,
                harmony.triad[2] - 12,
                harmony.triad[0],
                harmony.triad[2] - 12,
            ],
            velocity - 5,
            true,
        );
        push(
            &mut score,
            BASS,
            bar,
            0,
            Duration::Half,
            &[harmony.bass],
            velocity - 4,
            false,
        );
        push(
            &mut score,
            BASS,
            bar,
            960,
            Duration::Half,
            &[harmony.bass],
            velocity - 9,
            false,
        );
        motif(
            &mut score,
            FLUTE,
            bar,
            summit_melody[i].map(|pitch| if pitch < 84 { pitch + 12 } else { pitch }),
            velocity - 10,
        );
        quarters(
            &mut score,
            OBOE,
            bar,
            [
                summit_melody[i][0] - 12,
                summit_melody[i][1] - 12,
                summit_melody[i][3] - 12,
                harmony.triad[2] + 12,
            ],
            velocity - 12,
            false,
        );
        line(
            &mut score,
            CLARINET,
            bar,
            &[
                (0, Duration::Half, harmony.triad[2] + 12),
                (960, Duration::Quarter, harmony.triad[1] + 12),
                (1440, Duration::Quarter, harmony.triad[0] + 12),
            ],
            velocity - 11,
        );
        quarters(
            &mut score,
            BASSOON,
            bar,
            [
                harmony.triad[0],
                harmony.triad[2],
                harmony.triad[1],
                harmony.triad[0],
            ],
            velocity - 9,
            true,
        );
        motif(
            &mut score,
            HORN,
            bar,
            summit_melody[i].map(|pitch| pitch - 24),
            velocity - 3,
        );
        horn_call(&mut score, TRUMPET, bar, harmony, velocity - 2);
        horn_call(&mut score, TROMBONE, bar, harmony, velocity - 5);
        held(
            &mut score,
            TUBA,
            bar,
            &[harmony.triad[0] - 12],
            velocity - 5,
            false,
        );
        for pulse in 0..8 {
            push(
                &mut score,
                TIMPANI,
                bar,
                pulse * 240,
                Duration::Eighth,
                &[if pulse % 2 == 0 {
                    harmony.triad[0] - 12
                } else {
                    harmony.triad[2] - 12
                }],
                velocity - 2 + if pulse % 4 == 0 { 6 } else { 0 },
                false,
            );
        }
        arpeggio(&mut score, HARP, bar, harmony, 24, velocity - 22, false);
        push(
            &mut score,
            PIANO,
            bar,
            0,
            Duration::Half,
            &shift(harmony.triad, 12),
            velocity - 3,
            false,
        );
        push(
            &mut score,
            PIANO,
            bar,
            960,
            Duration::Half,
            &shift(harmony.triad, 24),
            velocity - 7,
            false,
        );
    }

    // Bars 35–36: a single, broad E-minor arrival, tied across the final barline.
    // Each wind/brass staff represents a section; chords are concert-pitch divisi.
    let final_voicings: [&[u8]; 16] = [
        &[40],
        &[28],
        &[76, 83],
        &[67, 71],
        &[59, 64],
        &[40, 52],
        &[88],
        &[79],
        &[71],
        &[52],
        &[55, 59, 64],
        &[64, 67, 71],
        &[47, 52],
        &[40],
        &[52, 59, 64, 67, 71, 76],
        &[64, 67, 71, 76],
    ];
    for (part, pitches) in final_voicings.into_iter().enumerate() {
        let velocity = match part {
            TIMPANI => 119,
            HARP | PIANO => 96,
            FLUTE | OBOE | CLARINET => 101,
            _ => 109,
        };
        held(&mut score, part, 34, pitches, velocity, true);
        held(&mut score, part, 35, pitches, velocity, false);
    }
    score
}

#[allow(clippy::too_many_arguments)]
fn push(
    score: &mut Score,
    part: usize,
    bar: usize,
    start: u32,
    duration: Duration,
    pitches: &[u8],
    velocity: u8,
    staccato: bool,
) {
    score.parts[part].measures[bar].notes.push(Note {
        voice: 0,
        voice_launch: None,
        articulations: Vec::new(),
        start,
        duration,
        dotted: false,
        tuplet: None,
        layer: 0,
        pitches: pitches
            .iter()
            .map(|pitch| Pitch::from_midi(*pitch))
            .collect(),
        velocity,
        staccato,
        tied: false,
    });
}
fn held(score: &mut Score, part: usize, bar: usize, pitches: &[u8], velocity: u8, tied: bool) {
    push(
        score,
        part,
        bar,
        0,
        Duration::Whole,
        pitches,
        velocity,
        false,
    );
    score.parts[part].measures[bar]
        .notes
        .last_mut()
        .unwrap()
        .tied = tied;
}
fn motif(score: &mut Score, part: usize, bar: usize, pitches: [u8; 4], velocity: u8) {
    for ((start, duration), pitch) in [
        (0, Duration::Quarter),
        (PPQ, Duration::Eighth),
        (PPQ + PPQ / 2, Duration::Eighth),
        (PPQ * 2, Duration::Half),
    ]
    .into_iter()
    .zip(pitches)
    {
        push(score, part, bar, start, duration, &[pitch], velocity, false);
    }
}
fn line(score: &mut Score, part: usize, bar: usize, events: &[(u32, Duration, u8)], velocity: u8) {
    for &(start, duration, pitch) in events {
        push(score, part, bar, start, duration, &[pitch], velocity, false);
    }
}
fn quarters(
    score: &mut Score,
    part: usize,
    bar: usize,
    pitches: [u8; 4],
    velocity: u8,
    staccato: bool,
) {
    for (i, pitch) in pitches.into_iter().enumerate() {
        push(
            score,
            part,
            bar,
            i as u32 * PPQ,
            Duration::Quarter,
            &[pitch],
            velocity,
            staccato,
        );
    }
}
fn eighths(
    score: &mut Score,
    part: usize,
    bar: usize,
    pitches: [u8; 8],
    velocity: u8,
    staccato: bool,
) {
    for (i, pitch) in pitches.into_iter().enumerate() {
        push(
            score,
            part,
            bar,
            i as u32 * PPQ / 2,
            Duration::Eighth,
            &[pitch],
            velocity.saturating_sub(if i % 4 == 0 { 0 } else { 5 }),
            staccato,
        );
    }
}
fn shift(pitches: [u8; 3], semitones: i8) -> [u8; 3] {
    pitches.map(|pitch| (i16::from(pitch) + i16::from(semitones)) as u8)
}
fn arpeggio(
    score: &mut Score,
    part: usize,
    bar: usize,
    harmony: Harmony,
    semitones: i8,
    velocity: u8,
    staccato: bool,
) {
    let [root, third, fifth] = shift(harmony.triad, semitones);
    eighths(
        score,
        part,
        bar,
        [root, third, fifth, third, root + 12, third, fifth, third],
        velocity,
        staccato,
    );
}
fn horn_call(score: &mut Score, part: usize, bar: usize, harmony: Harmony, velocity: u8) {
    let pitches = if part == TRUMPET {
        shift(harmony.triad, 12)
    } else {
        harmony.triad
    };
    push(
        score,
        part,
        bar,
        0,
        Duration::Half,
        &pitches,
        velocity,
        false,
    );
    push(
        score,
        part,
        bar,
        960,
        Duration::Quarter,
        &pitches,
        velocity.saturating_sub(4),
        true,
    );
    push(
        score,
        part,
        bar,
        1440,
        Duration::Quarter,
        &pitches,
        velocity,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_arrangement_is_valid_and_eighty_seconds() {
        let score = carol_of_the_bells();
        score.validate().unwrap();
        assert_eq!(score.parts.len(), 16);
        assert_eq!(score.measure_count(), 36);
        assert_eq!(score.key_fifths, 1);
        let seconds = score.measure_count() as f32 * score.measure_ticks() as f32 / PPQ as f32
            * 60.0
            / f32::from(score.tempo);
        assert!((seconds - 80.0).abs() < 0.001);
    }
    #[test]
    fn intro_is_exactly_four_solo_low_timpani_statements() {
        let score = carol_of_the_bells();
        for bar in 0..4 {
            for (part, staff) in score.parts.iter().enumerate() {
                if part != TIMPANI {
                    assert!(
                        staff.measures[bar].notes.is_empty(),
                        "Unexpected entry in intro: {}",
                        staff.name
                    );
                }
            }
            let notes = &score.parts[TIMPANI].measures[bar].notes;
            assert_eq!(
                notes
                    .iter()
                    .map(|note| note.pitches[0].midi())
                    .collect::<Vec<_>>(),
                vec![43, 42, 43, 40]
            );
            assert_eq!(
                notes
                    .iter()
                    .map(|note| (note.start, note.ticks()))
                    .collect::<Vec<_>>(),
                vec![(0, 480), (480, 240), (720, 240), (960, 960)]
            );
        }
    }
    #[test]
    fn fifth_bar_has_only_one_bowed_low_e_whole_note() {
        let score = carol_of_the_bells();
        let attacks: Vec<_> = score
            .parts
            .iter()
            .enumerate()
            .flat_map(|(part, staff)| staff.measures[4].notes.iter().map(move |note| (part, note)))
            .collect();
        assert_eq!(attacks.len(), 1);
        let (part, note) = attacks[0];
        assert_eq!(part, BASS);
        assert_eq!(score.parts[part].instrument, Instrument::DoubleBass);
        assert!(score.parts[part].name.contains("arco"));
        assert_eq!(note.start, 0);
        assert_eq!(note.duration, Duration::Whole);
        assert_eq!(note.pitches[0].name(), "E1");
    }
    #[test]
    fn orchestration_builds_then_breathes_then_finishes_tied() {
        let score = carol_of_the_bells();
        let density = |start: usize, end: usize| {
            score
                .parts
                .iter()
                .map(|part| {
                    part.measures[start..end]
                        .iter()
                        .map(|bar| bar.notes.len())
                        .sum::<usize>()
                })
                .sum::<usize>()
        };
        assert!(density(28, 32) > density(0, 4) * 8);
        assert!(density(20, 24) < density(12, 16));
        for part in &score.parts {
            let penultimate = &part.measures[34].notes;
            let last = &part.measures[35].notes;
            assert_eq!(penultimate.len(), 1);
            assert_eq!(last.len(), 1);
            assert!(penultimate[0].tied);
            assert!(!last[0].tied);
            assert_eq!(penultimate[0].pitches, last[0].pitches);
            assert_eq!(last[0].ticks(), 4 * PPQ);
        }
    }
}
