#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod midi_options;
mod shortcuts;

use encore::{audio::AudioEngine, engraving, export, model::Score, synth};
use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum BackendChoice {
    #[default]
    Default,
    Procedural,
    SoundFont(PathBuf),
}

#[derive(Debug, PartialEq, Eq)]
struct Arguments {
    command: Vec<String>,
    backend: BackendChoice,
}

/// Audio modifiers can precede, follow, or appear between a CLI command's
/// positional arguments. They do not initialize a bank during argument parsing.
fn parse_arguments(args: &[String]) -> Result<Arguments, String> {
    let mut command = Vec::new();
    let mut backend = BackendChoice::Default;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let choice = if arg == "--procedural" {
            Some(BackendChoice::Procedural)
        } else if arg == "--soundfont" {
            index += 1;
            let path = args
                .get(index)
                .filter(|p| !p.is_empty() && !p.starts_with("--"))
                .ok_or("--soundfont requires a SoundFont file path.")?;
            Some(BackendChoice::SoundFont(PathBuf::from(path)))
        } else if let Some(path) = arg.strip_prefix("--soundfont=") {
            if path.is_empty() {
                return Err("--soundfont requires a SoundFont file path.".into());
            }
            Some(BackendChoice::SoundFont(PathBuf::from(path)))
        } else {
            command.push(arg.clone());
            None
        };
        if let Some(choice) = choice {
            if backend != BackendChoice::Default {
                return Err(
                    "Use only one audio modifier: --procedural or --soundfont PATH.".into(),
                );
            }
            backend = choice;
        }
        index += 1;
    }
    if backend != BackendChoice::Default && !command.first().is_some_and(|s| s.starts_with('-')) {
        return Err("Audio modifiers require a CLI command. For the desktop editor, choose a SoundFont in Instrument lab or set ENCORE_SOUNDFONT.".into());
    }
    if command.first().is_some_and(|s| !s.starts_with('-')) && command.len() > 1 {
        return Err("Open one project at a time, or use --help for CLI commands.".into());
    }
    Ok(Arguments { command, backend })
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_arguments(&raw) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("Encore: {error}");
            return ExitCode::FAILURE;
        }
    };
    if args.command.first().is_some_and(|s| s.starts_with('-')) {
        return match cli(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Encore: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let initial = args.command.first().map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Encore — Music notation")
            .with_app_id("encore")
            .with_inner_size([1440.0, 960.0])
            .with_min_inner_size([980.0, 680.0]),
        ..Default::default()
    };
    match eframe::run_native(
        "Encore",
        options,
        Box::new(move |cc| Ok(Box::new(app::EncoreApp::new(cc, initial)))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Could not start Encore: {e}");
            ExitCode::FAILURE
        }
    }
}

fn selected_engine(choice: &BackendChoice) -> Result<AudioEngine, String> {
    match choice {
        BackendChoice::Procedural => Ok(AudioEngine::Procedural),
        BackendChoice::SoundFont(path) => AudioEngine::from_soundfont(path),
        BackendChoice::Default => {
            let (engine, warning) = AudioEngine::load_default();
            if let Some(warning) = warning {
                eprintln!("Encore: {warning}");
            }
            Ok(engine)
        }
    }
}

fn load_score(path: &Path) -> Result<Score, String> {
    let result = encore::import::load(path)?;
    for warning in result.warnings {
        eprintln!("Import note: {warning}");
    }
    Ok(result.score)
}

