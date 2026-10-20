//! Concert-pitch instrument guidance and short, register-appropriate auditions.
//!
//! Ranges are inclusive practical composing guides, not hard playback limits or
//! promises about every player. The original orchestral profiles exclude specialist extensions, pedal notes,
//! and harmonics. Additional GM voices use keyboard audition guides rather than
//! physical playing ranges; sampled effects can change character across the keyboard.
//! Double bass is a standard four-string instrument; trumpet uses
//! the C-trumpet compass; timpani describes a conventional four-drum set.
//!
//! Range references were checked against the manufacturer's instrument studies:
//! <https://www.vsl.co.at/academy> (strings, woodwinds, brass, percussion), notably
//! <https://www.vsl.co.at/academy/woodwinds/english-horn>,
//! <https://www.vsl.co.at/academy/woodwinds/clarinet>,
//! <https://www.vsl.co.at/academy/brass/horn-f>, and
//! <https://www.vsl.co.at/academy/brass/tenor-trombone>.
//! Conservative orchestral upper bounds are used where those studies also list
//! specialist extremes. The ensemble profile is a useful combined string compass.

use crate::model::{Duration, Instrument, Measure, Note, PPQ, Part, Pitch, Score};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstrumentProfile {
    /// Lowest guide pitch, inclusive. C4 is MIDI 60; added GM voices use keyboard guides.
    pub low: u8,
    /// Highest guide pitch, inclusive.
    pub high: u8,
    /// Starting pitch for the audition, chosen for the instrument's character.
    pub audition: u8,
    pub family: &'static str,
    pub character: &'static str,
    pub technique: &'static str,
}

pub fn profile(instrument: Instrument) -> InstrumentProfile {
    use Instrument::*;
    let (low, high, audition, family, character, technique) = match instrument {
        Piano => (
            21,
            108,
            60,
            "Keyboard",
            "A piano voice with a clear hammer attack and naturally fading string resonance.",
            "Listen to the detached attacks, broken chord, and the final note's natural decay.",
        ),
        Violin => (
            55,
            100,
            67,
            "Strings",
            "A bright bowed string voice with a slender, singing upper register.",
            "Compare the sustained notes with the shorter bow strokes in the eighth-note passage.",
        ),
        Viola => (
            48,
            93,
            55,
            "Strings",
            "A darker bowed string voice centered on the orchestra's inner voices.",
            "The lower-register phrase tests the warm attack and sustained middle-string sound.",
        ),
        Cello => (
            36,
            81,
            48,
            "Strings",
            "A resonant bowed string voice spanning bass support and a singing tenor register.",
            "The C3-based phrase compares a grounded attack with a held, lyrical final note.",
        ),
        DoubleBass => (
            28,
            67,
            28,
            "Strings",
            "A sustained bowed bass voice, distinct from the plucked acoustic bass in the Bass family.",
            "The phrase begins on sounding E1. Encore displays concert pitch, including this octave-transposing instrument.",
        ),
        Flute => (
            60,
            98,
            72,
            "Woodwinds",
            "An air-edged flute voice with a clear fundamental and a light upper octave.",
            "Listen for the breath-like attack, connected sustain, and lighter detached eighth notes.",
        ),
        Piccolo => (
            74,
            108,
            84,
            "Woodwinds",
            "A concentrated high flute voice that adds a bright top octave to an ensemble.",
            "The C6-based phrase is shown at sounding pitch; traditional piccolo parts are written an octave lower.",
        ),
        Oboe => (
            58,
            91,
            62,
            "Woodwinds",
            "A focused double reed voice with a firm attack and an expressive center.",
            "The D4-based phrase compares clearly tongued notes with an extended final sustain.",
        ),
        EnglishHorn => (
            52,
            81,
            55,
            "Woodwinds",
            "A lower double reed voice with a rounded, dark middle register.",
            "The G3-based audition stays in concert pitch; conventional English-horn parts transpose up a fifth.",
        ),
        Clarinet => (
            50,
            91,
            60,
            "Woodwinds",
            "A single reed voice with a round lower register and a brighter upper octave.",
            "Compare the low C4 attack with C5; the displayed pitches are sounding pitches for a B-flat clarinet compass.",
        ),
        Bassoon => (
            34,
            75,
            41,
            "Woodwinds",
            "A low double reed voice with a woody bass and a focused middle register.",
            "The audition starts on F2 so its bass character can be heard before the upper octave.",
        ),
        FrenchHorn => (
            41,
            77,
            48,
            "Brass",
            "A rounded brass voice with a soft attack and increasing brightness at stronger velocities.",
            "The C3-based phrase tests open sustained tone. Encore uses concert pitch rather than traditional F-horn transposition.",
        ),
        Trumpet => (
            54,
            84,
            60,
            "Brass",
            "A compact brass voice with a clear attack and a brilliant upper register.",
            "Compare the tongued eighth notes and the sustained ending; the guide uses a C-trumpet sounding range.",
        ),
        Trombone => (
            40,
            70,
            48,
            "Brass",
            "A direct brass voice with a firm lower middle register and a broad sustained tone.",
            "The C3-based phrase tests separated attacks and long tone; it does not add a slide glissando.",
        ),
        Tuba => (
            26,
            65,
            29,
            "Brass",
            "A deep brass voice with a rounded fundamental and a substantial attack.",
            "The audition starts on F1 to expose the low foundation before moving through the next octave.",
        ),
        Harp => (
            24,
            103,
            60,
            "Plucked strings",
            "A plucked string voice with a crisp onset and a freely decaying resonance.",
            "The broken-chord passage tests separation between plucks; the final whole note lets the resonance fade.",
        ),
        Timpani => (
            38,
            57,
            38,
            "Percussion",
            "A tuned drum voice with a low mallet impact and a decaying, inharmonic ring.",
            "The phrase alternates D2 and A2 without rolls, then leaves a rest for the last stroke to decay.",
        ),
        GeneralMidi(program) => return additional_profile(program),
        StringEnsemble => (
            36,
            96,
            60,
            "Strings",
            "A blended bowed string section with a broader sustain than the solo voices.",
            "The audition compares short section-like strokes with a held ending; this combined voice has no separate player parts.",
        ),
    };
    InstrumentProfile {
        low,
        high,
        audition,
        family,
        character,
        technique,
    }
}

