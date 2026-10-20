# Encore implementation contract

Encore 0.5 is an original clean-room Rust music notation application. No Finale source, assets, file parsers or proprietary UI are used. The native interface has persistent light/dark themes around a white, printable score page. No Finale proprietary code or assets are used; foreign readers use public format documentation.

## Score and instrument catalog

`src/model.rs` owns the serialized types. Time uses 480 ticks per quarter. Pitches are written at concert pitch: MIDI = `(octave + 1) * 12 + step semitone + alter`. Empty pitch vectors are rests. Notes have an absolute start within their measure; the single rhythmic voice on each staff cannot contain overlapping events. A piano grand staff uses two `Part`s. Serde JSON is the native `.encore` format; saves are validated and atomic.

`Instrument::ALL` contains each of the 128 melodic General MIDI programs exactly once, in zero-based bank order. `name()`, `family()`, `gm_program()` and `default_clef()` provide presentation and export metadata. `from_gm_program(u8)` returns the canonical variant for valid programs and `None` above 127. The original 18 named variants and their serialized names are retained; additional programs use `Instrument::GeneralMidi(u8)`. Explicit aliases for an original program are valid, and score validation rejects program values above 127. `Instrument::LEGACY` preserves the original 18 voices in their original order for procedural synthesis and regression checks.

Older `.encore` projects load without instrument conversion. Projects containing the additional `GeneralMidi` representation require Encore 0.4 or later and cannot be opened by older builds. MIDI and MusicXML export all 128 program identities; MusicXML converts the zero-based program to its one-based numbering.

Other core APIs:

- `Duration::ALL`, `ticks()` and `name()`; `Note::ticks()`.
- `Pitch::midi()`, `from_midi(u8)`, `staff_step()` (`C4 = 28`), `from_staff_step(i32, key_fifths)` and `name()`.
- `Score::demo()` (First Light), `Score::carol_demo()` (Winter Ascendant), `Score::blank()`, `measure_ticks()`, `measure_count()`, `validate()`, `add_measures()`, `add_part()`, `normalize()`, `load()` and `save()`.
- Export functions `musicxml(&Score)` and `midi(&Score)` honor key, time, gaps/rests, chords, ties, staccato, tempo, mixer state and program selection. Validation rejects malformed scores before export.

`src/instruments.rs` supplies character descriptions, concert-pitch or keyboard audition guides, and four-bar test phrases. The original orchestral ranges remain practical composing guides. Additional GM profiles describe useful keyboard registers rather than asserting a physical instrument's playing compass. All 128 phrases validate and remain inside their displayed guides.

## Engraving

`src/engraving.rs` turns a score and `EngravingOptions` into vector `Page`s. Options select a note, playhead and optional staff filter. Each page contains line, ellipse, path and outlined text primitives, staff regions and note hit maps. SVG and PDF output use these same pages. Text and notation are portable vector outlines with no runtime font dependency for printing.

Coordinates grow downward. Staff diatonic references are treble bottom-line E4 = 30, bass G2 = 18 and alto F3 = 24; a pitch's vertical position is `bottom_y - (staff_step - bottom_step) * spacing / 2`. Layout aims for four measures per system and adapts staff groups and page count to the score. The playhead uses absolute quarter beats.

## Audio

Application audio goes through `src/audio.rs` and `AudioEngine`, either `Procedural` or `Sampled(SampleBank)`. `load_default()` returns the selected engine and an optional visible fallback message. The bundled bank is the default. An explicit invalid `ENCORE_SOUNDFONT` never silently selects another bank. GUI playback, auditions and WAV exports clone the active engine; command-line audio operations load it once. GUI exports call `export_to_with_engine`. The original `synth::render` remains a deterministic procedural comparison.

`src/sampled.rs` loads a bounded, validated SF2 bank and requires all **128 melodic bank-0 programs**. A bank with missing programs is rejected instead of substituting a piano or procedural voice. All catalog entries, including woodwinds, harp, electronic presets and sound effects, use SoundFont sample regions. The bundled GeneralUser GS bank advertises 261 presets and 13 drum kits upstream; Encore currently exposes only its 128 melodic bank-0 programs. GS variations, bank selection and drum-kit mapping are outside this version's scope. Electronic and effect patches can use synthesized source material and SF2 processing; the catalog is not a collection of 128 acoustic recording sessions.

OxiSynth renders the samples and SF2 modulators. Each part receives a fresh synthesizer; a shared decoded font is loaned under a mutex and restored by RAII on success, error or unwinding. Loans end between parts. This avoids channel/program interference and duplicate sample decoding. Commands use rounded frame positions; OxiSynth's 64-frame DSP buffering can add up to 63 frames of latency. Ties merge matching chord pitches independently, and note-offs precede retriggers at the same frame. Mute/solo, staff volume and pan are honored. The bundled GeneralUser GS bank has measured orchestral mix trims; additional GM presets and other banks retain native relative levels.

`src/synth.rs` retains the original 18 procedural voices. New GM programs use related family approximations only when Procedural mode is explicitly selected or a visible fallback has occurred. These approximations do not replace presets in sampled mode. Both audio backends use the shared room/limiter pass in `synth::finish`, 44.1 kHz stereo output and the same WAV encoder. The original procedural Carol WAV files remain the comparison baseline.

`Playback` owns a persistent rodio output stream and sink, with play, stop, pause, resume, playing/paused state, position and volume controls. Score playback and auditions use worker jobs and discard stale results when an edit or source change cancels the request. Rendering is limited to ten minutes of music in memory. Retain bank attribution and the sampled engine's LGPL source/rebuild bundle with the application.

## Interface and commands

`src/app.rs` owns the native editor, note input, inspector, mixer, history and transport. The **Instrument library** opens from **Instruments**, the Instruments panel's **+**, or the keyboard's **Instrument lab** button. Search and family filters browse all 128 sounds. Selection exposes the actual preset, character and guide range, single-note and phrase auditions, velocity and hold controls, phrase WAV export, **Add staff** and **Use on active staff**. Changing the active staff's sound preserves its notes, changes its clef and participates in undo/redo. Playback sound controls select the sampled library, procedural mode or a replacement complete SF2 bank.

`src/main.rs` owns desktop startup and command-line operations. `--instruments` renders 128 audition WAV files. `--procedural` and `--soundfont PATH` select the CLI audio engine. `--compare-carol` renders the same score with both engines and writes matching opening excerpts. The launcher continues to use an available release binary or build one.

Automated tests cover native compatibility and validation, all program identities and MIDI/MusicXML exports, engraving, all sampled presets, the original procedural voices, audio scheduling and failure recovery, library filtering, instrument changes and undo, and keyboard/transport behavior. Display-independent tests render actual egui meshes and fonts for editor and library visual QA.

## Score import and preferences

`src/import.rs` exposes `load(&Path) -> Result<ImportResult, String>` with an editable score and visible warnings. Separate modules read partwise MusicXML/MXL, MuseScore XML/MSCZ and a bounded subset of modern Finale MUSX. Compressed inputs have entry, path and expanded-size limits; XML never resolves network resources. Unsupported rhythm/pitch structures fail explicitly. Importing does not retain a native save path, and `needs_native_save` protects unsaved imported documents. The original file is read only.

The app's `Theme` selects a shared palette for all chrome and configures egui visuals independently of font setup. The preference is stored through eframe persistence under `encore.theme`; changing it does not edit the score. macOS bank discovery includes `Contents/Resources/assets`, and native packaging/CI is documented in BUILDING.md.
