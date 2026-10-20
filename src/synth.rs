//! Original, sample-free orchestral synthesis and persistent device playback.
//!
//! These are playable procedural approximations: piano uses inharmonic decaying
//! strings, sustained instruments use shaped harmonic spectra, and timpani uses
//! tuned membrane modes. The extended GM catalog uses family approximations in
//! this explicitly selected procedural engine. No sound bank or asset is used.

use crate::model::{Instrument, Note, PPQ, Score};
use rodio::{OutputStream, OutputStreamBuilder, Sink, buffer::SamplesBuffer};
use std::{
    collections::HashSet,
    f32::consts::{FRAC_PI_2, TAU},
    sync::OnceLock,
};

pub const SAMPLE_RATE: u32 = 44_100;
/// Offline stereo rendering is bounded so a malformed score cannot exhaust RAM.
/// MIDI, notation and native-score exports have no corresponding audio limit.
pub const MAX_RENDER_SECONDS: f32 = 600.0;
const TAIL_SECONDS: f32 = 1.8;
const TABLE_SIZE: usize = 4096;

/// Written score length, excluding the reverberation/release tail.
pub fn duration_seconds(score: &Score) -> f32 {
    score.measure_count() as f32 * score.measure_ticks() as f32 / PPQ as f32 * 60.0
        / score.tempo.max(1) as f32
}

#[derive(Clone, Copy)]
struct Event<'a> {
    tick: u64,
    note: &'a Note,
}

/// Render the entire score to interleaved stereo samples at 44.1 kHz.
pub fn render(score: &Score) -> Vec<f32> {
    let mut output = render_dry(score);
    finish(&mut output);
    output
}

/// Dry procedural rendering before the shared room and output limiter.
pub(crate) fn render_dry(score: &Score) -> Vec<f32> {
    let seconds = duration_seconds(score);
    if !seconds.is_finite() || seconds > MAX_RENDER_SECONDS {
        return Vec::new();
    }
    let mut output = vec![0.0; ((seconds + TAIL_SECONDS) * SAMPLE_RATE as f32).ceil() as usize * 2];
    let seconds_per_tick = 60.0 / (score.tempo.max(1) as f32 * PPQ as f32);
    let ticks_per_measure = score.measure_ticks() as u64;
    let any_solo = score.parts.iter().any(|p| p.solo && !p.muted);

    for (part_index, part) in score.parts.iter().enumerate() {
        if part.muted || (any_solo && !part.solo) || part.volume <= 0.0 {
            continue;
        }
        let mut events = Vec::new();
        for (measure_index, measure) in part.measures.iter().enumerate() {
            for note in &measure.notes {
                events.push(Event {
                    tick: measure_index as u64 * ticks_per_measure + note.start as u64,
                    note,
                });
            }
        }
        events.sort_by_key(|e| e.tick);
        let mut consumed = HashSet::new();
        let pan = finite_clamp(part.pan, -1.0, 1.0, 0.0);
        let volume = finite_clamp(part.volume, 0.0, 2.0, 0.8);
        let left = ((pan + 1.0) * FRAC_PI_2 * 0.5).cos() * volume;
        let right = ((pan + 1.0) * FRAC_PI_2 * 0.5).sin() * volume;

        for (index, event) in events.iter().enumerate() {
            for pitch in &event.note.pitches {
                let midi = pitch.midi();
                if consumed.contains(&(index, midi)) || event.note.velocity == 0 {
                    continue;
                }
                let mut end = event.tick + event.note.ticks() as u64;
                let mut last_note = event.note;
                let mut current_index = index;
                // Ties join each matching pitch independently, including across barlines.
                while last_note.tied {
                    let next = events
                        .iter()
                        .enumerate()
                        .skip(current_index + 1)
                        .take_while(|(_, e)| e.tick <= end)
                        .find(|(_, e)| {
                            e.tick == end && e.note.pitches.iter().any(|p| p.midi() == midi)
                        });
                    let Some((next_index, next_event)) = next else {
                        break;
                    };
                    if !consumed.insert((next_index, midi)) {
                        break;
                    }
                    end += next_event.note.ticks() as u64;
                    last_note = next_event.note;
                    current_index = next_index;
                }
                let last_shortening = if last_note.staccato {
                    last_note.ticks() as f32 * 0.52
                } else {
                    0.0
                };
                let held_ticks = ((end - event.tick) as f32 - last_shortening).max(1.0);
                let voice = synth_voice_seeded(
                    part.instrument,
                    midi,
                    event.note.velocity,
                    held_ticks * seconds_per_tick,
                    event.tick ^ (part_index as u64 + 1).wrapping_mul(0x517C_C1B7_2722_0A95),
                );
                let offset =
                    (event.tick as f32 * seconds_per_tick * SAMPLE_RATE as f32).round() as usize;
                for (frame, sample) in voice.iter().enumerate() {
                    let target = (offset + frame) * 2;
                    if target + 1 >= output.len() {
                        break;
                    }
                    output[target] += sample * left;
                    output[target + 1] += sample * right;
                }
            }
        }
    }
    output
}

/// Audition one centered note using the same instrument as score playback.
pub fn render_note(instrument: Instrument, midi: u8, velocity: u8, duration_secs: f32) -> Vec<f32> {
    let duration = finite_clamp(duration_secs, 0.01, 120.0, 0.5);
    let mut output =
        vec![0.0; ((duration + TAIL_SECONDS) * SAMPLE_RATE as f32).ceil() as usize * 2];
    if velocity != 0 {
        let voice = synth_voice(instrument, midi.min(127), velocity.min(127), duration);
        for (index, sample) in voice.into_iter().enumerate() {
            if index * 2 + 1 >= output.len() {
                break;
            }
            output[index * 2] = sample * std::f32::consts::FRAC_1_SQRT_2;
            output[index * 2 + 1] = sample * std::f32::consts::FRAC_1_SQRT_2;
        }
    }
    finish(&mut output);
    output
}

fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

struct Timbre {
    harmonics: &'static [f32],
    attack: f32,
    release: f32,
    vibrato: f32,
    noise: f32,
    level: f32,
}

fn timbre(instrument: Instrument) -> Timbre {
    use Instrument::*;
    let (harmonics, attack, release, vibrato, noise, level): (&[f32], f32, f32, f32, f32, f32) =
        match instrument {
            Piano => (
                &[
                    1., 0.62, 0.36, 0.23, 0.15, 0.10, 0.07, 0.045, 0.025, 0.017, 0.010, 0.006,
                ],
                0.002,
                0.28,
                0.,
                0.045,
                0.44,
            ),
            Violin => (
                &[
                    1., 0.73, 0.62, 0.43, 0.37, 0.30, 0.25, 0.20, 0.17, 0.12, 0.10, 0.08,
                ],
                0.065,
                0.22,
                0.0032,
                0.035,
                0.31,
            ),
            Viola => (
                &[1., 0.68, 0.44, 0.35, 0.28, 0.18, 0.14, 0.10, 0.08, 0.06],
                0.085,
                0.25,
                0.0027,
                0.026,
                0.33,
            ),
            Cello => (
                &[1., 0.55, 0.48, 0.27, 0.19, 0.17, 0.12, 0.08, 0.06],
                0.095,
                0.29,
                0.0024,
                0.023,
                0.36,
            ),
            DoubleBass => (
                &[1., 0.70, 0.39, 0.25, 0.17, 0.10, 0.07, 0.04],
                0.17,
                0.43,
                0.0014,
                0.038,
                0.40,
            ),
            Flute => (
                &[1., 0.12, 0.065, 0.018, 0.008],
                0.055,
                0.17,
                0.0019,
                0.035,
                0.34,
            ),
            Piccolo => (
                &[1., 0.26, 0.12, 0.065, 0.026, 0.011],
                0.035,
                0.13,
                0.002,
                0.027,
                0.28,
            ),
            Oboe => (
                &[1., 0.68, 0.91, 0.52, 0.62, 0.34, 0.28, 0.13, 0.09],
                0.04,
                0.15,
                0.0017,
                0.014,
                0.29,
            ),
            EnglishHorn => (
                &[1., 0.55, 0.75, 0.37, 0.34, 0.20, 0.13, 0.08],
                0.07,
                0.19,
                0.0018,
                0.016,
                0.31,
            ),
            Clarinet => (
                &[1., 0.025, 0.60, 0.018, 0.34, 0.015, 0.16, 0.010, 0.07],
                0.045,
                0.16,
                0.0012,
                0.016,
                0.34,
            ),
            Bassoon => (
                &[1., 0.30, 0.61, 0.25, 0.33, 0.16, 0.15, 0.07, 0.04],
                0.065,
                0.20,
                0.0015,
                0.023,
                0.35,
            ),
            FrenchHorn => (
                &[1., 0.52, 0.32, 0.19, 0.115, 0.07, 0.045, 0.023],
                0.095,
                0.23,
                0.0012,
                0.008,
                0.39,
            ),
            Trumpet => (
                &[1., 0.72, 0.75, 0.59, 0.45, 0.32, 0.24, 0.17, 0.10],
                0.03,
                0.13,
                0.0014,
                0.012,
                0.34,
            ),
            Trombone => (
                &[1., 0.69, 0.47, 0.38, 0.25, 0.17, 0.11, 0.075, 0.04],
                0.065,
                0.22,
                0.0010,
                0.015,
                0.38,
            ),
            Tuba => (
                &[1., 0.61, 0.35, 0.23, 0.14, 0.085, 0.05],
                0.11,
                0.29,
                0.0008,
                0.018,
                0.43,
            ),
            Harp => (
                &[1., 0.46, 0.23, 0.13, 0.075, 0.043, 0.024, 0.014],
                0.002,
                1.35,
                0.,
                0.024,
                0.43,
            ),
            Timpani => (
                &[1., 0.67, 0.42, 0.27, 0.15, 0.09, 0.05, 0.027, 0.014],
                0.0045,
                1.65,
                0.,
                0.23,
                0.49,
            ),
            StringEnsemble => (
                &[1., 0.76, 0.57, 0.43, 0.32, 0.25, 0.19, 0.15, 0.11, 0.08],
                0.13,
                0.37,
                0.0028,
                0.021,
                0.35,
            ),
            GeneralMidi(_) => return timbre(procedural_instrument(instrument)),
        };
    Timbre {
        harmonics,
        attack,
        release,
        vibrato,
        noise,
        level,
    }
}

struct Partial {
    phase: f32,
    increment: f32,
    amplitude: f32,
    decay: f32,
    brightness: f32,
    group: usize,
}

fn sine_table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=TABLE_SIZE)
            .map(|i| (i as f32 * TAU / TABLE_SIZE as f32).sin())
            .collect()
    })
}

#[inline]
fn sine(table: &[f32], phase: f32) -> f32 {
    let position = phase * TABLE_SIZE as f32;
    let index = (position as usize).min(TABLE_SIZE - 1);
    table[index] + (table[index + 1] - table[index]) * (position - index as f32)
}