/// Useful keyboard registers for the additional sampled presets. These are
/// audition guides, not claims about an acoustic instrument's physical compass.
fn additional_profile(program: u8) -> InstrumentProfile {
    if let Some(instrument) = Instrument::from_gm_program(program)
        && !matches!(instrument, Instrument::GeneralMidi(_))
    {
        return profile(instrument);
    }
    let (low, high, audition, character, technique) = match program {
        0..=7 => (
            36,
            96,
            60,
            "A keyboard preset with its own attack, resonance, and decay.",
            "Compare the detached notes, broken chord, and held ending across the middle keyboard register.",
        ),
        8..=15 => (
            48,
            96,
            60,
            "A pitched mallet or plucked keyboard color with a distinct attack and decay.",
            "This is a keyboard audition guide. Listen for the initial strike and the space after each note.",
        ),
        16..=23 => (
            48,
            84,
            60,
            "A sustained organ, reed keyboard, or harmonica color.",
            "Compare separated notes with the held ending. Registration and timbre follow the selected sound bank.",
        ),
        24..=31 => (
            40,
            88,
            52,
            "A guitar preset shaped by its plucked attack and selected amplification or technique.",
            "The compact keyboard guide tests plucks, chord tones, and decay; it does not model guitar fingering.",
        ),
        32..=39 => (
            28,
            72,
            36,
            "A bass preset with a plucked, electric, or synthesized attack.",
            "Listen to the low-register definition and the transition to the upper octave. Bowed double bass is in Strings.",
        ),
        40..=47 => (
            36,
            96,
            60,
            "A string or tuned percussion color with the articulation supplied by the selected preset.",
            "The keyboard guide compares short notes and a held ending. Pizzicato and other decaying presets will fade naturally.",
        ),
        48..=55 => (
            48,
            96,
            60,
            "A layered string, vocal, or orchestral ensemble color.",
            "Listen for the blend and held texture. Choir presets provide vowel colors rather than lyrics.",
        ),
        56..=63 => (
            48,
            84,
            60,
            "A brass color ranging from muted acoustic tone to layered synthesized sections.",
            "Compare tongued attacks with the held ending. The keyboard guide follows the sound bank's preset.",
        ),
        64..=65 => (
            48,
            84,
            60,
            "A saxophone color with a focused reed attack and sustained body.",
            "This keyboard guide uses concert pitch and compares separated notes with a long final tone.",
        ),
        66..=67 => (
            36,
            72,
            48,
            "A lower saxophone color with a focused reed attack and sustained body.",
            "This keyboard guide uses concert pitch and compares the lower register with the upper octave.",
        ),
        68..=71 => (
            48,
            84,
            60,
            "A sustained reed color.",
            "Listen to the attack and held tone; the score displays concert pitch.",
        ),
        72..=79 => (
            60,
            96,
            72,
            "A flute, pipe, or whistle color with a clear high-register voice.",
            "This keyboard guide compares the breath or whistle attack with a held ending at concert pitch.",
        ),
        80..=87 => (
            48,
            84,
            60,
            "A sampled synthesizer lead preset with a distinctive waveform or layered tone.",
            "Use the detached notes to compare the attack, then hear the sustained lead texture.",
        ),
        88..=95 => (
            48,
            84,
            60,
            "A sampled synthesizer pad with a sustained, evolving texture.",
            "Give the held ending time to develop; shorter notes may end before the full texture opens.",
        ),
        96..=103 => (
            48,
            84,
            60,
            "A sampled sound design preset with a layered or changing texture.",
            "The keyboard guide previews the bank's pitch mapping; some effects emphasize texture over a clear note.",
        ),
        104..=111 => (
            48,
            84,
            60,
            "An additional plucked, bowed, or reed color from the General MIDI collection.",
            "This is a keyboard guide, not a traditional playing range or fingering model. Compare attacks and sustain.",
        ),
        112..=119 => (
            48,
            84,
            60,
            "A pitched percussion preset, independent of a MIDI drum kit.",
            "The phrase previews the bank's melodic keyboard mapping. Listen to the strike and natural decay.",
        ),
        _ => (
            48,
            84,
            60,
            "A sampled sound effect mapped across the keyboard.",
            "These are texture auditions; the preset may not follow equal-tempered pitch or sustain for the written duration.",
        ),
    };
    InstrumentProfile {
        low,
        high,
        audition,
        family: Instrument::GeneralMidi(program).family(),
        character,
        technique,
    }
}

