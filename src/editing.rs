//! Atomic, whole-measure regional editing over source score staves.
//!
//! Regions retain explicit source staff indices, so a rectangle in a linked
//! part can include nonadjacent source staves without editing hidden players.
use crate::document::{Anchor, Notation};
use crate::model::{MAX_MEASURES, MAX_PARTS, Measure, Score};
use crate::parts::Transposition;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_REGION_CLIP_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    /// Ordered source staff indices. They need not be adjacent in linked parts.
    pub staves: Vec<usize>,
    /// Inclusive, zero-based measure bounds.
    pub start: usize,
    pub end: usize,
}

impl Region {
    pub fn rectangle(
        first_staff: usize,
        last_staff: usize,
        first_bar: usize,
        last_bar: usize,
    ) -> Self {
        Self {
            staves: (first_staff.min(last_staff)..=first_staff.max(last_staff))
                .take(MAX_PARTS + 1)
                .collect(),
            start: first_bar.min(last_bar),
            end: first_bar.max(last_bar),
        }
    }
    pub fn all(score: &Score) -> Self {
        Self {
            staves: (0..score.parts.len()).collect(),
            start: 0,
            end: score.measure_count().saturating_sub(1),
        }
    }
    pub fn validate(&self, score: &Score) -> Result<(), String> {
        validate_staves(&self.staves, score.parts.len())?;
        if self.start > self.end || self.end >= score.measure_count() {
            return Err("Select an existing, nonempty measure range.".into());
        }
        Ok(())
    }
    pub fn measure_count(&self) -> usize {
        self.end.saturating_sub(self.start).saturating_add(1)
    }
    pub fn contains(&self, anchor: &Anchor) -> bool {
        self.staves.contains(&anchor.part) && self.contains_measure(anchor.measure)
    }
    pub fn contains_measure(&self, measure: usize) -> bool {
        self.start <= measure && measure <= self.end
    }
    pub fn is_full_stack(&self, score: &Score) -> bool {
        self.staves.len() == score.parts.len()
            && (0..score.parts.len()).all(|staff| self.staves.contains(&staff))
    }
}