// Two broad body/air resonances color the excitation rather than adding a
// generic hiss. Direct form II keeps each voice's filter state small.
struct Bandpass {
    b0: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Bandpass {
    fn new(frequency: f32, q: f32) -> Self {
        let omega = TAU * frequency.clamp(25.0, 16_000.0) / SAMPLE_RATE as f32;
        let alpha = omega.sin() / (2.0 * q);
        let inverse = 1.0 / (1.0 + alpha);
        Self {
            b0: alpha * inverse,
            a1: -2.0 * omega.cos() * inverse,
            a2: (1.0 - alpha) * inverse,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.z1;
        self.z1 = -self.a1 * output + self.z2;
        self.z2 = -self.b0 * input - self.a2 * output;
        output
    }
}

fn seeded_state(event: u64, instrument: Instrument, midi: u8) -> u32 {
    // Retain original voice seeds as the instrument catalog grows. The explicit
    // family mapping also keeps the optional procedural engine independent of
    // enum discriminants and arbitrary serialized GeneralMidi aliases.
    let instrument_id = Instrument::LEGACY
        .iter()
        .position(|kind| *kind == procedural_instrument(instrument))
        .unwrap_or(0) as u64;
    let mut value = event
        ^ (midi as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (instrument_id + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((value ^ (value >> 31)) as u32).max(1)
}

#[inline]
fn noise(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as f32 / u32::MAX as f32 * 2.0 - 1.0
}

fn resonance(frequency: f32, center: f32, width: f32) -> f32 {
    let offset = (frequency - center) / width;
    1.0 / (1.0 + offset * offset)
}

// Source spectra and body formants are evaluated in Hz. A low E on a bass thus
// excites many more body harmonics than the same harmonic recipe at middle C.
fn harmonic_weight(instrument: Instrument, harmonic: usize, frequency: f32, velocity: f32) -> f32 {
    use Instrument::*;
    let h = harmonic as f32;
    let hz = h * frequency;
    let register = ((frequency / 220.0).log2() / 4.0 + 0.5).clamp(0.0, 1.0);
    match instrument {
        Violin | Viola | Cello | DoubleBass | StringEnsemble => {
            let (low, body, bridge, rolloff) = match instrument {
                Violin => (280.0, 700.0, 2650.0, 0.92),
                Viola => (205.0, 520.0, 2200.0, 1.02),
                Cello => (110.0, 310.0, 1750.0, 1.07),
                DoubleBass => (82.0, 205.0, 1100.0, 1.10),
                _ => (235.0, 590.0, 2400.0, 0.99),
            };
            let body_shape = 0.62
                + 0.55 * resonance(hz, low, low * 0.40)
                + 0.75 * resonance(hz, body, body * 0.55)
                + (0.40 + velocity * 0.65) * resonance(hz, bridge, bridge * 0.34);
            let bow_position = (0.25 + (h * 0.73).sin().abs() * 0.75).sqrt();
            let fundamental = if harmonic == 1 && instrument == DoubleBass {
                1.48
            } else {
                1.0
            };
            body_shape * bow_position * fundamental / h.powf(rolloff + (1.0 - velocity) * 0.28)
        }
        Flute | Piccolo => {
            let base = if harmonic == 1 {
                1.0
            } else {
                (0.10 + register * 0.17 + velocity * 0.13) / h.powf(1.65)
            };
            base * (1.0
                + resonance(
                    hz,
                    if instrument == Piccolo {
                        4200.0
                    } else {
                        2400.0
                    },
                    1500.0,
                ) * 0.15)
        }
        Oboe | EnglishHorn | Bassoon => {
            let (first, second, slope) = match instrument {
                Oboe => (1050.0, 2500.0, 0.80),
                EnglishHorn => (750.0, 1800.0, 0.91),
                _ => (475.0, 1400.0, 0.96),
            };
            let odd = if harmonic % 2 == 1 { 1.0 } else { 0.77 };
            odd * (0.56
                + resonance(hz, first, first * 0.45) * 1.25
                + resonance(hz, second, second * 0.30) * (0.25 + velocity * 0.8))
                / h.powf(slope + (1.0 - velocity) * 0.28)
        }
        Clarinet => {
            let odd = if harmonic % 2 == 1 {
                1.0
            } else {
                0.025 + 0.16 * register + 0.11 * velocity * velocity
            };
            odd * (0.85 + 0.45 * resonance(hz, 1450.0, 650.0))
                / h.powf(0.95 + (1.0 - velocity) * 0.25)
        }
        FrenchHorn | Trumpet | Trombone | Tuba => {
            let cutoff = match instrument {
                Trumpet => 850.0 + velocity.powi(2) * 6500.0,
                Trombone => 650.0 + velocity.powi(2) * 4300.0,
                FrenchHorn => 430.0 + velocity.powi(2) * 2600.0,
                _ => 270.0 + velocity.powi(2) * 1500.0,
            };
            let rolloff = 1.0 / (1.0 + (hz / cutoff).powi(2));
            let bell = 0.76 + resonance(hz, cutoff * 0.52, cutoff * 0.40) * 0.50;
            rolloff * bell / h.powf(0.67 + (1.0 - velocity) * 0.70)
        }
        _ => timbre(instrument)
            .harmonics
            .get(harmonic - 1)
            .copied()
            .unwrap_or(0.0),
    }
}

fn synth_voice(instrument: Instrument, midi: u8, velocity: u8, held: f32) -> Vec<f32> {
    synth_voice_seeded(instrument, midi, velocity, held, 0)
}

// Samples remain the default for the full catalog. This deliberately small DSP
// vocabulary is an offline fallback chosen by the user, not a claim to emulate
// every GM patch (particularly guitars, organs, electronic voices and effects).
fn procedural_instrument(instrument: Instrument) -> Instrument {
    use Instrument::*;
    let GeneralMidi(program) = instrument else {
        return instrument;
    };
    if let Some(canonical) = Instrument::from_gm_program(program)
        && !matches!(canonical, GeneralMidi(_))
    {
        return canonical;
    }
    match program {
        0..=7 => Piano,
        8..=15 | 24..=31 | 45..=46 | 104..=115 => Harp,
        16..=23 | 64..=67 | 80..=87 => Clarinet,
        32..=39 => DoubleBass,
        40..=44 | 48..=55 | 88..=103 => StringEnsemble,
        47 | 116..=127 => Timpani,
        56..=63 => Trumpet,
        68..=79 => Flute,
        _ => Piano,
    }
}

fn synth_voice_seeded(
    instrument: Instrument,
    midi: u8,
    velocity: u8,
    held: f32,
    event: u64,
) -> Vec<f32> {
    use Instrument::*;
    let instrument = procedural_instrument(instrument);
    let tone = timbre(instrument);
    let table = sine_table();
    let frequency = 440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0);
    let velocity = velocity.min(127) as f32 / 127.0;
    let bowed = matches!(
        instrument,
        Violin | Viola | Cello | DoubleBass | StringEnsemble
    );
    let percussive = matches!(instrument, Piano | Harp | Timpani);
    let brass = matches!(instrument, FrenchHorn | Trumpet | Trombone | Tuba);
    let mut seed = seeded_state(event, instrument, midi);
    let character = noise(&mut seed);
    let strike_position = 0.13 + (character + 1.0) * 0.025;
    let start_phase = (noise(&mut seed) + 1.0) * 0.5;
    let attack_variation = 1.0 + character * 0.085;
    let partial_count = if bowed {
        24
    } else if brass {
        18
    } else if matches!(instrument, Oboe | EnglishHorn | Clarinet | Bassoon) {
        16
    } else {
        tone.harmonics.len()
    };
    let frames = ((held + tone.release) * SAMPLE_RATE as f32).ceil() as usize;
    let mut output = Vec::with_capacity(frames);
    let mut partials = Vec::with_capacity(partial_count + 20);
    for index in 0..partial_count {
        let harmonic = index as f32 + 1.0;
        let ratio = match instrument {
            Piano => {
                harmonic * (1.0 + (0.000035 + frequency * 0.00000028) * harmonic * harmonic).sqrt()
            }
            Harp => harmonic * (1.0 + harmonic * 0.00008),
            Timpani => [1.0, 1.505, 2.015, 2.52, 2.99, 3.48, 4.07, 4.65, 5.43][index],
            _ => harmonic,
        };
        if frequency * ratio >= SAMPLE_RATE as f32 * 0.455 {
            continue;
        }
        let decay_seconds = match instrument {
            Piano => (4.5 * (220.0 / frequency).powf(0.30) / (1.0 + harmonic.powf(1.35) * 0.19))
                .max(0.12),
            Harp => 4.0 * (220.0 / frequency).powf(0.18) / (1.0 + harmonic * 0.30),
            Timpani => (1.9 * (92.0 / frequency).powf(0.24)
                / (1.0 + (harmonic - 1.0).powf(1.2) * 0.23))
                .max(0.16),
            _ => f32::INFINITY,
        };
        let mut amplitude = harmonic_weight(instrument, index + 1, frequency, velocity);
        if matches!(instrument, Piano | Harp) {
            amplitude *= (0.42
                + 0.58
                    * (std::f32::consts::PI * harmonic * strike_position)
                        .sin()
                        .abs())
                * (0.28 + velocity * 0.72).powf((harmonic - 1.0) * 0.32);
        }
        if instrument == Timpani {
            amplitude *= (0.65 + velocity * 0.35).powf(index as f32 * 0.43)
                * (1.0 + character * 0.10 * (harmonic * 1.71).sin());
        }
        let base = Partial {
            phase: if percussive {
                (start_phase * 0.025 * harmonic).fract()
            } else {
                (start_phase + harmonic * 0.137).fract()
            },
            increment: frequency * ratio / SAMPLE_RATE as f32,
            amplitude,
            decay: (-1.0 / (decay_seconds * SAMPLE_RATE as f32)).exp(),
            brightness: if brass {
                ((harmonic - 1.0) / 12.0).min(0.88)
            } else if bowed {
                ((harmonic - 1.0) / 24.0).min(0.6)
            } else {
                0.0
            },
            group: 0,
        };
        partials.push(base);
        // Unison piano strings beat against one another; the bass register uses
        // one winding, the tenor two strings, and the upper register three.
        if instrument == Piano && index < 7 && midi >= 40 {
            let spread = 0.00055 + frequency * 0.00000055;
            let partners: &[f32] = if midi >= 55 { &[-1.0, 1.0] } else { &[1.0] };
            for detune in partners {
                partials.push(Partial {
                    phase: (start_phase * 0.017 * harmonic).fract(),
                    increment: frequency
                        * ratio
                        * (1.0 + detune * spread * (1.0 + character * 0.08))
                        / SAMPLE_RATE as f32,
                    amplitude: amplitude * 0.26,
                    decay: (-1.08 / (decay_seconds * SAMPLE_RATE as f32)).exp(),
                    brightness: 0.0,
                    group: 0,
                });
            }
        }
        if instrument == StringEnsemble && index < 8 {
            for (group, detune) in [(1, -0.0038), (2, 0.0043)] {
                partials.push(Partial {
                    phase: (start_phase + harmonic * 0.293 + group as f32 * 0.31).fract(),
                    increment: frequency * ratio * (1.0 + detune + character * 0.00035)
                        / SAMPLE_RATE as f32,
                    amplitude: amplitude * 0.43,
                    decay: 1.0,
                    brightness: 0.1,
                    group,
                });
            }
        }
    }
    // Normalize energy only after formants and unison strings have been added.
    // This preserves the intended spectral changes without making forte merely
    // a brighter but quieter version of piano.
    let normalizer = partials
        .iter()
        .map(|p| p.amplitude * p.amplitude)
        .sum::<f32>()
        .sqrt()
        .max(0.9);
    for partial in &mut partials {
        partial.amplitude /= normalizer;
    }
    let (body_low, body_high, body_q) = match instrument {
        Piano => (185.0, 2350.0, 1.4),
        Harp => (310.0, 2100.0, 1.8),
        Timpani => (frequency * 1.15, 680.0 + velocity * 750.0, 1.2),
        DoubleBass => (205.0, 1120.0, 1.7),
        Cello => (310.0, 1750.0, 1.7),
        Viola => (520.0, 2200.0, 1.6),
        Violin | StringEnsemble => (690.0, 2700.0, 1.4),
        Flute | Piccolo => (2100.0, 6500.0, 0.9),
        Oboe => (1050.0, 2500.0, 1.6),
        EnglishHorn => (750.0, 1800.0, 1.5),
        Clarinet => (1150.0, 3100.0, 1.4),
        Bassoon => (475.0, 1400.0, 1.5),
        _ => (frequency * 3.0, 1850.0, 1.0),
    };
    let mut body = Bandpass::new(body_low, body_q);
    let mut air = Bandpass::new(body_high, body_q);
    let mut vibrato_phase = start_phase;
    let mut second_phase = (start_phase + 0.31).fract();
    let mut third_phase = (start_phase + 0.67).fract();
    let vibrato_increment = (if instrument == DoubleBass { 4.65 } else { 5.15 } + character * 0.24)
        / SAMPLE_RATE as f32;
    let mut pressure_phase = start_phase;
    let pressure_increment = (0.67 + character * 0.13) / SAMPLE_RATE as f32;
    let mut drift = 0.0;
    let attack_frames = (tone.attack * attack_variation * SAMPLE_RATE as f32).max(1.0);
    let inverse_attack = 1.0 / attack_frames;
    // A timpani stroke and a plucked harp continue ringing after the notated
    // duration. Their final fade represents damping; bowed/wind releases stop
    // the excitation immediately and retain a short body tail.
    let release_length = if instrument == Timpani {
        0.28
    } else if instrument == Harp {
        0.48
    } else {
        tone.release
    };
    let release_start = held + tone.release - release_length;
    let inverse_release = 1.0 / (release_length * SAMPLE_RATE as f32);
    let release_frame = release_start * SAMPLE_RATE as f32;
    let mut transient = 1.0;
    let transient_decay = (-1.0
        / (SAMPLE_RATE as f32
            * if instrument == Timpani {
                0.018 + (1.0 - velocity) * 0.009
            } else {
                0.027
            }))
    .exp();
    let mut pitch_settle = if instrument == Timpani {
        0.008 + velocity * 0.006
    } else if brass {
        -0.0025
    } else {
        0.0
    };
    let pitch_settle_decay = (-1.0 / (SAMPLE_RATE as f32 * 0.035)).exp();
    let gain = velocity.powf(1.23) * tone.level * (1.0 + character * 0.018);
    for frame in 0..frames {
        let time = frame as f32 / SAMPLE_RATE as f32;
        let attack = (frame as f32 * inverse_attack).min(1.0);
        let attack = attack * attack * (3.0 - 2.0 * attack);
        let release_progress = ((frame as f32 - release_frame) * inverse_release).clamp(0.0, 1.0);
        let release = (1.0 - release_progress).powi(if percussive { 3 } else { 2 });
        let pressure = sine(table, pressure_phase);
        let vibrato_ramp = ((time - 0.07) / if bowed { 0.42 } else { 0.23 }).clamp(0.0, 1.0);
        let random = noise(&mut seed);
        drift += (random - drift) * 0.00038;
        let base_pitch = 1.0
            + pitch_settle
            + drift
                * if bowed {
                    0.006
                } else if percussive {
                    0.0
                } else {
                    0.0015
                };
        let vibratos = [
            base_pitch + tone.vibrato * sine(table, vibrato_phase) * vibrato_ramp,
            base_pitch + tone.vibrato * sine(table, second_phase) * vibrato_ramp * 1.09,
            base_pitch + tone.vibrato * sine(table, third_phase) * vibrato_ramp * 0.92,
        ];
        vibrato_phase += vibrato_increment;
        second_phase += vibrato_increment * 0.937;
        third_phase += vibrato_increment * 1.073;
        pressure_phase += pressure_increment;
        if vibrato_phase >= 1.0 {
            vibrato_phase -= 1.0;
        }
        if second_phase >= 1.0 {
            second_phase -= 1.0;
        }
        if third_phase >= 1.0 {
            third_phase -= 1.0;
        }
        if pressure_phase >= 1.0 {
            pressure_phase -= 1.0;
        }
        let spectral_pressure = if bowed { 0.13 + pressure * 0.09 } else { 0.0 };
        let mut sample = 0.0;
        for partial in &mut partials {
            let color = 1.0 - partial.brightness * ((1.0 - attack) * 0.80 + spectral_pressure);
            sample += sine(table, partial.phase) * partial.amplitude * color;
            partial.phase += partial.increment * vibratos[partial.group];
            if partial.phase >= 1.0 {
                partial.phase -= 1.0;
            }
            partial.amplitude *= partial.decay;
            if partial.amplitude.abs() < 1.0e-12 {
                partial.amplitude = 0.0;
            }
        }
        let body_noise = body.process(random);
        let air_noise = air.process(random);
        let excitation = match instrument {
            Piano => (body_noise * 0.65 + air_noise * (0.25 + velocity * 0.85)) * transient * 2.4,
            Timpani => (body_noise * 1.9 + air_noise * 0.55) * transient * 2.6,
            Harp => (body_noise * 0.8 + air_noise * 0.4) * transient * 1.8,
            Flute | Piccolo => air_noise * (0.55 + transient * 1.8) + body_noise * 0.3,
            _ if bowed => {
                (body_noise * 0.7 + air_noise * 0.6) * (0.7 + velocity * 0.45 + pressure * 0.15)
            }
            _ if brass => (body_noise * 0.5 + air_noise * 0.7) * (0.4 + transient * 2.0),
            _ => (body_noise * 0.7 + air_noise * 0.45) * (0.6 + transient * 1.4),
        };
        sample += excitation * tone.noise;
        let expression = if percussive {
            1.0
        } else {
            0.96 + pressure * if bowed { 0.04 } else { 0.023 }
                + sine(table, vibrato_phase) * if bowed { 0.012 } else { 0.005 }
        };
        let value = sample * attack * release * gain * expression;
        output.push(if value.is_finite() { value } else { 0.0 });
        transient = if transient > 1.0e-8 {
            transient * transient_decay
        } else {
            0.0
        };
        pitch_settle = if pitch_settle.abs() > 1.0e-8 {
            pitch_settle * pitch_settle_decay
        } else {
            0.0
        };
    }
    output
}

pub(crate) fn finish(samples: &mut [f32]) {
    // Four damped room reflections per side, with different delays for stereo width.
    struct Comb {
        delay: Vec<f32>,
        cursor: usize,
        low: f32,
    }
    let mut left: Vec<Comb> = [1499, 1789, 1999, 2137]
        .into_iter()
        .map(|n| Comb {
            delay: vec![0.0; n],
            cursor: 0,
            low: 0.0,
        })
        .collect();
    let mut right: Vec<Comb> = [1601, 1867, 2087, 2243]
        .into_iter()
        .map(|n| Comb {
            delay: vec![0.0; n],
            cursor: 0,
            low: 0.0,
        })
        .collect();
    for frame in samples.as_chunks_mut::<2>().0 {
        let mono = (frame[0] + frame[1]) * 0.5;
        let (left_channel, right_channel) = frame.split_at_mut(1);
        for (channel, combs) in [(left_channel, &mut left), (right_channel, &mut right)] {
            let mut wet = 0.0;
            for comb in combs {
                let delayed = comb.delay[comb.cursor];
                comb.low += (delayed - comb.low) * 0.36;
                comb.delay[comb.cursor] = mono + comb.low * 0.69;
                wet += delayed;
                comb.cursor += 1;
                if comb.cursor == comb.delay.len() {
                    comb.cursor = 0;
                }
            }
            let value = channel[0] + wet * 0.055;
            channel[0] = if value.is_finite() { value.tanh() } else { 0.0 };
        }
    }
    // End at exact zero to avoid clicks when a buffer finishes or is exported.
    let fade_frames = (SAMPLE_RATE as usize / 25).min(samples.len() / 2);
    let frames = samples.len() / 2;
    for index in 0..fade_frames {
        let gain = index as f32 / fade_frames.max(1) as f32;
        let at = (frames - 1 - index) * 2;
        samples[at] *= gain;
        samples[at + 1] *= gain;
    }
}

/// A self-contained RIFF/WAVE file: PCM signed 16-bit, two channels, 44.1 kHz.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let count = (samples.len() / 2 * 2).min(((u32::MAX as usize - 36) / 2) & !1);
    let data_length = (count * 2) as u32;
    let mut output = Vec::with_capacity(count * 2 + 44);
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&(36 + data_length).to_le_bytes());
    output.extend_from_slice(b"WAVEfmt ");
    output.extend_from_slice(&16_u32.to_le_bytes());
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&2_u16.to_le_bytes());
    output.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    output.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    output.extend_from_slice(&4_u16.to_le_bytes());
    output.extend_from_slice(&16_u16.to_le_bytes());
    output.extend_from_slice(b"data");
    output.extend_from_slice(&data_length.to_le_bytes());
    for sample in &samples[..count] {
        let value = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        output.extend_from_slice(&((value * i16::MAX as f32).round() as i16).to_le_bytes());
    }
    output
}

