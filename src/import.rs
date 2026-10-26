//! Bounded readers for public score interchange and MuseScore's XML format.
//!
//! Import is deliberately explicit about the limits of Encore's score model.
//! Up to eight notation voices share each staff as two voices in four layers; unsupported timing is
//! rejected instead of rounded, and display-only losses are reported.
use crate::model::{Duration, MAX_MEASURES, MAX_PARTS, Measure, Note, Part, Pitch, Score};
use roxmltree::{Document, Node};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

mod annotations;
mod chords;
mod entry_style;
mod expressions;
mod finale;
mod grace;
mod groups;
mod lines;
mod midi;
mod musescore;
mod musicxml;
mod ornaments;
mod percussion;
mod tremolos;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VoiceTarget {
    pub part: usize,
    pub layer: u8,
    pub voice: u8,
}
pub(super) type VoiceMapping = BTreeMap<(u32, String), VoiceTarget>;
impl VoiceTarget {
    pub(super) fn anchor(self, measure: usize, tick: u32) -> crate::document::Anchor {
        crate::document::Anchor {
            part: self.part,
            measure,
            tick,
            layer: self.layer,
            voice: self.voice,
        }
    }
}

pub(super) const MAX_XML_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_ARCHIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 2048;
const MAX_ARCHIVE_CONTENT: u64 = 128 * 1024 * 1024;

#[derive(Debug)]
pub struct ImportResult {
    pub score: Score,
    /// Features that the editable Encore score cannot retain.
    pub warnings: Vec<String>,
}

/// Open an Encore, MusicXML, MuseScore, modern Finale, or MIDI score.
pub fn load(path: &Path) -> Result<ImportResult, String> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension == "encore" || extension == "json" {
        return Score::load(path).map(|score| ImportResult {
            score,
            warnings: Vec::new(),
        });
    }
    if extension == "mus" {
        return Err("Legacy Finale .mus files cannot be read directly. Export MusicXML (.musicxml or .mxl) from Finale or another compatible notation program, then open that file in Encore.".into());
    }
    if ![
        "xml", "musicxml", "mxl", "mscx", "mscz", "musx", "mid", "midi",
    ]
    .contains(&extension.as_str())
    {
        return Err(
            "Open an .encore, .musicxml, .xml, .mxl, .mscx, .mscz, .musx, .mid, or .midi score."
                .into(),
        );
    }
    let bytes = read_import_bytes(
        path,
        MAX_ARCHIVE_BYTES,
        "Imported score exceeds the 64 MiB file limit.",
    )?;
    let mut imported = if extension == "mid" || extension == "midi" {
        midi::parse(&bytes)?
    } else if extension == "musx" {
        finale::parse(&bytes)?
    } else {
        let xml = if ["mxl", "mscz"].contains(&extension.as_str()) {
            archive_score(&bytes, extension == "mscz")?
        } else {
            decode_xml(bytes)?
        };
        let document = parse_xml(&xml)?;
        match document.root_element().tag_name().name() {
            "score-partwise" => musicxml::parse(&document)?,
            "score-timewise" => return Err("Timewise MusicXML is not supported yet. Export a partwise MusicXML score and open it in Encore.".into()),
            "museScore" => musescore::parse(&document)?,
            root => return Err(format!("Unrecognized score XML root <{root}>.")),
        }
    };
    imported.score.normalize();
    imported
        .score
        .validate()
        .map_err(|e| format!("Imported score cannot be represented safely: {e}"))?;
    Ok(imported)
}

/// Import a MIDI performance with an explicitly selected quantization grid.
///
/// Normal `load` remains strict. This optional path rounds note starts and
/// releases to the nearest grid (halfway positions round forward), preserves a
/// minimum one-grid duration, and reports all timing changes in import warnings.
/// The source file is read without modification. Grid sizes are in 480-PPQ ticks.
pub fn load_midi_quantized(path: &Path, grid_ticks: u32) -> Result<ImportResult, String> {
    midi::validate_quantization_grid(grid_ticks)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("mid") && !extension.eq_ignore_ascii_case("midi") {
        return Err("Choose a .mid or .midi file for quantized MIDI import.".into());
    }
    let bytes = read_import_bytes(path, midi::MAX_BYTES, "MIDI file exceeds the 16 MiB limit.")?;
    midi::parse_with_quantization(&bytes, Some(grid_ticks))
}