fn cli(parsed: &Arguments) -> Result<(), String> {
    let args = &parsed.command;
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "--version" | "-V" if args.len() == 1 => {
            println!("Encore {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "--help" | "-h" if args.len() == 1 => {
            println!(concat!(
                "Encore — original Rust music notation and synthesis\n\n",
                "encore [SCORE]                        Open Encore/MusicXML/MuseScore/Finale/MIDI\n",
                "encore --demo OUTPUT_DIR             Export First Light in all formats\n",
                "encore --demo-carol OUTPUT_DIR       Export Carol of the Bells — Epic\n",
                "encore --demo-all OUTPUT_DIR         Export both demos\n",
                "encore --compare-carol OUTPUT_DIR    Export procedural/sampled WAV comparisons\n",
                "encore --export INPUT OUTPUT         Export .pdf/.svg/.musicxml/.mid/.wav\n",
                "encore --parts INPUT OUTPUT_DIR      Export all linked parts as PDF\n",
                "encore --import INPUT OUTPUT.encore   Import a score into an Encore project\n",
                "encore --import-midi INPUT OUTPUT.encore --quantize 16th\n",
                "                                    Import performance MIDI on a chosen grid\n",
                "encore --validate INPUT              Validate an importable score\n",
                "encore --audio-check                 Test the audio output device\n",
                "encore --instruments OUTPUT_DIR      Render an audition WAV per instrument\n\n",
                "CLI audio options (place anywhere in a command):\n",
                "  --procedural                      Use procedural synthesis\n",
                "  --soundfont PATH                  Use this SoundFont; errors stop the command\n",
                "The options are mutually exclusive. By default, audio commands discover the\n",
                "bundled SoundFont or ENCORE_SOUNDFONT and report any procedural fallback.\n",
                "For the desktop editor, use Instrument lab or ENCORE_SOUNDFONT.\n",
                "Comparisons require a loaded SoundFont and include the first 16 seconds.\n"
            ));
            Ok(())
        }
        "--demo" | "--demo-carol" | "--demo-all" if args.len() <= 2 => {
            let engine = selected_engine(&parsed.backend)?;
            let dir = Path::new(args.get(1).map(String::as_str).unwrap_or("exports"));
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            println!("Audio backend: {}", engine.name());
            let demos = match command {
                "--demo" => vec![(Score::demo(), "First Light")],
                "--demo-carol" => vec![(Score::carol_demo(), "Carol of the Bells - Epic")],
                _ => vec![
                    (Score::demo(), "First Light"),
                    (Score::carol_demo(), "Carol of the Bells - Epic"),
                ],
            };
            for (score, stem) in demos {
                export_demo(&score, dir, stem, &engine)?;
                println!(
                    "{}: {:.1}s of music",
                    score.title,
                    synth::duration_seconds(&score)
                );
            }
            println!("Demos exported to {}", dir.display());
            Ok(())
        }
        "--compare-carol" if args.len() == 2 => {
            let sampled = selected_engine(&parsed.backend)?;
            if !matches!(sampled, AudioEngine::Sampled(_)) {
                return Err("A procedural/sampled comparison requires a loaded SoundFont. Supply --soundfont PATH, install the bundled bank, or set ENCORE_SOUNDFONT.".into());
            }
            compare_carol(Path::new(&args[1]), &sampled)
        }
        "--export" if args.len() == 3 => {
            let score = load_score(Path::new(&args[1]))?;
            let path = Path::new(&args[2]);
            if is_wav(path) {
                let engine = selected_engine(&parsed.backend)?;
                export_to_with_engine(&score, path, &engine)?;
                println!(
                    "Exported {} · {} · {:.1}s of music",
                    path.display(),
                    engine.name(),
                    synth::duration_seconds(&score)
                );
                Ok(())
            } else {
                export_to(&score, path)
            }
        }
        "--parts" if args.len() == 3 => {
            let score = load_score(Path::new(&args[1]))?;
            let paths = encore::parts::export_pdf_parts(&score, Path::new(&args[2]))?;
            println!("Exported {} linked parts to {}", paths.len(), args[2]);
            Ok(())
        }
        "--import" if args.len() == 3 => {
            let output = Path::new(&args[2]);
            if !output
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("encore"))
            {
                return Err(
                    "Import output must use the .encore extension. Use --export for other formats."
                        .into(),
                );
            }
            let score = load_score(Path::new(&args[1]))?;
            score.save(output)?;
            println!(
                "Imported {} → {} · {} staves, {} measures",
                args[1],
                output.display(),
                score.parts.len(),
                score.measure_count()
            );
            Ok(())
        }
        "--import-midi" if args.len() == 5 && args[3] == "--quantize" => {
            let grid = midi_options::parse_grid(&args[4])?;
            let output = Path::new(&args[2]);
            if !output
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("encore"))
            {
                return Err("Import output must use the .encore extension.".into());
            }
            let imported = encore::import::load_midi_quantized(Path::new(&args[1]), grid)?;
            for warning in &imported.warnings {
                eprintln!("Import note: {warning}");
            }
            imported.score.save(output)?;
            println!(
                "Imported {} → {} with explicit {} quantization",
                args[1],
                output.display(),
                args[4]
            );
            Ok(())
        }
        "--validate" if args.len() == 2 => {
            let score = load_score(Path::new(&args[1]))?;
            score.validate()?;
            println!(
                "Valid: {} — {} staves, {} measures",
                score.title,
                score.parts.len(),
                score.measure_count()
            );
            Ok(())
        }
        "--audio-check" if args.len() == 1 => {
            let engine = selected_engine(&parsed.backend)?;
            let samples = engine.render_note(encore::model::Instrument::Piano, 69, 80, 0.35)?;
            let seconds = samples.len() as f32 / (synth::SAMPLE_RATE as f32 * 2.0);
            let mut playback = synth::Playback::new()?;
            playback.set_volume(0.25);
            playback.play(samples);
            let started = std::time::Instant::now();
            while playback.is_playing() && started.elapsed().as_secs_f32() < 8.0 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            println!(
                "Audio output opened; {} piano audition completed ({seconds:.2}s audio).",
                engine.name()
            );
            Ok(())
        }
        "--instruments" if args.len() == 2 => {
            let engine = selected_engine(&parsed.backend)?;
            let dir = Path::new(&args[1]);
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            println!("Audio backend: {}", engine.name());
            for &instrument in encore::model::Instrument::ALL {
                let samples =
                    engine.render(&encore::instruments::audition_phrase(instrument, 96))?;
                let file = format!("{}.wav", instrument.name().to_lowercase().replace(' ', "-"));
                std::fs::write(dir.join(file), synth::wav(&samples)).map_err(|e| e.to_string())?;
                let source = engine
                    .preset_name(instrument)
                    .map(|name| format!("sampled preset {name}"))
                    .unwrap_or_else(|| "procedural voice".into());
                println!("{}: {source}", instrument.name());
            }
            println!("Instrument auditions exported to {}", dir.display());
            Ok(())
        }
        _ => Err("Invalid arguments. Use --help for usage.".into()),
    }
}