/// Owns the audio device for the application's entire lifetime.
pub struct Playback {
    stream: OutputStream,
    sink: Sink,
    volume: f32,
}

impl Playback {
    pub fn new() -> Result<Self, String> {
        let mut stream = OutputStreamBuilder::open_default_stream()
            .map_err(|e| format!("Could not open audio output: {e}"))?;
        stream.log_on_drop(false);
        let sink = Sink::connect_new(stream.mixer());
        Ok(Self {
            stream,
            sink,
            volume: 0.8,
        })
    }

    pub fn play(&mut self, samples: Vec<f32>) {
        self.stop();
        self.sink
            .append(SamplesBuffer::new(2, SAMPLE_RATE, samples));
        self.sink.play();
    }

    pub fn stop(&mut self) {
        self.sink.stop();
        self.sink = Sink::connect_new(self.stream.mixer());
        self.sink.set_volume(self.volume);
    }

    pub fn pause(&mut self) {
        self.sink.pause();
    }
    pub fn resume(&mut self) {
        self.sink.play();
    }
    pub fn is_playing(&self) -> bool {
        !self.sink.empty() && !self.sink.is_paused()
    }
    pub fn is_paused(&self) -> bool {
        !self.sink.empty() && self.sink.is_paused()
    }
    pub fn position_seconds(&self) -> f32 {
        self.sink.get_pos().as_secs_f32()
    }
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = finite_clamp(volume, 0.0, 2.0, 0.8);
        self.sink.set_volume(self.volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Pitch};

