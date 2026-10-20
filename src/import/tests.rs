use super::*;
use crate::model::{Clef, Instrument, Step};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

const MUSICXML: &str = include_str!("../../tests/fixtures/import-musicxml.musicxml");
const MUSESCORE: &str = include_str!("../../tests/fixtures/import-musescore.mscx");
static SERIAL: AtomicUsize = AtomicUsize::new(0);

fn musicxml(text: &str) -> Result<ImportResult, String> {
    let document = parse_xml(text)?;
    musicxml::parse(&document)
}
fn musescore(text: &str) -> Result<ImportResult, String> {
    let document = parse_xml(text)?;
    musescore::parse(&document)
}
fn simple_xml(notes: &str) -> String {
    format!(
        "<score-partwise><part-list><score-part id='P1'><part-name>Test</part-name></score-part></part-list><part id='P1'><measure number='1'><attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>{notes}</measure></part></score-partwise>"
    )
}
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn load_bytes(extension: &str, bytes: &[u8]) -> Result<ImportResult, String> {
    let path = std::env::temp_dir().join(format!(
        "encore-import-{}-{}.{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed),
        extension
    ));
    std::fs::write(&path, bytes).unwrap();
    let result = load(&path);
    std::fs::remove_file(path).unwrap();
    result
}

#[test]
fn independent_musicxml_preserves_staff_voice_chord_and_playback_data() {
    let imported = musicxml(MUSICXML).unwrap();
    let score = imported.score;
    score.validate().unwrap();
    assert_eq!(score.title, "Two staves & three voices");
    assert_eq!(score.composer, "Encore import fixture");
    assert_eq!(
        (
            score.tempo,
            score.key_fifths,
            score.measure_count(),
            score.parts.len()
        ),
        (96, 2, 2, 3)
    );
    let rh = &score.parts[0];
    assert_eq!(rh.instrument, Instrument::Piano);
    assert_eq!(rh.volume, 0.75);
    assert_eq!(rh.pan, -0.2);
    assert_eq!(
        rh.measures[0].notes[0]
            .pitches
            .iter()
            .map(|p| p.midi())
            .collect::<Vec<_>>(),
        vec![60, 64]
    );
    assert_eq!(rh.measures[0].notes[0].velocity, 90);
    assert!(rh.measures[0].notes[0].staccato);
    assert_eq!(rh.measures[0].notes[1].start, 480);
    assert_eq!(rh.measures[0].notes[1].ticks(), 720);
    assert_eq!(rh.measures[0].notes[1].pitches[0].alter, 1);
    assert_eq!(rh.measures[0].notes[1].velocity, 48);
    assert!(rh.measures[0].notes[2].pitches.is_empty());
    assert!(rh.measures[0].notes[3].tied);
    assert!(!rh.measures[1].notes[0].tied);
    assert_eq!(score.parts[1].measures[0].notes[0].pitches[0].midi(), 55);
    assert_eq!(score.parts[2].clef, Clef::Bass);
    assert_eq!(score.parts[2].measures[0].notes[0].pitches[0].midi(), 40);
    assert!(
        imported
            .warnings
            .iter()
            .any(|warning| warning.contains("voices"))
    );
}

#[test]
fn independent_musescore_preserves_voices_spelling_ties_and_mixer() {
    let imported = musescore(MUSESCORE).unwrap();
    let score = imported.score;
    score.validate().unwrap();
    assert_eq!(score.title, "Native MuseScore fixture");
    assert_eq!(
        (
            score.tempo,
            score.key_fifths,
            score.measure_count(),
            score.parts.len()
        ),
        (90, -1, 2, 3)
    );
    let rh = &score.parts[0];
    assert_eq!(
        rh.measures[0].notes[0].pitches[1],
        Pitch {
            step: Step::E,
            octave: 4,
            alter: -1
        }
    );
    assert!(rh.measures[0].notes[0].tied);
    assert!(rh.measures[0].notes[0].staccato);
    assert!(!rh.measures[0].notes[1].tied);
    assert_eq!(rh.measures[0].notes[1].ticks(), 1440);
    assert_eq!(rh.measures[0].notes[0].velocity, 48);
    assert!((rh.volume - 100.0 / 127.0).abs() < 0.00001);
    assert!((rh.pan + 16.0 / 63.0).abs() < 0.00001);
    assert_eq!(score.parts[1].measures[0].notes[0].pitches[0].midi(), 48);
    assert_eq!(score.parts[2].clef, Clef::Bass);
    assert!(score.parts[2].measures[0].notes[0].pitches.is_empty());
}

