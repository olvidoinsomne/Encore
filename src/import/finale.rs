//! Read-only MUSX import from publicly documented EnigmaXML records.
//!
//! Format references (no Finale implementation or assets are used):
//! https://github.com/Project-Attacca/enigmaxml-documentation/blob/main/docs/enigmaxml.md
//! https://github.com/chrisroode/denigma (MIT; score.dat cipher parameters)
//! https://github.com/openmusx/musxdom (MIT; record and pitch semantics)
//!
//! The supported subset is deliberately bounded. Unsupported rhythmic or pitch
//! structures return an error instead of producing a plausible, incorrect score.

use super::{ImportResult, MAX_ARCHIVE_BYTES, MAX_XML_BYTES, parse_xml, push_event};
use crate::model::{
    Clef, Instrument, MAX_MEASURES, MAX_PARTS, Measure, Part, Pitch, Score, Step, key_alter,
};
use flate2::read::GzDecoder;
use roxmltree::Node;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

const NS: &str = "http://www.makemusic.com/2012/finale";
const MAX_ENTRIES: usize = 200_000;

pub(super) fn parse(bytes: &[u8]) -> Result<ImportResult, String> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err("Finale MUSX exceeds the 64 MiB archive limit.".into());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Invalid Finale MUSX archive: {error}"))?;
    if archive.len() > 2048 {
        return Err("Finale archive contains too many files.".into());
    }
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = file.name();
        if file.enclosed_name().is_none()
            || name.contains(['\\', ':'])
            || name.split('/').any(|component| component == "..")
        {
            return Err("Finale archive contains an unsafe file path.".into());
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or("Invalid Finale archive size.")?;
        if expanded > 128 * 1024 * 1024 {
            return Err("Finale archive exceeds the 128 MiB expanded size limit.".into());
        }
    }
    let mut score_data = archive_file(&mut archive, "score.dat", true)?.unwrap();
    decode_score_data(&mut score_data);
    let mut xml = Vec::new();
    GzDecoder::new(score_data.as_slice())
        .take(MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut xml)
        .map_err(|error| format!("Cannot decode Finale score.dat: {error}"))?;
    if xml.len() > MAX_XML_BYTES {
        return Err("Decoded Finale score exceeds the XML size limit.".into());
    }
    let xml = std::str::from_utf8(&xml)
        .map_err(|_| "Finale score.dat must contain UTF-8 EnigmaXML.".to_string())?;
    let metadata = archive_file(&mut archive, "NotationMetadata.xml", false)?;
    parse_enigma(xml, metadata.as_deref())
}

fn archive_file(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
    required: bool,
) -> Result<Option<Vec<u8>>, String> {
    let count = archive
        .file_names()
        .filter(|candidate| *candidate == name)
        .count();
    if count > 1 {
        return Err(format!("Finale archive contains duplicate {name} files."));
    }
    if count == 0 {
        return if required {
            Err(format!("Finale MUSX archive is missing {name}."))
        } else {
            Ok(None)
        };
    }
    let mut file = archive.by_name(name).map_err(|error| error.to_string())?;
    if file.size() > MAX_XML_BYTES as u64 {
        return Err(format!("Finale {name} exceeds the size limit."));
    }
    let mut data = Vec::new();
    file.by_ref()
        .take(MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|error| format!("Cannot read Finale {name}: {error}"))?;
    if data.len() > MAX_XML_BYTES {
        return Err(format!("Finale {name} exceeds the size limit."));
    }
    Ok(Some(data))
}

// MUSX's fixed keystream is a container encoding, not an application license key.
// An original implementation of the public denigma format description.
fn decode_score_data(bytes: &mut [u8]) {
    let mut state = 0u32;
    for (index, byte) in bytes.iter_mut().enumerate() {
        if index % 0x20000 == 0 {
            state = 0x28006d45;
        }
        state = state.wrapping_mul(0x41c64e6d).wrapping_add(0x3039);
        let upper = state >> 16;
        *byte ^= upper.wrapping_add(upper / 255) as u8;
    }
}

fn child<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|item| item.is_element() && item.tag_name().name() == name)
}

fn value<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<&'a str> {
    child(node, name)
        .and_then(|item| item.text())
        .map(str::trim)
}

fn number(node: Node<'_, '_>, name: &str, default: i32) -> Result<i32, String> {
    value(node, name).map_or(Ok(default), |text| {
        text.parse()
            .map_err(|_| format!("Invalid Finale {name} value."))
    })
}

fn attribute(node: Node<'_, '_>, name: &str) -> Result<u32, String> {
    node.attribute(name)
        .ok_or_else(|| format!("Finale record is missing {name}."))?
        .parse()
        .map_err(|_| format!("Invalid Finale {name} identifier."))
}

