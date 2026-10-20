use super::annotations::{Annotations, SourceAnchor};
use super::{
    ImportResult, Warnings, child, finish_parts, initial_score, number, push_event,
    push_rhythmic_event, text, value,
};
use crate::model::{
    Articulation, Clef, Duration, Instrument, MAX_MEASURES, MAX_PARTS, Measure, Note, Part, Pitch,
    Step, Tuplet,
};
use roxmltree::{Document, Node};
use std::collections::BTreeMap;

#[derive(Clone)]
struct StaffInfo {
    name: String,
    instrument: Instrument,
    clef: Clef,
    volume: f32,
    pan: f32,
    muted: bool,
    transpose: i16,
}

pub(super) fn parse(document: &Document<'_>) -> Result<ImportResult, String> {
    let root = document.root_element();
    let major = root
        .attribute("version")
        .and_then(|v| v.split('.').next())
        .and_then(|v| v.parse::<u16>().ok())
        .ok_or("MuseScore XML is missing a valid version.")?;
    if !(2..=4).contains(&major) {
        return Err(format!(
            "MuseScore format version {major} is not supported yet. Export partwise MusicXML from MuseScore and open that file in Encore."
        ));
    }
    let source = child(root, "Score").ok_or("MuseScore file has no master Score.")?;
    let metadata = |key| {
        source
            .children()
            .find(|n| n.has_tag_name("metaTag") && n.attribute("name") == Some(key))
            .map(text)
            .unwrap_or_default()
    };
    let mut title = metadata("workTitle");
    let mut composer = metadata("composer");
    if title.trim().is_empty() {
        title = source
            .children()
            .filter(|n| n.has_tag_name("Staff"))
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("VBox"))
            .flat_map(|n| n.children())
            .find(|n| {
                n.has_tag_name("Text")
                    && value(*n, "style").is_some_and(|s| s.eq_ignore_ascii_case("title"))
            })
            .and_then(|n| child(n, "text"))
            .map(text)
            .unwrap_or_default();
    }
    if composer.trim().is_empty() {
        composer = source
            .children()
            .filter(|n| n.has_tag_name("Staff"))
            .flat_map(|n| n.children())
            .filter(|n| n.has_tag_name("VBox"))
            .flat_map(|n| n.children())
            .find(|n| {
                n.has_tag_name("Text")
                    && value(*n, "style").is_some_and(|s| s.eq_ignore_ascii_case("composer"))
            })
            .and_then(|n| child(n, "text"))
            .map(text)
            .unwrap_or_default();
    }
    let mut score = initial_score(title, composer);
    let mut warnings = Warnings::default();
    warnings.add("Encore re-engraves imported music; original page layout, fonts, beams, and custom engraving are not retained.");
    let division = number::<u32>(source, "Division")?.unwrap_or(480);
    if division == 0 || division > 1_000_000 {
        return Err("Invalid MuseScore tick division.".into());
    }
    let concert_pitch = child(source, "Style")
        .map(|s| number::<u8>(s, "concertPitch"))
        .transpose()?
        .flatten()
        .unwrap_or(0)
        != 0;
    let mut infos = BTreeMap::new();
    for definition in source.children().filter(|n| n.has_tag_name("Part")) {
        let instrument_node = child(definition, "Instrument");
        let channel = instrument_node.and_then(|n| child(n, "Channel"));
        if instrument_node.is_some_and(|n| {
            value(n, "useDrumset") == Some("1") || n.children().any(|n| n.has_tag_name("Drum"))
        }) {
            return Err("MuseScore drumsets and unpitched percussion are not supported yet. Pitched percussion such as timpani can be imported.".into());
        }
        let program = channel
            .and_then(|n| child(n, "program"))
            .and_then(|n| n.attribute("value"))
            .map(|s| {
                s.parse::<u8>()
                    .map_err(|_| "Invalid MuseScore MIDI program.")
            })
            .transpose()?
            .unwrap_or(0);
        let instrument =
            Instrument::from_gm_program(program).ok_or("MuseScore MIDI program must be 0–127.")?;
        let name = value(definition, "trackName")
            .filter(|s| !s.is_empty())
            .or_else(|| instrument_node.and_then(|n| value(n, "longName")))
            .unwrap_or(instrument.name())
            .to_owned();
        let transpose = instrument_node
            .map(|n| number::<i16>(n, "transposeChromatic"))
            .transpose()?
            .flatten()
            .unwrap_or(0);
        if !(-127..=127).contains(&transpose) {
            return Err("MuseScore transposition exceeds MIDI range.".into());
        }
        if transpose != 0 {
            warnings.add("Transposing MuseScore instruments use their stored concert pitches.");
        }
        let mut volume = 0.8;
        let mut pan = 0.0;
        if let Some(channel) = channel {
            for control in channel.children().filter(|n| n.has_tag_name("controller")) {
                let controller = control.attribute("ctrl").unwrap_or("");
                let raw = control
                    .attribute("value")
                    .unwrap_or("0")
                    .parse::<u8>()
                    .map_err(|_| "Invalid MuseScore MIDI controller.")?;
                if raw > 127 {
                    return Err("MuseScore MIDI controller value exceeds 127.".into());
                }
                if controller == "7" {
                    volume = f32::from(raw) / 127.0;
                }
                if controller == "10" {
                    pan = ((f32::from(raw) - 64.0) / 63.0).clamp(-1.0, 1.0);
                }
            }
        }
        let muted = channel
            .map(|n| number::<u8>(n, "mute"))
            .transpose()?
            .flatten()
            .unwrap_or(0)
            != 0;
        for (index, staff) in definition
            .children()
            .filter(|n| n.has_tag_name("Staff"))
            .enumerate()
        {
            if infos.len() >= MAX_PARTS {
                return Err(format!(
                    "MuseScore exceeds Encore's {MAX_PARTS}-staff limit."
                ));
            }
            let id = staff
                .attribute("id")
                .ok_or("MuseScore staff definition has no id.")?
                .to_owned();
            if child(staff, "StaffType")
                .and_then(|n| n.attribute("group"))
                .is_some_and(|s| s != "pitched")
            {
                return Err(
                    "MuseScore tablature and percussion staves are not supported yet.".into(),
                );
            }
            let clef = value(staff, "defaultConcertClef")
                .or_else(|| value(staff, "defaultClef"))
                .or_else(|| {
                    instrument_node.and_then(|n| {
                        n.children()
                            .filter(|n| n.has_tag_name("clef"))
                            .find(|n| {
                                n.attribute("staff").unwrap_or("1") == (index + 1).to_string()
                            })
                            .and_then(|n| n.text())
                    })
                });
            let clef = parse_clef(
                clef.unwrap_or("G"),
                instrument.default_clef(),
                &mut warnings,
            )?;
            if infos
                .insert(
                    id,
                    StaffInfo {
                        name: name.clone(),
                        instrument,
                        clef,
                        volume,
                        pan,
                        muted,
                        transpose,
                    },
                )
                .is_some()
            {
                return Err("MuseScore contains duplicate staff definitions.".into());
            }
        }
    }
    let mut initial_meter = None;
    let mut initial_key = None;
    let mut initial_tempo = None;
    let mut parts = Vec::new();
    let mut annotation_sets = Vec::new();
    let mut staff_ids = Vec::new();
    for staff in source.children().filter(|n| n.has_tag_name("Staff")) {
        if parts.len() >= MAX_PARTS {
            return Err(format!("Import exceeds Encore's {MAX_PARTS}-staff limit."));
        }
        let id = staff
            .attribute("id")
            .ok_or("MuseScore score staff has no id.")?;
        if staff_ids.contains(&id) {
            return Err("MuseScore contains duplicate score staves.".into());
        }
        staff_ids.push(id);
        let info = infos.get(id).cloned().unwrap_or_else(|| {
            warnings.add("A staff with no instrument definition was imported as piano.");
            StaffInfo {
                name: format!("Staff {id}"),
                instrument: Instrument::Piano,
                clef: Clef::Treble,
                volume: 0.8,
                pan: 0.0,
                muted: false,
                transpose: 0,
            }
        });
        let measures: Vec<_> = staff
            .children()
            .filter(|n| n.has_tag_name("Measure") && child(*n, "multiMeasureRest").is_none())
            .collect();
        if measures.is_empty() || measures.len() > MAX_MEASURES {
            return Err(format!(
                "MuseScore staff {id} must contain 1–{MAX_MEASURES} measures."
            ));
        }
        let mut annotations = Annotations::default();
        let source_staff = id
            .parse::<u32>()
            .map_err(|_| "MuseScore staff id must be numeric.")?;
        let mut annotation_mapping = BTreeMap::new();
        let mut lanes: BTreeMap<usize, Vec<Measure>> = BTreeMap::new();
        let mut clef = info.clef;
        let mut velocities = BTreeMap::new();
        let mut source_start = 0_i64;
        for (measure_index, measure) in measures.iter().enumerate() {
            let context = format!("{}, measure {}", info.name, measure_index + 1);
            let voices: Vec<_> = measure
                .children()
                .filter(|n| n.has_tag_name("voice"))
                .collect();
            if voices.len() > 4 {
                return Err("MuseScore staff has more than four independent voices; Encore supports four layers per staff.".into());
            }
            let voices = if voices.is_empty() {
                vec![*measure]
            } else {
                voices
            };
            for item in voices
                .iter()
                .flat_map(|n| n.children())
                .filter(|n| n.is_element())
            {
                match item.tag_name().name() {
                    "TimeSig" => {
                        let meter = (
                            number::<u8>(item, "sigN")?
                                .ok_or("MuseScore time signature has no numerator.")?,
                            number::<u8>(item, "sigD")?
                                .ok_or("MuseScore time signature has no denominator.")?,
                        );
                        if !(1..=32).contains(&meter.0)
                            || ![1, 2, 4, 8, 16, 32, 64, 128].contains(&meter.1)
                        {
                            return Err("Unsupported MuseScore time signature.".into());
                        }
                        if initial_meter.is_some_and(|first| first != meter) {
                            return Err("This MuseScore score changes time signature or uses simultaneous different meters. Encore currently requires one time signature for the whole score.".into());
                        }
                        if initial_meter.is_none() {
                            initial_meter = Some(meter);
                            score.time_numerator = meter.0;
                            score.time_denominator = meter.1;
                        }
                        if child(item, "stretch").is_some() {
                            return Err("Locally stretched MuseScore time signatures are not supported yet.".into());
                        }
                    }
                    "KeySig" => {
                        if child(item, "custom").is_some()
                            || child(item, "CustDef").is_some()
                            || child(item, "KeySym").is_some()
                        {
                            return Err(
                                "Custom MuseScore key signatures are not supported yet.".into()
                            );
                        }
                        let mut key = number::<i8>(item, "accidental")?.unwrap_or(0);
                        if !(-7..=7).contains(&key) {
                            return Err("Unsupported MuseScore key signature.".into());
                        }
                        if !concert_pitch && info.transpose != 0 {
                            key = concert_key(key, info.transpose);
                        }
                        if initial_key.is_some_and(|first| first != key) {
                            warnings.add("Key signature changes or different staff keys use the initial concert key; explicit note pitches are preserved.");
                        } else if initial_key.is_none() {
                            initial_key = Some(key);
                            score.key_fifths = key;
                        }
                    }
                    _ => (),
                }
            }
            let measure_ticks = score.measure_ticks();
            let actual_ticks = measure
                .attribute("len")
                .map(fraction_ticks)
                .transpose()?
                .unwrap_or(i64::from(measure_ticks));
            if actual_ticks <= 0 || actual_ticks > i64::from(measure_ticks) {
                return Err(format!(
                    "{context}: irregular measures longer than the time signature are not supported."
                ));
            }
            let pickup_shift = if measure_index == 0 && actual_ticks < i64::from(measure_ticks) {
                warnings.add("The pickup is right-aligned in a full first measure, preserving timing into the next measure.");
                measure_ticks - actual_ticks as u32
            } else if actual_ticks < i64::from(measure_ticks) {
                return Err(format!(
                    "{context}: irregular measure lengths after the pickup are not supported yet."
                ));
            } else {
                0
            };
            for (voice_index, voice) in voices.iter().enumerate() {
                let mut cursor = 0_i64;
                let mut velocity = *velocities.get(&voice_index).unwrap_or(&80);
                let mut notes = Vec::new();
                let mut tuplets = BTreeMap::new();
                let mut active_tuplet = None;
                for item in voice.children().filter(|n| n.is_element()) {
                    match item.tag_name().name() {
                        "Tuplet" => {
                            if child(item, "Tuplet").is_some() { return Err(format!("{context}: nested MuseScore tuplets are not supported yet.")); }
                            let ratio = Tuplet {
                                actual: number::<u8>(item, "actualNotes")?.ok_or("MuseScore tuplet is missing its actual-note count.")?,
                                normal: number::<u8>(item, "normalNotes")?.ok_or("MuseScore tuplet is missing its normal-note count.")?,
                            };
                            ratio.validate()?;
                            if let Some(id) = item.attribute("id") {
                                if tuplets.insert(id.to_owned(), ratio).is_some() { return Err("Duplicate MuseScore tuplet identifier.".into()); }
                            } else {
                                if active_tuplet.is_some() { return Err(format!("{context}: nested MuseScore tuplets are not supported yet.")); }
                                active_tuplet = Some(ratio);
                            }
                        }
                        "endTuplet" => {
                            if active_tuplet.take().is_none() { return Err("MuseScore tuplet end has no matching start.".into()); }
                        }
                        "tick" => {
                            let raw = item.text().unwrap_or("").trim().parse::<i64>().map_err(|_| "Invalid MuseScore tick position.")?;
                            let scaled = raw.checked_mul(480).ok_or("MuseScore tick overflowed.")?;
                            if scaled % i64::from(division) != 0 { return Err("MuseScore tick position is not on Encore's timing grid.".into()); }
                            cursor = (scaled / i64::from(division)).checked_sub(source_start).ok_or("MuseScore tick overflowed.")?;
                        }
                        "location" => {
                            if number::<i32>(item, "measures")?.unwrap_or(0) != 0 { return Err("Cross-measure MuseScore location changes are not supported yet.".into()); }
                            cursor = cursor.checked_add(value(item, "fractions").map(fraction_ticks).transpose()?.unwrap_or(0)).ok_or("MuseScore location overflowed.")?;
                        }
                        "Tempo" => {
                            let bpm = number::<f64>(item, "tempo")?.ok_or("MuseScore tempo is missing a value.")? * 60.0;
                            if !bpm.is_finite() || !(20.0..=400.0).contains(&bpm) { return Err("Imported tempo must be 20–400 BPM.".into()); }
                            let bpm = bpm.round() as u16;
                            if initial_tempo.is_some_and(|first| first != bpm) { warnings.add("Tempo changes are not retained; playback uses the first tempo."); }
                            else if initial_tempo.is_none() { initial_tempo = Some(bpm); score.tempo = bpm; }
                        }
                        "Dynamic" => {
                            let explicit = number::<i16>(item, "velocity")?.filter(|v| *v > 0);
                            velocity = if let Some(v) = explicit { if v > 127 { return Err("MuseScore dynamic exceeds MIDI velocity 127.".into()); } v as u8 }
                                else { super::musicxml::dynamic_velocity(value(item, "subtype").unwrap_or("mf")) };
                        }
                        "Clef" => {
                            let next = parse_clef(value(item, "concertClefType").or_else(|| value(item, "subtype")).unwrap_or("G"), info.instrument.default_clef(), &mut warnings)?;
                            if measure_index == 0 && cursor == 0 { clef = next; } else if next != clef { warnings.add("Clef changes use the initial clef; sounding pitches are preserved."); }
                        }
                        "Chord" | "Rest" => {
                            if item.children().any(|n| matches!(n.tag_name().name(), "acciaccatura" | "appoggiatura" | "grace4" | "grace16" | "grace32" | "grace8after" | "grace16after" | "grace32after")) { warnings.add("Grace notes are omitted; main-note timing is preserved."); continue; }
                            let tuplet = if let Some(id) = value(item, "Tuplet") {
                                if active_tuplet.is_some() { return Err("Nested MuseScore tuplets are not supported yet.".into()); }
                                Some(*tuplets.get(id).ok_or("MuseScore note references a missing tuplet.")?)
                            } else { active_tuplet };
                            let dots = number::<u8>(item, "dots")?.unwrap_or(0);
                            let rhythm = value(item, "durationType").and_then(Duration::from_musicxml_type).filter(|_| dots <= 1);
                            let dotted = dots == 1;
                            let ticks = if let Some(ratio) = tuplet {
                                let duration = rhythm.ok_or("Tuplets require a supported explicit note value and at most one dot.")?;
                                if child(item, "duration").is_none() {
                                    // Apply dots and ratio together, avoiding a fractional
                                    // intermediate for exact dotted 128th triplets.
                                    Note { start: 0, layer: 0, duration, dotted, tuplet,
                                        pitches: Vec::new(), velocity: 64, staccato: false, articulations: Vec::new(), tied: false }.checked_ticks()?
                                } else {
                                    let nominal_ticks = event_ticks(item, actual_ticks as u32)?;
                                    let numerator = nominal_ticks.checked_mul(u32::from(ratio.normal)).ok_or("MuseScore tuplet duration overflowed.")?;
                                    if !numerator.is_multiple_of(u32::from(ratio.actual)) { return Err("MuseScore tuplet cannot be represented exactly on Encore's 480-tick grid.".into()); }
                                    numerator / u32::from(ratio.actual)
                                }
                            } else { event_ticks(item, actual_ticks as u32)? };
                            if cursor < 0 || cursor.checked_add(i64::from(ticks)).is_none_or(|end| end > actual_ticks) { return Err(format!("{context}: notes extend outside the measure.")); }
                            let note_nodes: Vec<_> = item.children().filter(|n| n.has_tag_name("Note")).collect();
                            if item.has_tag_name("Chord") && note_nodes.is_empty() { return Err(format!("{context}: chord has no notes.")); }
                            let mut pitches = Vec::new();
                            let mut tied = None;
                            for note in &note_nodes {
                                let midi = number::<u8>(*note, "pitch")?.ok_or("MuseScore note has no MIDI pitch.")?;
                                if midi > 127 { return Err("MuseScore pitch exceeds MIDI 127.".into()); }
                                pitches.push(spelled_pitch(midi, number::<i32>(*note, "tpc")?));
                                let outgoing = child(*note, "Tie").is_some() || note.children().any(|n| n.has_tag_name("Spanner") && n.attribute("type") == Some("Tie") && child(n, "next").is_some());
                                if tied.is_some_and(|prior| prior != outgoing) { return Err(format!("{context}: partially tied chords are not supported yet; separate pitches into voices before importing.")); }
                                tied = Some(outgoing);
                                if child(*note, "Events").is_some() || child(*note, "velocity").is_some() || child(*note, "tuning").is_some() { warnings.add("Custom per-note playback events, tuning, and velocity overrides are not retained; written dynamics are used."); }
                            }
                            annotations.muse(item,SourceAnchor{staff:source_staff,voice:voice_index.to_string(),measure:measure_index,tick:cursor as u32+pickup_shift},division,&mut warnings)?;
                            let (staccato, articulations) = read_articulations(item, &mut warnings);
                            if let Some(duration) = rhythm {
                                push_rhythmic_event(&mut notes, Note { start: cursor as u32 + pickup_shift,
                                    duration, dotted, tuplet, layer: 0, pitches, velocity, staccato, articulations, tied: tied.unwrap_or(false) }, ticks)
                            } else {
                                let first_piece = notes.len();
                                push_event(&mut notes, cursor as u32 + pickup_shift, ticks, pitches, velocity, staccato, tied.unwrap_or(false))
                                    .map(|()| super::musicxml::decorate_split_event(&mut notes[first_piece..], articulations, &mut warnings))
                            }.map_err(|e| format!("{context}: {e}"))?;
                            cursor += i64::from(ticks);
                            if item.descendants().any(|n| n.has_tag_name("Ornament") || n.has_tag_name("Tremolo")) { warnings.add("Ornaments and tremolo notation are not retained in the editable score."); }
                            if number::<i16>(item, "staffMove")?.unwrap_or(0) != 0 { warnings.add("Cross-staff notes remain on their source staff; sounding pitches are preserved."); }
                        }
                        "Slur" | "endSpanner" => annotations.muse_spanner(item,SourceAnchor{staff:source_staff,voice:voice_index.to_string(),measure:measure_index,tick:cursor.try_into().map_err(|_|"Invalid MuseScore slur location.")?},&mut warnings)?,
                        "Spanner" if item.attribute("type")==Some("Slur") => annotations.muse_spanner(item,SourceAnchor{staff:source_staff,voice:voice_index.to_string(),measure:measure_index,tick:cursor.try_into().map_err(|_|"Invalid MuseScore slur location.")?},&mut warnings)?,
                        "Spanner" => { if item.attribute("type") != Some("Tie") && item.attribute("type") != Some("Volta") { warnings.add("Hairpins, pedals, and other expression lines are not retained."); } }
                        "InstrumentChange" | "StaffState" => warnings.add("Instrument changes use the staff's initial instrument for playback."),
                        "Harmony" | "FiguredBass" => warnings.add("Chord symbols and figured bass are not retained in the editable score."),
                        "RepeatMeasure" | "MeasureRepeat" => return Err("Measure-repeat shorthand is not supported yet. Expand those measures before importing.".into()),

                        "SystemText" if child(item, "swing").is_some() => warnings.add("Swing playback settings are not retained; notes use their written durations."),
                        _ => (),
                    }
                }
                if active_tuplet.is_some() {
                    return Err(format!(
                        "{context}: MuseScore tuplet is missing its end marker."
                    ));
                }
                velocities.insert(voice_index, velocity);
                if !notes.is_empty() {
                    if !lanes.contains_key(&voice_index) && lanes.len() >= 4 {
                        return Err("MuseScore staff has more than four independent voices; Encore supports four layers per staff.".into());
                    }
                    lanes
                        .entry(voice_index)
                        .or_insert_with(|| vec![Measure::default(); measures.len()])
                        [measure_index]
                        .notes = notes;
                }
            }
            source_start = source_start
                .checked_add(actual_ticks)
                .ok_or("MuseScore measure position overflowed.")?;
        }
        if lanes.is_empty() {
            lanes.insert(0, vec![Measure::default(); measures.len()]);
        }
        let repeat_map = crate::repeats::parse_musescore(
            staff,
            measures.len(),
            division,
            score.measure_ticks(),
        )?;
        if !repeat_map.is_empty() {
            if !score.notation.repeats.is_empty() && score.notation.repeats != repeat_map {
                return Err("MuseScore staves disagree on repeat navigation.".into());
            }
            score.notation.repeats = repeat_map;
        }
        let navigation = crate::repeats::parse_musescore_navigation(staff, measures.len())?;
        if !navigation.is_empty() {
            if !score.notation.navigation.is_empty() && score.notation.navigation != navigation {
                return Err("MuseScore staves disagree on navigation instructions.".into());
            }
            score.notation.navigation = navigation;
        }
        let mut combined = vec![Measure::default(); measures.len()];
        if lanes.len() > 1 {
            warnings.add("Independent MuseScore voices are preserved as rhythmic layers on their source staff.");
        }
        for (voice, measures) in lanes {
            annotation_mapping.insert(
                (source_staff, voice.to_string()),
                (parts.len(), voice as u8),
            );
            if voice > 3 {
                return Err("MuseScore staff has more than four independent voices.".into());
            }
            for (destination, source) in combined.iter_mut().zip(measures) {
                destination
                    .notes
                    .extend(source.notes.into_iter().map(|mut note| {
                        note.layer = voice as u8;
                        note
                    }));
            }
        }
        let same_name_staves = infos
            .values()
            .filter(|other| other.name == info.name)
            .count()
            > 1;
        parts.push(Part {
            name: if same_name_staves {
                format!("{} · staff {id}", info.name)
            } else {
                info.name.clone()
            },
            instrument: info.instrument,
            clef,
            measures: combined,
            volume: info.volume,
            pan: info.pan,
            muted: info.muted,
            solo: false,
        });
        annotation_sets.push((annotations, annotation_mapping));
    }
    if initial_tempo.is_none() {
        // MuseScore's documented default when a score has no tempo marking:
        // https://handbook.musescore.org/text/tempo-markings
        warnings.add("No explicit tempo was found; playback uses MuseScore's default of 120 quarter notes per minute. Set the tempo to match your intended performance.");
    }
    score.parts = parts;
    for (annotations, mapping) in annotation_sets {
        annotations.finish(&mut score, &mapping, &mut warnings)?;
    }
    let parts = std::mem::take(&mut score.parts);
    finish_parts(score, parts, warnings)
}

