use super::annotations::{Annotations, SourceAnchor};
use super::{
    ImportResult, Warnings, child, finish_parts, initial_score, number, push_event,
    push_rhythmic_event, text, value,
};
use crate::model::{
    Articulation, Clef, Duration, Instrument, MAX_MEASURES, MAX_PARTS, Measure, Note, PPQ, Part,
    Pitch, Step, Tuplet,
};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;

#[derive(Clone)]
struct PartInfo {
    name: String,
    instrument: Instrument,
    volume: f32,
    pan: f32,
}

#[derive(Clone)]
struct Event {
    start: u32,
    ticks: u32,
    pitches: Vec<Pitch>,
    velocity: u8,
    staccato: bool,
    articulations: Vec<Articulation>,
    tied: bool,
    rhythm: Option<(Duration, bool, Option<Tuplet>)>,
}

pub(super) fn parse(document: &Document<'_>) -> Result<ImportResult, String> {
    let root = document.root_element();
    let title = child(root, "work")
        .and_then(|n| value(n, "work-title"))
        .or_else(|| value(root, "movement-title"))
        .unwrap_or("Imported score")
        .to_owned();
    let composer = child(root, "identification")
        .and_then(|n| {
            n.children()
                .find(|n| n.has_tag_name("creator") && n.attribute("type") == Some("composer"))
        })
        .map(text)
        .unwrap_or_default();
    let mut score = initial_score(title, composer);
    let mut warnings = Warnings::default();
    warnings.add("Encore re-engraves imported music; original page layout, fonts, and unsupported engraving are not retained.");
    let mut infos = BTreeMap::new();
    if let Some(list) = child(root, "part-list") {
        for definition in list.children().filter(|n| n.has_tag_name("score-part")) {
            if infos.len() >= MAX_PARTS {
                return Err(format!(
                    "MusicXML exceeds Encore's {MAX_PARTS}-staff limit."
                ));
            }
            let id = definition
                .attribute("id")
                .ok_or("MusicXML score-part has no id.")?;
            let midi = child(definition, "midi-instrument");
            if midi.is_some_and(|n| child(n, "midi-unpitched").is_some()) {
                return Err("Unpitched percussion maps are not supported yet. Pitched percussion such as timpani can be imported.".into());
            }
            let program = midi
                .map(|n| number::<u16>(n, "midi-program"))
                .transpose()?
                .flatten();
            if program.is_some_and(|program| !(1..=128).contains(&program)) {
                return Err("MusicXML MIDI programs must be numbered 1–128.".into());
            }
            let instrument = if let Some(program) = program {
                Instrument::from_gm_program((program - 1) as u8)
                    .ok_or("Invalid MIDI instrument.")?
            } else {
                let instrument_name = child(definition, "score-instrument")
                    .and_then(|n| value(n, "instrument-name"))
                    .or_else(|| value(definition, "part-name"))
                    .unwrap_or("");
                let inferred = Instrument::ALL
                    .iter()
                    .copied()
                    .find(|instrument| instrument.name().eq_ignore_ascii_case(instrument_name));
                let instrument = inferred.unwrap_or(Instrument::Piano);
                warnings.add(format!("Part {id} has no MIDI program; {} was {}. Confirm its instrument in the mixer.", instrument.name(), if inferred.is_some() { "inferred from its name" } else { "selected as a fallback" }));
                instrument
            };
            if definition
                .children()
                .filter(|n| n.has_tag_name("midi-instrument"))
                .count()
                > 1
            {
                warnings.add("Parts with multiple instrument definitions use their first MIDI instrument for playback.");
            }
            let name = value(definition, "part-name")
                .unwrap_or(instrument.name())
                .to_owned();
            let volume = midi
                .map(|n| number::<f32>(n, "volume"))
                .transpose()?
                .flatten()
                .unwrap_or(80.0)
                / 100.0;
            let pan = midi
                .map(|n| number::<f32>(n, "pan"))
                .transpose()?
                .flatten()
                .unwrap_or(0.0)
                / 90.0;
            if !volume.is_finite()
                || !(0.0..=1.0).contains(&volume)
                || !pan.is_finite()
                || !(-1.0..=1.0).contains(&pan)
            {
                return Err("Invalid MusicXML mixer volume or pan.".into());
            }
            if infos
                .insert(
                    id.to_owned(),
                    PartInfo {
                        name,
                        instrument,
                        volume,
                        pan,
                    },
                )
                .is_some()
            {
                return Err("MusicXML has duplicate part ids.".into());
            }
        }
    }
    let mut parts = Vec::new();
    let mut annotation_sets = Vec::new();
    let mut expression_sets = Vec::new();
    let mut initial_meter = None;
    let mut initial_key = None;
    let mut initial_tempo = None;
    let mut seen_parts = Vec::new();
    for source in root.children().filter(|n| n.has_tag_name("part")) {
        if parts.len() >= MAX_PARTS {
            return Err(format!("Import exceeds Encore's {MAX_PARTS}-staff limit."));
        }
        let id = source.attribute("id").ok_or("MusicXML part has no id.")?;
        if seen_parts.contains(&id) {
            return Err("MusicXML has duplicate score parts.".into());
        }
        seen_parts.push(id);
        let info = infos.get(id).cloned().unwrap_or_else(|| {
            warnings.add("A part without MIDI instrumentation was imported as piano.");
            PartInfo {
                name: id.into(),
                instrument: Instrument::Piano,
                volume: 0.8,
                pan: 0.0,
            }
        });
        let measures: Vec<_> = source
            .children()
            .filter(|n| n.has_tag_name("measure"))
            .collect();
        if measures.is_empty() || measures.len() > MAX_MEASURES {
            return Err(format!(
                "MusicXML part {id} must contain 1–{MAX_MEASURES} measures."
            ));
        }
        let mut annotations = Annotations::default();
        let mut expressions = super::expressions::Expressions::default();
        let mut annotation_mapping = BTreeMap::new();
        let repeat_map = crate::repeats::parse_musicxml(source, measures.len())?;
        if !repeat_map.is_empty() {
            if !score.notation.repeats.is_empty() && score.notation.repeats != repeat_map {
                return Err("MusicXML parts disagree on repeat navigation.".into());
            }
            score.notation.repeats = repeat_map;
        }
        let navigation = crate::repeats::parse_musicxml_navigation(source, measures.len())?;
        if !navigation.is_empty() {
            if !score.notation.navigation.is_empty() && score.notation.navigation != navigation {
                return Err("MusicXML parts disagree on navigation instructions.".into());
            }
            score.notation.navigation = navigation;
        }
        // Preserve source staves and map their independent voices into four layers.
        let mut lanes: BTreeMap<(u32, String), Vec<Measure>> = BTreeMap::new();
        let mut clefs = BTreeMap::new();
        let mut declared_staves = 1_u32;
        let mut transpose = BTreeMap::new();
        let mut default_transpose = 0_i16;
        let mut divisions = None;
        let mut velocities = BTreeMap::new();
        let mut part_meter = (4_u8, 4_u8);
        for (measure_index, measure) in measures.iter().enumerate() {
            let context = format!("{}, measure {}", info.name, measure_index + 1);
            let mut cursor = 0_u32;
            let mut chord_start = None;
            let mut events: BTreeMap<(u32, String), Vec<Event>> = BTreeMap::new();
            for item in measure.children().filter(|n| n.is_element()) {
                match item.tag_name().name() {
                    "attributes" => {
                        if let Some(staves) = number::<u32>(item, "staves")? {
                            if staves == 0 || staves as usize > MAX_PARTS - parts.len() {
                                return Err(format!(
                                    "MusicXML declaration exceeds Encore's {MAX_PARTS}-staff limit."
                                ));
                            }
                            declared_staves = declared_staves.max(staves);
                        }
                        if let Some(d) = number::<f64>(item, "divisions")? {
                            if !d.is_finite() || d <= 0.0 || d > 1_000_000.0 {
                                return Err(format!("{context}: invalid MusicXML divisions."));
                            }
                            divisions = Some(d);
                        }
                        if let Some(time) = child(item, "time") {
                            if time.children().filter(|n| n.has_tag_name("beats")).count() != 1 {
                                return Err("Composite and senza-misura time signatures are not supported yet.".into());
                            }
                            part_meter = (
                                number::<u8>(time, "beats")?
                                    .ok_or("Time signature is missing beats.")?,
                                number::<u8>(time, "beat-type")?
                                    .ok_or("Time signature is missing beat-type.")?,
                            );
                            check_meter(part_meter)?;
                            if let Some(first) = initial_meter {
                                if first != part_meter {
                                    return Err("This score changes time signature or uses different simultaneous meters. Encore currently requires one time signature for the whole score.".into());
                                }
                            } else {
                                initial_meter = Some(part_meter);
                                score.time_numerator = part_meter.0;
                                score.time_denominator = part_meter.1;
                            }
                        }
                        for transposition in item.children().filter(|n| n.has_tag_name("transpose"))
                        {
                            if child(transposition, "double").is_some() {
                                return Err("MusicXML octave doubling transpositions are not supported yet.".into());
                            }
                            let semitones = number::<i16>(transposition, "chromatic")?
                                .unwrap_or(0)
                                .checked_add(
                                    number::<i16>(transposition, "octave-change")?
                                        .unwrap_or(0)
                                        .checked_mul(12)
                                        .ok_or("Invalid octave transposition.")?,
                                )
                                .ok_or("Invalid MusicXML transposition.")?;
                            if !(-127..=127).contains(&semitones) {
                                return Err("MusicXML transposition exceeds MIDI range.".into());
                            }
                            if let Some(staff) = transposition.attribute("number") {
                                transpose.insert(
                                    staff
                                        .parse::<u32>()
                                        .map_err(|_| "Invalid transpose staff number.")?,
                                    semitones,
                                );
                            } else {
                                default_transpose = semitones;
                            }
                            if semitones != 0 {
                                warnings.add(
                                    "Transposing instruments were converted to concert pitch.",
                                );
                            }
                        }
                        for key in item.children().filter(|n| n.has_tag_name("key")) {
                            let fifths = number::<i8>(key, "fifths")?
                                .ok_or("Custom key signatures are not supported yet.")?;
                            if !(-7..=7).contains(&fifths) {
                                return Err("Unsupported MusicXML key signature.".into());
                            }
                            // Transposition nodes above have already been processed, even
                            // though keys conventionally precede them in document order.
                            let staff = key
                                .attribute("number")
                                .unwrap_or("1")
                                .parse::<u32>()
                                .map_err(|_| "Invalid key staff number.")?;
                            let shift = transpose.get(&staff).copied().unwrap_or(default_transpose);
                            let concert = concert_key(fifths, shift);
                            if let Some(first) = initial_key {
                                if first != concert {
                                    warnings.add("Key signature changes or different staff keys use the first concert key signature; every note retains its explicit sounding pitch.");
                                }
                            } else {
                                initial_key = Some(concert);
                                score.key_fifths = concert;
                            }
                        }
                        for clef in item.children().filter(|n| n.has_tag_name("clef")) {
                            let staff = clef
                                .attribute("number")
                                .unwrap_or("1")
                                .parse::<u32>()
                                .map_err(|_| "Invalid clef staff number.")?;
                            let parsed = match (value(clef, "sign").unwrap_or("G"), value(clef, "line").unwrap_or("2")) {
                                ("G", "2") => Clef::Treble,
                                ("F", "4") => Clef::Bass,
                                ("C", "3") => Clef::Alto,
                                ("percussion", _) | ("TAB", _) => return Err("Percussion and tablature staves are not supported by this importer yet.".into()),
                                _ => { warnings.add("An unsupported clef was replaced by the instrument's default clef; sounding pitches are preserved."); info.instrument.default_clef() }
                            };
                            if measure_index != 0
                                && clefs
                                    .get(&staff)
                                    .is_some_and(|previous| *previous != parsed)
                            {
                                warnings.add("Clef changes use the initial clef; sounding pitches are preserved.");
                            } else {
                                clefs.insert(staff, parsed);
                            }
                            if number::<i8>(clef, "clef-octave-change")?.unwrap_or(0) != 0 {
                                warnings.add("Octave-clef glyphs use the nearest standard clef; MusicXML sounding pitches are preserved.");
                            }
                        }
                    }
                    "backup" | "forward" => {
                        let ticks = xml_ticks(item, divisions, &context)?;
                        if item.has_tag_name("backup") {
                            cursor = cursor.checked_sub(ticks).ok_or_else(|| {
                                format!("{context}: MusicXML backup moves before the measure.")
                            })?;
                        } else {
                            cursor = cursor
                                .checked_add(ticks)
                                .ok_or("MusicXML position overflowed.")?;
                        }
                        chord_start = None;
                    }
                    "direction" | "sound" => {
                        let imported_tempo = tempo(item)?;
                        if let Some(bpm) = imported_tempo {
                            if !bpm.is_finite() || !(20.0..=400.0).contains(&bpm) {
                                return Err("Imported tempo must be 20–400 BPM.".into());
                            }
                            if initial_tempo.is_none() {
                                initial_tempo = Some(bpm.round() as u16);
                                score.tempo = bpm.round() as u16;
                            }
                        }
                        let offset = number::<f64>(item, "offset")?.unwrap_or(0.0);
                        let scaled = offset * 480.0 / divisions.unwrap_or(480.0);
                        if !scaled.is_finite() || (scaled - scaled.round()).abs() > 0.000001 {
                            return Err("Expression offset is not on Encore's timing grid.".into());
                        }
                        let direction_tick = (f64::from(cursor) + scaled).round();
                        if direction_tick < 0.0 || direction_tick > f64::from(u32::MAX) {
                            return Err("Expression offset extends outside its measure.".into());
                        }
                        expressions.xml(
                            item,
                            SourceAnchor {
                                staff: number::<u32>(item, "staff")?.unwrap_or(1),
                                voice: value(item, "voice").unwrap_or("1").to_owned(),
                                measure: measure_index,
                                tick: direction_tick as u32,
                            },
                            imported_tempo.map(|bpm| bpm.round() as u16),
                            &mut warnings,
                        )?;
                        let staff = number::<u32>(item, "staff")?.unwrap_or(1);
                        if let Some(dynamic) =
                            item.descendants().find(|n| n.has_tag_name("dynamics"))
                            && let Some(mark) = dynamic.children().find(|n| n.is_element())
                        {
                            velocities.insert(staff, dynamic_velocity(mark.tag_name().name()));
                        }
                        if item
                            .descendants()
                            .any(|n| n.has_tag_name("pedal") || n.has_tag_name("octave-shift"))
                        {
                            warnings.add("Pedal marks and octave lines are not retained; pitches, dynamics and supported hairpins are imported.");
                        }
                    }
                    "note" => {
                        if child(item, "grace").is_some() {
                            warnings.add("Grace notes are omitted; main-note timing is preserved.");
                            continue;
                        }
                        if child(item, "unpitched").is_some() {
                            return Err(format!(
                                "{context}: unpitched percussion is not supported yet."
                            ));
                        }
                        let ticks = xml_ticks(item, divisions, &context)?;
                        let rhythm = read_rhythm(item, ticks)
                            .map_err(|error| format!("{context}: {error}"))?;
                        let chord = child(item, "chord").is_some();
                        let start = if chord {
                            chord_start
                                .ok_or_else(|| format!("{context}: chord has no preceding note."))?
                        } else {
                            chord_start = Some(cursor);
                            cursor
                        };
                        if !chord {
                            cursor = cursor
                                .checked_add(ticks)
                                .ok_or("MusicXML position overflowed.")?;
                        }
                        let staff = number::<u32>(item, "staff")?.unwrap_or(1);
                        if staff == 0 || staff > 64 {
                            return Err("MusicXML staff number must be 1–64.".into());
                        }
                        let voice = value(item, "voice").unwrap_or("1").to_owned();
                        if voice.len() > 64 {
                            return Err("MusicXML voice identifier is too long.".into());
                        }
                        let shift = transpose.get(&staff).copied().unwrap_or(default_transpose);
                        let pitches = if let Some(pitch) = child(item, "pitch") {
                            vec![read_pitch(pitch, shift)?]
                        } else if child(item, "rest").is_some() {
                            Vec::new()
                        } else {
                            return Err(format!("{context}: a note is missing pitch or rest."));
                        };
                        let velocity = if let Some(raw) = item.attribute("dynamics") {
                            let value = raw
                                .parse::<f32>()
                                .map_err(|_| "Invalid MusicXML note dynamics.")?;
                            if !value.is_finite() || !(0.0..=150.0).contains(&value) {
                                return Err("Invalid MusicXML note dynamics.".into());
                            }
                            (value * 0.9).round().clamp(0.0, 127.0) as u8
                        } else {
                            *velocities.get(&staff).unwrap_or(&80)
                        };
                        let (staccato, articulations) = read_articulations(item, &mut warnings);
                        let tied = item
                            .children()
                            .any(|n| n.has_tag_name("tie") && n.attribute("type") == Some("start"))
                            || item.descendants().any(|n| {
                                n.has_tag_name("tied") && n.attribute("type") == Some("start")
                            });
                        annotations.xml(
                            item,
                            SourceAnchor {
                                staff,
                                voice: voice.clone(),
                                measure: measure_index,
                                tick: start,
                            },
                            &mut warnings,
                        )?;
                        let key = (staff, voice);
                        if !events.contains_key(&key)
                            && events
                                .keys()
                                .filter(|(source_staff, _)| *source_staff == staff)
                                .count()
                                >= 4
                        {
                            return Err(format!(
                                "MusicXML staff {staff} has more than four independent voices; Encore supports four layers per staff."
                            ));
                        }
                        if !events.contains_key(&key)
                            && events.len() >= 4 * (MAX_PARTS - parts.len())
                        {
                            return Err(format!(
                                "Separating MusicXML voices exceeds Encore's {MAX_PARTS}-staff limit."
                            ));
                        }
                        let lane = events.entry(key).or_default();
                        if chord && lane.last().is_some_and(|event| event.start == start) {
                            let prior = lane.last_mut().ok_or("Missing chord event.")?;
                            if prior.ticks != ticks || prior.rhythm != rhythm {
                                return Err(format!(
                                    "{context}: chords with unequal note lengths are not supported yet."
                                ));
                            }
                            prior.tied |= tied;
                            if prior.pitches.is_empty() || pitches.is_empty() {
                                return Err("A rest cannot be a member of a chord.".into());
                            }
                            prior.pitches.extend(pitches);
                            if prior.velocity != velocity {
                                warnings.add("Different dynamics within a chord use the chord's first note velocity.");
                            }
                            // MusicXML commonly writes a chord's marking on only
                            // one constituent note. Retain markings from any tone.
                            prior.staccato |= staccato;
                            for articulation in articulations {
                                if !prior.articulations.contains(&articulation) {
                                    prior.articulations.push(articulation);
                                }
                            }
                        } else {
                            lane.push(Event {
                                start,
                                ticks,
                                pitches,
                                velocity,
                                staccato,
                                articulations,
                                tied,
                                rhythm,
                            });
                        }
                        if item
                            .descendants()
                            .any(|n| n.has_tag_name("ornaments") || n.has_tag_name("technical"))
                        {
                            warnings.add("Ornaments and playing instructions are not retained in the editable score.");
                        }
                    }
                    "barline" => (),
                    "harmony" | "figured-bass" => warnings.add(
                        "Chord symbols and figured bass are not retained in the editable score.",
                    ),
                    "print" | "bookmark" | "link" => (),
                    _ => (),
                }
            }
            let meter = initial_meter.unwrap_or(part_meter);
            let measure_ticks = u32::from(meter.0) * PPQ * 4 / u32::from(meter.1);
            let extent = events
                .values()
                .flat_map(|events| events.iter())
                .map(|e| e.start.saturating_add(e.ticks))
                .max()
                .unwrap_or(cursor)
                .max(cursor);
            let pickup_shift = if measure_index == 0
                && measure.attribute("implicit") == Some("yes")
                && extent > 0
                && extent < measure_ticks
            {
                warnings.add("The pickup is right-aligned in a full first measure, preserving its timing into the next measure.");
                measure_ticks - extent
            } else {
                0
            };
            annotations.shift(measure_index, pickup_shift);
            expressions.shift(measure_index, pickup_shift);
            if extent > measure_ticks {
                return Err(format!(
                    "{context}: music extends beyond the fixed time signature."
                ));
            }
            for (key, events) in events {
                if !lanes.contains_key(&key)
                    && lanes.keys().filter(|(staff, _)| *staff == key.0).count() >= 4
                {
                    return Err(format!(
                        "MusicXML staff {} has more than four independent voices; Encore supports four layers per staff.",
                        key.0
                    ));
                }
                if !lanes.contains_key(&key) && lanes.len() >= 4 * (MAX_PARTS - parts.len()) {
                    return Err(format!(
                        "Separating MusicXML voices exceeds Encore's {MAX_PARTS}-staff limit."
                    ));
                }
                let measures = lanes
                    .entry(key)
                    .or_insert_with(|| vec![Measure::default(); measures.len()]);
                for event in events {
                    let notes = &mut measures[measure_index].notes;
                    if let Some((duration, dotted, tuplet)) = event.rhythm {
                        push_rhythmic_event(
                            notes,
                            Note {
                                start: event.start + pickup_shift,
                                duration,
                                dotted,
                                tuplet,
                                layer: 0,
                                pitches: event.pitches,
                                velocity: event.velocity,
                                staccato: event.staccato,
                                articulations: event.articulations,
                                tied: event.tied,
                            },
                            event.ticks,
                        )
                    } else {
                        let first_piece = notes.len();
                        push_event(
                            notes,
                            event.start + pickup_shift,
                            event.ticks,
                            event.pitches,
                            event.velocity,
                            event.staccato,
                            event.tied,
                        )
                        .map(|()| {
                            decorate_split_event(
                                &mut notes[first_piece..],
                                event.articulations,
                                &mut warnings,
                            );
                        })
                    }
                    .map_err(|e| format!("{context}: {e}"))?;
                }
            }
        }
        for staff in 1..=declared_staves {
            if !lanes.keys().any(|(source_staff, _)| *source_staff == staff) {
                lanes.insert(
                    (staff, "1".into()),
                    vec![Measure::default(); measures.len()],
                );
            }
        }
        let multiple_staves = lanes
            .keys()
            .map(|(staff, _)| *staff)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1;
        let staff_order = lanes
            .keys()
            .map(|(staff, _)| *staff)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut staff_layers: BTreeMap<u32, (Vec<Measure>, usize)> = BTreeMap::new();
        for ((staff, voice), measures) in lanes {
            let (combined, layer_count) = staff_layers
                .entry(staff)
                .or_insert_with(|| (vec![Measure::default(); measures.len()], 0));
            if (*layer_count as u8).count_ones() >= 4 {
                return Err(format!(
                    "MusicXML staff {staff} has more than four independent voices; Encore supports four layers per staff."
                ));
            }
            let preferred = (!multiple_staves)
                .then(|| voice.parse::<u8>().ok())
                .flatten()
                .filter(|voice| (1..=4).contains(voice))
                .map(|voice| voice - 1);
            let layer = preferred
                .filter(|layer| *layer_count & (1 << layer) == 0)
                .unwrap_or_else(|| {
                    (0..4)
                        .find(|layer| *layer_count & (1 << layer) == 0)
                        .unwrap_or(0)
                });
            annotation_mapping.insert(
                (staff, voice),
                (
                    parts.len() + staff_order.iter().position(|s| *s == staff).unwrap(),
                    layer,
                ),
            );
            *layer_count |= 1 << layer;
            for (destination, source) in combined.iter_mut().zip(measures) {
                destination
                    .notes
                    .extend(source.notes.into_iter().map(|mut note| {
                        note.layer = layer;
                        note
                    }));
            }
        }
        let split = staff_layers.len() > 1;
        for (staff, (measures, layer_count)) in staff_layers {
            if layer_count.count_ones() > 1 {
                warnings.add("Independent MusicXML voices are preserved as rhythmic layers on their source staff.");
            }
            parts.push(Part {
                name: if split {
                    format!("{} · staff {staff}", info.name)
                } else {
                    info.name.clone()
                },
                instrument: info.instrument,
                clef: clefs
                    .get(&staff)
                    .copied()
                    .unwrap_or(info.instrument.default_clef()),
                measures,
                volume: info.volume,
                pan: info.pan,
                muted: false,
                solo: false,
            });
        }
        expression_sets.push((expressions, annotation_mapping.clone()));
        annotation_sets.push((annotations, annotation_mapping));
    }
    if initial_tempo.is_none() {
        warnings.add("No explicit tempo was found; playback uses 120 quarter notes per minute. Set the tempo to match your intended performance.");
    }
    score.parts = parts;
    for (annotations, mapping) in annotation_sets {
        annotations.finish(&mut score, &mapping, &mut warnings)?;
    }
    for (expressions, mapping) in expression_sets {
        expressions.resolve(&mut score, &mapping, &mut warnings)?;
    }
    let parts = std::mem::take(&mut score.parts);
    finish_parts(score, parts, warnings)
}