    fn one_note() -> Score {
        let mut score = Score::blank();
        score.tempo = 120;
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes = vec![Note {
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 100,
            staccato: false,
            tied: false,
        }];
        score
    }

    #[test]
    fn all_instruments_produce_finite_audible_samples() {
        for instrument in Instrument::ALL {
            let samples = render_note(*instrument, 60, 100, 0.12);
            assert_eq!(samples.len() % 2, 0);
            assert!(
                samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                "{instrument:?}"
            );
            assert!(samples.iter().any(|s| s.abs() > 0.01), "{instrument:?}");
            assert_eq!(samples[samples.len() - 1], 0.0);
        }
    }

    #[test]
    fn score_duration_has_release_tail_and_follows_tempo() {
        let mut score = one_note();
        assert!((duration_seconds(&score) - 2.0).abs() < 0.001);
        let samples = render(&score);
        assert!((samples.len() as f32 / (SAMPLE_RATE as f32 * 2.0) - 3.8).abs() < 0.001);
        score.tempo = 60;
        assert!((duration_seconds(&score) - 4.0).abs() < 0.001);
    }

    #[test]
    fn silence_mute_and_solo_are_honored() {
        let mut score = one_note();
        score.parts[0].muted = true;
        assert!(render(&score).iter().all(|s| *s == 0.0));
        score.parts[0].muted = false;
        let mut silent_solo = score.parts[0].clone();
        silent_solo.measures[0].notes.clear();
        silent_solo.solo = true;
        score.parts.push(silent_solo);
        assert!(render(&score).iter().all(|s| *s == 0.0));
        assert!(
            render_note(Instrument::Piano, 60, 0, 0.1)
                .iter()
                .all(|s| *s == 0.0)
        );
    }

