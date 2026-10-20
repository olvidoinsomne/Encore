//! Explicit MIDI notation grids shared by the desktop dialog and command line.
use std::path::Path;

pub const GRIDS: &[(&str, &str, u32)] = &[
    ("quarter", "Quarter notes", 480),
    ("8th", "Eighth notes", 240),
    ("8th-triplet", "Eighth-note triplets", 160),
    ("16th", "Sixteenth notes", 120),
    ("16th-triplet", "Sixteenth-note triplets", 80),
    ("32nd", "32nd notes", 60),
    ("32nd-triplet", "32nd-note triplets", 40),
    ("64th", "64th notes", 30),
    ("64th-triplet", "64th-note triplets", 20),
    ("128th", "128th notes", 15),
    ("128th-triplet", "128th-note triplets", 10),
];

pub fn parse_grid(name: &str) -> Result<u32, String> {
    GRIDS.iter().find(|(key, _, _)| *key == name).map(|(_, _, ticks)| *ticks)
        .ok_or_else(|| format!("Unknown MIDI grid {name:?}. Use quarter, 8th, 16th, 32nd, 64th, 128th, or an 8th-triplet through 128th-triplet grid."))
}

pub fn is_midi(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mid") || e.eq_ignore_ascii_case("midi"))
}