// MusicXML 4.0: notations/articulations contains distinct staccato and
// staccatissimo symbols; strong-accent is the vertical (marcato) accent.
// https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/articulations/
fn read_articulations(note: Node<'_, '_>, warnings: &mut Warnings) -> (bool, Vec<Articulation>) {
    let mut staccato = false;
    let mut articulations = Vec::new();
    for container in note
        .descendants()
        .filter(|node| node.has_tag_name("articulations"))
    {
        for marking in container.children().filter(|node| node.is_element()) {
            let articulation = match marking.tag_name().name() {
                "staccato" => {
                    staccato = true;
                    continue;
                }
                "accent" => Articulation::Accent,
                "strong-accent" => Articulation::Marcato,
                "tenuto" => Articulation::Tenuto,
                "staccatissimo" => Articulation::Staccatissimo,
                unsupported => {
                    warnings.add(format!("MusicXML articulation '{unsupported}' is not retained; supported markings on the same note are preserved."));
                    continue;
                }
            };
            if !articulations.contains(&articulation) {
                articulations.push(articulation);
            }
        }
    }
    (staccato, articulations)
}

/// A multiply-dotted source value may require several tied Encore notes.
/// Attack markings belong to the first piece; duration markings to the last.
pub(super) fn decorate_split_event(
    notes: &mut [Note],
    articulations: Vec<Articulation>,
    warnings: &mut Warnings,
) {
    let Some(last) = notes.len().checked_sub(1) else {
        return;
    };
    if last > 0 && articulations.contains(&Articulation::Marcato) {
        warnings.add("Marcato on a split tied duration retains its attack accent; the tie continuation takes precedence over its detached release.");
    }
    for articulation in articulations {
        let index = match articulation {
            Articulation::Accent | Articulation::Marcato => 0,
            Articulation::Tenuto | Articulation::Staccatissimo => last,
        };
        if !notes[index].articulations.contains(&articulation) {
            notes[index].articulations.push(articulation);
        }
    }
}