fn compare_carol(dir: &Path, sampled: &AudioEngine) -> Result<(), String> {
    let score = Score::carol_demo();
    score.validate()?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    println!(
        "Carol comparison · {:.1}s of identical score data",
        synth::duration_seconds(&score)
    );
    for (label, engine) in [
        ("Procedural", &AudioEngine::Procedural),
        ("Sampled", sampled),
    ] {
        let samples = engine.render(&score)?;
        let seconds = samples.len() as f32 / (synth::SAMPLE_RATE as f32 * 2.0);
        let full = dir.join(format!("Carol of the Bells - {label}.wav"));
        std::fs::write(&full, synth::wav(&samples)).map_err(|e| e.to_string())?;
        let opening_samples = samples.len().min(16 * synth::SAMPLE_RATE as usize * 2);
        let opening = dir.join(format!("Carol of the Bells - Opening - {label}.wav"));
        std::fs::write(&opening, synth::wav(&samples[..opening_samples]))
            .map_err(|e| e.to_string())?;
        println!(
            "{label}: {} · {seconds:.2}s rendered · {}",
            engine.name(),
            full.display()
        );
    }
    Ok(())
}

fn export_demo(score: &Score, dir: &Path, stem: &str, engine: &AudioEngine) -> Result<(), String> {
    score.save(&dir.join(format!("{stem}.encore")))?;
    for ext in ["pdf", "svg", "musicxml", "mid", "wav"] {
        export_to_with_engine(score, &dir.join(format!("{stem}.{ext}")), engine)?;
    }
    if stem == "Carol of the Bells - Epic" {
        let mut opening = score.clone();
        opening.title = "Carol of the Bells — The opening".into();
        opening.parts.retain(|p| {
            matches!(
                p.instrument,
                encore::model::Instrument::Timpani | encore::model::Instrument::DoubleBass
            )
        });
        for p in &mut opening.parts {
            p.measures.truncate(8);
        }
        export_to_with_engine(
            &opening,
            &dir.join("Carol of the Bells - Opening.pdf"),
            engine,
        )?;
    }
    Ok(())
}

fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
}

/// Convenience entry point. GUI audio exports should pass their already-loaded
/// active engine through `export_to_with_engine` instead of loading a bank again.
pub(crate) fn export_to(score: &Score, path: &Path) -> Result<(), String> {
    let engine = if is_wav(path) {
        selected_engine(&BackendChoice::Default)?
    } else {
        AudioEngine::Procedural
    };
    export_to_with_engine(score, path, &engine)
}