fn read_import_bytes(path: &Path, limit: usize, limit_message: &str) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("Cannot open score: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit as u64 {
        return Err(limit_message.into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read score: {e}"))?;
    if bytes.len() > limit {
        return Err(limit_message.into());
    }
    Ok(bytes)
}

fn decode_xml(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.len() > MAX_XML_BYTES {
        return Err("Score XML exceeds the 16 MiB limit.".into());
    }
    String::from_utf8(bytes)
        .map_err(|_| "Score XML must be UTF-8. Re-export it as UTF-8 MusicXML.".into())
}

fn safe_archive_path(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && name
            .split('/')
            .all(|component| component != ".." && component != ".")
}

fn zip_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    let entry = archive
        .by_name(name)
        .map_err(|e| format!("Cannot read score archive entry {name}: {e}"))?;
    if entry.size() > MAX_XML_BYTES as u64 {
        return Err("Score XML exceeds the 16 MiB limit.".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_XML_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot decompress score XML: {e}"))?;
    decode_xml(bytes)
}

fn archive_score(bytes: &[u8], muse: bool) -> Result<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Invalid score ZIP archive: {e}"))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("Score archive contains too many entries (limit 2048).".into());
    }
    let mut names = BTreeSet::new();
    let mut candidates = Vec::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| format!("Cannot inspect score archive: {e}"))?;
        let name = entry.name();
        if !safe_archive_path(name) {
            return Err("Score archive contains an unsafe path.".into());
        }
        if !names.insert(name.to_owned()) {
            return Err("Score archive contains duplicate entry names.".into());
        }
        total = total
            .checked_add(entry.size())
            .ok_or("Score archive is too large.")?;
        if total > MAX_ARCHIVE_CONTENT {
            return Err("Score archive expands beyond the 128 MiB limit.".into());
        }
        let extension = Path::new(name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !name.starts_with("META-INF/")
            && if muse {
                extension == "mscx"
            } else {
                extension == "musicxml" || extension == "xml"
            }
        {
            candidates.push(name.to_owned());
        }
    }
    let root = if names.contains("META-INF/container.xml") {
        let manifest = zip_text(&mut archive, "META-INF/container.xml")?;
        let document = parse_xml(&manifest)?;
        document
            .descendants()
            .filter(|n| n.has_tag_name("rootfile"))
            .filter_map(|n| n.attribute("full-path"))
            .find(|name| candidates.iter().any(|candidate| candidate == name))
            .map(str::to_owned)
            .ok_or("The score archive manifest does not identify a supported score file.")?
    } else if candidates.len() == 1 {
        candidates.remove(0)
    } else if candidates.is_empty() {
        return Err("No supported score XML was found inside the archive.".into());
    } else {
        return Err("The archive contains multiple scores without a rootfile manifest.".into());
    };
    zip_text(&mut archive, &root)
}

/// roxmltree never accesses the network. External DTD declarations are permitted
/// for ordinary MusicXML, but internal subsets and custom entities are rejected.
pub(super) fn parse_xml(text: &str) -> Result<Document<'_>, String> {
    if text.len() > MAX_XML_BYTES {
        return Err("Score XML exceeds the 16 MiB limit.".into());
    }
    // An external MusicXML DOCTYPE is conventional and does not define entities.
    // Reject internal declarations before enabling the parser's DTD tokenizer.
    if text.contains("<!ENTITY") {
        return Err("Custom XML entities are not supported in score files.".into());
    }
    if let Some(start) = text.find("<!DOCTYPE") {
        let mut quote = None;
        let mut ended = false;
        for ch in text[start + 9..].chars() {
            match (quote, ch) {
                (Some(q), c) if q == c => quote = None,
                (None, '\'' | '"') => quote = Some(ch),
                (None, '[') => {
                    return Err("Internal XML DTD subsets are not supported in score files.".into());
                }
                (None, '>') => {
                    ended = true;
                    break;
                }
                _ => (),
            }
        }
        if !ended {
            return Err("Unterminated XML DOCTYPE declaration.".into());
        }
    }
    Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 1_000_000,
        },
    )
    .map_err(|e| format!("Invalid score XML: {e}"))
}

pub(super) fn child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|n| n.has_tag_name(tag))
}
pub(super) fn value<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<&'a str> {
    child(node, tag).and_then(|n| n.text()).map(str::trim)
}
pub(super) fn number<T: std::str::FromStr>(
    node: Node<'_, '_>,
    tag: &str,
) -> Result<Option<T>, String> {
    value(node, tag)
        .map(|s| {
            s.parse::<T>()
                .map_err(|_| format!("Invalid <{tag}> value: {s}"))
        })
        .transpose()
}
/// MusicXML alter is a decimal number of semitones; this model supports
/// integral values through double sharps/flats, including decimal spellings.
pub(super) fn pitch_alter(node: Node<'_, '_>) -> Result<i8, String> {
    let alter = number::<f64>(node, "alter")?.unwrap_or(0.);
    if !alter.is_finite() || alter.fract() != 0. || !(-2.0..=2.0).contains(&alter) {
        return Err(
            "Microtonal pitches and accidentals beyond double sharps/flats are not supported yet."
                .into(),
        );
    }
    Ok(alter as i8)
}
pub(super) fn text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
}

