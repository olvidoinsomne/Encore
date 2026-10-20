//! Staff-beat harmony collection before independent source voices are mapped.
use super::{Warnings, child, number, value};
use crate::{
    chords::{self, ChordSymbol, PitchClass},
    document::Anchor,
    model::{Score, Step},
    parts::Transposition,
};
use roxmltree::Node;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Chords {
    items: Vec<(u32, ChordSymbol)>,
    positions: BTreeMap<(u32, usize, u32), Vec<usize>>,
}
impl Chords {
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        staff: u32,
        measure: usize,
        tick: u32,
        interval: Transposition,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let anchor = Anchor {
            part: 0,
            measure,
            tick,
            layer: 0,
        };
        let symbols = match chords::musicxml::read_harmony(node, anchor) {
            Ok(symbols) => symbols,
            Err(error) => {
                warnings.add(&error);
                return Ok(());
            }
        };
        for mut symbol in symbols {
            chords::transpose_symbol(&mut symbol, interval)?;
            self.push(staff, symbol, warnings)?;
        }
        Ok(())
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        staff: u32,
        measure: usize,
        tick: u32,
        interval: Transposition,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if number::<u8>(node, "harmonyType")?.unwrap_or(0) != 0 || child(node, "function").is_some()
        {
            warnings.add("MuseScore functional/Nashville harmony is not retained; use letter-name chords for interchange.");
            return Ok(());
        }
        let suffix = value(node, "name").unwrap_or("");
        let root = number::<i32>(node, "root")?.map(tpc).transpose()?;
        let name = if let Some(root) = root {
            format!(
                "{}{}{}",
                root.label(),
                if let Some(tail) = suffix.strip_prefix('=') {
                    format!("m{tail}")
                } else {
                    suffix.into()
                },
                number::<i32>(node, "base")?
                    .map(tpc)
                    .transpose()?
                    .map_or_else(String::new, |bass| format!("/{}", bass.label()))
            )
        } else {
            suffix.into()
        };
        // A numeric extension refers to a document-specific suffix library. It
        // cannot be inferred from its identifier alone.
        if root.is_some()
            && suffix.is_empty()
            && number::<i32>(node, "extension")?.is_some_and(|id| id != 1)
        {
            warnings.add("A MuseScore chord using only a private suffix-library number was omitted; export it through MusicXML to retain its semantics.");
            return Ok(());
        }
        let spec = match chords::parse(&name) {
            Ok(spec) => spec,
            Err(error) => {
                warnings.add(format!("MuseScore chord '{name}' was omitted: {error}"));
                return Ok(());
            }
        };
        let mut symbol = ChordSymbol::new(
            Anchor {
                part: 0,
                measure,
                tick,
                layer: 0,
            },
            spec,
        );
        symbol.visible = value(node, "visible") != Some("0");
        symbol.play = value(node, "play") != Some("0");
        chords::transpose_symbol(&mut symbol, interval)?;
        if node.children().any(|n| n.has_tag_name("degree")) {
            warnings.add("MuseScore explicit chord-degree records are represented by its readable suffix; use MusicXML to preserve independent degree overrides.");
        }
        self.push(staff, symbol, warnings)
    }
    fn push(
        &mut self,
        staff: u32,
        mut symbol: ChordSymbol,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if self.items.len() >= chords::MAX_CHORDS {
            return Err("Imported chord count exceeds the document limit.".into());
        }
        let positions = self
            .positions
            .entry((staff, symbol.anchor.measure, symbol.anchor.tick))
            .or_default();
        if positions.iter().any(|index| {
            let prior = &self.items[*index].1;
            prior.anchor == symbol.anchor
                && prior.chord == symbol.chord
                && prior.fretboard == symbol.fretboard
        }) {
            return Ok(());
        }
        while positions.iter().any(|index| {
            let prior = &self.items[*index].1;
            prior.anchor == symbol.anchor && prior.slot == symbol.slot
        }) {
            symbol.slot += 1;
            if symbol.slot > 7 {
                warnings.add("More than eight simultaneous chord symbols were omitted.");
                return Ok(());
            }
        }
        positions.push(self.items.len());
        self.items.push((staff, symbol));
        Ok(())
    }
    pub(super) fn shift(&mut self, measure: usize, ticks: u32) {
        for (_, symbol) in &mut self.items {
            if symbol.anchor.measure == measure {
                symbol.anchor.tick = symbol.anchor.tick.saturating_add(ticks);
            }
        }
    }
    pub(super) fn finish(
        self,
        score: &mut Score,
        mapping: &BTreeMap<(u32, String), (usize, u8)>,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        for (staff, mut symbol) in self.items {
            let Some((part, _)) = mapping
                .iter()
                .find_map(|((source, _), target)| (*source == staff).then_some(target))
            else {
                warnings.add("A chord symbol on a missing source staff was omitted.");
                continue;
            };
            symbol.anchor.part = *part;
            symbol.anchor.layer = 0;
            if symbol.anchor.tick >= score.measure_ticks() {
                return Err("Imported chord symbol extends outside its measure.".into());
            }
            score.notation.chords.push(symbol);
        }
        chords::validate(score)
    }
}
fn tpc(value: i32) -> Result<PitchClass, String> {
    if !(-1..=33).contains(&value) {
        return Err("MuseScore chord tonal pitch class is outside its supported range.".into());
    }
    let (step, natural) = [
        (Step::F, 13),
        (Step::C, 14),
        (Step::G, 15),
        (Step::D, 16),
        (Step::A, 17),
        (Step::E, 18),
        (Step::B, 19),
    ][(value - 13).rem_euclid(7) as usize];
    let alter = (value - natural) / 7;
    if !(-2..=2).contains(&alter) {
        return Err("MuseScore chord tonal pitch class exceeds double accidentals.".into());
    }
    Ok(PitchClass {
        step,
        alter: alter as i8,
    })
}

/// Prefer the declared diatonic interval. If absent, use the conventional
/// spelling for a chromatic transposing-instrument interval.
pub(super) fn concert_interval(
    semitones: i16,
    diatonic: Option<i16>,
) -> Result<Transposition, String> {
    let chromatic = semitones.rem_euclid(12);
    let octaves = semitones.div_euclid(12);
    let inferred = [0, 1, 1, 2, 2, 3, 3, 4, 5, 5, 6, 6][chromatic as usize] + octaves * 7;
    let interval = Transposition {
        semitones: semitones
            .try_into()
            .map_err(|_| "Chord transposition exceeds its supported range.")?,
        diatonic_steps: diatonic
            .unwrap_or(inferred)
            .try_into()
            .map_err(|_| "Chord transposition exceeds its supported range.")?,
    };
    interval.validate()?;
    Ok(interval)
}