fn score_record(node: Node<'_, '_>) -> bool {
    node.attribute("part").is_none_or(|part| part == "0") && node.attribute("shared").is_none()
}

fn records<'a, 'input>(pool: Node<'a, 'input>, tag: &str) -> Vec<Node<'a, 'input>> {
    pool.children()
        .filter(|node| node.is_element() && node.tag_name().name() == tag && score_record(*node))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    fifths: i8,
    minor: bool,
}

fn read_key(measure: Node<'_, '_>) -> Result<Key, String> {
    let raw = match child(measure, "keySig") {
        Some(key) => number(key, "key", 0)?,
        None => 0,
    };
    if !(0..=511).contains(&raw) {
        return Err("Finale custom/nonlinear key signatures are not supported; export MusicXML from Finale.".into());
    }
    let minor = raw >= 256;
    let fifths = raw % 256;
    let fifths = if fifths > 127 { fifths - 256 } else { fifths };
    if !(-7..=7).contains(&fifths) {
        return Err(
            "Finale custom key signatures are not supported; export MusicXML from Finale.".into(),
        );
    }
    Ok(Key {
        fifths: fifths as i8,
        minor,
    })
}

fn read_meter(measure: Node<'_, '_>) -> Result<(u8, u8), String> {
    let beats = number(measure, "beats", 4)?;
    let divbeat = number(measure, "divbeat", 1024)?;
    let (numerator, denominator) = if divbeat > 0 && divbeat % 1536 == 0 {
        (
            beats
                .checked_mul(3)
                .and_then(|beats| beats.checked_mul(divbeat / 1536)),
            8,
        )
    } else if divbeat > 0 && 4096 % divbeat == 0 {
        (Some(beats), 4096 / divbeat)
    } else {
        return Err(
            "Finale composite time signature is not supported; export MusicXML from Finale.".into(),
        );
    };
    let numerator = numerator.ok_or("Invalid Finale time signature.")?;
    if !(1..=32).contains(&numerator) || ![1, 2, 4, 8, 16, 32, 64, 128].contains(&denominator) {
        return Err("Finale time signature is outside Encore's supported range.".into());
    }
    Ok((numerator as u8, denominator as u8))
}

fn read_pitch(note: Node<'_, '_>, key: Key) -> Result<Pitch, String> {
    let level = number(note, "harmLev", 0)?;
    if !(-100..=100).contains(&level) {
        return Err("Finale pitch displacement is outside MIDI range.".into());
    }
    let displacement = level - if key.minor { 2 } else { 0 };
    let tonic = (i32::from(key.fifths) * 4).rem_euclid(7);
    let diatonic = displacement + tonic;
    let step = Step::from_index(diatonic);
    let alter = number(note, "harmAlt", 0)? + i32::from(key_alter(step, key.fifths));
    let octave = 4 + diatonic.div_euclid(7);
    if !(-2..=2).contains(&alter) || !(-1..=9).contains(&octave) {
        return Err("Finale pitch or accidental is outside Encore's supported range.".into());
    }
    let midi = (octave + 1) * 12 + step.semitone() + alter;
    if !(0..=127).contains(&midi) {
        return Err("Finale pitch is outside MIDI range.".into());
    }
    Ok(Pitch {
        step,
        octave: octave as i8,
        alter: alter as i8,
    })
}

