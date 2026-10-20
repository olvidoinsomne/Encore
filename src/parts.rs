//! Linked parts are definitions over the concert-pitch score, never detached copies.
//! A projection can be regenerated after every source edit without changing the
//! document, its undo history, or its sounding pitches.
use crate::document::Anchor;
use crate::model::{Clef, Instrument, Pitch, Score, Selection, Step};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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

/// Notes to retain within each selected entry. Ranks count sounding pitches,
/// independently of the order pitches were entered into the source chord.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChordFilter {
    #[default]
    All,
    Highest,
    Lowest,
    Ranked {
        from_top: bool,
        ranks: Vec<u8>,
    },
}
impl ChordFilter {
    pub fn name(&self) -> &'static str {
        match self {
            Self::All => "All chord notes",
            Self::Highest => "Highest note",
            Self::Lowest => "Lowest note",
            Self::Ranked { .. } => "Selected chord positions",
        }
    }
    fn validate(&self) -> Result<(), String> {
        if let Self::Ranked { ranks, .. } = self {
            let unique: BTreeSet<_> = ranks.iter().copied().collect();
            if ranks.is_empty()
                || ranks.len() > 16
                || unique.len() != ranks.len()
                || ranks.iter().any(|rank| !(1..=16).contains(rank))
            {
                return Err(
                    "Voicing chord positions must be unique numbers from 1 through 16.".into(),
                );
            }
        }
        Ok(())
    }
    fn indices(&self, pitches: &[Pitch], include_single_notes: bool) -> Vec<usize> {
        if pitches.len() == 1 && !include_single_notes && *self != Self::All {
            return Vec::new();
        }
        if *self == Self::All || pitches.len() <= 1 {
            return (0..pitches.len()).collect();
        }
        let mut sorted: Vec<_> = (0..pitches.len()).collect();
        sorted.sort_unstable_by_key(|&index| pitches[index].midi());
        let mut indices = match self {
            Self::All => sorted,
            Self::Highest => sorted.last().copied().into_iter().collect(),
            Self::Lowest => sorted.first().copied().into_iter().collect(),
            Self::Ranked { from_top, ranks } => ranks
                .iter()
                .filter_map(|&rank| {
                    let position = if *from_top {
                        sorted.len().checked_sub(usize::from(rank))?
                    } else {
                        usize::from(rank).checked_sub(1)?
                    };
                    sorted.get(position).copied()
                })
                .collect(),
        };
        indices.sort_unstable();
        indices
    }
}
fn all_lanes() -> u8 {
    u8::MAX
}
fn include_single() -> bool {
    true
}
/// A rule for one source staff in a linked part. The lane mask uses bit
/// `layer + 4 * voice`; a missing rule keeps every source entry and pitch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaffVoicing {
    pub staff: usize,
    #[serde(default = "all_lanes")]
    pub lanes: u8,
    #[serde(default)]
    pub chord: ChordFilter,
    #[serde(default = "include_single")]
    pub include_single_notes: bool,
    /// When a source bar uses multiple layers, use whole chords from this
    /// zero-based layer instead of applying the single-layer chord selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple_layers: Option<u8>,
}
impl StaffVoicing {
    pub fn new(staff: usize) -> Self {
        Self {
            staff,
            lanes: all_lanes(),
            chord: ChordFilter::All,
            include_single_notes: true,
            multiple_layers: None,
        }
    }
    pub fn is_filtered(&self) -> bool {
        self.lanes != all_lanes()
            || self.chord != ChordFilter::All
            || self.multiple_layers.is_some()
    }
    fn validate(&self) -> Result<(), String> {
        if self.lanes == 0 {
            return Err("Part voicing must include at least one layer/voice.".into());
        }
        if let Some(layer) = self.multiple_layers
            && (layer > 3 || self.lanes & ((1 << layer) | (1 << (layer + 4))) == 0)
        {
            return Err(
                "The multiple-layer fallback must be a selected layer from 1 through 4.".into(),
            );
        }
        self.chord.validate()
    }
    fn policy(&self, measure: &crate::model::Measure) -> BarVoicing {
        let multiple = self.multiple_layers.filter(|_| {
            let layers: BTreeSet<_> = measure.notes.iter().map(|note| note.layer).collect();
            layers.len() > 1
        });
        if let Some(layer) = multiple {
            BarVoicing {
                lanes: self.lanes & ((1 << layer) | (1 << (layer + 4))),
                chord: ChordFilter::All,
                include_single_notes: true,
            }
        } else {
            BarVoicing {
                lanes: self.lanes,
                chord: self.chord.clone(),
                include_single_notes: self.include_single_notes,
            }
        }
    }
}
#[derive(Clone)]
struct BarVoicing {
    lanes: u8,
    chord: ChordFilter,
    include_single_notes: bool,
}
impl Default for BarVoicing {
    fn default() -> Self {
        Self {
            lanes: all_lanes(),
            chord: ChordFilter::All,
            include_single_notes: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkedPart {
    /// Stable identity independent of the display name and ordering.
    pub id: String,
    pub name: String,
    pub staves: Vec<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voicing: Vec<StaffVoicing>,
    #[serde(default)]
    pub transposition: Transposition,
    #[serde(default)]
    pub layout: PartLayout,
}

impl LinkedPart {
    pub fn is_voiced(&self) -> bool {
        self.voicing.iter().any(StaffVoicing::is_filtered)
    }
    pub fn source_staff_is_voiced(&self, staff: usize) -> bool {
        self.voicing
            .iter()
            .any(|rule| rule.staff == staff && rule.is_filtered())
    }
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
        if self.voicing.len() > self.staves.len() {
            return Err(
                "A linked part can have at most one voicing rule per included staff.".into(),
            );
        }
        let mut ruled = BTreeSet::new();
        for rule in &self.voicing {
            if !seen.contains(&rule.staff) || !ruled.insert(rule.staff) {
                return Err("Voicing rules must reference included staves once only.".into());
            }
            rule.validate()?;
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
            voicing: Vec::new(),
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
struct ProjectedEntry {
    source: Selection,
    source_pitch_indices: Vec<usize>,
}
#[derive(Clone, Debug)]
pub struct Projection {
    pub score: Score,
    /// Displayed staff index -> source score staff index.
    pub source_staves: Vec<usize>,
    pub layout: PartLayout,
    pub transposition: Transposition,
    /// Summarized losses of note-attached notation caused by the voicing rule.
    pub warnings: Vec<String>,
    entries: Vec<Vec<Vec<ProjectedEntry>>>,
    reverse_entries: BTreeMap<(usize, usize, usize), Selection>,
    voiced_staves: BTreeSet<usize>,
}
impl Projection {
    pub fn is_voiced(&self) -> bool {
        !self.voiced_staves.is_empty()
    }
    pub fn source_staff_is_voiced(&self, staff: usize) -> bool {
        self.voiced_staves.contains(&staff)
    }
    fn entry(&self, selection: Selection) -> Option<&ProjectedEntry> {
        self.entries
            .get(selection.part)?
            .get(selection.measure)?
            .get(selection.note)
    }
    pub fn source_selection(&self, selection: Selection) -> Option<Selection> {
        Some(self.entry(selection)?.source)
    }
    pub fn project_selection(&self, selection: Selection) -> Option<Selection> {
        self.reverse_entries
            .get(&(selection.part, selection.measure, selection.note))
            .copied()
    }
    /// Resolve the retained display chord tone to its exact original tone.
    pub fn source_pitch(&self, selection: Selection, pitch: usize) -> Option<(Selection, usize)> {
        let entry = self.entry(selection)?;
        Some((entry.source, *entry.source_pitch_indices.get(pitch)?))
    }
    pub fn project_pitch(&self, selection: Selection, pitch: usize) -> Option<(Selection, usize)> {
        let displayed = self.project_selection(selection)?;
        let index = self
            .entry(displayed)?
            .source_pitch_indices
            .iter()
            .position(|source| *source == pitch)?;
        Some((displayed, index))
    }
}
/// A written-pitch view of one part. The source score is never modified.
pub fn project(score: &Score, part: &LinkedPart) -> Result<Projection, String> {
    project_internal(score, part, true)
}
/// Make an independent score containing this part's selected source content.
/// Concert extraction preserves sounding pitches; written extraction follows the
/// part's spelled interval for notation interchange and printing.
pub fn extract(score: &Score, part: &LinkedPart, written: bool) -> Result<Score, String> {
    project_internal(score, part, written).map(|projection| projection.score)
}
fn project_internal(score: &Score, part: &LinkedPart, written: bool) -> Result<Projection, String> {
    score.validate()?;
    part.validate(score.parts.len())?;
    part.layout.validate_for_score(score.measure_count())?;
    let interval = if written {
        part.transposition
    } else {
        Transposition::CONCERT
    };
    let mut projected = score.clone();
    projected.title = format!("{} — {}", score.title, part.name);
    projected.key_fifths = interval.written_key(score.key_fifths)?;
    projected.parts.clear();
    let mut entries = Vec::new();
    let mut reverse_entries = BTreeMap::new();
    let mut branch_breaks = BTreeSet::new();
    let mut policies = BTreeMap::new();
    for (display_staff, &source_staff) in part.staves.iter().enumerate() {
        let mut staff = score.parts[source_staff].clone();
        let rule = part
            .voicing
            .iter()
            .find(|rule| rule.staff == source_staff && rule.is_filtered());
        let mut staff_entries = Vec::new();
        for (measure_index, measure) in staff.measures.iter_mut().enumerate() {
            let policy = rule.map_or_else(BarVoicing::default, |rule| rule.policy(measure));
            if rule.is_some() {
                policies.insert((source_staff, measure_index), policy.clone());
            }
            let mut measure_entries = Vec::new();
            let mut retained = Vec::new();
            for (source_note, mut note) in
                std::mem::take(&mut measure.notes).into_iter().enumerate()
            {
                if policy.lanes & (1 << note.lane()) == 0 {
                    continue;
                }
                let source_pitch_indices = policy
                    .chord
                    .indices(&note.pitches, policy.include_single_notes);
                note.pitches = source_pitch_indices
                    .iter()
                    .map(|&index| note.pitches[index])
                    .collect();
                if note.pitches.is_empty() {
                    note.tied = false;
                }
                let source = Selection {
                    part: source_staff,
                    measure: measure_index,
                    note: source_note,
                };
                let displayed = Selection {
                    part: display_staff,
                    measure: measure_index,
                    note: retained.len(),
                };
                reverse_entries.insert((source.part, source.measure, source.note), displayed);
                measure_entries.push(ProjectedEntry {
                    source,
                    source_pitch_indices,
                });
                retained.push(note);
            }
            // A filtered primary launch must not leave a dangling reference in
            // the independent score. Keep the secondary rhythm and its branches.
            let primary: BTreeSet<_> = retained
                .iter()
                .filter(|n| n.voice == 0)
                .map(|n| (n.layer, n.start))
                .collect();
            let mut branches = BTreeSet::new();
            for note in &mut retained {
                if let Some(launch) = note.voice_launch
                    && !primary.contains(&(note.layer, launch))
                {
                    if branches.insert((note.layer, launch)) {
                        branch_breaks.insert(Anchor {
                            part: display_staff,
                            measure: measure_index,
                            tick: note.start,
                            layer: note.layer,
                            voice: note.voice,
                        });
                    }
                    note.voice_launch = None;
                }
            }
            measure.notes = retained;
            staff_entries.push(measure_entries);
        }
        projected.parts.push(staff);
        entries.push(staff_entries);
    }
    let mut warnings = project_notation(&mut projected, part, &policies, &branch_breaks)?;
    let removed_ties = filter_ties(&mut projected, part);
    if removed_ties > 0 {
        warnings.push(format!(
            "Voicing removed {removed_ties} ties without a retained continuation."
        ));
    }
    transpose_projection(&mut projected, interval)?;
    projected.notation.layout = part.layout.clone();
    projected.validate()?;
    Ok(Projection {
        score: projected,
        source_staves: part.staves.clone(),
        layout: part.layout.clone(),
        transposition: interval,
        warnings,
        entries,
        reverse_entries,
        voiced_staves: part
            .voicing
            .iter()
            .filter(|rule| rule.is_filtered())
            .map(|rule| rule.staff)
            .collect(),
    })
}
fn project_notation(
    score: &mut Score,
    definition: &LinkedPart,
    policies: &BTreeMap<(usize, usize), BarVoicing>,
    branch_breaks: &BTreeSet<Anchor>,
) -> Result<Vec<String>, String> {
    let mut losses = BTreeMap::<&str, usize>::new();
    let mut count = |kind| {
        *losses.entry(kind).or_default() += 1;
    };
    let source_staves = &definition.staves;
    let map = |anchor: &mut Anchor| {
        if let Some(index) = source_staves.iter().position(|staff| *staff == anchor.part) {
            anchor.part = index;
            true
        } else {
            false
        }
    };
    // Layer-scoped directions still govern either selected rhythmic voice.
    let layer_selected = |at: &Anchor| {
        definition
            .voicing
            .iter()
            .find(|rule| rule.staff == at.part)
            .is_none_or(|rule| rule.lanes & ((1 << at.layer) | (1 << (at.layer + 4))) != 0)
    };
    let mut onsets = BTreeMap::<Anchor, Vec<u8>>::new();
    for (part, staff) in score.parts.iter().enumerate() {
        for (measure, bar) in staff.measures.iter().enumerate() {
            for note in &bar.notes {
                onsets.insert(
                    Anchor {
                        part,
                        measure,
                        tick: note.start,
                        layer: note.layer,
                        voice: note.voice,
                    },
                    note.pitches.iter().map(|pitch| pitch.midi()).collect(),
                );
            }
        }
    }
    let pitched = |anchor: &Anchor| {
        onsets
            .get(anchor)
            .is_some_and(|pitches| !pitches.is_empty())
    };
    score.notation.lyrics.retain_mut(|lyric| {
        if !map(&mut lyric.anchor) {
            return false;
        }
        if !pitched(&lyric.anchor) {
            count("lyric syllables");
            return false;
        }
        if let Some(mut end) = lyric.extend_to
            && (!map(&mut end) || !pitched(&end))
        {
            // Keep the syllable and end the melisma at the last retained note.
            lyric.extend_to = onsets
                .range(lyric.anchor..=end)
                .filter(|(at, pitches)| {
                    at.lane() == lyric.anchor.lane()
                        && !pitches.is_empty()
                        && crate::lyrics::position(at) > crate::lyrics::position(&lyric.anchor)
                        && crate::lyrics::position(at) <= crate::lyrics::position(&end)
                })
                .map(|(at, _)| *at)
                .next_back();
            count("word extensions shortened or removed");
        } else if let Some(end) = &mut lyric.extend_to {
            map(end);
        }
        true
    });
    score.notation.slurs.retain_mut(|slur| {
        if !map(&mut slur.start) || !map(&mut slur.end) {
            return false;
        }
        if pitched(&slur.start) && pitched(&slur.end) {
            true
        } else {
            count("slurs with hidden endpoints");
            false
        }
    });
    score.notation.entries.retain_mut(|entry| {
        if !map(&mut entry.anchor) {
            return false;
        }
        let Some(pitches) = onsets.get(&entry.anchor) else {
            return false;
        };
        entry
            .accidentals
            .retain(|mark| pitches.contains(&mark.pitch));
        if let Some(ties) = &mut entry.tie_pitches {
            ties.retain(|pitch| pitches.contains(pitch));
        }
        true
    });
    let mut style_indices: BTreeMap<_, _> = score
        .notation
        .entries
        .iter()
        .enumerate()
        .map(|(index, style)| (style.anchor, index))
        .collect();
    for &anchor in branch_breaks {
        if let Some(&index) = style_indices.get(&anchor) {
            score.notation.entries[index].beam = crate::entry_style::BeamStyle::Break;
        } else {
            let mut style = crate::entry_style::EntryStyle::new(anchor);
            style.beam = crate::entry_style::BeamStyle::Break;
            style_indices.insert(anchor, score.notation.entries.len());
            score.notation.entries.push(style);
        }
    }
    score.notation.expressions.retain_mut(|mark| {
        if mark.system {
            mark.anchor.part = 0;
            true
        } else {
            layer_selected(&mark.anchor) && map(&mut mark.anchor)
        }
    });
    score.notation.spanners.retain_mut(|line| {
        layer_selected(&line.start) && map(&mut line.start) && map(&mut line.end)
    });
    score
        .notation
        .chords
        .retain_mut(|symbol| map(&mut symbol.anchor));
    score.notation.ornaments.retain_mut(|mark| {
        if !map(&mut mark.anchor) {
            return false;
        }
        let length = onsets.get(&mark.anchor).map_or(0, Vec::len);
        let compatible = if mark.kind.chord() {
            (2..=16).contains(&length)
        } else {
            length == 1
        };
        if !compatible {
            count("ornaments incompatible with retained pitches");
        }
        compatible
    });
    score.notation.grace.retain_mut(|group| {
        let source = group.anchor;
        if !map(&mut group.anchor) {
            return false;
        }
        if !pitched(&group.anchor) {
            count("grace groups on hidden principals");
            return false;
        }
        if let Some(policy) = policies.get(&(source.part, source.measure)) {
            group.notes.retain_mut(|note| {
                let indices = policy
                    .chord
                    .indices(&note.pitches, policy.include_single_notes);
                note.pitches = indices
                    .into_iter()
                    .map(|index| note.pitches[index])
                    .collect();
                !note.pitches.is_empty()
            });
        }
        if group.notes.is_empty() {
            count("empty grace groups");
            false
        } else {
            true
        }
    });
    let measure_ticks = score.measure_ticks();
    let absolute = |at: Anchor| at.measure as u64 * u64::from(measure_ticks) + u64::from(at.tick);
    let mut lane_onsets = BTreeMap::<(usize, u8), Vec<(u64, bool)>>::new();
    for (at, pitches) in &onsets {
        lane_onsets
            .entry((at.part, at.lane()))
            .or_default()
            .push((absolute(*at), !pitches.is_empty()));
    }
    score.notation.lines.retain_mut(|line| {
        use crate::lines::LineKind;
        let filtered = definition
            .voicing
            .iter()
            .any(|rule| rule.staff == line.start.part && rule.is_filtered());
        let selected = match line.kind {
            LineKind::Pedal { .. } => true,
            _ => layer_selected(&line.start),
        };
        if !selected || !map(&mut line.start) || !map(&mut line.end) {
            return false;
        }
        let lane = lane_onsets
            .get(&(line.start.part, line.start.lane()))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let left = lane.partition_point(|(at, _)| *at < absolute(line.start));
        let right = lane.partition_point(|(at, _)| *at < absolute(line.end));
        let valid = !filtered
            || match line.kind {
                LineKind::Glissando => {
                    onsets.get(&line.start).is_some_and(|p| p.len() == 1)
                        && onsets.get(&line.end).is_some_and(|p| p.len() == 1)
                        && lane.partition_point(|(at, _)| *at <= absolute(line.start)) == right
                }
                // Trill spans use free beat endpoints, not necessarily note onsets.
                LineKind::Trill { .. } => lane[left..right].iter().any(|(_, pitched)| *pitched),
                _ => true,
            };
        if !valid {
            count("performance lines without retained notes");
        }
        valid
    });
    score.notation.parts.clear();
    Ok(losses
        .into_iter()
        .map(|(kind, count)| format!("Voicing omitted or adjusted {count} {kind}."))
        .collect())
}
fn filter_ties(score: &mut Score, definition: &LinkedPart) -> usize {
    if !definition.is_voiced() {
        return 0;
    }
    let bar_ticks = u64::from(score.measure_ticks());
    let mut onsets = BTreeMap::new();
    for (part, staff) in score.parts.iter().enumerate() {
        for (measure, bar) in staff.measures.iter().enumerate() {
            for note in &bar.notes {
                onsets.insert(
                    (
                        part,
                        note.lane(),
                        measure as u64 * bar_ticks + u64::from(note.start),
                    ),
                    note.pitches
                        .iter()
                        .map(|pitch| pitch.midi())
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
    let index = crate::entry_style::Index::new(score);
    let mut changes = Vec::new();
    let mut removed = 0;
    for (part, staff) in score.parts.iter().enumerate() {
        if !definition.source_staff_is_voiced(definition.staves[part]) {
            continue;
        }
        for (measure, bar) in staff.measures.iter().enumerate() {
            for (entry, note) in bar.notes.iter().enumerate() {
                let anchor = Anchor {
                    part,
                    measure,
                    tick: note.start,
                    layer: note.layer,
                    voice: note.voice,
                };
                let tied: Vec<_> = note
                    .pitches
                    .iter()
                    .map(|p| p.midi())
                    .filter(|&midi| index.ties_forward(&anchor, note, midi))
                    .collect();
                if tied.is_empty() {
                    continue;
                }
                let end =
                    measure as u64 * bar_ticks + u64::from(note.start) + u64::from(note.ticks());
                let next = onsets.get(&(part, note.lane(), end));
                let retained: Vec<_> = tied
                    .iter()
                    .copied()
                    .filter(|midi| next.is_some_and(|pitches| pitches.contains(midi)))
                    .collect();
                if retained.len() != tied.len() {
                    removed += tied.len() - retained.len();
                    changes.push((anchor, entry, retained));
                }
            }
        }
    }
    let mut styles: BTreeMap<_, _> = score
        .notation
        .entries
        .iter()
        .enumerate()
        .map(|(index, style)| (style.anchor, index))
        .collect();
    for (anchor, entry, retained) in changes {
        score.parts[anchor.part].measures[anchor.measure].notes[entry].tied = !retained.is_empty();
        if let Some(&index) = styles.get(&anchor) {
            score.notation.entries[index].tie_pitches = Some(retained);
        } else if !retained.is_empty() {
            let mut style = crate::entry_style::EntryStyle::new(anchor);
            style.tie_pitches = Some(retained);
            styles.insert(anchor, score.notation.entries.len());
            score.notation.entries.push(style);
        }
    }
    removed
}
fn transpose_projection(score: &mut Score, interval: Transposition) -> Result<(), String> {
    for staff in &mut score.parts {
        for measure in &mut staff.measures {
            for note in &mut measure.notes {
                for pitch in &mut note.pitches {
                    *pitch = interval.written_pitch(*pitch)?;
                }
            }
        }
    }
    for entry in &mut score.notation.entries {
        for accidental in &mut entry.accidentals {
            accidental.pitch = (i16::from(accidental.pitch) + i16::from(interval.semitones)) as u8;
        }
        if let Some(ties) = &mut entry.tie_pitches {
            for pitch in ties {
                *pitch = (i16::from(*pitch) + i16::from(interval.semitones)) as u8;
            }
        }
    }
    for symbol in &mut score.notation.chords {
        crate::chords::transpose_symbol(symbol, interval)?;
    }
    for group in &mut score.notation.grace {
        for note in &mut group.notes {
            for pitch in &mut note.pitches {
                *pitch = interval.written_pitch(*pitch)?;
            }
        }
    }
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
            voice: first.voice,
            part: 2,
            measure: 0,
            tick: first.start,
            layer: first.layer,
        };
        let b = Anchor {
            voice: second.voice,
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
            voice: 0,
            part: 2,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor {
            voice: 0,
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
            voicing: Vec::new(),
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
            voice: 0,
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
            voicing: Vec::new(),
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

#[cfg(test)]
mod secondary_voice_tests {
    use super::*;
    use crate::{
        document::Anchor,
        entry_style::{self, AccidentalDisplay, AccidentalOverride, EntryStyle},
        lyrics::{self, Lyric},
        model::Duration,
    };
    #[test]
    fn projection_keeps_secondary_identity_launch_and_source_annotations() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        let mut primary = Score::demo().parts[0].measures[0].notes[0].clone();
        primary.start = 0;
        primary.duration = Duration::Whole;
        primary.dotted = false;
        primary.tied = false;
        primary.pitches = vec![Pitch::from_midi(60)];
        let mut secondary = primary.clone();
        secondary.voice = 1;
        secondary.voice_launch = Some(0);
        secondary.pitches = vec![Pitch::from_midi(64)];
        score.parts[0].measures[0].notes = vec![primary, secondary];
        let at = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 1,
        };
        lyrics::assign(&mut score, Lyric::new(at, "voice two")).unwrap();
        let mut style = EntryStyle::new(at);
        style.accidentals.push(AccidentalOverride {
            pitch: 64,
            display: AccidentalDisplay::Courtesy,
        });
        entry_style::assign(&mut score, style).unwrap();
        let before = score.clone();
        let part = LinkedPart {
            id: "clarinet".into(),
            name: "Clarinet".into(),
            staves: vec![0],
            voicing: Vec::new(),
            transposition: Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
            layout: Default::default(),
        };
        let projected = project(&score, &part).unwrap().score;
        assert_eq!(projected.parts[0].measures[0].notes[1].voice, 1);
        assert_eq!(
            projected.parts[0].measures[0].notes[1].voice_launch,
            Some(0)
        );
        assert_eq!(projected.notation.lyrics[0].anchor, at);
        assert_eq!(projected.notation.entries[0].anchor.voice, 1);
        assert_eq!(projected.notation.entries[0].accidentals[0].pitch, 66);
        assert_eq!(score, before);
    }
}

#[cfg(test)]
mod voicing_tests {
    use super::*;
    use crate::{
        entry_style::{AccidentalDisplay, AccidentalOverride, BeamStyle, EntryStyle},
        expressions::{Expression, Hairpin},
        grace::{GraceGroup, GraceNote},
        lyrics::Lyric,
        model::{Duration, Note},
        ornaments::{Ornament, OrnamentKind},
        slurs::Slur,
    };
    fn note(start: u32, lane: u8, pitches: &[u8]) -> Note {
        Note {
            layer: lane % 4,
            voice: lane / 4,
            voice_launch: None,
            start,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: pitches.iter().copied().map(Pitch::from_midi).collect(),
            velocity: 80,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        }
    }
    fn part(staff: usize, chord: ChordFilter) -> LinkedPart {
        let mut rule = StaffVoicing::new(staff);
        rule.lanes = 1;
        rule.chord = chord;
        LinkedPart {
            id: "voiced".into(),
            name: "Solo".into(),
            staves: vec![staff],
            voicing: vec![rule],
            transposition: Transposition::CONCERT,
            layout: Default::default(),
        }
    }
    fn at(part: usize, measure: usize, tick: u32, lane: u8) -> Anchor {
        Anchor {
            part,
            measure,
            tick,
            layer: lane % 4,
            voice: lane / 4,
        }
    }
    #[test]
    fn voiced_entry_and_pitch_maps_resolve_actual_source_indices() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![
            note(0, 1, &[48]),
            note(0, 0, &[67, 60, 64]),
            note(0, 4, &[72]),
        ];
        score.parts[1].measures[0].notes = vec![note(0, 0, &[36])];
        let mut definition = part(0, ChordFilter::Highest);
        definition.staves = vec![1, 0];
        let before = score.clone();
        let projection = project(&score, &definition).unwrap();
        let display = Selection {
            part: 1,
            measure: 0,
            note: 0,
        };
        let source = Selection {
            part: 0,
            measure: 0,
            note: 1,
        };
        assert_eq!(projection.source_selection(display), Some(source));
        assert_eq!(projection.project_selection(source), Some(display));
        assert_eq!(projection.source_pitch(display, 0), Some((source, 0)));
        assert_eq!(projection.project_pitch(source, 0), Some((display, 0)));
        assert_eq!(projection.project_pitch(source, 1), None);
        assert_eq!(
            projection.project_selection(Selection { note: 0, ..source }),
            None
        );
        assert_eq!(
            projection.source_selection(Selection { note: 1, ..display }),
            None
        );
        assert_eq!(
            projection.score.parts[1].measures[0].notes[0].pitches[0].midi(),
            67
        );
        assert!(projection.is_voiced());
        assert!(!projection.source_staff_is_voiced(1));
        assert!(projection.source_staff_is_voiced(0));
        assert_eq!(score, before);
        for (filter, source_pitch, midi) in [
            (ChordFilter::Lowest, 1, 60),
            (
                ChordFilter::Ranked {
                    from_top: true,
                    ranks: vec![2],
                },
                2,
                64,
            ),
            (
                ChordFilter::Ranked {
                    from_top: false,
                    ranks: vec![2, 3],
                },
                0,
                67,
            ),
        ] {
            definition.voicing[0].chord = filter;
            let projection = project(&score, &definition).unwrap();
            assert_eq!(
                projection.source_pitch(display, 0),
                Some((source, source_pitch))
            );
            assert_eq!(
                projection.score.parts[1].measures[0].notes[0].pitches[0].midi(),
                midi
            );
        }
    }
    #[test]
    fn conditional_layer_fallback_and_single_note_passages_match_policy() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![
            note(0, 0, &[60, 64, 67]),
            note(480, 0, &[62]),
            note(960, 0, &[]),
        ];
        score.parts[0].measures[1].notes = vec![note(0, 0, &[60, 64]), note(0, 1, &[48, 55])];
        let mut definition = part(0, ChordFilter::Highest);
        definition.voicing[0].lanes = u8::MAX;
        definition.voicing[0].multiple_layers = Some(1);
        definition.voicing[0].include_single_notes = false;
        let projected = project(&score, &definition).unwrap().score;
        let single = &projected.parts[0].measures[0].notes;
        assert_eq!(single[0].pitches, vec![Pitch::from_midi(67)]);
        assert!(single[1].pitches.is_empty());
        assert!(single[2].pitches.is_empty());
        let multiple = &projected.parts[0].measures[1].notes;
        assert_eq!(multiple.len(), 1);
        assert_eq!(multiple[0].layer, 1);
        assert_eq!(
            multiple[0].pitches,
            vec![Pitch::from_midi(48), Pitch::from_midi(55)]
        );
    }
    #[test]
    fn secondary_only_projection_keeps_rhythm_and_separates_orphaned_branches() {
        let mut score = Score::blank();
        let mut secondary = vec![
            note(0, 4, &[64]),
            note(480, 4, &[65]),
            note(960, 4, &[67]),
            note(1440, 4, &[69]),
        ];
        for entry in &mut secondary {
            entry.voice_launch = Some(if entry.start < 960 { 0 } else { 960 });
        }
        let mut notes = vec![note(0, 0, &[48]), note(960, 0, &[50])];
        notes.extend(secondary);
        score.parts[0].measures[0].notes = notes;
        score.validate().unwrap();
        let mut definition = part(0, ChordFilter::All);
        definition.voicing[0].lanes = 1 << 4;
        let projection = project(&score, &definition).unwrap();
        let notes = &projection.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 4);
        assert!(
            notes
                .iter()
                .all(|n| n.voice == 1 && n.voice_launch.is_none())
        );
        for tick in [0, 960] {
            assert_eq!(
                crate::entry_style::at(&projection.score, &at(0, 0, tick, 4))
                    .unwrap()
                    .beam,
                BeamStyle::Break
            );
        }
        assert!(crate::entry_style::at(&projection.score, &at(0, 0, 480, 4)).is_none());
        assert_eq!(score.parts[0].measures[0].notes[2].voice_launch, Some(0));
    }
    #[test]
    fn metadata_follows_retained_pitched_anchors_and_pitch_subsets() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(0, 0, &[60, 64, 67]), note(480, 0, &[62])];
        let start = at(0, 0, 0, 0);
        let end = at(0, 0, 480, 0);
        let mut lyric = Lyric::new(start, "Sing");
        lyric.extend_to = Some(end);
        score.notation.lyrics.push(lyric);
        score.notation.slurs.push(Slur::new(start, end));
        let mut style = EntryStyle::new(start);
        style.accidentals = [60, 64, 67]
            .into_iter()
            .map(|pitch| AccidentalOverride {
                pitch,
                display: AccidentalDisplay::Courtesy,
            })
            .collect();
        style.tie_pitches = Some(vec![60, 64]);
        score.notation.entries.push(style);
        let mut grace = GraceNote::new(Pitch::from_midi(60), Duration::Sixteenth);
        grace.pitches = vec![
            Pitch::from_midi(60),
            Pitch::from_midi(64),
            Pitch::from_midi(67),
        ];
        score.notation.grace.push(GraceGroup::new(
            start,
            vec![
                grace,
                GraceNote::new(Pitch::from_midi(62), Duration::Sixteenth),
            ],
        ));
        score
            .notation
            .ornaments
            .push(Ornament::new(start, OrnamentKind::ArpeggioUp));
        score
            .notation
            .ornaments
            .push(Ornament::new(end, OrnamentKind::Turn));
        score
            .notation
            .expressions
            .push(Expression::dynamic(start, "mf", 80));
        score
            .notation
            .expressions
            .push(Expression::text(at(0, 0, 0, 1), "hidden layer"));
        score.notation.spanners.push(Hairpin::new(start, end));
        score.validate().unwrap();
        let before = score.clone();
        let mut definition = part(
            0,
            ChordFilter::Ranked {
                from_top: true,
                ranks: vec![2],
            },
        );
        definition.voicing[0].include_single_notes = false;
        let projection = project(&score, &definition).unwrap();
        let metadata = &projection.score.notation;
        assert_eq!(metadata.lyrics.len(), 1);
        assert_eq!(metadata.lyrics[0].extend_to, None);
        assert!(metadata.slurs.is_empty());
        assert_eq!(metadata.entries[0].accidentals.len(), 1);
        assert_eq!(metadata.entries[0].accidentals[0].pitch, 64);
        assert_eq!(metadata.entries[0].tie_pitches, Some(vec![]));
        assert_eq!(metadata.grace[0].notes.len(), 1);
        assert_eq!(
            metadata.grace[0].notes[0].pitches,
            vec![Pitch::from_midi(64)]
        );
        assert!(metadata.ornaments.is_empty());
        assert_eq!(metadata.expressions.len(), 1);
        assert_eq!(metadata.spanners.len(), 1);
        assert!(!projection.warnings.is_empty());
        assert_eq!(score, before);
    }
    #[test]
    fn shortened_melisma_ends_at_last_retained_note() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![
            note(0, 0, &[60, 64]),
            note(480, 0, &[62, 65]),
            note(960, 0, &[67]),
        ];
        let mut lyric = Lyric::new(at(0, 0, 0, 0), "Ah");
        lyric.extend_to = Some(at(0, 0, 960, 0));
        score.notation.lyrics.push(lyric);
        let mut definition = part(0, ChordFilter::Lowest);
        definition.voicing[0].include_single_notes = false;
        let projection = project(&score, &definition).unwrap();
        assert_eq!(
            projection.score.notation.lyrics[0].extend_to,
            Some(at(0, 0, 480, 0))
        );
    }
    #[test]
    fn standalone_concert_and_written_parts_are_independent_and_roundtrip() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(0, 0, &[60, 64]), note(480, 1, &[48])];
        let mut definition = part(0, ChordFilter::Lowest);
        definition.transposition = Transposition {
            semitones: 2,
            diatonic_steps: 1,
        };
        score.notation.parts.push(definition.clone());
        let before = score.clone();
        for written in [false, true] {
            let extracted = extract(&score, &definition, written).unwrap();
            assert_eq!(extracted.parts.len(), 1);
            assert_eq!(extracted.parts[0].measures[0].notes.len(), 1);
            assert_eq!(
                extracted.parts[0].measures[0].notes[0].pitches[0].midi(),
                if written { 62 } else { 60 }
            );
            assert_eq!(extracted.key_fifths, if written { 2 } else { 0 });
            assert!(extracted.notation.parts.is_empty());
            let serialized = serde_json::to_vec(&extracted).unwrap();
            let restored: Score = serde_json::from_slice(&serialized).unwrap();
            assert_eq!(restored, extracted);
            restored.validate().unwrap();
        }
        assert_eq!(score, before);
        score.parts[0].measures[0].notes[0].pitches[0] = Pitch::from_midi(59);
        assert_eq!(
            project(&score, &definition).unwrap().score.parts[0].measures[0].notes[0].pitches[0]
                .midi(),
            61
        );
    }
    #[test]
    fn invalid_voicing_is_rejected_atomically_and_legacy_defaults_are_unfiltered() {
        let score = Score::blank();
        let before = score.clone();
        let legacy = serde_json::json!({"id":"old","name":"Old","staves":[0]});
        let restored: LinkedPart = serde_json::from_value(legacy).unwrap();
        assert!(restored.voicing.is_empty());
        assert!(!project(&score, &restored).unwrap().is_voiced());
        for invalid in [
            StaffVoicing {
                lanes: 0,
                ..StaffVoicing::new(0)
            },
            StaffVoicing {
                staff: 1,
                ..StaffVoicing::new(0)
            },
            StaffVoicing {
                multiple_layers: Some(4),
                ..StaffVoicing::new(0)
            },
            StaffVoicing {
                chord: ChordFilter::Ranked {
                    from_top: true,
                    ranks: vec![0],
                },
                ..StaffVoicing::new(0)
            },
            StaffVoicing {
                chord: ChordFilter::Ranked {
                    from_top: true,
                    ranks: vec![1, 1],
                },
                ..StaffVoicing::new(0)
            },
        ] {
            let mut definition = restored.clone();
            definition.voicing.push(invalid);
            assert!(project(&score, &definition).is_err());
            assert_eq!(score, before);
        }
        let mut duplicate = restored;
        duplicate.voicing = vec![StaffVoicing::new(0), StaffVoicing::new(0)];
        assert!(duplicate.validate(score.parts.len()).is_err());
    }
    #[test]
    fn cross_voice_slurs_keep_geometry_until_an_endpoint_is_hidden() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(0, 0, &[60, 64]), note(480, 4, &[65, 69])];
        let mut slur = Slur::new(at(0, 0, 0, 0), at(0, 0, 480, 4));
        slur.style = crate::slurs::SlurStyle::Dashed;
        slur.start_offset = [4., -3.];
        score.notation.slurs.push(slur.clone());
        let mut definition = part(0, ChordFilter::Highest);
        definition.voicing[0].lanes = 1 | (1 << 4);
        let projection = project(&score, &definition).unwrap();
        assert_eq!(projection.score.notation.slurs, vec![slur]);
        definition.voicing[0].lanes = 1;
        let projection = project(&score, &definition).unwrap();
        assert!(projection.score.notation.slurs.is_empty());
        assert!(
            projection
                .warnings
                .iter()
                .any(|warning| warning.contains("slur"))
        );
    }
    #[test]
    fn staff_harmony_pedal_and_global_form_survive_voice_filtering() {
        use crate::{
            chords::{self, ChordSymbol},
            lines::{LineKind, PedalKind, PerformanceLine},
        };
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(0, 0, &[60]), note(0, 4, &[64])];
        score.notation.chords.push(ChordSymbol::new(
            at(0, 0, 240, 0),
            chords::parse("C/E").unwrap(),
        ));
        score.notation.lines.push(PerformanceLine::new(
            at(0, 0, 0, 0),
            at(0, 0, 960, 0),
            LineKind::Pedal {
                kind: PedalKind::Damper,
            },
        ));
        score.notation.lines.push(PerformanceLine::new(
            at(0, 0, 0, 0),
            at(0, 0, 960, 0),
            LineKind::Ottava { semitones: 12 },
        ));
        score.notation.lines.push(PerformanceLine::new(
            at(0, 0, 0, 0),
            at(0, 0, 960, 0),
            LineKind::Trill {
                interval: 2,
                rate: 60,
            },
        ));
        score
            .notation
            .tempo
            .push(crate::expressions::TempoChange::new(
                crate::expressions::TimePoint {
                    measure: 0,
                    tick: 0,
                },
                120,
            ));
        score
            .notation
            .repeats
            .push(crate::repeats::RepeatRegion::simple(0, 1));
        let mut definition = part(0, ChordFilter::All);
        definition.voicing[0].lanes = 1 << 4;
        let projection = project(&score, &definition).unwrap();
        assert_eq!(projection.score.notation.chords, score.notation.chords);
        assert_eq!(projection.score.notation.lines.len(), 2);
        assert!(
            projection
                .score
                .notation
                .lines
                .iter()
                .any(|line| matches!(line.kind, LineKind::Pedal { .. }))
        );
        assert!(
            projection
                .score
                .notation
                .lines
                .iter()
                .any(|line| matches!(line.kind, LineKind::Ottava { .. }))
        );
        assert_eq!(projection.score.notation.tempo, score.notation.tempo);
        assert_eq!(projection.score.notation.repeats, score.notation.repeats);
    }
    #[test]
    fn extracted_musicxml_preserves_selected_voice_and_concert_pitch() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![
            note(0, 0, &[48]),
            note(0, 4, &[60, 64]),
            note(480, 4, &[62, 65]),
        ];
        let mut definition = part(0, ChordFilter::Highest);
        definition.voicing[0].lanes = 1 << 4;
        definition.transposition = Transposition {
            semitones: 2,
            diatonic_steps: 1,
        };
        let extracted = extract(&score, &definition, false).unwrap();
        let path = std::env::temp_dir().join(format!(
            "encore-voiced-musicxml-{}.musicxml",
            std::process::id()
        ));
        std::fs::write(&path, crate::export::musicxml(&extracted)).unwrap();
        let result = crate::import::load(&path);
        let _ = std::fs::remove_file(path);
        let imported = result.unwrap().score;
        let source: Vec<_> = extracted.parts[0].measures[0]
            .notes
            .iter()
            .map(|note| (note.start, note.lane(), note.pitches.clone()))
            .collect();
        let retained: Vec<_> = imported.parts[0].measures[0]
            .notes
            .iter()
            .filter(|note| note.voice == 1)
            .map(|note| (note.start, note.lane(), note.pitches.clone()))
            .collect();
        assert_eq!(retained, source);
        assert!(retained.iter().all(|(_, lane, _)| *lane == 4));
        assert_eq!(retained[0].2[0].midi(), 64);
    }
    #[test]
    fn independent_voicing_rules_and_layouts_roundtrip_and_follow_source_edits() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(0, 0, &[60, 64, 67])];
        let mut high = part(0, ChordFilter::Highest);
        high.id = "flute-1".into();
        high.name = "Flute 1".into();
        let mut low = part(0, ChordFilter::Lowest);
        low.id = "flute-2".into();
        low.name = "Flute 2".into();
        low.layout.page_size = PageSize::A4;
        score.notation.parts = vec![high, low];
        let serialized = serde_json::to_vec(&score).unwrap();
        let mut restored: Score = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(restored, score);
        restored.parts[0].measures[0].notes[0].pitches =
            vec![Pitch::from_midi(59), Pitch::from_midi(69)];
        let before = restored.clone();
        let high = project(&restored, &restored.notation.parts[0]).unwrap();
        let low = project(&restored, &restored.notation.parts[1]).unwrap();
        assert_eq!(
            high.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            69
        );
        assert_eq!(
            low.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            59
        );
        assert_eq!(high.layout.page_size, PageSize::Letter);
        assert_eq!(low.layout.page_size, PageSize::A4);
        assert_eq!(restored, before);
    }
    #[test]
    fn absent_chord_rank_becomes_rest_but_monophonic_passages_can_be_shared() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(0, 0, &[60, 64]), note(480, 0, &[67])];
        let definition = part(
            0,
            ChordFilter::Ranked {
                from_top: true,
                ranks: vec![3],
            },
        );
        let projection = project(&score, &definition).unwrap();
        assert!(
            projection.score.parts[0].measures[0].notes[0]
                .pitches
                .is_empty()
        );
        assert_eq!(projection.score.parts[0].measures[0].notes[0].ticks(), 480);
        assert_eq!(
            projection.score.parts[0].measures[0].notes[1].pitches[0].midi(),
            67
        );
        score.parts[0].measures[0].notes[0].layer = 9;
        assert!(project(&score, &definition).is_err());
    }
}
