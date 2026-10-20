//! Selectable audio rendering with one shared, already-loaded sound bank.
//!
//! Use `AudioEngine::load_default` at startup and surface its optional message.
//! Selecting a broken bank never masquerades as sampled playback: loading fails
//! explicitly, or startup identifies the procedural fallback in its message.

use crate::{
    model::{Instrument, Score},
    sampled::SampleBank,
    synth,
};
use std::path::{Path, PathBuf};

pub const BUNDLED_BANK_RELATIVE: &str = "assets/soundfonts/GeneralUser-GS.sf2";

#[derive(Clone)]
pub enum AudioEngine {
    Procedural,
    Sampled(SampleBank),
}

impl AudioEngine {
    /// Load an explicitly chosen SoundFont. Errors remain visible to the caller;
    /// this method never substitutes procedural synthesis for a failed bank.
    pub fn from_soundfont(path: &Path) -> Result<Self, String> {
        SampleBank::load(path).map(Self::Sampled)
    }

    /// Load ENCORE_SOUNDFONT when explicitly supplied, otherwise look for the
    /// bundled bank near the working directory, executable, and source tree.
    /// The optional message explains any fallback or unusable bundled candidate.
    /// Cloning the resulting engine shares the loaded bank across render jobs.
    pub fn load_default() -> (Self, Option<String>) {
        let explicit = std::env::var_os("ENCORE_SOUNDFONT").map(PathBuf::from);
        let cwd = std::env::current_dir().ok();
        let exe = std::env::current_exe().ok();
        let candidates = bank_candidates(
            cwd.as_deref(),
            exe.as_deref(),
            Path::new(env!("CARGO_MANIFEST_DIR")),
        );
        load_from_sources(explicit.as_deref(), &candidates)
    }

    pub fn name(&self) -> String {
        match self {
            Self::Procedural => "Procedural synthesis".into(),
            Self::Sampled(bank) => format!("SoundFont · {}", bank.name()),
        }
    }

    pub fn bank_path(&self) -> Option<&Path> {
        match self {
            Self::Procedural => None,
            Self::Sampled(bank) => Some(bank.path()),
        }
    }

    /// The actual selected SoundFont preset for any catalog instrument.
    pub fn preset_name(&self, instrument: Instrument) -> Option<&str> {
        match self {
            Self::Procedural => None,
            Self::Sampled(bank) => bank.preset_name(instrument),
        }
    }

    /// Render the validated score with the selected backend. Both backends use
    /// the same stereo sample rate and obey the same ten-minute score limit.
    pub fn render(&self, score: &Score) -> Result<Vec<f32>, String> {
        validate_score(score)?;
        match self {
            Self::Procedural => Ok(synth::render(score)),
            Self::Sampled(bank) => bank.render(score),
        }
    }

    /// Render one sounding MIDI pitch. Velocity zero is valid silence; malformed
    /// MIDI values and non-finite or excessive note lengths are rejected.
    pub fn render_note(
        &self,
        instrument: Instrument,
        midi: u8,
        velocity: u8,
        duration_secs: f32,
    ) -> Result<Vec<f32>, String> {
        if instrument.gm_program() > 127 {
            return Err("Instrument program must be a General MIDI program from 0 to 127.".into());
        }
        validate_note(midi, velocity, duration_secs)?;
        match self {
            Self::Procedural => Ok(synth::render_note(
                instrument,
                midi,
                velocity,
                duration_secs,
            )),
            Self::Sampled(bank) => bank.render_note(instrument, midi, velocity, duration_secs),
        }
    }
}

/// Startup convenience for callers that prefer a module-level entry point.
pub fn load_default() -> (AudioEngine, Option<String>) {
    AudioEngine::load_default()
}

fn validate_score(score: &Score) -> Result<(), String> {
    score
        .validate()
        .map_err(|e| format!("Cannot render score: {e}"))?;
    let seconds = synth::duration_seconds(score);
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("Cannot render a score with an invalid duration.".into());
    }
    if seconds > synth::MAX_RENDER_SECONDS {
        return Err(format!(
            "Audio rendering supports scores up to {} minutes. This score is {:.1} minutes long.",
            synth::MAX_RENDER_SECONDS / 60.0,
            seconds / 60.0,
        ));
    }
    Ok(())
}

fn validate_note(midi: u8, velocity: u8, seconds: f32) -> Result<(), String> {
    if midi > 127 {
        return Err("Audition pitch must be a MIDI note from 0 to 127.".into());
    }
    if velocity > 127 {
        return Err("Audition velocity must be from 0 to 127.".into());
    }
    if !seconds.is_finite() || !(0.01..=120.0).contains(&seconds) {
        return Err("Audition duration must be finite and between 0.01 and 120 seconds.".into());
    }
    Ok(())
}

// Pure path construction keeps discovery testable without changing process-wide
// environment variables or depending on a bank installed on the test machine.
fn bank_candidates(cwd: Option<&Path>, exe: Option<&Path>, manifest: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut add = |directory: &Path| {
        let candidate = directory.join(BUNDLED_BANK_RELATIVE);
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };
    if let Some(cwd) = cwd {
        add(cwd);
    }
    if let Some(exe) = exe {
        // Also reaches the project directory from target/release/encore, while
        // supporting a portable binary beside its own assets directory.
        if let Some(directory) = exe.parent() {
            // Standard macOS application layout: Contents/MacOS/encore with
            // the bank under Contents/Resources/assets.
            if directory.file_name().is_some_and(|name| name == "MacOS")
                && let Some(contents) = directory.parent()
            {
                add(&contents.join("Resources"));
            }
            for ancestor in directory.ancestors().take(3) {
                add(ancestor);
            }
        }
    }
    add(manifest);
    candidates
}

