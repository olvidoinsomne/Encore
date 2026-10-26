use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Barre {
    pub from: usize,
    pub to: usize,
    pub fret: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fretboard {
    /// Low-to-high string order; MusicXML numbers these in reverse.
    pub frets: Vec<Option<u8>>,
    /// None means the source supplies geometry without identifying its tuning.
    #[serde(default)]
    pub tuning: Option<Vec<u8>>,
    #[serde(default)]
    pub fingers: Vec<Option<u8>>,
    pub first_fret: u8,
    pub displayed_frets: u8,
    #[serde(default)]
    pub barres: Vec<Barre>,
}
impl Fretboard {
    pub const GUITAR: [u8; 6] = [40, 45, 50, 55, 59, 64];
    pub fn guitar(frets: Vec<Option<u8>>) -> Self {
        let first = frets
            .iter()
            .flatten()
            .copied()
            .filter(|f| *f > 0)
            .min()
            .unwrap_or(1);
        let first_fret = if frets.iter().flatten().copied().max().unwrap_or(0) <= 4 {
            1
        } else {
            first
        };
        Self {
            fingers: vec![None; frets.len()],
            frets,
            tuning: Some(Self::GUITAR.to_vec()),
            first_fret,
            displayed_frets: 4,
            barres: vec![],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(4..=12).contains(&self.frets.len())
            || !(1..=36).contains(&self.first_fret)
            || !(1..=8).contains(&self.displayed_frets)
            || u16::from(self.first_fret) + u16::from(self.displayed_frets) > 37
        {
            return Err("Fretboards require 4–12 strings and a window within frets 1–36.".into());
        }
        if !self.fingers.is_empty()
            && (self.fingers.len() != self.frets.len()
                || self.fingers.iter().flatten().any(|f| *f > 5))
        {
            return Err("Fretboard fingerings must match the string count and be 0–5.".into());
        }
        if let Some(tuning) = &self.tuning
            && (tuning.len() != self.frets.len()
                || tuning.iter().any(|pitch| *pitch > 127)
                || tuning
                    .iter()
                    .zip(&self.frets)
                    .any(|(pitch, fret)| u16::from(*pitch) + u16::from(fret.unwrap_or(0)) > 127))
        {
            return Err("Fretboard tuning and frets must remain within MIDI 0–127.".into());
        }
        let end = self.first_fret + self.displayed_frets - 1;
        if self
            .frets
            .iter()
            .flatten()
            .any(|fret| *fret > 36 || (*fret > 0 && (*fret < self.first_fret || *fret > end)))
        {
            return Err("Every stopped string must fit the displayed fret window.".into());
        }
        if self.barres.len() > 12
            || self.barres.iter().any(|barre| {
                barre.from >= barre.to
                    || barre.to >= self.frets.len()
                    || barre.fret < self.first_fret
                    || barre.fret > end
                    || self.frets[barre.from] != Some(barre.fret)
                    || self.frets[barre.to] != Some(barre.fret)
            })
        {
            return Err(
                "A barre needs two existing stopped-string endpoints in the displayed window."
                    .into(),
            );
        }
        Ok(())
    }
    /// Preserve a custom shape by shifting frets, including formerly open
    /// strings. Octave transpositions leave the geometry unchanged.
    pub fn transpose(&mut self, semitones: i8) -> Result<(), String> {
        let semitones = semitones.rem_euclid(12);
        if semitones == 0 {
            return Ok(());
        }
        let mut choices = [semitones, semitones - 12];
        choices.sort_by_key(|shift| shift.abs());
        for shift in choices {
            let mut candidate = self.clone();
            let valid = candidate.frets.iter_mut().flatten().all(|fret| {
                let new = i16::from(*fret) + i16::from(shift);
                if !(0..=36).contains(&new) {
                    return false;
                }
                *fret = new as u8;
                true
            });
            if !valid {
                continue;
            }
            for barre in &mut candidate.barres {
                let new = i16::from(barre.fret) + i16::from(shift);
                if !(1..=36).contains(&new) {
                    continue;
                }
                barre.fret = new as u8;
            }
            let min = candidate
                .frets
                .iter()
                .flatten()
                .copied()
                .filter(|f| *f > 0)
                .min()
                .unwrap_or(1);
            let max = candidate.frets.iter().flatten().copied().max().unwrap_or(0);
            candidate.first_fret = if max <= candidate.displayed_frets {
                1
            } else {
                min
            };
            if candidate.validate().is_ok() {
                *self = candidate;
                return Ok(());
            }
        }
        Err("This custom fretboard cannot be transposed within its fret window; edit or remove the diagram first.".into())
    }
    pub fn sounding_pitches(&self) -> Option<Vec<u8>> {
        let tuning = self.tuning.as_ref()?;
        Some(
            tuning
                .iter()
                .zip(&self.frets)
                .filter_map(|(pitch, fret)| fret.and_then(|fret| pitch.checked_add(fret)))
                .collect(),
        )
    }
}

/// A bounded original search over standard guitar tuning, rather than a copied
/// fretboard library. Higher extensions may omit the fifth and unaltered 11th.
pub fn suggest_fretboards(chord: &ChordSpec) -> Vec<Fretboard> {
    let Some(root) = chord.root else {
        return vec![];
    };
    let allowed = pitches(chord);
    if allowed.len() < 2 {
        return vec![];
    }
    let mut required = vec![root.midi_class()];
    for (degree, interval) in chord.tones() {
        if matches!(degree, 3 | 7)
            || chord
                .degrees
                .iter()
                .any(|d| d.value == degree && d.operation != DegreeOperation::Subtract)
        {
            required
                .push((i16::from(root.midi_class()) + i16::from(interval)).rem_euclid(12) as u8);
        }
    }
    required.sort_unstable();
    required.dedup();
    if required.len() > 6 {
        return vec![];
    }
    let bass = chord.bass.map(PitchClass::midi_class);
    let mut found = Vec::new();
    let mut budget = 180_000usize;
    for first in 1..=12u8 {
        let options = Fretboard::GUITAR
            .iter()
            .map(|pitch| {
                let mut values = vec![None];
                if allowed.contains(&(pitch % 12)) {
                    values.push(Some(0));
                }
                for fret in first..first + 4 {
                    if allowed.contains(&((pitch + fret) % 12)) {
                        values.push(Some(fret));
                    }
                }
                values
            })
            .collect::<Vec<_>>();
        let mut shape = vec![None; 6];
        search(
            0,
            &options,
            &mut shape,
            &required,
            bass,
            &mut budget,
            &mut found,
        );
        if budget == 0 {
            break;
        }
    }
    found.sort_by_key(|(cost, shape)| (*cost, shape.frets.clone()));
    found.dedup_by(|a, b| a.1.frets == b.1.frets);
    found.into_iter().take(12).map(|(_, frame)| frame).collect()
}
fn search(
    index: usize,
    options: &[Vec<Option<u8>>],
    shape: &mut Vec<Option<u8>>,
    required: &[u8],
    bass: Option<u8>,
    budget: &mut usize,
    found: &mut Vec<(u32, Fretboard)>,
) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;
    if index < shape.len() {
        for fret in &options[index] {
            shape[index] = *fret;
            search(index + 1, options, shape, required, bass, budget, found);
        }
        return;
    }
    let played = shape.iter().filter(|f| f.is_some()).count();
    if played < 3 {
        return;
    }
    let sounding = shape
        .iter()
        .zip(Fretboard::GUITAR)
        .filter_map(|(fret, pitch)| fret.map(|fret| pitch + fret))
        .collect::<Vec<_>>();
    if required
        .iter()
        .any(|pitch| !sounding.iter().any(|note| note % 12 == *pitch))
        || bass.is_some_and(|bass| sounding.first().is_none_or(|pitch| pitch % 12 != bass))
    {
        return;
    }
    let stopped = shape
        .iter()
        .flatten()
        .copied()
        .filter(|fret| *fret > 0)
        .collect::<Vec<_>>();
    let min = stopped.iter().copied().min().unwrap_or(1);
    let max = stopped.iter().copied().max().unwrap_or(0);
    let min_count = stopped.iter().filter(|fret| **fret == min).count();
    let fingers = stopped.len().saturating_sub(min_count.saturating_sub(1));
    if fingers > 4 || max.saturating_sub(min) > 3 {
        return;
    }
    let mut frame = Fretboard::guitar(shape.clone());
    if min_count > 1 {
        let start = shape.iter().position(|f| *f == Some(min)).unwrap();
        let end = shape.iter().rposition(|f| *f == Some(min)).unwrap();
        if shape[start..=end]
            .iter()
            .all(|f| f.is_some_and(|f| f >= min))
        {
            frame.barres.push(Barre {
                from: start,
                to: end,
                fret: min,
            });
        }
    }
    if frame.validate().is_err() {
        return;
    }
    let cost = shape
        .iter()
        .flatten()
        .map(|fret| u32::from(*fret))
        .sum::<u32>()
        + u32::from(max.saturating_sub(min)) * 4
        + (6 - played) as u32 * 4
        + u32::from(min) * 2;
    found.push((cost, frame));
    if found.len() > 256 {
        found.sort_by_key(|(cost, _)| *cost);
        found.truncate(128);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_suggestions_contain_only_chord_tones() {
        for text in ["C", "Am", "G7", "F#m7b5", "C/E"] {
            let chord = parse(text).unwrap();
            let shapes = suggest_fretboards(&chord);
            assert!(!shapes.is_empty(), "{text}");
            for shape in shapes {
                shape.validate().unwrap();
                let notes = shape.sounding_pitches().unwrap();
                assert!(
                    notes
                        .iter()
                        .all(|pitch| pitches(&chord).contains(&(pitch % 12)))
                );
                if let Some(bass) = chord.bass {
                    assert_eq!(notes[0] % 12, bass.midi_class());
                }
            }
        }
    }
    #[test]
    fn custom_frame_transposition_preserves_sound_and_unknown_tuning() {
        let mut frame = Fretboard::guitar(vec![None, Some(3), Some(2), Some(0), Some(1), Some(0)]);
        let before = frame.sounding_pitches().unwrap();
        frame.transpose(2).unwrap();
        assert_eq!(
            frame.sounding_pitches().unwrap(),
            before.iter().map(|n| n + 2).collect::<Vec<_>>()
        );
        frame.tuning = None;
        frame.transpose(12).unwrap();
        assert!(frame.sounding_pitches().is_none());
    }
}