pub(crate) fn export_to_with_engine(
    score: &Score,
    path: &Path,
    engine: &AudioEngine,
) -> Result<(), String> {
    export_to_with_layout(score, path, engine, None)
}

pub(crate) fn export_to_with_layout(
    score: &Score,
    path: &Path,
    engine: &AudioEngine,
    layout: Option<encore::parts::PartLayout>,
) -> Result<(), String> {
    score.validate()?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let data = match ext.as_str() {
        "encore" | "json" => return score.save(path),
        "musicxml" | "xml" => export::musicxml(score).into_bytes(),
        "mid" | "midi" => export::midi(score),
        "wav" => synth::wav(&engine.render(score)?),
        "pdf" | "svg" => {
            let pages = engraving::engrave(
                score,
                &encore::engraving::EngravingOptions {
                    layout,
                    ..Default::default()
                },
            );
            if ext == "pdf" {
                engraving::pdf(&pages)
            } else {
                for (i, page) in pages.iter().enumerate().skip(1) {
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                    let next = path.with_file_name(format!("{stem}-{}.svg", i + 1));
                    std::fs::write(next, engraving::svg(page)).map_err(|e| e.to_string())?;
                }
                engraving::svg(pages.first().ok_or("Score produced no pages")?).into_bytes()
            }
        }
        _ => return Err("Choose a .encore, .pdf, .svg, .musicxml, .mid, or .wav file.".into()),
    };
    std::fs::write(path, data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn audio_modifiers_can_appear_anywhere_in_a_cli_command() {
        for values in [
            vec!["--procedural", "--demo", "out"],
            vec!["--demo", "--procedural", "out"],
            vec!["--demo", "out", "--procedural"],
        ] {
            let parsed = parse_arguments(&strings(&values)).unwrap();
            assert_eq!(parsed.backend, BackendChoice::Procedural);
            assert_eq!(parsed.command, strings(&["--demo", "out"]));
        }
        let parsed = parse_arguments(&strings(&[
            "--export",
            "input.encore",
            "--soundfont",
            "bank with spaces.sf2",
            "out.wav",
        ]))
        .unwrap();
        assert_eq!(
            parsed.backend,
            BackendChoice::SoundFont(PathBuf::from("bank with spaces.sf2"))
        );
        assert_eq!(
            parsed.command,
            strings(&["--export", "input.encore", "out.wav"])
        );
        assert_eq!(
            parse_arguments(&strings(&["--soundfont=bank.sf2", "--audio-check"]))
                .unwrap()
                .backend,
            BackendChoice::SoundFont(PathBuf::from("bank.sf2"))
        );
    }

    #[test]
    fn conflicting_missing_or_gui_only_audio_modifiers_are_rejected() {
        for values in [
            vec!["--demo", "--procedural", "--soundfont", "bank.sf2"],
            vec!["--soundfont", "bank.sf2", "--demo", "--procedural"],
            vec!["--procedural", "--procedural", "--demo"],
            vec!["--soundfont"],
            vec!["--soundfont", "--procedural"],
            vec!["--soundfont=", "--demo"],
            vec!["--procedural"],
            vec!["project.encore", "--procedural"],
        ] {
            assert!(parse_arguments(&strings(&values)).is_err(), "{values:?}");
        }
    }

    #[test]
    fn help_version_and_validation_do_not_initialize_the_selected_bank() {
        for command in ["--help", "--version"] {
            let parsed = parse_arguments(&strings(&[
                command,
                "--soundfont",
                "missing-bank-for-help.sf2",
            ]))
            .unwrap();
            cli(&parsed).unwrap();
        }
        let parsed = parse_arguments(&strings(&[
            "--validate",
            "missing-score-for-validation.encore",
            "--soundfont",
            "missing-bank-for-help.sf2",
        ]))
        .unwrap();
        let error = cli(&parsed).unwrap_err();
        assert!(!error.contains("SoundFont") && !error.contains("soundfont"));
        assert_eq!(
            parse_arguments(&[]).unwrap().backend,
            BackendChoice::Default
        );
        assert_eq!(
            parse_arguments(&strings(&["score.encore"]))
                .unwrap()
                .command,
            strings(&["score.encore"])
        );
    }
}