/// A four-bar phrase at 100 BPM. Its notes stay in a compact octave in C major,
/// beginning in a useful register for each instrument. All pitches are concert
/// pitch, matching the score editor. Timpani uses two drum pitches and a final
/// rest so a decaying strike cannot be mistaken for a sustained roll.
pub fn audition_phrase(instrument: Instrument, velocity: u8) -> Score {
    let info = profile(instrument);
    let velocity = velocity.clamp(1, 127);
    let base = Pitch::from_midi(info.audition).staff_step();
    let pitch = |degree: i32| {
        let p = Pitch::from_staff_step(base + degree, 0);
        // The chosen profiles fit the complete phrase; retain bounds if a future
        // profile narrows its range without updating its audition register.
        Pitch::from_midi(p.midi().clamp(info.low, info.high))
    };
    let note = |start, duration, degree, staccato| Note {
        voice: 0,
        voice_launch: None,
        articulations: Vec::new(),
        start,
        duration,
        dotted: false,
        tuplet: None,
        layer: 0,
        pitches: vec![pitch(degree)],
        velocity,
        staccato,
        tied: false,
    };
    let mut measures = vec![Measure::default(); 4];
    if instrument.gm_program() == Instrument::Timpani.gm_program() {
        for (beat, degree) in [0, 4, 0, 4].into_iter().enumerate() {
            measures[0]
                .notes
                .push(note(beat as u32 * PPQ, Duration::Quarter, degree, false));
        }
        measures[1].notes = vec![
            note(0, Duration::Half, 0, false),
            note(PPQ * 2, Duration::Quarter, 4, false),
            note(PPQ * 3, Duration::Quarter, 0, false),
        ];
        for (eighth, degree) in [0, 4, 0, 0, 4, 4, 0, 4].into_iter().enumerate() {
            measures[2].notes.push(note(
                eighth as u32 * PPQ / 2,
                Duration::Eighth,
                degree,
                false,
            ));
        }
        measures[3].notes = vec![
            note(0, Duration::Half, 0, false),
            Note {
                voice: 0,
                voice_launch: None,
                articulations: Vec::new(),
                start: PPQ * 2,
                duration: Duration::Half,
                dotted: false,
                tuplet: None,
                layer: 0,
                pitches: vec![],
                velocity,
                staccato: false,
                tied: false,
            },
        ];
    } else {
        for (beat, degree) in [0, 1, 2, 4].into_iter().enumerate() {
            measures[0]
                .notes
                .push(note(beat as u32 * PPQ, Duration::Quarter, degree, false));
        }
        measures[1].notes = vec![
            note(0, Duration::Half, 7, false),
            note(PPQ * 2, Duration::Half, 4, false),
        ];
        for (eighth, degree) in [0, 2, 4, 7, 4, 2, 1, 0].into_iter().enumerate() {
            measures[2].notes.push(note(
                eighth as u32 * PPQ / 2,
                Duration::Eighth,
                degree,
                eighth % 2 == 1,
            ));
        }
        measures[3].notes.push(note(0, Duration::Whole, 0, false));
    }
    Score {
        title: format!("{} · sound study", instrument.name()),
        composer: "Encore instrument laboratory".into(),
        tempo: 100,
        time_numerator: 4,
        time_denominator: 4,
        key_fifths: 0,
        notation: Default::default(),
        parts: vec![Part {
            name: instrument.name().into(),
            instrument,
            percussion: None,
            clef: instrument.default_clef(),
            measures,
            volume: 0.8,
            pan: 0.0,
            muted: false,
            solo: false,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_useful_register_and_description() {
        for &instrument in Instrument::ALL {
            let p = profile(instrument);
            assert!(p.low < p.high && p.high <= 108, "{}", instrument.name());
            assert!(
                p.low <= p.audition && p.audition + 12 <= p.high,
                "{}",
                instrument.name()
            );
            assert!(!p.family.is_empty() && !p.character.is_empty() && !p.technique.is_empty());
        }
        assert_eq!(profile(Instrument::DoubleBass).audition, 28);
        assert_eq!(profile(Instrument::Timpani).audition, 38);
        assert!(profile(Instrument::Tuba).audition < 36);
    }

    #[test]
    fn all_auditions_validate_and_stay_in_their_concert_ranges() {
        for &instrument in Instrument::ALL {
            for velocity in [0, 70, 127, 255] {
                let score = audition_phrase(instrument, velocity);
                score
                    .validate()
                    .unwrap_or_else(|e| panic!("{}: {e}", instrument.name()));
                assert_eq!(score.parts.len(), 1);
                assert_eq!(score.measure_count(), 4);
                assert_eq!(score.parts[0].clef, instrument.default_clef());
                let p = profile(instrument);
                let mut highest = p.low;
                for measure in &score.parts[0].measures {
                    assert_eq!(
                        measure.notes.last().map(|n| n.start + n.ticks()),
                        Some(score.measure_ticks())
                    );
                    for note in &measure.notes {
                        assert!((1..=127).contains(&note.velocity));
                        for pitch in &note.pitches {
                            let midi = pitch.midi();
                            highest = highest.max(midi);
                            assert!((p.low..=p.high).contains(&midi));
                            assert_eq!(
                                pitch.alter, 0,
                                "Auditions should use clear diatonic spelling"
                            );
                        }
                    }
                }
                assert!(highest > p.audition);
            }
        }
    }

    #[test]
    fn explicit_gm_aliases_keep_legacy_concert_guidance() {
        for &instrument in Instrument::LEGACY {
            assert_eq!(
                profile(Instrument::GeneralMidi(instrument.gm_program())),
                profile(instrument)
            );
        }
        assert_ne!(
            profile(Instrument::GeneralMidi(32)),
            profile(Instrument::DoubleBass)
        );
        assert!(
            profile(Instrument::GeneralMidi(120))
                .technique
                .contains("texture auditions")
        );
    }

    #[test]
    fn timpani_audition_uses_two_drums_and_leaves_a_decay_rest() {
        let score = audition_phrase(Instrument::Timpani, 80);
        let part = &score.parts[0];
        let mut pitches: Vec<u8> = part
            .measures
            .iter()
            .flat_map(|m| &m.notes)
            .flat_map(|n| &n.pitches)
            .map(|pitch| pitch.midi())
            .collect();
        pitches.sort_unstable();
        pitches.dedup();
        assert_eq!(pitches, vec![38, 45]);
        assert!(part.measures[3].notes.last().unwrap().pitches.is_empty());
        assert_eq!(
            audition_phrase(Instrument::Violin, 80).parts[0].measures[3].notes[0].duration,
            Duration::Whole
        );
    }
}