#[test]
fn musicxml_transposition_changes_sounding_pitch_and_concert_key() {
    let xml = simple_xml(
        "<attributes><key><fifths>2</fifths></key><transpose><diatonic>-1</diatonic><chromatic>-2</chromatic></transpose></attributes><note><pitch><step>C</step><octave>5</octave></pitch><duration>4</duration></note>",
    );
    let imported = musicxml(&xml).unwrap();
    assert_eq!(
        imported.score.parts[0].measures[0].notes[0].pitches[0].midi(),
        70
    );
    assert_eq!(imported.score.key_fifths, 0);
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("concert pitch"))
    );
}

#[test]
fn musicxml_without_midi_program_reports_inference_or_fallback() {
    let source = simple_xml("<note><rest/><duration>16</duration></note>");
    let violin = musicxml(&source.replace(
        "<part-name>Test</part-name>",
        "<part-name>Violin</part-name>",
    ))
    .unwrap();
    assert_eq!(violin.score.parts[0].instrument, Instrument::Violin);
    assert!(
        violin
            .warnings
            .iter()
            .any(|w| w.contains("inferred from its name"))
    );
    let unknown = musicxml(&source).unwrap();
    assert_eq!(unknown.score.parts[0].instrument, Instrument::Piano);
    assert_eq!(unknown.score.tempo, 120);
    assert!(
        unknown
            .warnings
            .iter()
            .any(|w| w.contains("No explicit tempo") && w.contains("120"))
    );
    assert!(unknown.warnings.iter().any(|w| w.contains("fallback")));
}

#[test]
fn musescore_stored_pitch_is_already_concert_pitch() {
    let xml = MUSESCORE.replace("<instrumentId>keyboard.piano</instrumentId>", "<instrumentId>wind.reed.clarinet</instrumentId><transposeChromatic>-2</transposeChromatic>").replace("<concertPitch>1</concertPitch>", "<concertPitch>0</concertPitch>");
    let imported = musescore(&xml).unwrap();
    assert_eq!(
        imported.score.parts[0].measures[0].notes[0].pitches[0].midi(),
        60
    );
    assert_eq!(imported.score.key_fifths, -3);
}

#[test]
fn compressed_musicxml_and_musescore_follow_container_manifests() {
    for (extension, name, source, expected) in [
        (
            "mxl",
            "scores/main.musicxml",
            MUSICXML,
            "Two staves & three voices",
        ),
        ("mscz", "Native.mscx", MUSESCORE, "Native MuseScore fixture"),
    ] {
        let manifest =
            format!("<container><rootfiles><rootfile full-path='{name}'/></rootfiles></container>");
        let bytes = archive(&[
            ("META-INF/container.xml", manifest.as_bytes()),
            (name, source.as_bytes()),
            ("thumbnail.png", b"unused thumbnail"),
        ]);
        let imported = load_bytes(extension, &bytes).unwrap();
        assert_eq!(imported.score.title, expected);
    }
}

#[test]
fn archive_fallback_accepts_single_score_and_rejects_ambiguous_paths() {
    let one = archive(&[("score.mscx", MUSESCORE.as_bytes())]);
    assert!(load_bytes("mscz", &one).is_ok());
    let many = archive(&[
        ("one.mscx", MUSESCORE.as_bytes()),
        ("two.mscx", MUSESCORE.as_bytes()),
    ]);
    assert!(load_bytes("mscz", &many).unwrap_err().contains("multiple"));
    let traversal = archive(&[("../score.mscx", MUSESCORE.as_bytes())]);
    assert!(
        load_bytes("mscz", &traversal)
            .unwrap_err()
            .contains("unsafe path")
    );
    let absolute = archive(&[("/score.mscx", MUSESCORE.as_bytes())]);
    assert!(
        load_bytes("mscz", &absolute)
            .unwrap_err()
            .contains("unsafe path")
    );
}