fn parse_enigma(xml: &str, metadata: Option<&[u8]>) -> Result<ImportResult, String> {
    let document = parse_xml(xml)?;
    let root = document.root_element();
    if root.tag_name().name() != "finale" || root.tag_name().namespace() != Some(NS) {
        return Err("Finale score.dat does not contain a recognized EnigmaXML score.".into());
    }
    let others = child(root, "others").ok_or("Finale score is missing its others pool.")?;
    let details = child(root, "details").ok_or("Finale score is missing its details pool.")?;
    let entry_pool = child(root, "entries").ok_or("Finale score is missing its entries pool.")?;
    let mut warnings = vec![
        "Finale import preserves basic notes, chords, rests, and whole-chord ties. Page layout, lyrics, slurs, expressions, articulations, repeats, plug-ins, and mixer settings are not retained; repeat playback is not expanded.".into(),
    ];

    let mut measures = records(others, "measSpec");
    measures.sort_by_key(|measure| {
        measure
            .attribute("cmper")
            .and_then(|id| id.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    if measures.is_empty() || measures.len() > MAX_MEASURES {
        return Err(format!(
            "Finale score must contain 1–{MAX_MEASURES} measures."
        ));
    }
    let key = read_key(measures[0])?;
    let meter = read_meter(measures[0])?;
    let mut measure_ids = HashSet::new();
    for measure in &measures {
        if !measure_ids.insert(attribute(*measure, "cmper")?) {
            return Err("Finale score contains duplicate measure identifiers.".into());
        }
        if read_key(*measure)? != key || read_meter(*measure)? != meter {
            return Err("Finale key or meter changes are not supported by Encore yet; export MusicXML and simplify the changes before importing.".into());
        }
    }
    let mut staves = records(others, "staffSpec");
    staves.retain(|staff| staff.attribute("cmper") != Some("32767"));
    if staves.is_empty() || staves.len() > MAX_PARTS {
        return Err(format!("Finale score must contain 1–{MAX_PARTS} staves."));
    }
    let mut staff_ids = HashSet::new();
    for staff in &staves {
        if !staff_ids.insert(attribute(*staff, "cmper")?) {
            return Err("Finale score contains duplicate staff identifiers.".into());
        }
    }
    // instUsed/cmper=0 is the scroll-view ordering; staffSpec IDs alone do not
    // identify score order and may contain gaps after staff deletion.
    let mut staff_order = Vec::new();
    let mut instrument_list = records(others, "instUsed");
    instrument_list.retain(|item| item.attribute("cmper") == Some("0"));
    instrument_list.sort_by_key(|item| {
        item.attribute("inci")
            .and_then(|id| id.parse::<u32>().ok())
            .unwrap_or(0)
    });
    for item in instrument_list {
        let id = number(item, "inst", 0)? as u32;
        if staff_ids.contains(&id) && !staff_order.contains(&id) {
            staff_order.push(id);
        }
    }
    if staff_order.len() != staves.len() {
        warnings.push(
            "The Finale staff-order list is incomplete; remaining staves follow their file order."
                .into(),
        );
        for staff in &staves {
            let id = attribute(*staff, "cmper")?;
            if !staff_order.contains(&id) {
                staff_order.push(id);
            }
        }
    }
    staves.sort_by_key(|staff| {
        let staff_id = staff
            .attribute("cmper")
            .and_then(|id| id.parse::<u32>().ok());
        staff_order
            .iter()
            .position(|id| Some(*id) == staff_id)
            .unwrap_or(usize::MAX)
    });

    let mut entries = HashMap::new();
    for entry in records(entry_pool, "entry") {
        let id = attribute(entry, "entnum")?;
        if entries.insert(id, entry).is_some() {
            return Err("Finale score contains duplicate entry identifiers.".into());
        }
    }
    if entries.len() > MAX_ENTRIES {
        return Err("Finale score exceeds the entry limit.".into());
    }
    let mut frames = HashMap::<u32, Vec<Node<'_, '_>>>::new();
    for frame in records(others, "frameSpec") {
        if child(frame, "startEntry").is_some() {
            frames
                .entry(attribute(frame, "cmper")?)
                .or_default()
                .push(frame);
        }
    }
    let mut holds = HashMap::<(u32, u32), Vec<Node<'_, '_>>>::new();
    for hold in records(details, "gfhold") {
        let cell = (attribute(hold, "cmper1")?, attribute(hold, "cmper2")?);
        holds.entry(cell).or_default().push(hold);
    }
    let options = child(root, "options");
    let playback = options.and_then(|options| child(options, "playbackOptions"));
    let mut tempo = playback
        .map(|playback| number(playback, "beatsPerMinute", 120))
        .transpose()?
        .unwrap_or(120);
    let edus_per_beat = playback
        .map(|playback| number(playback, "edusPerBeat", 1024))
        .transpose()?
        .unwrap_or(1024);
    if edus_per_beat <= 0 {
        return Err("Invalid Finale playback beat duration.".into());
    }
    let tempo_units = tempo
        .checked_mul(edus_per_beat)
        .ok_or("Invalid Finale tempo.")?;
    if tempo_units % 1024 != 0 {
        warnings.push(
            "The initial Finale tempo was rounded to Encore's nearest whole quarter-note BPM."
                .into(),
        );
    }
    tempo = tempo_units
        .checked_add(512)
        .ok_or("Invalid Finale tempo.")?
        / 1024;
    if !(20..=400).contains(&tempo) {
        return Err("Finale tempo must fall between 20 and 400 quarter notes per minute.".into());
    }
    if playback
        .and_then(|playback| value(playback, "beatsPerMinute"))
        .is_none()
    {
        warnings.push(
            "No initial playback tempo was found; Encore uses 120 quarter notes per minute.".into(),
        );
    }
    let velocity = playback
        .map(|playback| number(playback, "baseVelocity", 80))
        .transpose()?
        .unwrap_or(80);
    if !(1..=127).contains(&velocity) {
        return Err("Invalid Finale base velocity.".into());
    }
    let mut score = Score {
        title: "Imported Finale score".into(),
        composer: String::new(),
        tempo: tempo as u16,
        time_numerator: meter.0,
        time_denominator: meter.1,
        key_fifths: key.fifths,
        parts: Vec::new(),
        notation: Default::default(),
    };
    if let Some(metadata) = metadata {
        if let Ok(text) = std::str::from_utf8(metadata) {
            if let Ok(metadata) = parse_xml(text) {
                if let Some(info) = child(metadata.root_element(), "fileInfo") {
                    if let Some(title) = value(info, "title").filter(|title| !title.is_empty()) {
                        score.title = title.into();
                    }
                    if let Some(composer) = value(info, "composer") {
                        score.composer = composer.into();
                    }
                }
            } else {
                warnings.push(
                    "Finale metadata could not be read; title and composer may be missing.".into(),
                );
            }
        } else {
            warnings.push("Finale metadata uses an unsupported text encoding; title and composer may be missing.".into());
        }
    }

    let mut imported_events = 0usize;
    for staff in staves {
        let staff_id = attribute(staff, "cmper")?;
        for unsupported in [
            "indepKey",
            "indepTime",
            "hasStyles",
            "percMapID",
            "percussionMap",
            "fretInst",
        ] {
            if child(staff, unsupported).is_some() {
                return Err(format!(
                    "Finale staff {staff_id} uses independent signatures, staff styles, percussion maps, or tablature that Encore cannot import yet; export MusicXML from Finale."
                ));
            }
        }
        let name = staff_name(root, staff).unwrap_or_else(|| format!("Staff {staff_id}"));
        let instrument = instrument_from_uuid(value(staff, "instUuid"));
        let instrument = instrument.unwrap_or_else(|| {
            warnings.push(format!("{name}: instrument assignment is not recognized; piano is selected. Choose a sound in the instrument library."));
            Instrument::Piano
        });
        if child(staff, "transposition").is_some() {
            warnings.push(format!(
                "{name}: imported in concert pitch; Finale's staff transposition is not retained."
            ));
        }
        let mut part = Part {
            name,
            instrument,
            clef: instrument.default_clef(),
            measures: vec![Measure::default(); measures.len()],
            volume: 0.8,
            pan: 0.0,
            muted: false,
            solo: false,
        };
        let mut clef_seen = false;
        for (measure_index, measure) in measures.iter().enumerate() {
            let measure_id = attribute(*measure, "cmper")?;
            let Some(cell) = holds.get(&(staff_id, measure_id)) else {
                continue;
            };
            if cell.len() > 1 {
                return Err("Finale staff/measure has ambiguous frame assignments.".into());
            }
            let Some(&hold) = cell.first() else {
                continue;
            };
            if let Some(clef) = value(hold, "clefID") {
                if !clef_seen {
                    part.clef = read_clef(options, clef, &mut warnings)?;
                    clef_seen = true;
                } else if read_clef(options, clef, &mut Vec::new())? != part.clef {
                    warnings.push(format!(
                        "{}: clef changes are displayed using the first clef.",
                        part.name
                    ));
                }
            }
            if child(hold, "clefListID").is_some() {
                warnings.push(format!(
                    "{}: mid-measure clef changes are not retained.",
                    part.name
                ));
            }
            let assigned: Vec<_> = (1..=4)
                .filter_map(|layer| value(hold, &format!("frame{layer}")).map(|id| (layer, id)))
                .collect();
            for (layer, frame_id) in assigned {
                let frame_id: u32 = frame_id
                    .parse()
                    .map_err(|_| "Invalid Finale frame identifier.")?;
                let candidates = frames
                    .get(&frame_id)
                    .ok_or_else(|| format!("Finale frame {frame_id} is missing."))?;
                if candidates.len() != 1 {
                    return Err(format!("Finale frame {frame_id} is missing or ambiguous."));
                }
                let frame = candidates[0];
                if frame.children().any(|node| {
                    node.is_element()
                        && !["startEntry", "endEntry"].contains(&node.tag_name().name())
                }) {
                    return Err("Finale frame uses unsupported timing offsets or mirror references; export MusicXML from Finale.".into());
                }
                let first = number(frame, "startEntry", 0)?;
                let last = number(frame, "endEntry", 0)?;
                if first <= 0 || last <= 0 {
                    return Err("Finale frame has invalid entry bounds.".into());
                }
                let mut current = first as u32;
                let mut visited = HashSet::new();
                let mut position = 0u32;
                loop {
                    if !visited.insert(current) || visited.len() > MAX_ENTRIES {
                        return Err(
                            "Finale entry chain contains a cycle or exceeds the entry limit."
                                .into(),
                        );
                    }
                    let entry = *entries.get(&current).ok_or_else(|| {
                        format!("Finale frame refers to missing entry {current}.")
                    })?;
                    for unsupported in [
                        "v2",
                        "controller",
                        "graceNote",
                        "tupletStart",
                        "crossStaff",
                        "dummy",
                    ] {
                        if child(entry, unsupported).is_some() {
                            return Err(format!(
                                "{} measure {} contains unsupported Finale voices, tuplets, grace notes, or cross-staff rhythms; export MusicXML or simplify those passages first.",
                                part.name,
                                measure_index + 1
                            ));
                        }
                    }
                    let dura = number(entry, "dura", 0)?;
                    if dura <= 0 {
                        return Err("Finale note duration must be positive.".into());
                    }
                    let tick_units = (dura as u32)
                        .checked_mul(480)
                        .ok_or("Invalid Finale duration.")?;
                    if tick_units % 1024 != 0 {
                        return Err("Finale note duration is not exactly representable in Encore's tick resolution.".into());
                    }
                    let mut ticks = tick_units / 1024;
                    let notes: Vec<_> = entry
                        .children()
                        .filter(|node| node.has_tag_name((NS, "note")))
                        .collect();
                    let is_note = child(entry, "isNote").is_some();
                    // Finale uses a whole rest for a complete empty measure in any
                    // meter; its displayed duration is not the elapsed bar length.
                    if !is_note && dura == 4096 && current == first as u32 && current == last as u32
                    {
                        ticks = score.measure_ticks();
                    }
                    let end_position = position
                        .checked_add(ticks)
                        .ok_or("Invalid Finale measure duration.")?;
                    if end_position > score.measure_ticks() {
                        return Err(format!(
                            "{} measure {} extends past its bar line; pickups or irregular Finale measure timing cannot be imported yet.",
                            part.name,
                            measure_index + 1
                        ));
                    }
                    let mut pitches = Vec::new();
                    let mut tied_count = 0usize;
                    if is_note {
                        if notes.is_empty() || notes.len() > 16 {
                            return Err("Finale note entry must contain 1–16 pitches.".into());
                        }
                        for note in &notes {
                            if child(*note, "crossStaff").is_some() {
                                return Err(
                                    "Finale cross-staff notes are not supported yet.".into()
                                );
                            }
                            pitches.push(read_pitch(*note, key)?);
                            if child(*note, "tieStart").is_some() {
                                tied_count += 1;
                            }
                        }
                        if tied_count != 0 && tied_count != notes.len() {
                            return Err("Finale partial-chord ties cannot be represented yet; export MusicXML with separate staves for the voices.".into());
                        }
                    }
                    let previous_count = part.measures[measure_index].notes.len();
                    push_event(
                        &mut part.measures[measure_index].notes,
                        position,
                        ticks,
                        pitches,
                        velocity as u8,
                        false,
                        tied_count > 0,
                    )?;
                    for note in &mut part.measures[measure_index].notes[previous_count..] {
                        note.layer = (layer - 1) as u8;
                    }
                    imported_events += part.measures[measure_index].notes.len() - previous_count;
                    if imported_events > MAX_ENTRIES {
                        return Err("Imported Finale score exceeds the 200,000-event limit.".into());
                    }
                    position = end_position;
                    if current == last as u32 {
                        break;
                    }
                    current = attribute(entry, "next")?;
                    if current == 0 {
                        return Err("Finale entry chain ends before its frame boundary.".into());
                    }
                }
            }
        }
        score.parts.push(part);
    }
    score.normalize();
    score
        .validate()
        .map_err(|error| format!("Finale score cannot be represented in Encore: {error}"))?;
    let mut unique = HashSet::new();
    warnings.retain(|warning| unique.insert(warning.clone()));
    Ok(ImportResult { score, warnings })
}

fn read_clef(
    options: Option<Node<'_, '_>>,
    id: &str,
    warnings: &mut Vec<String>,
) -> Result<Clef, String> {
    let definition = options
        .and_then(|options| child(options, "clefOptions"))
        .and_then(|clefs| {
            clefs.children().find(|clef| {
                clef.has_tag_name((NS, "clefDef")) && clef.attribute("index") == Some(id)
            })
        });
    let Some(definition) = definition else {
        warnings.push("A Finale clef definition was absent; treble clef is displayed.".into());
        return Ok(Clef::Treble);
    };
    let symbol = number(definition, "clefChar", 38)?;
    let displacement = number(definition, "clefYDisp", -6)?;
    match (symbol, displacement) {
        (38 | 57424, -6) => Ok(Clef::Treble),
        (63 | 57442, -2) => Ok(Clef::Bass),
        (66 | 57436, -4) => Ok(Clef::Alto),
        _ => {
            warnings.push("A Finale clef cannot be displayed exactly; notes use a standard treble clef at concert pitch.".into());
            Ok(Clef::Treble)
        }
    }
}

fn staff_name(root: Node<'_, '_>, staff: Node<'_, '_>) -> Option<String> {
    let id = value(staff, "fullName")?;
    let block = child(root, "others")?
        .children()
        .find(|node| node.has_tag_name((NS, "textBlock")) && node.attribute("cmper") == Some(id))?;
    let id = value(block, "textID")?;
    let text = child(root, "texts")?
        .children()
        .find(|node| node.has_tag_name((NS, "blockText")) && node.attribute("number") == Some(id))?
        .text()?;
    // Finale text formatting is a sequence of ^command(...) inserts. Retain
    // readable staff names without importing arbitrary font instructions.
    let mut name = String::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '^' {
            while chars
                .peek()
                .is_some_and(|character| character.is_alphanumeric())
            {
                chars.next();
            }
            if chars.peek() == Some(&'(') {
                chars.next();
                for character in chars.by_ref() {
                    if character == ')' {
                        break;
                    }
                }
            }
        } else {
            name.push(character);
        }
    }
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn instrument_from_uuid(uuid: Option<&str>) -> Option<Instrument> {
    // Public, factual identifiers for the core orchestral instruments. They
    // select Encore's own GM sample library, never a Finale/Garritan asset.
    match uuid? {
        "776a2734-3f38-4cac-8fa2-90e62b368ec1" | "38cd3cd9-3160-4baa-9bfd-c2d9f353dafd" => {
            Some(Instrument::Piano)
        }
        "29d81c10-dfed-42e4-8d93-64af7d6d5689" => Some(Instrument::Violin),
        "992dcb2b-64a2-454b-b043-2a530d24fc18" => Some(Instrument::Viola),
        "33894688-c326-4f8f-b4e2-7a08745e7bcc" => Some(Instrument::Cello),
        "4924f736-97a2-443d-b386-03de2ae6a848" | "3dd79e8b-13a2-475f-a55d-d3221183f7f6" => {
            Some(Instrument::DoubleBass)
        }
        "2575136d-9927-47bd-a727-f554c169257e" => Some(Instrument::Flute),
        "ec6fcc9b-d539-4a4a-a677-92a37298fec1" => Some(Instrument::Bassoon),
        "e4fc7b0a-5cd3-445b-ae1f-776764513741" => Some(Instrument::EnglishHorn),
        "616b3bf9-58df-4b44-8339-9f66461ac910" | "06662c43-783f-488c-9614-d94b52e5699d" => {
            Some(Instrument::FrenchHorn)
        }
        "69e76d62-3913-4e4a-bb6f-6c05be22abf8" | "f0458710-e401-4a33-a9a7-bff5b24006df" => {
            Some(Instrument::Trumpet)
        }
        "157d105d-c3a4-49ab-98ab-2b46102aa3dc" | "97284c36-deb2-4e77-9600-14d3081a31cd" => {
            Some(Instrument::Trombone)
        }
        "9b2b7b02-f914-4e75-82d5-6fcc18843516" => Some(Instrument::Tuba),
        "059784fb-9d88-4a78-bd2d-09fe393496e5" => Some(Instrument::Timpani),
        "5bab3582-2dbd-4864-baa9-b022efd075ad"
        | "5a55eca7-40f5-454b-b920-c8879fa03bcc"
        | "255c105e-78a1-4c78-8755-8febc86d0f43" => Some(Instrument::StringEnsemble),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    // Independently authored schema fixture. Entry IDs intentionally do not
    // increase, and a disconnected scratch entry must never enter the score.
    fn fixture() -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<finale xmlns="{NS}" version="27.4">
 <options>
  <playbackOptions><beatsPerMinute>96</beatsPerMinute><edusPerBeat>1024</edusPerBeat><baseVelocity>72</baseVelocity></playbackOptions>
  <clefOptions><clefDef index="0"><clefChar>38</clefChar><clefYDisp>-6</clefYDisp></clefDef></clefOptions>
 </options>
 <others>
  <staffSpec cmper="7"><staffLines>5</staffLines><instUuid>059784fb-9d88-4a78-bd2d-09fe393496e5</instUuid><fullName>3</fullName></staffSpec>
  <staffSpec cmper="32767"><customStaff/><instUuid>scratch</instUuid></staffSpec>
  <instUsed cmper="0" inci="0"><inst>7</inst></instUsed>
  <measSpec cmper="1"><beats>4</beats><divbeat>1024</divbeat><keySig><key>1</key></keySig></measSpec>
  <frameSpec cmper="9" inci="0"><startEntry>40</startEntry><endEntry>3</endEntry></frameSpec>
  <textBlock cmper="3"><textID>2</textID></textBlock>
 </others>
 <details><gfhold cmper1="7" cmper2="1"><clefID>0</clefID><frame1>9</frame1></gfhold></details>
 <entries>
  <entry entnum="40" prev="0" next="3"><dura>1024</dura><isNote/><note id="1"><harmLev>-4</harmLev><harmAlt>0</harmAlt><tieStart/></note></entry>
  <entry entnum="3" prev="40" next="0"><dura>3072</dura><isNote/><note id="1"><harmLev>-4</harmLev><harmAlt>0</harmAlt></note></entry>
  <entry entnum="999" prev="0" next="0"><dura>4096</dura><graceNote/><v2/></entry>
 </entries>
 <texts><blockText number="2">^font(Times New Roman)^size(12)Timpani</blockText></texts>
</finale>"#
        )
    }

    fn metadata() -> &'static [u8] {
        br#"<metadata xmlns="http://www.makemusic.com/2012/NotationMetadata"><fileInfo><title>Independent import test</title><composer>Encore tests</composer></fileInfo></metadata>"#
    }

    fn archive(xml: &str) -> Vec<u8> {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(xml.as_bytes()).unwrap();
        let mut score_data = gzip.finish().unwrap();
        decode_score_data(&mut score_data);
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("score.dat", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&score_data).unwrap();
        archive
            .start_file(
                "NotationMetadata.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(metadata()).unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn imports_musx_container_linked_entries_and_metadata() {
        let result = parse(&archive(&fixture())).unwrap();
        let score = result.score;
        assert_eq!(score.title, "Independent import test");
        assert_eq!(score.composer, "Encore tests");
        assert_eq!(score.tempo, 96);
        assert_eq!(score.key_fifths, 1);
        assert_eq!(score.parts.len(), 1);
        let part = &score.parts[0];
        assert_eq!(part.name, "Timpani");
        assert_eq!(part.instrument, Instrument::Timpani);
        assert_eq!(part.clef, Clef::Treble);
        assert_eq!(part.measures[0].notes.len(), 2);
        assert_eq!(part.measures[0].notes[0].pitches[0].midi(), 60);
        assert!(part.measures[0].notes[0].tied);
        assert_eq!(part.measures[0].notes[1].start, 480);
        assert_eq!(part.measures[0].notes[1].ticks(), 1440);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn imports_real_finale27_fixture_with_expected_pitches_and_timing() {
        let bytes = include_bytes!("../../tests/fixtures/finale27-basic.musx");
        let result = parse(bytes).unwrap();
        let score = result.score;
        assert_eq!(score.tempo, 96);
        assert_eq!((score.time_numerator, score.time_denominator), (4, 4));
        assert_eq!(score.key_fifths, 0);
        assert_eq!(score.parts.len(), 2);
        assert!(score.parts.iter().all(|part| part.measures.len() == 1));
        let events = |part: &Part| {
            part.measures[0]
                .notes
                .iter()
                .map(|note| {
                    (
                        note.start,
                        note.ticks(),
                        note.pitches
                            .iter()
                            .map(|pitch| pitch.midi())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            events(&score.parts[0]),
            vec![
                (0, 480, vec![74]),
                (480, 480, vec![76]),
                (960, 480, vec![72]),
                (1440, 480, vec![])
            ]
        );
        assert_eq!(
            events(&score.parts[1]),
            vec![(0, 480, vec![]), (480, 960, vec![72]), (1440, 480, vec![])]
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("layout"))
        );
    }

    #[test]
    fn imports_dotted_sixteenth_and_model_maximum_tempo() {
        let xml = fixture()
            .replace("<dura>1024</dura>", "<dura>384</dura>")
            .replace(
                "<beatsPerMinute>96</beatsPerMinute>",
                "<beatsPerMinute>400</beatsPerMinute>",
            );
        let score = parse(&archive(&xml)).unwrap().score;
        assert_eq!(score.tempo, 400);
        let note = &score.parts[0].measures[0].notes[0];
        assert_eq!(note.duration, crate::model::Duration::Sixteenth);
        assert!(note.dotted);
        assert_eq!(note.ticks(), 180);
        assert_eq!(score.parts[0].measures[0].notes[1].start, 180);
    }

    #[test]
    fn cipher_matches_an_independent_real_musx_header() {
        // First sixteen score.dat bytes from musxdom's public slur.musx format
        // fixture. Its decoded gzip header is independent of our test encoder.
        let mut header = [
            22, 215, 83, 169, 169, 141, 245, 95, 167, 204, 60, 173, 151, 35, 6, 233,
        ];
        decode_score_data(&mut header);
        assert_eq!(&header[..3], &[0x1f, 0x8b, 0x08]);
        assert_eq!(&header[3..10], &[0, 0, 0, 0, 0, 0, 3]);
        let mut block = vec![0u8; 0x20000 + 16];
        decode_score_data(&mut block);
        assert_eq!(&block[..16], &block[0x20000..]);
    }

    #[test]
    fn standard_key_pitch_mapping_preserves_register_and_accidentals() {
        let fixture = fixture();
        let document = parse_xml(&fixture).unwrap();
        let note = document
            .descendants()
            .find(|node| node.has_tag_name((NS, "note")))
            .unwrap();
        assert_eq!(
            read_pitch(
                note,
                Key {
                    fifths: 1,
                    minor: false
                }
            )
            .unwrap()
            .name(),
            "C4"
        );
        assert_eq!(
            read_pitch(
                note,
                Key {
                    fifths: 1,
                    minor: true
                }
            )
            .unwrap()
            .name(),
            "A3"
        );
        assert_eq!(
            read_pitch(
                note,
                Key {
                    fifths: -1,
                    minor: false
                }
            )
            .unwrap()
            .name(),
            "B♭3"
        );
    }

    #[test]
    fn refuses_cycles_missing_links_and_unsupported_rhythms() {
        for changed in [
            fixture().replace("next=\"3\"", "next=\"40\""),
            fixture().replace("next=\"3\"", "next=\"12345\""),
            fixture().replace("<dura>1024</dura>", "<dura>16</dura>"),
            fixture().replace("<dura>1024</dura>", "<dura>1024</dura><tupletStart/>"),
            fixture().replace("<dura>1024</dura>", "<dura>1024</dura><v2/>"),
            fixture().replace("<dura>1024</dura>", "<dura>1024</dura><graceNote/>"),
        ] {
            assert!(parse(&archive(&changed)).is_err());
        }
    }

    #[test]
    fn empty_three_four_measure_uses_bar_length() {
        let xml = fixture()
            .replace("<beats>4</beats>", "<beats>3</beats>")
            .replace("<endEntry>3</endEntry>", "<endEntry>40</endEntry>")
            .replace("<dura>1024</dura><isNote/>", "<dura>4096</dura>");
        let result = parse(&archive(&xml)).unwrap();
        let notes = &result.score.parts[0].measures[0].notes;
        assert!(notes.iter().all(|note| note.pitches.is_empty()));
        assert_eq!(notes.iter().map(|note| note.ticks()).sum::<u32>(), 1440);
    }

    #[test]
    fn rejects_partial_chord_ties_and_key_changes() {
        let xml = fixture().replace(
            "<tieStart/></note>",
            "<tieStart/></note><note id=\"2\"><harmLev>-2</harmLev><harmAlt>0</harmAlt></note>",
        );
        assert!(parse(&archive(&xml)).unwrap_err().contains("partial-chord"));
        let xml = fixture().replace("<frameSpec cmper=\"9\"", "<measSpec cmper=\"2\"><beats>4</beats><divbeat>1024</divbeat><keySig><key>2</key></keySig></measSpec><frameSpec cmper=\"9\"");
        assert!(
            parse(&archive(&xml))
                .unwrap_err()
                .contains("key or meter changes")
        );
    }

    #[test]
    fn rejects_invalid_or_inflated_containers() {
        assert!(parse(b"not a zip").is_err());
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("other.dat", zip::write::SimpleFileOptions::default())
            .unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert!(parse(&bytes).unwrap_err().contains("score.dat"));
        let mut gzip = GzEncoder::new(Vec::new(), Compression::best());
        gzip.write_all(&vec![b' '; MAX_XML_BYTES + 1]).unwrap();
        let mut data = gzip.finish().unwrap();
        decode_score_data(&mut data);
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("score.dat", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&data).unwrap();
        assert!(
            parse(&archive.finish().unwrap().into_inner())
                .unwrap_err()
                .contains("size limit")
        );
    }

    #[test]
    fn optional_real_musx_fixture() {
        // Real third-party scores stay outside the distributed application. Set
        // this variable for interoperability QA against a licensed local file.
        let Some(path) = std::env::var_os("ENCORE_TEST_MUSX") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let result = parse(&bytes).unwrap();
        result.score.validate().unwrap();
        assert!(!result.score.parts.is_empty());
        assert!(result.score.parts.iter().any(|part| {
            part.measures
                .iter()
                .any(|measure| !measure.notes.is_empty())
        }));
    }
    #[test]
    fn imports_all_four_finale_layers_on_the_original_staff() {
        let xml = fixture().replace(
            "<frame1>9</frame1>",
            "<frame1>9</frame1><frame2>9</frame2><frame3>9</frame3><frame4>9</frame4>",
        );
        let score = parse(&archive(&xml)).unwrap().score;
        assert_eq!(score.parts.len(), 1);
        for layer in 0..4 {
            let notes = score.parts[0].measures[0]
                .notes
                .iter()
                .filter(|note| note.layer == layer)
                .collect::<Vec<_>>();
            assert_eq!(notes.len(), 2);
            assert_eq!(
                (
                    notes[0].start,
                    notes[0].ticks(),
                    notes[1].start,
                    notes[1].ticks()
                ),
                (0, 480, 480, 1440)
            );
            assert!(notes[0].tied);
        }
    }
}