fn parse_clef(name: &str, fallback: Clef, warnings: &mut Warnings) -> Result<Clef, String> {
    match name.trim() {
        "G" | "0" => Ok(Clef::Treble),
        "F" | "4" => Ok(Clef::Bass),
        "C3" => Ok(Clef::Alto),
        "PERC" | "PERC2" | "TAB" | "TAB4" => {
            Err("MuseScore percussion and tablature clefs are not supported yet.".into())
        }
        _ => {
            warnings.add("An unsupported clef was replaced by the instrument's default clef; sounding pitches are preserved.");
            Ok(fallback)
        }
    }
}

// MuseScore stores an Articulation/subtype as its SMuFL symbol name. Above
// and Below encode placement, which Encore currently recomputes on engraving.
// https://github.com/musescore/MuseScore/blob/v3.6.2/libmscore/articulation.cpp
fn read_articulations(chord: Node<'_, '_>, warnings: &mut Warnings) -> (bool, Vec<Articulation>) {
    let mut staccato = false;
    let mut articulations = Vec::new();
    for marking in chord
        .children()
        .filter(|node| node.has_tag_name("Articulation"))
    {
        let subtype = value(marking, "subtype").unwrap_or("unknown");
        let articulation = match subtype {
            "articStaccatoAbove" | "articStaccatoBelow" | "staccato" => {
                staccato = true;
                continue;
            }
            "articAccentAbove" | "articAccentBelow" | "sforzato" => Articulation::Accent,
            "articMarcatoAbove" | "articMarcatoBelow" | "marcato" => Articulation::Marcato,
            "articTenutoAbove" | "articTenutoBelow" | "tenuto" => Articulation::Tenuto,
            "articStaccatissimoAbove" | "articStaccatissimoBelow" | "staccatissimo" => {
                Articulation::Staccatissimo
            }
            unsupported => {
                warnings.add(format!("MuseScore articulation '{unsupported}' is not retained; supported markings on the same chord are preserved."));
                continue;
            }
        };
        if !articulations.contains(&articulation) {
            articulations.push(articulation);
        }
    }
    (staccato, articulations)
}