fn read_rhythm(
    node: Node<'_, '_>,
    ticks: u32,
) -> Result<Option<(Duration, bool, Option<Tuplet>)>, String> {
    if child(node, "rest").is_some_and(|rest| rest.attribute("measure") == Some("yes")) {
        return Ok(None);
    }
    let modification = child(node, "time-modification");
    let tuplet = modification
        .map(|modification| {
            let ratio = Tuplet {
                actual: number::<u8>(modification, "actual-notes")?
                    .ok_or("Tuplet is missing its actual-note count.")?,
                normal: number::<u8>(modification, "normal-notes")?
                    .ok_or("Tuplet is missing its normal-note count.")?,
            };
            ratio.validate()?;
            Ok::<_, String>(ratio)
        })
        .transpose()?;
    let duration = value(node, "type").and_then(Duration::from_musicxml_type);
    let dots = node
        .children()
        .filter(|child| child.has_tag_name("dot"))
        .count();
    if let Some(duration) = duration.filter(|_| dots <= 1) {
        let rhythm = (duration, dots == 1, tuplet);
        let probe = Note {
            start: 0,
            duration,
            dotted: dots == 1,
            tuplet,
            layer: 0,
            pitches: Vec::new(),
            velocity: 64,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        };
        if probe.checked_ticks()? != ticks {
            return Err(
                "Written note value and tuplet ratio disagree with the MusicXML duration.".into(),
            );
        }
        Ok(Some(rhythm))
    } else if tuplet.is_some() {
        Err(
            "Tuplets require a supported explicit note type and at most one augmentation dot."
                .into(),
        )
    } else {
        Ok(None)
    }
}