#[test]
fn invalid_manifest_and_non_zip_input_are_actionable() {
    let bytes = archive(&[
        (
            "META-INF/container.xml",
            b"<container><rootfile full-path='../other.mscx'/></container>",
        ),
        ("score.mscx", MUSESCORE.as_bytes()),
    ]);
    assert!(load_bytes("mscz", &bytes).unwrap_err().contains("manifest"));
    assert!(
        load_bytes("mxl", b"not a ZIP archive")
            .unwrap_err()
            .contains("ZIP")
    );
}

#[test]
fn archive_and_xml_size_limits_apply_before_parsing() {
    assert!(
        parse_xml(&" ".repeat(MAX_XML_BYTES + 1))
            .unwrap_err()
            .contains("16 MiB")
    );
    let oversized = vec![b' '; MAX_XML_BYTES + 1];
    let bytes = archive(&[("score.mscx", &oversized)]);
    assert!(load_bytes("mscz", &bytes).unwrap_err().contains("16 MiB"));
}

#[test]
fn external_musicxml_doctype_is_read_without_entity_resolution() {
    assert!(musicxml(MUSICXML).is_ok());
    let entity = "<!DOCTYPE score-partwise [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]><score-partwise>&secret;</score-partwise>";
    assert!(parse_xml(entity).unwrap_err().contains("entities"));
    let subset = "<!DOCTYPE score-partwise [ ]><score-partwise/>";
    assert!(parse_xml(subset).unwrap_err().contains("DTD subsets"));
    assert!(parse_xml("<score-partwise>&missing;</score-partwise>").is_err());
}

#[test]
fn unsupported_rhythms_never_round_or_drop_notes_silently() {
    let tuplet = simple_xml(
        "<note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification></note>",
    );
    assert!(musicxml(&tuplet).unwrap_err().contains("tuplets"));
    let unquantized = simple_xml("<note><rest/><duration>0.5</duration></note>");
    assert!(
        musicxml(&unquantized)
            .unwrap_err()
            .contains("cannot be represented exactly")
    );
    let mut notes = Vec::new();
    assert!(push_event(&mut notes, 0, 160, vec![], 64, false, false).is_err());
    assert!(notes.is_empty());
    let muse = MUSESCORE.replace(
        "<durationType>quarter</durationType>",
        "<durationType>32nd</durationType>",
    );
    assert!(
        musescore(&muse)
            .unwrap_err()
            .contains("shorter than a sixteenth")
    );
}

#[test]
fn time_changes_and_microtones_are_rejected_with_explanations() {
    let xml = MUSICXML.replace("<measure number=\"2\">", "<measure number=\"2\"><attributes><time><beats>3</beats><beat-type>4</beat-type></time></attributes>");
    assert!(
        musicxml(&xml)
            .unwrap_err()
            .contains("changes time signature")
    );
    let muse = MUSESCORE.replace(
        "<Measure><voice><Rest>",
        "<Measure><voice><TimeSig><sigN>3</sigN><sigD>4</sigD></TimeSig><Rest>",
    );
    assert!(
        musescore(&muse)
            .unwrap_err()
            .contains("changes time signature")
    );
    let xml = MUSICXML.replace("<alter>1</alter>", "<alter>0.5</alter>");
    assert!(musicxml(&xml).unwrap_err().contains("Microtonal"));
}

#[test]
fn native_scores_and_legacy_finale_errors_use_shared_open_path() {
    let source = serde_json::to_vec(&Score::blank()).unwrap();
    assert_eq!(load_bytes("encore", &source).unwrap().score, Score::blank());
    assert_eq!(load_bytes("json", &source).unwrap().score, Score::blank());
    assert!(
        load_bytes("mus", b"legacy")
            .unwrap_err()
            .contains("Export MusicXML")
    );
    assert!(load_bytes("txt", b"not music").is_err());
}