fn event_ticks(node: Node<'_, '_>, measure_ticks: u32) -> Result<u32, String> {
    if let Some(duration) = value(node, "duration") {
        let ticks = fraction_ticks(duration)?;
        if ticks <= 0 || ticks > 61_440 {
            return Err("Invalid MuseScore duration fraction.".into());
        }
        return Ok(ticks as u32);
    }
    let ticks: u32 = match value(node, "durationType").unwrap_or("quarter") {
        "long" => 7680,
        "breve" => 3840,
        "whole" => 1920,
        "half" => 960,
        "quarter" => 480,
        "eighth" => 240,
        "16th" => 120,
        "32nd" => 60,
        "64th" => 30,
        "128th" => 15,
        "measure" => return Ok(measure_ticks),
        other => {
            return Err(format!(
                "MuseScore duration '{other}' is not supported yet; values through 128th notes are supported."
            ));
        }
    };
    let dots = number::<u8>(node, "dots")?.unwrap_or(0);
    if dots > 3 {
        return Err("MuseScore notes with more than three dots are not supported yet.".into());
    }
    let divisor = 1_u32 << dots;
    let numerator = ticks * (2 * divisor - 1);
    if !numerator.is_multiple_of(divisor) {
        return Err("MuseScore dotted duration is not exactly representable.".into());
    }
    Ok(numerator / divisor)
}