fn check_meter((numerator, denominator): (u8, u8)) -> Result<(), String> {
    if !(1..=32).contains(&numerator) || ![1, 2, 4, 8, 16, 32, 64, 128].contains(&denominator) {
        return Err(
            "Unsupported time signature: Encore supports 1–32 beats over 1, 2, 4, 8, or 16.".into(),
        );
    }
    Ok(())
}

fn xml_ticks(node: Node<'_, '_>, divisions: Option<f64>, context: &str) -> Result<u32, String> {
    let divisions = divisions.ok_or_else(|| {
        format!("{context}: MusicXML divisions must be declared before durations.")
    })?;
    let duration = number::<f64>(node, "duration")?
        .ok_or_else(|| format!("{context}: note or cursor movement is missing duration."))?;
    let ticks = duration * f64::from(PPQ) / divisions;
    if !ticks.is_finite()
        || ticks <= 0.0
        || ticks > 61_440.0
        || (ticks - ticks.round()).abs() > 0.0001
    {
        return Err(format!(
            "{context}: duration is not exactly representable on Encore's 480-tick grid."
        ));
    }
    Ok(ticks.round() as u32)
}

fn read_pitch(node: Node<'_, '_>, transpose: i16) -> Result<Pitch, String> {
    let step = match value(node, "step") {
        Some("C") => Step::C,
        Some("D") => Step::D,
        Some("E") => Step::E,
        Some("F") => Step::F,
        Some("G") => Step::G,
        Some("A") => Step::A,
        Some("B") => Step::B,
        _ => return Err("MusicXML pitch step must be C–B.".into()),
    };
    let octave = number::<i8>(node, "octave")?.ok_or("MusicXML pitch is missing an octave.")?;
    let alter = number::<f32>(node, "alter")?.unwrap_or(0.0);
    if !alter.is_finite() || alter.fract() != 0.0 || !(-2.0..=2.0).contains(&alter) {
        return Err(
            "Microtonal pitches and accidentals beyond double sharps/flats are not supported yet."
                .into(),
        );
    }
    let midi = (i16::from(octave) + 1) * 12 + step.semitone() as i16 + alter as i16 + transpose;
    if !(0..=127).contains(&midi) {
        return Err("Imported pitch lies outside MIDI range 0–127.".into());
    }
    if transpose == 0 {
        Ok(Pitch {
            step,
            octave,
            alter: alter as i8,
        })
    } else {
        Ok(Pitch::from_midi(midi as u8))
    }
}