#[test]
fn pickup_preserves_spacing_into_the_next_measure() {
    let xml = simple_xml(
        "<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration></note>",
    )
    .replace(
        "<measure number='1'>",
        "<measure number='1' implicit='yes'>",
    );
    let imported = musicxml(&xml).unwrap();
    assert_eq!(imported.score.parts[0].measures[0].notes[0].start, 1440);
    assert!(imported.warnings.iter().any(|w| w.contains("pickup")));
    let muse = MUSESCORE.replace(
        "<Measure>\n        <voice>",
        "<Measure len='1/4'>\n        <voice>",
    );
    assert!(
        musescore(&muse)
            .unwrap_err()
            .contains("outside the measure")
    );
}

#[test]
fn musescore_explicit_ticks_follow_a_pickup_without_double_shifting() {
    let xml = "<museScore version='2.06'><Score><Division>480</Division><Part><Staff id='1'/><trackName>Piano</trackName><Instrument><Channel><program value='0'/></Channel></Instrument></Part><Staff id='1'><Measure len='1/4'><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><tick>0</tick><Chord><durationType>quarter</durationType><Note><pitch>60</pitch><tpc>14</tpc></Note></Chord></Measure><Measure><tick>480</tick><Chord><durationType>whole</durationType><Note><pitch>62</pitch><tpc>16</tpc></Note></Chord></Measure></Staff></Score></museScore>";
    let imported = musescore(xml).unwrap();
    assert_eq!(imported.score.tempo, 120);
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("No explicit tempo") && w.contains("120"))
    );
    let part = &imported.score.parts[0];
    assert_eq!(part.measures[0].notes[0].start, 1440);
    assert_eq!(part.measures[1].notes[0].start, 0);
    assert_eq!(part.measures[1].notes[0].pitches[0].midi(), 62);
}

#[test]
fn hostile_numbers_return_errors_instead_of_overflowing() {
    for tags in [
        "<transpose><chromatic>32767</chromatic><octave-change>32767</octave-change></transpose>",
        "<transpose><chromatic>-32768</chromatic><octave-change>-32768</octave-change></transpose>",
        "<key><fifths>1</fifths></key><transpose><chromatic>20</chromatic><octave-change>32767</octave-change></transpose>",
    ] {
        let xml = simple_xml(&format!(
            "<attributes>{tags}</attributes><note><rest/><duration>4</duration></note>"
        ));
        assert!(musicxml(&xml).is_err());
    }
    let muse = MUSESCORE.replace(
        "<instrumentId>keyboard.piano</instrumentId>",
        "<instrumentId>keyboard.piano</instrumentId><transposeChromatic>32767</transposeChromatic>",
    );
    assert!(musescore(&muse).is_err());
    let muse = MUSESCORE.replace(
        "<Chord><durationType>quarter",
        "<tick>-9223372036854775808</tick><Chord><durationType>quarter",
    );
    assert!(musescore(&muse).is_err());
}

#[test]
fn excess_voices_are_rejected_before_allocating_measure_grids() {
    let voices = (1..=65).map(|voice| format!("<note><rest/><duration>4</duration><voice>{voice}</voice></note><backup><duration>4</duration></backup>")).collect::<String>();
    let error = musicxml(&simple_xml(&voices)).unwrap_err();
    assert!(error.contains("64-staff limit"));
    let xml = format!(
        "<museScore version='4.30'><Score><Staff id='1'><Measure>{}</Measure></Staff></Score></museScore>",
        "<voice><Rest><durationType>whole</durationType></Rest></voice>".repeat(65)
    );
    assert!(musescore(&xml).unwrap_err().contains("64-staff limit"));
}

#[test]
fn partially_tied_chords_fail_without_converting_to_wrong_ties() {
    let music = MUSICXML.replace(
        "<note dynamics=\"100\"><chord/>",
        "<note dynamics=\"100\"><tie type='start'/><chord/>",
    );
    assert!(
        musicxml(&music)
            .unwrap_err()
            .contains("partially tied chords")
    );
    let muse = MUSESCORE.replacen("<pitch>60</pitch><tpc>14</tpc><Spanner type=\"Tie\"><next><location><fractions>1/4</fractions></location></next></Spanner>", "<pitch>60</pitch><tpc>14</tpc>", 1);
    assert!(
        musescore(&muse)
            .unwrap_err()
            .contains("partially tied chords")
    );
}