#[derive(Default)]
pub(super) struct Warnings(BTreeSet<String>);
impl Warnings {
    pub(super) fn add(&mut self, warning: impl Into<String>) {
        if self.0.len() < 128 {
            self.0.insert(warning.into());
        }
    }
    pub(super) fn finish(self) -> Vec<String> {
        self.0.into_iter().collect()
    }
}

pub(super) fn initial_score(title: String, composer: String) -> Score {
    Score {
        title: if title.trim().is_empty() {
            "Imported score".into()
        } else {
            title
        },
        composer,
        tempo: 120,
        time_numerator: 4,
        time_denominator: 4,
        key_fifths: 0,
        parts: Vec::new(),
        notation: Default::default(),
    }
}

pub(super) fn finish_parts(
    mut score: Score,
    mut parts: Vec<Part>,
    warnings: Warnings,
) -> Result<ImportResult, String> {
    if parts.is_empty() {
        return Err("The file does not contain any supported staves.".into());
    }
    if parts.len() > MAX_PARTS {
        return Err(format!(
            "Import needs {} staves after separating voices; Encore supports at most {MAX_PARTS}.",
            parts.len()
        ));
    }
    let count = parts.iter().map(|p| p.measures.len()).max().unwrap_or(0);
    if count == 0 || count > MAX_MEASURES {
        return Err(format!(
            "Imported score must contain 1–{MAX_MEASURES} measures."
        ));
    }
    for part in &mut parts {
        part.measures.resize_with(count, Measure::default);
    }
    score.parts = parts;
    score.normalize();
    score
        .validate()
        .map_err(|e| format!("Imported score cannot be represented safely: {e}"))?;
    Ok(ImportResult {
        score,
        warnings: warnings.finish(),
    })
}

/// Preserve exact event time by decomposing longer/multiple-dot values into
/// conventional notes down to 128ths. Tuplet notation requires explicit source
/// metadata and is handled by `push_rhythmic_event`, never guessed from timing.
pub(super) fn push_event(
    notes: &mut Vec<Note>,
    start: u32,
    ticks: u32,
    pitches: Vec<Pitch>,
    velocity: u8,
    staccato: bool,
    tied: bool,
) -> Result<(), String> {
    if ticks == 0 {
        return Err("A note or rest has zero duration.".into());
    }
    if ticks > 61_440 {
        return Err("Imported note duration exceeds the largest supported measure.".into());
    }
    if pitches.len() > 16 {
        return Err("Encore supports chords with at most 16 pitches.".into());
    }
    let mut remaining = ticks;
    let mut position = start;
    let mut pieces = Vec::new();
    while remaining > 0 {
        // Choose the longest glyph that leaves a representable remainder.
        let choice = Duration::ALL
            .iter()
            .flat_map(|d| [(*d, true), (*d, false)])
            .map(|(d, dotted)| (d, dotted, d.ticks() * if dotted { 3 } else { 2 } / 2))
            .filter(|(d, dotted, _)| !dotted || d.ticks().is_multiple_of(2))
            .find(|(_, _, size)| *size <= remaining && (remaining - size).is_multiple_of(15));
        let Some((duration, dotted, size)) = choice else {
            return Err(format!(
                "A {ticks}-tick rhythm cannot be represented exactly by ordinary note values on Encore's 480-tick grid. Tuplets require explicit time-modification metadata in the source score."
            ));
        };
        pieces.push(Note {
            voice: 0,
            voice_launch: None,
            articulations: Vec::new(),
            start: position,
            duration,
            dotted,
            tuplet: None,
            layer: 0,
            pitches: pitches.clone(),
            velocity,
            staccato,
            tied: false,
        });
        remaining -= size;
        position = position
            .checked_add(size)
            .ok_or("Imported event position overflowed.")?;
    }
    let last = pieces.len() - 1;
    for (i, note) in pieces.iter_mut().enumerate() {
        note.tied = !pitches.is_empty() && (i != last || tied);
        if i != last {
            note.staccato = false;
        }
    }
    notes.extend(pieces);
    Ok(())
}

/// Preserve a source's explicit written value and time modification, and reject
/// contradictory playback duration instead of changing the notation silently.
pub(super) fn push_rhythmic_event(
    notes: &mut Vec<Note>,
    note: Note,
    ticks: u32,
) -> Result<(), String> {
    if note.checked_ticks()? != ticks {
        return Err(format!(
            "Written note value and tuplet ratio disagree with the source duration ({ticks} ticks)."
        ));
    }
    notes.push(note);
    Ok(())
}

#[cfg(test)]
mod tests;