pub(super) fn fraction_ticks(fraction: &str) -> Result<i64, String> {
    let (numerator, denominator) = fraction
        .trim()
        .split_once('/')
        .ok_or("Invalid MuseScore duration fraction.")?;
    let numerator = numerator
        .parse::<i64>()
        .map_err(|_| "Invalid MuseScore fraction numerator.")?;
    let denominator = denominator
        .parse::<i64>()
        .map_err(|_| "Invalid MuseScore fraction denominator.")?;
    let scaled = numerator
        .checked_mul(1920)
        .ok_or("MuseScore fraction overflowed.")?;
    if denominator <= 0 || scaled % denominator != 0 {
        return Err("MuseScore fraction is not on Encore's 480-tick grid.".into());
    }
    Ok(scaled / denominator)
}

fn spelled_pitch(midi: u8, tpc: Option<i32>) -> Pitch {
    let Some(tpc) = tpc.filter(|tpc| (-1..=33).contains(tpc)) else {
        return Pitch::from_midi(midi);
    };
    let (step, natural) = [
        (Step::F, 13),
        (Step::C, 14),
        (Step::G, 15),
        (Step::D, 16),
        (Step::A, 17),
        (Step::E, 18),
        (Step::B, 19),
    ][(tpc - 13).rem_euclid(7) as usize];
    let alter = (tpc - natural) / 7;
    let octave_base = i32::from(midi) - step.semitone() - alter;
    if octave_base.rem_euclid(12) != 0 || !(-2..=2).contains(&alter) {
        return Pitch::from_midi(midi);
    }
    Pitch {
        step,
        octave: (octave_base / 12 - 1) as i8,
        alter: alter as i8,
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

#[cfg(test)]
mod articulation_tests {
    use super::*;

    fn import(chords: &str, version: &str) -> ImportResult {
        let xml = format!(
            "<museScore version='{version}'><Score><Staff id='1'><Measure><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig>{chords}</voice></Measure></Staff></Score></museScore>"
        );
        let document = Document::parse(&xml).unwrap();
        parse(&document).unwrap()
    }

    #[test]
    fn native_markings_preserve_above_below_variants_and_distinguish_staccatissimo() {
        for version in ["2.06", "3.02", "4.30"] {
            let imported = import(
                "<Chord><durationType>quarter</durationType><Articulation><subtype>articAccentAbove</subtype></Articulation><Articulation><subtype>articAccentBelow</subtype></Articulation><Articulation><subtype>articMarcatoBelow</subtype></Articulation><Articulation><subtype>articTenutoAbove</subtype></Articulation><Articulation><subtype>articStaccatoBelow</subtype></Articulation><Articulation><subtype>ornamentTrill</subtype></Articulation><Note><pitch>60</pitch></Note><Note><pitch>64</pitch></Note></Chord>
                 <Chord><durationType>quarter</durationType><Articulation><subtype>articStaccatissimoBelow</subtype></Articulation><Note><pitch>62</pitch></Note></Chord>",
                version,
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
            assert!(!notes[1].staccato);
            assert_eq!(notes[1].articulations, vec![Articulation::Staccatissimo]);
            assert!(
                imported
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("'ornamentTrill'"))
            );
            assert!(
                !imported
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("'articAccentAbove'"))
            );
        }
    }

    #[test]
    fn split_native_value_keeps_attack_and_release_markings_on_correct_pieces() {
        let imported = import(
            "<Chord><durationType>quarter</durationType><dots>2</dots><Articulation><subtype>articMarcatoAbove</subtype></Articulation><Articulation><subtype>articStaccatissimoAbove</subtype></Articulation><Note><pitch>60</pitch></Note></Chord>",
            "4.30",
        );
        let notes = &imported.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes.iter().map(Note::ticks).sum::<u32>(), 840);
        assert_eq!(notes[0].articulations, vec![Articulation::Marcato]);
        assert!(notes[0].tied);
        assert_eq!(notes[1].articulations, vec![Articulation::Staccatissimo]);
        assert!(!notes[1].tied);
        assert!(imported.warnings.iter().any(|warning| warning == "Marcato on a split tied duration retains its attack accent; the tie continuation takes precedence over its detached release."));
    }
}