fn concert_key(fifths: i8, transpose: i16) -> i8 {
    if transpose.rem_euclid(12) == 0 {
        return fifths;
    }
    let tonic = (i16::from(fifths) * 7 + transpose).rem_euclid(12);
    (-7_i8..=7)
        .filter(|n| (i16::from(*n) * 7).rem_euclid(12) == tonic)
        .min_by_key(|n| n.unsigned_abs())
        .unwrap_or(0)
}

fn tempo(node: Node<'_, '_>) -> Result<Option<f64>, String> {
    if let Some(sound) = node
        .descendants()
        .find(|n| n.has_tag_name("sound") && n.attribute("tempo").is_some())
    {
        return sound
            .attribute("tempo")
            .map(|s| s.parse().map_err(|_| "Invalid MusicXML tempo.".into()))
            .transpose();
    }
    if let Some(mark) = node.descendants().find(|n| n.has_tag_name("metronome"))
        && let Some(bpm) = number::<f64>(mark, "per-minute")?
    {
        let factor = match value(mark, "beat-unit").unwrap_or("quarter") {
            "whole" => 4.0,
            "half" => 2.0,
            "quarter" => 1.0,
            "eighth" => 0.5,
            "16th" => 0.25,
            _ => return Err("Unsupported metronome beat unit.".into()),
        };
        let dots = mark
            .children()
            .filter(|n| n.has_tag_name("beat-unit-dot"))
            .count();
        if dots > 1 {
            return Err("Multiple-dot metronome marks are not supported yet.".into());
        }
        return Ok(Some(bpm * factor * if dots == 1 { 1.5 } else { 1.0 }));
    }
    Ok(None)
}