    #[test]
    fn tied_notes_have_one_continuous_envelope() {
        let mut tied = one_note();
        tied.parts[0].instrument = Instrument::Flute;
        let first = &mut tied.parts[0].measures[0].notes[0];
        first.tied = true;
        let mut second = first.clone();
        second.start = PPQ;
        second.tied = false;
        tied.parts[0].measures[0].notes.push(second);
        let mut long = tied.clone();
        long.parts[0].measures[0].notes.truncate(1);
        long.parts[0].measures[0].notes[0].duration = Duration::Half;
        long.parts[0].measures[0].notes[0].tied = false;
        assert_eq!(render(&tied), render(&long));
    }

    #[test]
    fn instrument_spectra_are_distinct() {
        let sounds: Vec<_> = Instrument::LEGACY
            .iter()
            .map(|instrument| {
                let mut samples = render_note(*instrument, 60, 100, 0.2);
                samples.truncate((SAMPLE_RATE as f32 * 0.2) as usize * 2);
                samples
            })
            .collect();
        for first in 0..sounds.len() {
            for second in first + 1..sounds.len() {
                let difference = sounds[first]
                    .iter()
                    .zip(&sounds[second])
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f32>();
                assert!(
                    difference > 20.0,
                    "{:?} and {:?}",
                    Instrument::LEGACY[first],
                    Instrument::LEGACY[second]
                );
            }
        }
    }