fn load_from_sources(
    explicit: Option<&Path>,
    candidates: &[PathBuf],
) -> (AudioEngine, Option<String>) {
    if let Some(path) = explicit {
        if path.as_os_str().is_empty() {
            return (
                AudioEngine::Procedural,
                Some("ENCORE_SOUNDFONT is empty. Using procedural synthesis; choose a valid SoundFont to enable sampled instruments.".into()),
            );
        }
        return match AudioEngine::from_soundfont(path) {
            Ok(engine) => (engine, None),
            Err(error) => (
                AudioEngine::Procedural,
                Some(format!(
                    "Could not load ENCORE_SOUNDFONT '{}': {error}. Using procedural synthesis.",
                    path.display()
                )),
            ),
        };
    }
    let mut failures = Vec::new();
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        match AudioEngine::from_soundfont(path) {
            Ok(engine) => {
                let warning = if failures.is_empty() {
                    None
                } else {
                    Some(format!(
                        "Loaded SoundFont '{}'; skipped unusable bank(s): {}",
                        path.display(),
                        failures.join("; ")
                    ))
                };
                return (engine, warning);
            }
            Err(error) => failures.push(format!("'{}': {error}", path.display())),
        }
    }
    let message = if failures.is_empty() {
        "No SoundFont bank was found. Using procedural synthesis. Place GeneralUser-GS.sf2 in assets/soundfonts or set ENCORE_SOUNDFONT to a compatible bank.".into()
    } else {
        format!(
            "Could not load a bundled SoundFont: {}. Using procedural synthesis.",
            failures.join("; ")
        )
    };
    (AudioEngine::Procedural, Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_discovery_covers_portable_and_release_layouts_without_duplicates() {
        let root = Path::new("/encore-test-project");
        let candidates =
            bank_candidates(Some(root), Some(&root.join("target/release/encore")), root);
        assert_eq!(candidates[0], root.join(BUNDLED_BANK_RELATIVE));
        assert!(candidates.contains(&root.join("target/release").join(BUNDLED_BANK_RELATIVE)));
        assert!(candidates.contains(&root.join("target").join(BUNDLED_BANK_RELATIVE)));
        assert_eq!(
            candidates
                .iter()
                .filter(|p| **p == root.join(BUNDLED_BANK_RELATIVE))
                .count(),
            1
        );
        let portable = bank_candidates(
            None,
            Some(Path::new("/portable/encore")),
            Path::new("/source"),
        );
        assert_eq!(
            portable[0],
            Path::new("/portable").join(BUNDLED_BANK_RELATIVE)
        );
        assert!(portable.contains(&Path::new("/source").join(BUNDLED_BANK_RELATIVE)));
        let mac = bank_candidates(
            None,
            Some(Path::new("/Applications/Encore.app/Contents/MacOS/encore")),
            Path::new("/source"),
        );
        assert_eq!(
            mac[0],
            Path::new("/Applications/Encore.app/Contents/Resources").join(BUNDLED_BANK_RELATIVE)
        );
    }

    #[test]
    fn missing_default_is_explicit_procedural_without_environment_mutation() {
        let (engine, warning) = load_from_sources(None, &[]);
        assert!(matches!(engine, AudioEngine::Procedural));
        assert!(engine.bank_path().is_none());
        assert!(engine.preset_name(Instrument::Piano).is_none());
        assert!(engine.name().contains("Procedural"));
        assert!(warning.unwrap().contains("No SoundFont"));
    }

    #[test]
    fn invalid_explicit_bank_never_silently_selects_another_source() {
        let invalid = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(AudioEngine::from_soundfont(&invalid).is_err());
        let (engine, warning) = load_from_sources(Some(&invalid), &[]);
        assert!(matches!(engine, AudioEngine::Procedural));
        let warning = warning.unwrap();
        assert!(warning.contains("ENCORE_SOUNDFONT") && warning.contains("procedural"));
        let (engine, warning) = load_from_sources(Some(Path::new("")), &[]);
        assert!(matches!(engine, AudioEngine::Procedural));
        assert!(warning.unwrap().contains("empty"));
    }

    #[test]
    fn invalid_bundled_candidate_reports_the_load_error() {
        let invalid = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let (engine, warning) = load_from_sources(None, &[invalid]);
        assert!(matches!(engine, AudioEngine::Procedural));
        assert!(
            warning
                .unwrap()
                .contains("Could not load a bundled SoundFont")
        );
    }

    #[test]
    fn procedural_render_validates_score_and_note_limits() {
        let engine = AudioEngine::Procedural;
        let mut score = Score::blank();
        score.tempo = 0;
        assert!(
            engine
                .render(&score)
                .unwrap_err()
                .contains("Cannot render score")
        );
        score.tempo = 20;
        score.add_measures(60);
        assert!(engine.render(&score).unwrap_err().contains("up to"));
        for seconds in [f32::NAN, f32::INFINITY, -1.0, 0.0, 0.009, 120.1] {
            assert!(
                engine
                    .render_note(Instrument::Piano, 60, 80, seconds)
                    .is_err()
            );
        }
        assert!(engine.render_note(Instrument::Piano, 128, 80, 0.1).is_err());
        assert!(engine.render_note(Instrument::Piano, 60, 128, 0.1).is_err());
        assert!(
            engine
                .render_note(Instrument::GeneralMidi(255), 60, 80, 0.1)
                .is_err()
        );
        assert!(validate_note(0, 0, 0.01).is_ok());
        assert!(validate_note(127, 127, 120.0).is_ok());
        let samples = engine.render_note(Instrument::Piano, 60, 80, 0.01).unwrap();
        assert!(!samples.is_empty() && samples.len().is_multiple_of(2));
        assert!(samples.iter().all(|s| s.is_finite()));
    }
}