pub(super) fn dynamic_velocity(name: &str) -> u8 {
    match name {
        "pppp" | "ppp" => 28,
        "pp" => 36,
        "p" => 48,
        "mp" => 64,
        "mf" => 80,
        "f" => 101,
        "ff" => 115,
        "fff" | "ffff" => 125,
        "sfz" | "sffz" | "fp" => 104,
        _ => 80,
    }
}

#[cfg(test)]
mod articulation_tests {
    use super::*;

    fn import(notes: &str) -> ImportResult {
        let xml = format!(
            "<score-partwise><part-list><score-part id='P1'><part-name>Piano</part-name></score-part></part-list><part id='P1'><measure number='1'><attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>{notes}</measure></part></score-partwise>"
        );
        let document = Document::parse(&xml).unwrap();
        parse(&document).unwrap()
    }

    #[test]
    fn chord_markings_are_unioned_deduplicated_and_unknown_markings_warn() {
        let imported = import(
            "<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><type>quarter</type><notations><articulations><accent/><accent/><strong-accent/><staccato/><falloff/></articulations></notations></note>
             <note><chord/><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><type>quarter</type><notations><articulations><tenuto/><accent/></articulations></notations></note>
             <note><pitch><step>D</step><octave>4</octave></pitch><duration>4</duration><type>quarter</type><notations><articulations><staccatissimo/></articulations></notations></note>",
        );
        let notes = &imported.score.parts[0].measures[0].notes;
        assert_eq!(notes[0].pitches.len(), 2);
        assert!(notes[0].staccato);
        assert_eq!(
            notes[0].articulations,
            vec![
                Articulation::Accent,
                Articulation::Marcato,
                Articulation::Tenuto
            ]
        );
        assert!(
            !notes[1].staccato,
            "staccatissimo must retain its own symbol"
        );
        assert_eq!(notes[1].articulations, vec![Articulation::Staccatissimo]);
        assert!(
            imported
                .warnings
                .iter()
                .any(|warning| warning.contains("'falloff'"))
        );
        assert!(
            !imported
                .warnings
                .iter()
                .any(|warning| warning.contains("'accent'"))
        );
    }

    #[test]
    fn split_written_value_keeps_attack_and_release_markings_on_correct_pieces() {
        let imported = import(
            "<note><pitch><step>C</step><octave>4</octave></pitch><duration>7</duration><type>quarter</type><dot/><dot/><notations><articulations><accent/><tenuto/><staccatissimo/></articulations></notations></note>",
        );
        let notes = &imported.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes.iter().map(Note::ticks).sum::<u32>(), 840);
        assert_eq!(notes[0].articulations, vec![Articulation::Accent]);
        assert!(notes[0].tied);
        assert_eq!(
            notes[1].articulations,
            vec![Articulation::Tenuto, Articulation::Staccatissimo]
        );
        assert!(!notes[1].tied);
    }
}