    #[test]
    fn staccato_shortens_the_gate_and_pan_steers_the_dry_signal() {
        let mut score = one_note();
        score.parts[0].instrument = Instrument::Flute;
        score.parts[0].pan = -1.0;
        let sustained = render(&score);
        // The first room reflection starts after 33 ms, so this tests direct pan.
        let direct = &sustained[0..1200 * 2];
        assert!(direct.as_chunks::<2>().0.iter().any(|f| f[0].abs() > 0.01));
        assert!(
            direct
                .as_chunks::<2>()
                .0
                .iter()
                .all(|f| f[1].abs() < 0.000001)
        );
        score.parts[0].measures[0].notes[0].staccato = true;
        let short = render(&score);
        let interval =
            (0.44 * SAMPLE_RATE as f32) as usize * 2..(0.49 * SAMPLE_RATE as f32) as usize * 2;
        let sustained_energy: f32 = sustained[interval.clone()].iter().map(|s| s * s).sum();
        let short_energy: f32 = short[interval].iter().map(|s| s * s).sum();
        assert!(short_energy < sustained_energy * 0.2);
    }

    #[test]
    fn excessive_duration_is_rejected_before_allocating_audio() {
        let mut score = one_note();
        score.time_numerator = 32;
        score.time_denominator = 1;
        score.tempo = 1;
        assert!(duration_seconds(&score) > MAX_RENDER_SECONDS);
        assert!(render(&score).is_empty());
    }

