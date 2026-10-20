use super::{
    ImportResult, Warnings, child, finish_parts, initial_score, number, push_event, text, value,
};
use crate::model::{Clef, Instrument, MAX_MEASURES, MAX_PARTS, Measure, Part, Pitch, Step};
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
            if voices.len() > MAX_PARTS - parts.len() {
                return Err(format!(
                    "Separating MuseScore voices exceeds Encore's {MAX_PARTS}-staff limit."
                ));
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
                        if !(1..=32).contains(&meter.0) || ![1, 2, 4, 8, 16].contains(&meter.1) {
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
                    "Tuplet" | "endTuplet" => {
                        return Err(format!(
                            "{context}: tuplets are not supported yet; simplify them before importing."
                        ));
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
                for item in voice.children().filter(|n| n.is_element()) {
                    match item.tag_name().name() {
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
                            if child(item, "Tuplet").is_some() { return Err(format!("{context}: tuplets are not supported yet.")); }
                            let ticks = event_ticks(item, actual_ticks as u32)?;
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
                            let staccato = item.children().filter(|n| n.has_tag_name("Articulation")).filter_map(|n| value(n, "subtype")).any(|s| s.to_ascii_lowercase().contains("staccat"));
                            if item.children().filter(|n| n.has_tag_name("Articulation")).filter_map(|n| value(n, "subtype")).any(|s| !s.to_ascii_lowercase().contains("staccat")) { warnings.add("Articulations other than staccato are not retained in the editable score."); }
                            push_event(&mut notes, cursor as u32 + pickup_shift, ticks, pitches, velocity, staccato, tied.unwrap_or(false)).map_err(|e| format!("{context}: {e}"))?;
                            cursor += i64::from(ticks);
                            if item.descendants().any(|n| n.has_tag_name("Lyrics") || n.has_tag_name("Ornament") || n.has_tag_name("Tremolo")) { warnings.add("Lyrics, ornaments, and tremolo notation are not retained in the editable score."); }
                            if number::<i16>(item, "staffMove")?.unwrap_or(0) != 0 { warnings.add("Cross-staff notes remain on their source staff; sounding pitches are preserved."); }
                        }
                        "Spanner" => { if item.attribute("type") != Some("Tie") { warnings.add("Slurs, hairpins, pedals, and other expression lines are not retained."); } }
                        "InstrumentChange" | "StaffState" => warnings.add("Instrument changes use the staff's initial instrument for playback."),
                        "Harmony" | "FiguredBass" => warnings.add("Chord symbols and figured bass are not retained in the editable score."),
                        "RepeatMeasure" | "MeasureRepeat" => return Err("Measure-repeat shorthand is not supported yet. Expand those measures before importing.".into()),
                        "startRepeat" | "endRepeat" | "Jump" | "Marker" => warnings.add("Repeats, jumps, and alternate endings are imported in written order and play once."),
                        "SystemText" if child(item, "swing").is_some() => warnings.add("Swing playback settings are not retained; notes use their written durations."),
                        _ => (),
                    }
                }
                velocities.insert(voice_index, velocity);
                if !notes.is_empty() {
                    if !lanes.contains_key(&voice_index) && lanes.len() >= MAX_PARTS - parts.len() {
                        return Err(format!(
                            "Separating MuseScore voices exceeds Encore's {MAX_PARTS}-staff limit."
                        ));
                    }
                    lanes
                        .entry(voice_index)
                        .or_insert_with(|| vec![Measure::default(); measures.len()])
                        [measure_index]
                        .notes = notes;
                }
            }
            if child(*measure, "startRepeat").is_some() || child(*measure, "endRepeat").is_some() {
                warnings.add("Repeats, jumps, and alternate endings are imported in written order and play once.");
            }
            source_start = source_start
                .checked_add(actual_ticks)
                .ok_or("MuseScore measure position overflowed.")?;
        }
        if lanes.is_empty() {
            lanes.insert(0, vec![Measure::default(); measures.len()]);
        }
        let split = lanes.len() > 1;
        if split {
            warnings.add("Independent voices were separated into editable Encore staves.");
        }
        let same_name_staves = infos
            .values()
            .filter(|other| other.name == info.name)
            .count()
            > 1;
        for (voice, measures) in lanes {
            parts.push(Part {
                name: if split {
                    format!("{} · staff {id}, voice {}", info.name, voice + 1)
                } else if same_name_staves {
                    format!("{} · staff {id}", info.name)
                } else {
                    info.name.clone()
                },
                instrument: info.instrument,
                clef,
                measures,
                volume: info.volume,
                pan: info.pan,
                muted: info.muted,
                solo: false,
            });
        }
    }
    if initial_tempo.is_none() {
        // MuseScore's documented default when a score has no tempo marking:
        // https://handbook.musescore.org/text/tempo-markings
        warnings.add("No explicit tempo was found; playback uses MuseScore's default of 120 quarter notes per minute. Set the tempo to match your intended performance.");
    }
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
        "measure" => return Ok(measure_ticks),
        other => {
            return Err(format!(
                "MuseScore duration '{other}' is not supported yet; notes shorter than a sixteenth and tuplets cannot be imported exactly."
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

fn fraction_ticks(fraction: &str) -> Result<i64, String> {
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