#[test]
fn optional_upstream_musescore_fixture_preserves_every_sounding_pitch() {
    let Some(path) = std::env::var_os("ENCORE_TEST_MUSESCORE") else {
        return;
    };
    let source = std::fs::read_to_string(&path).unwrap();
    let document = parse_xml(&source).unwrap();
    let source_score = child(document.root_element(), "Score").unwrap();
    let source_pitches: Vec<u8> = source_score
        .children()
        .filter(|n| n.has_tag_name("Staff"))
        .flat_map(|staff| staff.children().filter(|n| n.has_tag_name("Measure")))
        .flat_map(|measure| measure.descendants().filter(|n| n.has_tag_name("Note")))
        .map(|note| number::<u8>(note, "pitch").unwrap().unwrap())
        .collect();
    let imported = load(Path::new(&path)).unwrap();
    let pitches: Vec<u8> = imported
        .score
        .parts
        .iter()
        .flat_map(|part| &part.measures)
        .flat_map(|measure| &measure.notes)
        .flat_map(|note| note.pitches.iter().map(|p| p.midi()))
        .collect();
    assert!(!source_pitches.is_empty());
    assert_eq!(pitches, source_pitches);
    imported.score.validate().unwrap();
}

#[test]
fn transparent_warnings_cover_layout_expression_and_repeat_losses() {
    let xml = MUSICXML.replace("<measure number=\"2\">", "<measure number=\"2\"><direction><sound tempo='120'/></direction><attributes><key><fifths>-2</fifths></key></attributes><barline><repeat direction='backward'/></barline>");
    let imported = musicxml(&xml).unwrap();
    assert_eq!(imported.score.tempo, 96);
    assert_eq!(imported.score.key_fifths, 2);
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("Tempo changes"))
    );
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("Key signature changes"))
    );
    assert!(imported.warnings.iter().any(|w| w.contains("play once")));
}

#[test]
fn zero_and_oversized_chords_are_rejected() {
    let xml = simple_xml("<note><rest/><duration>0</duration></note>");
    assert!(musicxml(&xml).unwrap_err().contains("duration"));
    let mut notes = Vec::new();
    assert!(
        push_event(
            &mut notes,
            0,
            480,
            vec![Pitch::from_midi(60); 17],
            64,
            false,
            false
        )
        .is_err()
    );
    assert!(push_event(&mut notes, 0, 0, vec![], 64, false, false).is_err());
}

#[test]
fn musicxml_export_reopens_with_exact_existing_note_data() {
    let original = Score::demo();
    let imported = musicxml(&crate::export::musicxml(&original)).unwrap().score;
    assert_eq!(
        (
            imported.title.as_str(),
            imported.composer.as_str(),
            imported.tempo,
            imported.time_numerator,
            imported.time_denominator,
            imported.key_fifths
        ),
        (
            original.title.as_str(),
            original.composer.as_str(),
            original.tempo,
            original.time_numerator,
            original.time_denominator,
            original.key_fifths
        )
    );
    assert_eq!(imported.parts.len(), original.parts.len());
    for (actual, expected) in imported.parts.iter().zip(&original.parts) {
        assert_eq!(actual.instrument, expected.instrument);
        assert_eq!(actual.clef, expected.clef);
        for (actual, expected) in actual.measures.iter().zip(&expected.measures) {
            assert_eq!(
                actual
                    .notes
                    .iter()
                    .filter(|n| !n.pitches.is_empty())
                    .collect::<Vec<_>>(),
                expected
                    .notes
                    .iter()
                    .filter(|n| !n.pitches.is_empty())
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn exact_duration_decomposition_preserves_chord_ties() {
    let mut notes = Vec::new();
    push_event(
        &mut notes,
        120,
        2400,
        vec![Pitch::from_midi(60), Pitch::from_midi(64)],
        91,
        true,
        false,
    )
    .unwrap();
    assert_eq!(notes.iter().map(Note::ticks).sum::<u32>(), 2400);
    assert_eq!(notes[0].start, 120);
    assert!(notes[0].tied);
    assert!(!notes.last().unwrap().tied);
    assert!(!notes[0].staccato);
    assert!(notes.last().unwrap().staccato);
}