    #[test]
    fn wave_has_correct_header_and_sanitizes_nonfinite_samples() {
        let bytes = wav(&[0.0, 0.0, 1.0, -1.0, f32::NAN, f32::INFINITY]);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 12);
        assert_eq!(bytes.len(), 56);
        assert_eq!(&bytes[52..], &[0, 0, 0, 0]);
    }

    fn segment(samples: &[f32], start: f32, end: f32) -> &[f32] {
        &samples[(start * SAMPLE_RATE as f32) as usize..(end * SAMPLE_RATE as f32) as usize]
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    // A windowed frequency projection measures the actual generated signal,
    // including envelopes, noise, vibrato and the post-formant normalization.
    fn amplitude_at(samples: &[f32], frequency: f32) -> f32 {
        let mut real = 0.0;
        let mut imaginary = 0.0;
        for (index, sample) in samples.iter().enumerate() {
            let window = 0.5 - 0.5 * (TAU * index as f32 / samples.len() as f32).cos();
            let phase = TAU * frequency * index as f32 / SAMPLE_RATE as f32;
            real += sample * window * phase.cos();
            imaginary += sample * window * phase.sin();
        }
        (real * real + imaginary * imaginary).sqrt() * 4.0 / samples.len() as f32
    }

    fn relative_high_frequency_energy(samples: &[f32]) -> f32 {
        samples
            .windows(2)
            .map(|s| (s[1] - s[0]).powi(2))
            .sum::<f32>()
            / samples.iter().map(|s| s * s).sum::<f32>()
    }

    #[test]
    fn bowed_low_e_has_a_strong_fundamental_and_sustains_its_weight() {
        let samples = synth_voice(Instrument::DoubleBass, 28, 100, 2.4);
        let steady = segment(&samples, 0.65, 1.65);
        let fundamental = amplitude_at(steady, 41.20345);
        assert!(fundamental > 0.08, "E1 fundamental: {fundamental}");
        assert!(fundamental > amplitude_at(steady, 55.0) * 8.0);
        let early = rms(segment(&samples, 0.5, 1.2));
        let late = rms(segment(&samples, 1.6, 2.3));
        assert!(
            late > early * 0.75 && late < early * 1.30,
            "{early} => {late}"
        );
    }

    #[test]
    fn timpani_low_e_is_tuned_and_rings_beyond_the_notated_stroke() {
        let samples = synth_voice(Instrument::Timpani, 40, 108, 0.20);
        let ring = segment(&samples, 0.18, 0.80);
        let fundamental = amplitude_at(ring, 82.4069);
        assert!(fundamental > 0.07, "E2 fundamental: {fundamental}");
        assert!(fundamental > amplitude_at(ring, 98.0) * 6.0);
        let strike = rms(segment(&samples, 0.05, 0.25));
        let sustained_ring = rms(segment(&samples, 0.85, 1.20));
        assert!(
            sustained_ring > strike * 0.20,
            "{strike} => {sustained_ring}"
        );
    }

    #[test]
    fn dynamics_change_tone_color_as_well_as_loudness() {
        for instrument in [
            Instrument::Piano,
            Instrument::Trumpet,
            Instrument::FrenchHorn,
        ] {
            let soft = synth_voice(instrument, 60, 45, 0.5);
            let loud = synth_voice(instrument, 60, 120, 0.5);
            let soft = segment(&soft, 0.10, 0.40);
            let loud = segment(&loud, 0.10, 0.40);
            assert!(rms(loud) > rms(soft) * 2.0, "{instrument:?} loudness");
            let darkness = relative_high_frequency_energy(soft);
            let brightness = relative_high_frequency_energy(loud);
            assert!(
                brightness > darkness * 1.35,
                "{instrument:?}: {darkness} => {brightness}"
            );
        }
    }

    #[test]
    fn clarinet_register_changes_the_even_harmonic_balance() {
        let balance = |midi: u8| {
            let samples = synth_voice(Instrument::Clarinet, midi, 95, 0.5);
            let samples = segment(&samples, 0.12, 0.42);
            let frequency = 440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0);
            let even =
                amplitude_at(samples, frequency * 2.0) + amplitude_at(samples, frequency * 4.0);
            let odd = amplitude_at(samples, frequency) + amplitude_at(samples, frequency * 3.0);
            even / odd
        };
        let low = balance(48);
        let high = balance(84);
        assert!(high > low * 1.30, "low {low}, high {high}");
    }

    #[test]
    fn successive_strokes_vary_but_renders_remain_reproducible() {
        for (instrument, midi) in [
            (Instrument::Timpani, 40),
            (Instrument::DoubleBass, 28),
            (Instrument::Piano, 60),
        ] {
            let first = synth_voice_seeded(instrument, midi, 100, 0.6, 480);
            let repeated = synth_voice_seeded(instrument, midi, 100, 0.6, 480);
            let next = synth_voice_seeded(instrument, midi, 100, 0.6, 960);
            assert_eq!(first, repeated, "{instrument:?}");
            let difference = first
                .iter()
                .zip(&next)
                .take(SAMPLE_RATE as usize / 5)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f32>();
            let energy = first
                .iter()
                .take(SAMPLE_RATE as usize / 5)
                .map(|s| s * s)
                .sum::<f32>();
            assert!(
                difference > energy * 0.003,
                "{instrument:?}: no audible onset variation"
            );
        }
    }

    #[test]
    fn low_and_high_register_releases_are_finite_and_damped() {
        for (instrument, midi) in [
            (Instrument::DoubleBass, 28),
            (Instrument::Timpani, 40),
            (Instrument::Piano, 96),
            (Instrument::FrenchHorn, 41),
            (Instrument::Piccolo, 108),
            (Instrument::StringEnsemble, 79),
        ] {
            let samples = synth_voice(instrument, midi, 127, 0.3);
            assert!(samples.iter().all(|s| s.is_finite()), "{instrument:?}");
            assert!(
                samples[samples.len() - 1].abs() < 0.000001,
                "{instrument:?}"
            );
            let tail = &samples[samples.len() - SAMPLE_RATE as usize / 50..];
            assert!(rms(tail) < rms(&samples) * 0.02, "{instrument:?}");
        }
    }
}