fn validate_staves(staves: &[usize], staff_count: usize) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    if staves.is_empty()
        || staves.len() > MAX_PARTS
        || staves
            .iter()
            .any(|staff| *staff >= staff_count || !seen.insert(*staff))
    {
        return Err("A region requires distinct existing source staves.".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionClip {
    pub version: u8,
    pub time_numerator: u8,
    pub time_denominator: u8,
    pub key_fifths: i8,
    /// Staff rows containing equal numbers of complete measures.
    pub measures: Vec<Vec<Measure>>,
    #[serde(default)]
    pub notation: Notation,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl RegionClip {
    pub fn measure_count(&self) -> usize {
        self.measures.first().map_or(0, Vec::len)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("This region clipboard requires a different Encore version.".into());
        }
        if self.measures.is_empty()
            || self.measures.len() > MAX_PARTS
            || self.measure_count() == 0
            || self.measure_count() > MAX_MEASURES
            || self
                .measures
                .iter()
                .any(|row| row.len() != self.measure_count())
        {
            return Err(
                "A region clipboard must contain 1–64 equal staff rows and 1–1024 measures.".into(),
            );
        }
        if self.warnings.len() > 64 || self.warnings.iter().any(|warning| warning.len() > 4096) {
            return Err("Region clipboard warnings exceed their limit.".into());
        }
        checked_notation(&self.notation)?;
        if !self.notation.parts.is_empty() {
            return Err("A content clipboard cannot replace linked part definitions.".into());
        }
        let mut score = Score::blank();
        score.parts.truncate(1);
        let prototype = score.parts[0].clone();
        score.parts = self
            .measures
            .iter()
            .map(|measures| {
                let mut part = prototype.clone();
                part.measures = measures.clone();
                part
            })
            .collect();
        score.time_numerator = self.time_numerator;
        score.time_denominator = self.time_denominator;
        score.key_fifths = self.key_fifths;
        score.notation = self.notation.clone();
        score.validate()
    }
    pub fn encode(&self) -> Result<String, String> {
        self.validate()?;
        let text = serde_json::to_string(self).map_err(|error| error.to_string())?;
        if text.len() > MAX_REGION_CLIP_BYTES {
            return Err("Region clipboard exceeds 16 MiB.".into());
        }
        Ok(text)
    }
    pub fn decode(text: &str) -> Result<Self, String> {
        if text.len() > MAX_REGION_CLIP_BYTES {
            return Err("Region clipboard exceeds 16 MiB.".into());
        }
        let value: Value = serde_json::from_str(text)
            .map_err(|error| format!("Invalid region clipboard: {error}"))?;
        if let Some(notation) = value.get("notation").and_then(Value::as_object) {
            ensure_known_fields(notation)?;
        }
        let clip: Self = serde_json::from_value(value)
            .map_err(|error| format!("Invalid region clipboard: {error}"))?;
        clip.validate()?;
        Ok(clip)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteMode {
    /// Replace complete destination measures, including attached annotations.
    Replace,
    /// Require empty destination measures; existing notation is never replaced.
    EmptyOnly,
    /// Insert a measure stack in all staves before pasting the selected rows.
    Insert,
}

/// Content fields are enumerated so adding a notation feature cannot silently
/// lose its data during a regional edit. Unknown populated fields fail visibly.
fn checked_notation(notation: &Notation) -> Result<Map<String, Value>, String> {
    let value = serde_json::to_value(notation).map_err(|error| error.to_string())?;
    let object = value.as_object().ok_or("Invalid notation metadata.")?;
    ensure_known_fields(object)?;
    Ok(object.clone())
}
fn ensure_known_fields(object: &Map<String, Value>) -> Result<(), String> {
    for key in object.keys() {
        if ![
            "lyrics",
            "slurs",
            "entries",
            "expressions",
            "spanners",
            "chords",
            "grace",
            "lines",
            "tempo",
            "repeats",
            "navigation",
            "parts",
            "layout",
        ]
        .contains(&key.as_str())
        {
            return Err(format!(
                "Region editing does not yet support {key} metadata; the score was not changed."
            ));
        }
    }
    Ok(())
}

fn read_anchor(value: &Value) -> Option<Anchor> {
    let object = value.as_object()?;
    if ["part", "measure", "tick", "layer"]
        .iter()
        .all(|key| object.contains_key(*key))
    {
        serde_json::from_value(value.clone()).ok()
    } else {
        None
    }
}
fn anchor_in(value: &Value, region: &Region) -> bool {
    read_anchor(value).is_some_and(|anchor| region.contains(&anchor))
}
fn anchors(value: &Value, found: &mut Vec<Anchor>) {
    if let Some(anchor) = read_anchor(value) {
        found.push(anchor);
        return;
    }
    match value {
        Value::Object(object) => {
            for child in object.values() {
                anchors(child, found);
            }
        }
        Value::Array(array) => {
            for child in array {
                anchors(child, found);
            }
        }
        _ => {}
    }
}
fn overlaps_region(attached: &[Anchor], region: &Region) -> bool {
    region.staves.iter().any(|staff| {
        let mut positions = attached
            .iter()
            .filter(|anchor| anchor.part == *staff)
            .map(|anchor| anchor.measure);
        let Some(first) = positions.next() else {
            return false;
        };
        let (start, end) = positions.fold((first, first), |(start, end), measure| {
            (start.min(measure), end.max(measure))
        });
        start <= region.end && region.start <= end
    })
}

fn map_anchors(value: &mut Value, map: &impl Fn(Anchor) -> Option<Anchor>) -> Result<(), String> {
    if let Some(anchor) = read_anchor(value) {
        *value = serde_json::to_value(
            map(anchor).ok_or("Annotation anchor falls outside the edited range.")?,
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                map_anchors(child, map)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                map_anchors(child, map)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn records<'a>(object: &'a Map<String, Value>, key: &str) -> &'a [Value] {
    object
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
fn set_records(object: &mut Map<String, Value>, key: &str, records: Vec<Value>) {
    if records.is_empty() {
        object.remove(key);
    } else {
        object.insert(key.into(), Value::Array(records));
    }
}
fn from_notation(object: Map<String, Value>) -> Result<Notation, String> {
    serde_json::from_value(Value::Object(object))
        .map_err(|error| format!("Invalid edited notation: {error}"))
}
fn index(value: &Value, key: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|index| index.try_into().ok())
        .ok_or_else(|| format!("Invalid metadata {key} index."))
}
fn set_index(value: &mut Value, key: &str, index: usize) {
    value[key] = Value::from(index);
}

pub fn copy_region(score: &Score, region: &Region) -> Result<RegionClip, String> {
    score.validate()?;
    region.validate(score)?;
    let source = checked_notation(&score.notation)?;
    let mut target = Map::new();
    let mut warnings = Vec::new();
    let map = |anchor: Anchor| {
        Some(Anchor {
            part: region
                .staves
                .iter()
                .position(|staff| *staff == anchor.part)?,
            measure: anchor.measure.checked_sub(region.start)?,
            ..anchor
        })
    };
    let mut omitted = 0;
    let mut omitted_repeats = 0;
    for key in [
        "lyrics",
        "slurs",
        "entries",
        "expressions",
        "spanners",
        "chords",
        "grace",
        "lines",
    ] {
        let mut copied = Vec::new();
        for original in records(&source, key) {
            let mut item = original.clone();
            if key == "lyrics" {
                if !item
                    .get("anchor")
                    .is_some_and(|anchor| anchor_in(anchor, region))
                {
                    let mut span = Vec::new();
                    anchors(&item, &mut span);
                    if overlaps_region(&span, region) {
                        omitted += 1;
                    }
                    continue;
                }
                if item
                    .get("extend_to")
                    .is_some_and(|anchor| !anchor.is_null() && !anchor_in(anchor, region))
                {
                    item.as_object_mut().unwrap().remove("extend_to");
                    omitted += 1;
                }
            }
            let mut attached = Vec::new();
            anchors(&item, &mut attached);
            if !attached.is_empty() && attached.iter().all(|anchor| region.contains(anchor)) {
                map_anchors(&mut item, &map)?;
                copied.push(item);
            } else if overlaps_region(&attached, region) {
                omitted += 1;
            }
        }
        set_records(&mut target, key, copied);
    }
    if omitted > 0 {
        warnings.push(format!(
            "{omitted} cross-boundary phrase/word spans were omitted or shortened in this copy."
        ));
    }
    if region.is_full_stack(score) {
        let mut repeats = Vec::new();
        for original in records(&source, "repeats") {
            let start = index(original, "start")?;
            let end = index(original, "end")?;
            if region.contains_measure(start) && region.contains_measure(end) {
                let mut item = original.clone();
                shift_range(&mut item, region.start, false)?;
                repeats.push(item);
            } else if start <= region.end && region.start <= end {
                omitted_repeats += 1;
            }
        }
        set_records(&mut target, "repeats", repeats);
        let mut navigation = records(&source, "navigation")
            .iter()
            .filter(|item| {
                index(item, "measure").is_ok_and(|measure| region.contains_measure(measure))
            })
            .cloned()
            .collect::<Vec<_>>();
        for item in &mut navigation {
            set_index(item, "measure", index(item, "measure")? - region.start);
        }
        let before = navigation.len();
        retain_navigation_dependencies(&mut navigation, region.start == 0);
        if navigation.len() != before {
            warnings.push(
                "Navigation jumps whose targets are outside the copied bars were omitted.".into(),
            );
        }
        set_records(&mut target, "navigation", navigation);
        let mut tempo = Vec::new();
        for original in records(&source, "tempo") {
            if !original
                .get("at")
                .and_then(|at| index(at, "measure").ok())
                .is_some_and(|measure| region.contains_measure(measure))
            {
                continue;
            }
            let mut item = original.clone();
            if item
                .get("ramp")
                .filter(|ramp| !ramp.is_null())
                .and_then(|ramp| ramp.get("end"))
                .and_then(|at| index(at, "measure").ok())
                .is_some_and(|measure| !region.contains_measure(measure))
            {
                item.as_object_mut().unwrap().remove("ramp");
                warnings.push(
                    "A tempo ramp crossing the copied boundary was shortened to its initial tempo."
                        .into(),
                );
            }
            map_timepoints(&mut item, &|measure| measure.checked_sub(region.start))?;
            tempo.push(item);
        }
        set_records(&mut target, "tempo", tempo);
    } else if ["repeats", "navigation", "tempo"]
        .iter()
        .any(|key| !records(&source, key).is_empty())
    {
        warnings.push(
            "Score-wide repeats, navigation and tempo are copied only from full staff stacks."
                .into(),
        );
    }
    if omitted_repeats > 0 {
        warnings.push(format!(
            "{omitted_repeats} repeat regions crossing the copied boundary were omitted."
        ));
    }
    let clip = RegionClip {
        version: 1,
        time_numerator: score.time_numerator,
        time_denominator: score.time_denominator,
        key_fifths: score.key_fifths,
        measures: region
            .staves
            .iter()
            .map(|staff| score.parts[*staff].measures[region.start..=region.end].to_vec())
            .collect(),
        notation: from_notation(target)?,
        warnings,
    };
    clip.validate()?;
    Ok(clip)
}

fn retain_navigation_dependencies(items: &mut Vec<Value>, includes_score_start: bool) {
    let kinds = items
        .iter()
        .filter_map(|item| item.get("kind").and_then(Value::as_str))
        .map(String::from)
        .collect::<BTreeSet<_>>();
    items.retain(|item| match item.get("kind").and_then(Value::as_str) {
        Some("DalSegno") => kinds.contains("Segno"),
        Some("DalSegnoAlFine") => kinds.contains("Segno") && kinds.contains("Fine"),
        Some("DalSegnoAlCoda") => ["Segno", "ToCoda", "Coda"]
            .iter()
            .all(|kind| kinds.contains(*kind)),
        Some("DaCapo") => includes_score_start,
        Some("DaCapoAlFine") => includes_score_start && kinds.contains("Fine"),
        Some("DaCapoAlCoda") => {
            includes_score_start && kinds.contains("ToCoda") && kinds.contains("Coda")
        }
        _ => true,
    });
}
fn shift_range(item: &mut Value, amount: usize, add: bool) -> Result<(), String> {
    for key in ["start", "end"] {
        let value = index(item, key)?;
        set_index(
            item,
            key,
            if add {
                value
                    .checked_add(amount)
                    .ok_or("Measure position overflow.")?
            } else {
                value
                    .checked_sub(amount)
                    .ok_or("Invalid relative measure position.")?
            },
        );
    }
    if let Some(endings) = item.get_mut("endings").and_then(Value::as_array_mut) {
        for ending in endings {
            shift_range(ending, amount, add)?;
        }
    }
    Ok(())
}
fn map_timepoints(value: &mut Value, map: &impl Fn(usize) -> Option<usize>) -> Result<(), String> {
    if value.get("measure").is_some() && value.get("tick").is_some() && value.get("part").is_none()
    {
        let old = index(value, "measure")?;
        set_index(
            value,
            "measure",
            map(old).ok_or(
                "A tempo ramp endpoint falls inside removed measures; adjust the ramp first.",
            )?,
        );
        return Ok(());
    }
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                map_timepoints(child, map)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                map_timepoints(child, map)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn clear_region(score: &mut Score, region: &Region) -> Result<(), String> {
    score.validate()?;
    region.validate(score)?;
    let mut candidate = score.clone();
    clear_in_place(&mut candidate, region)?;
    candidate.validate()?;
    *score = candidate;
    Ok(())
}
fn clear_in_place(score: &mut Score, region: &Region) -> Result<(), String> {
    let mut notation = checked_notation(&score.notation)?;
    for &staff in &region.staves {
        for measure in &mut score.parts[staff].measures[region.start..=region.end] {
            measure.notes.clear();
        }
    }
    for key in [
        "lyrics",
        "slurs",
        "entries",
        "expressions",
        "spanners",
        "chords",
        "grace",
        "lines",
    ] {
        let mut kept = Vec::new();
        for original in records(&notation, key) {
            let mut item = original.clone();
            if key == "lyrics" {
                if item
                    .get("anchor")
                    .is_some_and(|anchor| anchor_in(anchor, region))
                {
                    continue;
                }
                if item
                    .get("extend_to")
                    .is_some_and(|anchor| anchor_in(anchor, region))
                {
                    item.as_object_mut().unwrap().remove("extend_to");
                }
            }
            let mut attached = Vec::new();
            anchors(&item, &mut attached);
            if !attached.iter().any(|anchor| region.contains(anchor)) {
                kept.push(item);
            }
        }
        set_records(&mut notation, key, kept);
    }
    if region.is_full_stack(score) {
        for key in ["navigation", "tempo", "repeats"] {
            let mut kept = Vec::new();
            for item in records(&notation, key) {
                let remove = match key {
                    "navigation" => region.contains_measure(index(item, "measure")?),
                    "tempo" => region.contains_measure(index(
                        item.get("at").ok_or("Missing tempo position.")?,
                        "measure",
                    )?),
                    _ => {
                        region.contains_measure(index(item, "start")?)
                            || region.contains_measure(index(item, "end")?)
                    }
                };
                if !remove {
                    kept.push(item.clone());
                }
            }
            set_records(&mut notation, key, kept);
        }
    }
    score.notation = from_notation(notation)?;
    Ok(())
}

pub fn paste_region(
    score: &mut Score,
    clip: &RegionClip,
    target_staves: &[usize],
    start: usize,
    mode: PasteMode,
) -> Result<Region, String> {
    score.validate()?;
    clip.validate()?;
    validate_staves(target_staves, score.parts.len())?;
    if target_staves.len() != clip.measures.len() {
        return Err(
            "The destination must have the same number of staves as the copied region.".into(),
        );
    }
    if (score.time_numerator, score.time_denominator)
        != (clip.time_numerator, clip.time_denominator)
    {
        return Err("Region pasting requires matching time signatures.".into());
    }
    if start > score.measure_count() {
        return Err("Paste must begin in the score or immediately after its last measure.".into());
    }
    let end = start
        .checked_add(clip.measure_count())
        .and_then(|end| end.checked_sub(1))
        .ok_or("Region paste position overflows.")?;
    if end >= MAX_MEASURES {
        return Err("Pasting would exceed the 1024-measure limit.".into());
    }
    let mut candidate = score.clone();
    if mode == PasteMode::Insert {
        insert_in_place(&mut candidate, start, clip.measure_count())?;
    }
    let count = candidate.measure_count();
    if end >= count {
        for part in &mut candidate.parts {
            part.measures.resize_with(end + 1, Measure::default);
        }
    }
    let destination = Region {
        staves: target_staves.to_vec(),
        start,
        end,
    };
    if mode == PasteMode::EmptyOnly
        && target_staves.iter().any(|staff| {
            candidate.parts[*staff].measures[start..=end]
                .iter()
                .any(|measure| !measure.notes.is_empty())
        })
    {
        return Err("The destination contains music; choose Replace explicitly or paste into empty measures.".into());
    }
    let untouched = (mode == PasteMode::EmptyOnly).then(|| candidate.clone());
    clear_in_place(&mut candidate, &destination)?;
    if untouched
        .as_ref()
        .is_some_and(|before| *before != candidate)
    {
        return Err("The destination contains attached notation; choose Replace explicitly or paste into empty measures.".into());
    }
    for (row, &staff) in target_staves.iter().enumerate() {
        candidate.parts[staff].measures[start..=end].clone_from_slice(&clip.measures[row]);
    }
    let mut notation = checked_notation(&candidate.notation)?;
    let source = checked_notation(&clip.notation)?;
    let map = |anchor: Anchor| {
        Some(Anchor {
            part: *target_staves.get(anchor.part)?,
            measure: start.checked_add(anchor.measure)?,
            ..anchor
        })
    };
    for key in [
        "lyrics",
        "slurs",
        "entries",
        "expressions",
        "spanners",
        "chords",
        "grace",
        "lines",
    ] {
        let mut merged = records(&notation, key).to_vec();
        for item in records(&source, key) {
            let mut item = item.clone();
            map_anchors(&mut item, &map)?;
            merged.push(item);
        }
        set_records(&mut notation, key, merged);
    }
    if start != 0
        && records(&source, "navigation").iter().any(|item| {
            matches!(
                item.get("kind").and_then(Value::as_str),
                Some("DaCapo" | "DaCapoAlFine" | "DaCapoAlCoda")
            )
        })
    {
        return Err("D.C. targets the start of the document. Paste this region at measure 1, or use an explicit D.S./segno target before copying.".into());
    }
    let global = ["repeats", "navigation", "tempo"]
        .iter()
        .any(|key| !records(&source, key).is_empty());
    if global && target_staves.len() != candidate.parts.len() {
        return Err(
            "This clipboard includes score-wide metadata; paste into a full staff stack.".into(),
        );
    }
    for key in ["repeats", "navigation", "tempo"] {
        let mut merged = records(&notation, key).to_vec();
        for original in records(&source, key) {
            let mut item = original.clone();
            match key {
                "repeats" => shift_range(&mut item, start, true)?,
                "navigation" => {
                    let old = index(&item, "measure")?;
                    set_index(
                        &mut item,
                        "measure",
                        old.checked_add(start)
                            .ok_or("Navigation position overflow.")?,
                    );
                }
                _ => map_timepoints(&mut item, &|measure| start.checked_add(measure))?,
            }
            merged.push(item);
        }
        set_records(&mut notation, key, merged);
    }
    candidate.notation = from_notation(notation)?;
    candidate.validate()?;
    *score = candidate;
    Ok(destination)
}

pub fn transpose_region(
    score: &mut Score,
    region: &Region,
    interval: Transposition,
) -> Result<(), String> {
    interval.validate()?;
    transpose_with(score, region, |pitch| interval.written_pitch(pitch))
}

/// Chromatic editing chooses the canonical spelling of the resulting MIDI pitch;
/// use `transpose_region` when the diatonic spelling must be preserved.
pub fn transpose_chromatic_region(
    score: &mut Score,
    region: &Region,
    semitones: i8,
) -> Result<(), String> {
    transpose_with(score, region, |pitch| {
        let midi = i16::from(pitch.midi()) + i16::from(semitones);
        if !(0..=127).contains(&midi) {
            return Err("Transposition would move a region pitch outside MIDI 0–127; no notes were changed.".into());
        }
        Ok(crate::model::Pitch::from_midi(midi as u8))
    })
}

/// Move through the current key's diatonic scale while carrying each note's
/// chromatic alteration relative to that key. The key signature itself stays.
pub fn transpose_diatonic_region(
    score: &mut Score,
    region: &Region,
    steps: i8,
) -> Result<(), String> {
    if !(-21..=21).contains(&steps) {
        return Err("Diatonic transposition is limited to three octaves.".into());
    }
    let fifths = score.key_fifths;
    transpose_with(score, region, |pitch| {
        let staff_step = pitch.staff_step() + i32::from(steps);
        let step = crate::model::Step::from_index(staff_step);
        let octave = staff_step.div_euclid(7);
        let alter = pitch.alter - crate::model::key_alter(pitch.step, fifths)
            + crate::model::key_alter(step, fifths);
        let midi = (octave + 1) * 12 + step.semitone() + i32::from(alter);
        if !(-2..=2).contains(&alter) || !(0..=127).contains(&midi) {
            return Err(
                "Diatonic transposition exceeds the pitch/accidental range; no notes were changed."
                    .into(),
            );
        }
        Ok(crate::model::Pitch {
            step,
            octave: octave as i8,
            alter,
        })
    })
}

fn transpose_with(
    score: &mut Score,
    region: &Region,
    transform: impl Fn(crate::model::Pitch) -> Result<crate::model::Pitch, String>,
) -> Result<(), String> {
    score.validate()?;
    region.validate(score)?;
    checked_notation(&score.notation)?;
    let mut candidate = score.clone();
    let mut pitches = BTreeMap::new();
    for &staff in &region.staves {
        for measure in region.start..=region.end {
            for note in &mut candidate.parts[staff].measures[measure].notes {
                let anchor = Anchor {
                    part: staff,
                    measure,
                    tick: note.start,
                    layer: note.layer,
                };
                let mut changes = Vec::new();
                for pitch in &mut note.pitches {
                    let old = pitch.midi();
                    *pitch = transform(*pitch)?;
                    changes.push((old, pitch.midi()));
                }
                pitches.insert(anchor, changes);
            }
        }
    }
    for symbol in &mut candidate.notation.chords {
        if region.contains(&symbol.anchor) {
            crate::chords::map_symbol_pitches(symbol, &transform)?;
        }
    }
    for group in &mut candidate.notation.grace {
        if region.contains(&group.anchor) {
            for note in &mut group.notes {
                for pitch in &mut note.pitches {
                    *pitch = transform(*pitch)?;
                }
            }
        }
    }
    // Preserve per-tone accidental and tie choices by chord position, just as a
    // single-entry pitch edit does, without a quadratic scan over entry styles.
    for style in &mut candidate.notation.entries {
        if let Some(changes) = pitches.get(&style.anchor) {
            let remap = |pitch: u8| {
                changes
                    .iter()
                    .find(|(old, _)| *old == pitch)
                    .map(|(_, new)| *new)
            };
            style.accidentals = style
                .accidentals
                .iter()
                .filter_map(|mark| {
                    remap(mark.pitch)
                        .map(|pitch| crate::entry_style::AccidentalOverride { pitch, ..*mark })
                })
                .collect();
            if let Some(ties) = &mut style.tie_pitches {
                *ties = ties.iter().filter_map(|pitch| remap(*pitch)).collect();
                ties.sort_unstable();
                ties.dedup();
            }
        }
    }
    candidate.validate()?;
    *score = candidate;
    Ok(())
}

pub fn insert_measures(score: &mut Score, at: usize, count: usize) -> Result<(), String> {
    score.validate()?;
    let mut candidate = score.clone();
    insert_in_place(&mut candidate, at, count)?;
    candidate.validate()?;
    *score = candidate;
    Ok(())
}
fn insert_in_place(score: &mut Score, at: usize, count: usize) -> Result<(), String> {
    let old_count = score.measure_count();
    if count == 0
        || at > old_count
        || old_count
            .checked_add(count)
            .is_none_or(|count| count > MAX_MEASURES)
    {
        return Err("Insert 1 or more measures within the 1024-measure score limit.".into());
    }
    let mapping = (0..old_count)
        .map(|measure| {
            Some(if measure < at {
                measure
            } else {
                measure + count
            })
        })
        .collect::<Vec<_>>();
    for part in &mut score.parts {
        part.measures
            .splice(at..at, std::iter::repeat_with(Measure::default).take(count));
    }
    remap_measures(score, &mapping)
}

pub fn delete_measures(score: &mut Score, start: usize, end: usize) -> Result<(), String> {
    score.validate()?;
    if start > end || end >= score.measure_count() || end - start + 1 >= score.measure_count() {
        return Err(
            "Delete an existing measure range while retaining at least one measure.".into(),
        );
    }
    let count = end - start + 1;
    let mapping = (0..score.measure_count())
        .map(|measure| {
            if measure < start {
                Some(measure)
            } else if measure > end {
                Some(measure - count)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut candidate = score.clone();
    for part in &mut candidate.parts {
        part.measures.drain(start..=end);
    }
    remap_measures(&mut candidate, &mapping)?;
    candidate.validate()?;
    *score = candidate;
    Ok(())
}

fn remap_measures(score: &mut Score, mapping: &[Option<usize>]) -> Result<(), String> {
    let mut notation = checked_notation(&score.notation)?;
    let map_measure = |measure: usize| mapping.get(measure).copied().flatten();
    for key in [
        "lyrics",
        "slurs",
        "entries",
        "expressions",
        "spanners",
        "chords",
        "grace",
        "lines",
    ] {
        let mut kept = Vec::new();
        for original in records(&notation, key) {
            let mut item = original.clone();
            if key == "lyrics"
                && item
                    .get("extend_to")
                    .and_then(read_anchor)
                    .is_some_and(|anchor| map_measure(anchor.measure).is_none())
            {
                item.as_object_mut().unwrap().remove("extend_to");
            }
            let mut attached = Vec::new();
            anchors(&item, &mut attached);
            if attached
                .iter()
                .any(|anchor| map_measure(anchor.measure).is_none())
            {
                continue;
            }
            map_anchors(&mut item, &|anchor| {
                Some(Anchor {
                    measure: map_measure(anchor.measure)?,
                    ..anchor
                })
            })?;
            kept.push(item);
        }
        set_records(&mut notation, key, kept);
    }
    let mut repeats = Vec::new();
    for original in records(&notation, "repeats") {
        let mut item = original.clone();
        if !remap_range(&mut item, mapping)? {
            continue;
        }
        if let Some(endings) = item.get_mut("endings").and_then(Value::as_array_mut) {
            for ending in endings {
                if !remap_range(ending, mapping)? {
                    return Err("Deleting this range removes an alternate ending; remove or edit the repeat first.".into());
                }
            }
        }
        repeats.push(item);
    }
    set_records(&mut notation, "repeats", repeats);
    let mut navigation = Vec::new();
    for original in records(&notation, "navigation") {
        if let Some(measure) = map_measure(index(original, "measure")?) {
            let mut item = original.clone();
            set_index(&mut item, "measure", measure);
            navigation.push(item);
        }
    }
    set_records(&mut notation, "navigation", navigation);
    let mut tempo = Vec::new();
    for original in records(&notation, "tempo") {
        if original
            .get("at")
            .and_then(|at| index(at, "measure").ok())
            .is_some_and(|measure| map_measure(measure).is_none())
        {
            continue;
        }
        let mut item = original.clone();
        map_timepoints(&mut item, &map_measure)?;
        tempo.push(item);
    }
    set_records(&mut notation, "tempo", tempo);
    score.notation = from_notation(notation)?;
    crate::layout::remap_measures(&mut score.notation.layout, mapping);
    for part in &mut score.notation.parts {
        crate::layout::remap_measures(&mut part.layout, mapping);
    }
    Ok(())
}
fn remap_range(item: &mut Value, mapping: &[Option<usize>]) -> Result<bool, String> {
    let start = index(item, "start")?;
    let end = index(item, "end")?;
    let mapped = mapping
        .get(start..=end)
        .ok_or("Repeat range is outside the source score.")?
        .iter()
        .copied()
        .flatten()
        .collect::<Vec<_>>();
    let Some(first) = mapped.first() else {
        return Ok(false);
    };
    set_index(item, "start", *first);
    set_index(item, "end", *mapped.last().unwrap());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document,
        entry_style::{self, AccidentalDisplay, AccidentalOverride, EntryStyle},
        expressions::{Expression, Hairpin, TempoChange, TempoRampEnd, TimePoint},
        layout::{LockedSystem, MeasureWidth, SystemBreak},
        lyrics::{self, Lyric},
        model::{Pitch, Step},
        repeats::{Ending, NavigationKind, NavigationMark, RepeatRegion},
        slurs::{self, Slur},
    };

    fn anchor(part: usize, measure: usize) -> Anchor {
        Anchor {
            part,
            measure,
            tick: 0,
            layer: 0,
        }
    }
    fn annotated() -> Score {
        let mut score = Score::demo();
        let mut lyric = Lyric::new(anchor(2, 0), "Sing");
        lyric.extend_to = Some(anchor(2, 1));
        lyrics::assign(&mut score, lyric).unwrap();
        slurs::add(&mut score, Slur::new(anchor(2, 0), anchor(2, 1))).unwrap();
        score
            .notation
            .expressions
            .push(Expression::dynamic(anchor(2, 0), "p", 48));
        score
            .notation
            .spanners
            .push(Hairpin::new(anchor(2, 0), anchor(2, 1)));
        let mut style = EntryStyle::new(anchor(2, 0));
        let pitch = document::note_at(&score, &style.anchor).unwrap().pitches[0].midi();
        style.accidentals.push(AccidentalOverride {
            pitch,
            display: AccidentalDisplay::Courtesy,
        });
        style.tie_pitches = Some(vec![pitch]);
        entry_style::assign(&mut score, style).unwrap();
        score.validate().unwrap();
        score
    }
    fn empty_like(score: &Score) -> Score {
        let mut empty = score.clone();
        for staff in &mut empty.parts {
            for measure in &mut staff.measures {
                measure.notes.clear();
            }
        }
        empty.notation = Notation::default();
        empty
    }

    #[test]
    fn copied_rows_remap_annotations_and_keep_nonadjacent_source_indices() {
        let score = annotated();
        let before = score.clone();
        let region = Region {
            staves: vec![2, 0],
            start: 0,
            end: 1,
        };
        let clip = copy_region(&score, &region).unwrap();
        assert_eq!(score, before);
        assert_eq!(clip.notation.lyrics[0].anchor, anchor(0, 0));
        assert_eq!(clip.notation.slurs[0].end, anchor(0, 1));
        assert_eq!(clip.notation.entries[0].anchor.part, 0);
        let mut target = empty_like(&score);
        let original_piano = target.parts[0].clone();
        let pasted = paste_region(&mut target, &clip, &[4, 1], 3, PasteMode::Replace).unwrap();
        assert_eq!(pasted.staves, vec![4, 1]);
        assert_eq!(target.parts[4].measures[3], score.parts[2].measures[0]);
        assert_eq!(target.notation.lyrics[0].anchor, anchor(4, 3));
        assert_eq!(target.notation.lyrics[0].extend_to, Some(anchor(4, 4)));
        assert_eq!(target.notation.spanners[0].end, anchor(4, 4));
        assert_eq!(target.parts[0], original_piano);
        target.validate().unwrap();
    }

    #[test]
    fn boundary_spans_warn_and_lyrics_keep_their_base_syllable() {
        let score = annotated();
        let clip = copy_region(
            &score,
            &Region {
                staves: vec![2],
                start: 0,
                end: 0,
            },
        )
        .unwrap();
        assert_eq!(clip.notation.lyrics.len(), 1);
        assert!(clip.notation.lyrics[0].extend_to.is_none());
        assert!(clip.notation.slurs.is_empty() && clip.notation.spanners.is_empty());
        assert!(
            clip.warnings
                .iter()
                .any(|warning| warning.contains("cross-boundary"))
        );
    }

    #[test]
    fn spans_covering_the_entire_copy_region_still_report_boundary_loss() {
        let mut score = annotated();
        score.notation.slurs.clear();
        slurs::add(&mut score, Slur::new(anchor(2, 0), anchor(2, 4))).unwrap();
        let clip = copy_region(
            &score,
            &Region {
                staves: vec![2],
                start: 2,
                end: 2,
            },
        )
        .unwrap();
        assert!(clip.notation.slurs.is_empty());
        assert!(
            clip.warnings
                .iter()
                .any(|warning| warning.contains("cross-boundary"))
        );
    }

    #[test]
    fn full_staff_stack_copies_contained_repeats_navigation_and_tempo_once() {
        let mut score = annotated();
        score.notation.repeats.push(RepeatRegion {
            start: 0,
            end: 1,
            times: 2,
            endings: vec![],
        });
        score.notation.navigation = vec![
            NavigationMark {
                measure: 0,
                kind: NavigationKind::Segno,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 1,
                kind: NavigationKind::Fine,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 3,
                kind: NavigationKind::DalSegnoAlFine,
                play_repeats_after_jump: false,
            },
        ];
        score.notation.tempo.push(TempoChange {
            at: TimePoint {
                measure: 1,
                tick: 0,
            },
            bpm: 100,
            ramp: Some(TempoRampEnd {
                end: TimePoint {
                    measure: 2,
                    tick: 0,
                },
                bpm: 120,
            }),
            text: "accelerando".into(),
        });
        score.validate().unwrap();
        let clip = copy_region(
            &score,
            &Region {
                staves: (0..score.parts.len()).collect(),
                start: 0,
                end: 3,
            },
        )
        .unwrap();
        assert_eq!(clip.notation.repeats.len(), 1);
        assert_eq!(clip.notation.navigation.len(), 3);
        assert_eq!(clip.notation.tempo.len(), 1);
        let partial = copy_region(
            &score,
            &Region {
                staves: vec![2],
                start: 0,
                end: 3,
            },
        )
        .unwrap();
        assert!(
            partial.notation.repeats.is_empty()
                && partial.notation.navigation.is_empty()
                && partial.notation.tempo.is_empty()
        );
        let mut target = empty_like(&score);
        paste_region(
            &mut target,
            &clip,
            &(0..score.parts.len()).collect::<Vec<_>>(),
            2,
            PasteMode::Replace,
        )
        .unwrap();
        assert_eq!(target.notation.repeats[0].start, 2);
        assert_eq!(
            target
                .notation
                .navigation
                .iter()
                .find(|mark| mark.kind == NavigationKind::Fine)
                .unwrap()
                .measure,
            3
        );
        assert_eq!(target.notation.tempo[0].at.measure, 3);
        assert_eq!(
            target.notation.tempo[0].ramp.as_ref().unwrap().end.measure,
            4
        );
        target.validate().unwrap();
    }

    #[test]
    fn navigation_jumps_with_targets_outside_copy_are_explicitly_omitted() {
        let mut score = Score::demo();
        score.notation.navigation = vec![
            NavigationMark {
                measure: 0,
                kind: NavigationKind::Segno,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 3,
                kind: NavigationKind::DalSegno,
                play_repeats_after_jump: false,
            },
        ];
        let clip = copy_region(
            &score,
            &Region {
                staves: (0..score.parts.len()).collect(),
                start: 2,
                end: 4,
            },
        )
        .unwrap();
        assert!(clip.notation.navigation.is_empty());
        assert!(
            clip.warnings
                .iter()
                .any(|warning| warning.contains("targets"))
        );
    }

    #[test]
    fn da_capo_implicit_document_start_is_not_silently_retargeted_by_copy_or_paste() {
        let mut score = Score::demo();
        score.notation.navigation.push(NavigationMark {
            measure: 3,
            kind: NavigationKind::DaCapo,
            play_repeats_after_jump: false,
        });
        let cropped = copy_region(
            &score,
            &Region {
                staves: (0..score.parts.len()).collect(),
                start: 2,
                end: 4,
            },
        )
        .unwrap();
        assert!(cropped.notation.navigation.is_empty());
        assert!(
            cropped
                .warnings
                .iter()
                .any(|warning| warning.contains("targets"))
        );
        let full = copy_region(
            &score,
            &Region {
                staves: (0..score.parts.len()).collect(),
                start: 0,
                end: 3,
            },
        )
        .unwrap();
        let mut target = empty_like(&score);
        let before = target.clone();
        assert!(
            paste_region(
                &mut target,
                &full,
                &(0..score.parts.len()).collect::<Vec<_>>(),
                2,
                PasteMode::Replace
            )
            .unwrap_err()
            .contains("D.C.")
        );
        assert_eq!(target, before);
        paste_region(
            &mut target,
            &full,
            &(0..score.parts.len()).collect::<Vec<_>>(),
            0,
            PasteMode::Replace,
        )
        .unwrap();
        assert_eq!(target.notation.navigation[0].kind, NavigationKind::DaCapo);
    }

    #[test]
    fn insert_delete_roundtrip_remaps_anchors_global_marks_and_every_part_layout() {
        let mut score = annotated();
        score.notation.layout.breaks.push(SystemBreak {
            before: 4,
            page: true,
        });
        score
            .notation
            .layout
            .locked_systems
            .push(LockedSystem { start: 4, end: 5 });
        score.notation.layout.measure_widths.push(MeasureWidth {
            measure: 6,
            weight: 1.5,
        });
        score.notation.parts = crate::parts::default_definitions(&score);
        score.notation.parts[0].layout = score.notation.layout.clone();
        score.notation.tempo.push(TempoChange::new(
            TimePoint {
                measure: 4,
                tick: 0,
            },
            112,
        ));
        let before = score.clone();
        insert_measures(&mut score, 0, 2).unwrap();
        assert_eq!(score.notation.lyrics[0].anchor.measure, 2);
        assert_eq!(score.notation.expressions[0].anchor.measure, 2);
        assert_eq!(score.notation.entries[0].anchor.measure, 2);
        assert_eq!(score.notation.layout.breaks[0].before, 6);
        assert_eq!(score.notation.parts[0].layout.locked_systems[0].start, 6);
        assert_eq!(score.notation.tempo[0].at.measure, 6);
        delete_measures(&mut score, 0, 1).unwrap();
        assert_eq!(score, before);
    }

    #[test]
    fn clear_keeps_later_positions_and_shortens_incoming_word_extension() {
        let mut score = annotated();
        let later = score.parts[2].measures[2].clone();
        clear_region(
            &mut score,
            &Region {
                staves: vec![2],
                start: 1,
                end: 1,
            },
        )
        .unwrap();
        assert!(score.parts[2].measures[1].notes.is_empty());
        assert_eq!(score.parts[2].measures[2], later);
        assert_eq!(score.notation.lyrics[0].text, "Sing");
        assert!(score.notation.lyrics[0].extend_to.is_none());
        assert!(score.notation.slurs.is_empty() && score.notation.spanners.is_empty());
        assert_eq!(score.notation.expressions.len(), 1);
    }

    #[test]
    fn collisions_bounds_and_meter_errors_leave_the_entire_score_unchanged() {
        let source = annotated();
        let clip = copy_region(
            &source,
            &Region {
                staves: vec![2],
                start: 0,
                end: 1,
            },
        )
        .unwrap();
        let mut score = source.clone();
        for (staves, start, mode) in [
            (vec![2], 0, PasteMode::EmptyOnly),
            (vec![2, 2], 0, PasteMode::Replace),
            (vec![2], MAX_MEASURES - 1, PasteMode::Replace),
            (vec![2], 100, PasteMode::Replace),
        ] {
            let before = score.clone();
            assert!(paste_region(&mut score, &clip, &staves, start, mode).is_err());
            assert_eq!(score, before);
        }
        let mut other_meter = empty_like(&score);
        other_meter.time_numerator = 3;
        let before = other_meter.clone();
        assert!(paste_region(&mut other_meter, &clip, &[2], 0, PasteMode::Replace).is_err());
        assert_eq!(other_meter, before);
        assert!(insert_measures(&mut score, 0, MAX_MEASURES).is_err());
        assert_eq!(score, source);
        let last = score.measure_count() - 1;
        assert!(delete_measures(&mut score, 0, last).is_err());
    }

    #[test]
    fn empty_only_paste_does_not_erase_text_attached_to_an_empty_measure() {
        let source = annotated();
        let clip = copy_region(
            &source,
            &Region {
                staves: vec![2],
                start: 0,
                end: 0,
            },
        )
        .unwrap();
        let mut target = empty_like(&source);
        target
            .notation
            .expressions
            .push(Expression::text(anchor(2, 3), "solo"));
        let before = target.clone();
        assert!(paste_region(&mut target, &clip, &[2], 3, PasteMode::EmptyOnly).is_err());
        assert_eq!(target, before);
    }

    #[test]
    fn deleting_targets_endings_or_ramp_endpoints_is_atomic() {
        let mut score = Score::demo();
        score.notation.navigation = vec![
            NavigationMark {
                measure: 0,
                kind: NavigationKind::ToCoda,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 2,
                kind: NavigationKind::DaCapoAlCoda,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 7,
                kind: NavigationKind::Coda,
                play_repeats_after_jump: false,
            },
        ];
        let before = score.clone();
        assert!(delete_measures(&mut score, 7, 7).is_err());
        assert_eq!(score, before);
        score.notation.navigation.clear();
        score.notation.repeats.push(RepeatRegion {
            start: 0,
            end: 3,
            times: 2,
            endings: vec![
                Ending {
                    start: 2,
                    end: 2,
                    passes: vec![1],
                },
                Ending {
                    start: 3,
                    end: 3,
                    passes: vec![2],
                },
            ],
        });
        let before = score.clone();
        assert!(delete_measures(&mut score, 2, 2).is_err());
        assert_eq!(score, before);
        score.notation.repeats.clear();
        score.notation.tempo.push(TempoChange {
            at: TimePoint {
                measure: 0,
                tick: 0,
            },
            bpm: 100,
            ramp: Some(TempoRampEnd {
                end: TimePoint {
                    measure: 4,
                    tick: 0,
                },
                bpm: 130,
            }),
            text: String::new(),
        });
        let before = score.clone();
        assert!(delete_measures(&mut score, 4, 4).is_err());
        assert_eq!(score, before);
    }

    #[test]
    fn spelled_and_chromatic_transposition_preserve_per_pitch_styles_and_fail_atomically() {
        let mut score = annotated();
        let region = Region {
            staves: vec![2],
            start: 0,
            end: 0,
        };
        let old = score.notation.entries[0].accidentals[0].pitch;
        transpose_region(
            &mut score,
            &region,
            Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
        )
        .unwrap();
        assert_eq!(score.notation.entries[0].accidentals[0].pitch, old + 2);
        assert_eq!(score.notation.entries[0].tie_pitches, Some(vec![old + 2]));
        transpose_chromatic_region(&mut score, &region, -2).unwrap();
        assert_eq!(score.notation.entries[0].accidentals[0].pitch, old);
        score.parts[2].measures[0].notes[0].pitches[0] = Pitch {
            step: Step::G,
            octave: 9,
            alter: 0,
        };
        score.notation.entries.clear();
        let before = score.clone();
        assert!(transpose_chromatic_region(&mut score, &region, 1).is_err());
        assert_eq!(score, before);
    }

    #[test]
    fn diatonic_transposition_uses_the_key_scale_and_keeps_chromatic_inflections() {
        let mut score = Score::demo();
        score.key_fifths = 2;
        score.parts[2].measures[0].notes[0].pitches = vec![
            Pitch {
                step: Step::F,
                octave: 4,
                alter: 1,
            },
            Pitch {
                step: Step::A,
                octave: 4,
                alter: 1,
            },
        ];
        transpose_diatonic_region(
            &mut score,
            &Region {
                staves: vec![2],
                start: 0,
                end: 0,
            },
            1,
        )
        .unwrap();
        assert_eq!(
            score.parts[2].measures[0].notes[0].pitches,
            vec![
                Pitch {
                    step: Step::G,
                    octave: 4,
                    alter: 0
                },
                Pitch {
                    step: Step::B,
                    octave: 4,
                    alter: 1
                }
            ]
        );
        assert_eq!(score.key_fifths, 2);
    }

    #[test]
    fn clipboard_roundtrip_rejects_future_metadata_without_silent_loss() {
        let score = annotated();
        let clip = copy_region(
            &score,
            &Region {
                staves: vec![2],
                start: 0,
                end: 1,
            },
        )
        .unwrap();
        assert_eq!(RegionClip::decode(&clip.encode().unwrap()).unwrap(), clip);
        let mut value = serde_json::to_value(&clip).unwrap();
        value["notation"]["future_spanners"] = serde_json::json!([{"anchor": anchor(0, 0)}]);
        assert!(
            RegionClip::decode(&serde_json::to_string(&value).unwrap())
                .unwrap_err()
                .contains("future_spanners")
        );
    }
}

#[cfg(test)]
mod new_metadata_tests {
    use super::*;
    use crate::{
        chords::{self, ChordSymbol},
        grace::{GraceGroup, GraceNote},
        lines::{LineKind, PerformanceLine},
        model::{Duration, Pitch},
    };
    #[test]
    fn new_notation_survives_region_copy_paste_transpose_and_structural_edits() {
        let mut score = Score::demo();
        let at = Anchor {
            part: 2,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        score.notation.chords.push(ChordSymbol::new(
            Anchor { tick: 240, ..at },
            chords::parse("C7/E").unwrap(),
        ));
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
        let region = Region {
            staves: vec![2],
            start: 0,
            end: 1,
        };
        let clip = copy_region(&score, &region).unwrap();
        assert_eq!(clip.notation.chords[0].anchor.part, 0);
        assert_eq!(clip.notation.grace[0].anchor.part, 0);
        paste_region(&mut score, &clip, &[4], 4, PasteMode::Replace).unwrap();
        let target = Region {
            staves: vec![4],
            start: 4,
            end: 5,
        };
        transpose_region(
            &mut score,
            &target,
            Transposition {
                semitones: 2,
                diatonic_steps: 1,
            },
        )
        .unwrap();
        let chord = score
            .notation
            .chords
            .iter()
            .find(|s| s.anchor.part == 4)
            .unwrap();
        assert_eq!(chord.chord.label(), "D7/F#");
        let grace = score
            .notation
            .grace
            .iter()
            .find(|s| s.anchor.part == 4)
            .unwrap();
        assert_eq!(grace.notes[0].pitches[0].midi(), 62);
        insert_measures(&mut score, 3, 1).unwrap();
        assert!(
            score.notation.lines.iter().any(|line| line.start.part == 4
                && line.start.measure == 5
                && line.end.measure == 6)
        );
        delete_measures(&mut score, 3, 3).unwrap();
        assert!(
            score
                .notation
                .chords
                .iter()
                .any(|symbol| symbol.anchor.part == 4 && symbol.anchor.measure == 4)
        );
        score.validate().unwrap();
        let before = score.clone();
        let region = Region {
            staves: vec![4],
            start: 4,
            end: 5,
        };
        assert!(paste_region(&mut score, &clip, &[4], 4, PasteMode::EmptyOnly).is_err());
        assert_eq!(score, before);
        clear_region(&mut score, &region).unwrap();
        assert!(!score.notation.grace.iter().any(|g| g.anchor.part == 4));
        assert!(!score.notation.chords.iter().any(|g| g.anchor.part == 4));
        assert!(!score.notation.lines.iter().any(|g| g.start.part == 4));
    }
}
