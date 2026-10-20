use crate::shortcuts::{self, EntryCommand, EntryProfile};
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use encore::{
    audio::AudioEngine,
    engraving::{self, EngravingOptions, Page, Primitive},
    instruments,
    model::*,
    sampled::SampleBank,
    synth,
};
use std::{path::PathBuf, sync::mpsc, time::Duration as StdDuration};

mod clipboard;
mod entry;
mod notation;
mod parts;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Theme {
    #[default]
    Dark,
    Light,
}

impl Theme {
    fn from_storage(storage: Option<&dyn eframe::Storage>) -> Self {
        match storage
            .and_then(|s| s.get_string("encore.theme"))
            .as_deref()
        {
            Some("light") => Self::Light,
            _ => Self::Dark,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    text: Color32,
    muted: Color32,
    accent: Color32,
    surface: Color32,
    raised: Color32,
    border: Color32,
    selected: Color32,
    workspace: Color32,
    hovered: Color32,
    key_ink: Color32,
    key_hover: Color32,
    playhead: Color32,
}

impl Palette {
    fn for_theme(theme: Theme) -> Self {
        let rgb = Color32::from_rgb;
        match theme {
            Theme::Dark => Self {
                text: Color32::from_gray(236),
                muted: Color32::from_gray(172),
                accent: Color32::from_gray(225),
                surface: Color32::from_gray(26),
                raised: Color32::from_gray(36),
                border: Color32::from_gray(62),
                selected: Color32::from_gray(64),
                workspace: Color32::from_gray(17),
                hovered: Color32::from_gray(49),
                key_ink: Color32::from_gray(35),
                key_hover: Color32::from_gray(221),
                playhead: Color32::from_rgba_unmultiplied(95, 95, 95, 180),
            },
            Theme::Light => Self {
                text: rgb(32, 44, 61),
                muted: rgb(88, 104, 125),
                accent: rgb(17, 111, 86),
                surface: rgb(247, 249, 252),
                raised: rgb(233, 239, 246),
                border: rgb(201, 211, 224),
                selected: rgb(212, 237, 226),
                workspace: rgb(221, 228, 238),
                hovered: rgb(219, 231, 241),
                key_ink: rgb(35, 44, 48),
                key_hover: rgb(211, 234, 224),
                playhead: Color32::from_rgba_unmultiplied(40, 150, 120, 180),
            },
        }
    }
    fn for_context(ctx: &egui::Context) -> Self {
        Self::for_theme(if ctx.style().visuals.dark_mode {
            Theme::Dark
        } else {
            Theme::Light
        })
    }
}

#[derive(Clone, Copy)]
enum Action {
    New,
    Demo,
    CarolDemo,
    Open,
    Quit,
}
enum JobResult {
    Audio(Result<Vec<f32>, String>),
    Export(Result<PathBuf, String>),
}
type AuditionResult = Result<(Vec<f32>, String), String>;

pub struct EncoreApp {
    score: Score,
    saved: Score,
    path: Option<PathBuf>,
    undo: Vec<Score>,
    redo: Vec<Score>,
    skip_history: bool,
    selected: Option<Selection>,
    active_part: usize,
    active_layer: u8,
    cursor_measure: usize,
    cursor_tick: u32,
    duration: Duration,
    dotted: bool,
    rest: bool,
    accidental: Option<i8>,
    octave: i8,
    entry_pitch: Pitch,
    entry_profile: EntryProfile,
    tuplet: Option<Tuplet>,
    tuplet_remaining: Option<u8>,
    write_mode: bool,
    zoom: f32,
    part_view: bool,
    show_parts: bool,
    selected_linked_part: usize,
    previous_part_view: Option<Option<usize>>,
    notation_editor: notation::Editor,
    master: f32,
    theme: Theme,
    needs_native_save: bool,
    import_warnings: Vec<String>,
    show_import_report: bool,
    midi_import: Option<PathBuf>,
    midi_grid: Option<u32>,
    midi_import_error: Option<String>,
    playback: Option<synth::Playback>,
    worker: Option<mpsc::Receiver<JobResult>>,
    rendering_audio: bool,
    pending: Option<Action>,
    help: bool,
    show_history: bool,
    properties: bool,
    status: String,
    cached_pages: Vec<Page>,
    cached_score: Option<Score>,
    cached_selection: Option<Selection>,
    cached_filter: Option<usize>,
    preview_sample: Option<(Instrument, u8)>,
    audition_playback: Option<synth::Playback>,
    audition_worker: Option<mpsc::Receiver<AuditionResult>>,
    audio_engine: AudioEngine,
    sampled_bank: Option<SampleBank>,
    bank_worker: Option<mpsc::Receiver<Result<SampleBank, String>>>,
    lab_open: bool,
    lab_instrument: Instrument,
    lab_midi: u8,
    lab_velocity: u8,
    lab_hold: f32,
    library_search: String,
    library_family: Option<&'static str>,
    library_reveal: bool,
    keyboard_base: u8,
    keyboard_instrument: Instrument,
}

impl EncoreApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        configure_context(&cc.egui_ctx);
        let mut app = Self::from_score(
            Score::demo(),
            None,
            "Welcome to Encore. Press Space to hear First Light, or create a new score.".into(),
        );
        if let Some(path) = path
            && let Err(error) = app.begin_open_document(path)
        {
            app.status = format!("Could not open score: {error}");
        }
        app.theme = Theme::from_storage(cc.storage);
        app.entry_profile = EntryProfile::from_key(
            cc.storage
                .and_then(|s| s.get_string("encore.entry-profile"))
                .as_deref(),
        );
        apply_theme(&cc.egui_ctx, app.theme);
        let (engine, warning) = AudioEngine::load_default();
        if let AudioEngine::Sampled(bank) = &engine {
            app.sampled_bank = Some(bank.clone());
        }
        app.audio_engine = engine;
        if let Some(warning) = warning {
            app.status = warning;
        }
        app
    }

    fn from_score(score: Score, path: Option<PathBuf>, status: String) -> Self {
        Self {
            saved: score.clone(),
            score,
            path,
            undo: Vec::new(),
            redo: Vec::new(),
            skip_history: false,
            selected: None,
            active_part: 0,
            active_layer: 0,
            cursor_measure: 0,
            cursor_tick: 0,
            duration: Duration::Quarter,
            dotted: false,
            rest: false,
            accidental: None,
            octave: 4,
            entry_pitch: Pitch::from_midi(60),
            entry_profile: EntryProfile::default(),
            tuplet: None,
            tuplet_remaining: None,
            write_mode: false,
            zoom: 0.9,
            part_view: false,
            show_parts: false,
            selected_linked_part: 0,
            previous_part_view: None,
            notation_editor: Default::default(),
            master: 0.8,
            theme: Theme::Dark,
            needs_native_save: false,
            import_warnings: Vec::new(),
            show_import_report: false,
            midi_import: None,
            midi_grid: None,
            midi_import_error: None,
            playback: None,
            worker: None,
            rendering_audio: false,
            pending: None,
            help: false,
            show_history: false,
            properties: false,
            status,
            cached_pages: Vec::new(),
            cached_score: None,
            cached_selection: None,
            cached_filter: None,
            preview_sample: None,
            audition_playback: None,
            audition_worker: None,
            audio_engine: AudioEngine::Procedural,
            sampled_bank: None,
            bank_worker: None,
            lab_open: false,
            lab_instrument: Instrument::Piano,
            lab_midi: 60,
            lab_velocity: 96,
            lab_hold: 1.5,
            library_search: String::new(),
            library_family: None,
            library_reveal: true,
            keyboard_base: 48,
            keyboard_instrument: Instrument::Piano,
        }
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: Theme) {
        self.theme = theme;
        apply_theme(ctx, theme);
    }

    fn dirty(&self) -> bool {
        self.needs_native_save || self.score != self.saved
    }

    fn request(&mut self, action: Action, ctx: &egui::Context) {
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action, ctx);
        }
    }

    fn perform(&mut self, action: Action, ctx: &egui::Context) {
        let loaded = match action {
            Action::New => Some((Score::blank(), None)),
            Action::Demo => Some((Score::demo(), None)),
            Action::CarolDemo => Some((Score::carol_demo(), None)),
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Scores",
                        &[
                            "encore", "json", "musicxml", "xml", "mxl", "mscx", "mscz", "musx",
                            "mus", "mid", "midi",
                        ],
                    )
                    .add_filter("Encore projects", &["encore", "json"])
                    .add_filter("MusicXML", &["musicxml", "xml", "mxl"])
                    .add_filter("MuseScore", &["mscx", "mscz"])
                    .add_filter("Finale", &["musx", "mus"])
                    .add_filter("MIDI performance", &["mid", "midi"])
                    .pick_file()
                    && let Err(error) = self.begin_open_document(path)
                {
                    self.status = format!("Could not open score: {error}");
                }
                None
            }
            Action::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.saved = self.score.clone();
                self.needs_native_save = false;
                None
            }
        };
        if let Some((score, path)) = loaded {
            self.replace_document(score, path);
        }
    }

    fn replace_document(&mut self, score: Score, path: Option<PathBuf>) {
        self.stop();
        self.score = score;
        self.saved = self.score.clone();
        self.path = path;
        self.needs_native_save = false;
        self.import_warnings.clear();
        self.show_import_report = false;
        self.midi_import = None;
        self.midi_import_error = None;
        self.selected = None;
        self.active_part = 0;
        self.active_layer = 0;
        self.duration = Duration::Quarter;
        self.dotted = false;
        self.rest = false;
        self.accidental = None;
        self.entry_pitch = Pitch::from_midi(60);
        self.octave = 4;
        self.tuplet = None;
        self.tuplet_remaining = None;
        self.show_history = false;
        self.show_parts = false;
        self.selected_linked_part = 0;
        self.previous_part_view = None;
        self.part_view = false;
        self.cached_score = None;
        self.cached_filter = None;
        self.notation_editor = Default::default();
        self.cursor_measure = 0;
        self.cursor_tick = 0;
        self.undo.clear();
        self.redo.clear();
        self.skip_history = true;
        self.status = "Ready. Choose Write and click a staff, or enter notes with A–G.".into();
    }

    fn begin_open_document(&mut self, path: PathBuf) -> Result<(), String> {
        if crate::midi_options::is_midi(&path) {
            self.midi_import = Some(path);
            self.midi_grid = None;
            self.midi_import_error = None;
            self.properties = false;
            self.help = false;
            self.show_history = false;
            self.show_import_report = false;
            Ok(())
        } else {
            self.open_document(path)
        }
    }

    fn open_document(&mut self, path: PathBuf) -> Result<(), String> {
        let result = encore::import::load(&path)?;
        self.accept_import(path, result);
        Ok(())
    }

    fn accept_import(&mut self, path: PathBuf, result: encore::import::ImportResult) {
        let native = path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("encore") || s.eq_ignore_ascii_case("json"));
        let label = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.replace_document(result.score, native.then_some(path));
        self.needs_native_save = !native;
        self.import_warnings = result.warnings;
        self.show_import_report = !self.import_warnings.is_empty();
        self.status = if native {
            format!("Opened {label}")
        } else {
            format!(
                "Imported {label}. Save as an Encore project{}.",
                if self.import_warnings.is_empty() {
                    ""
                } else {
                    "; review the import notes"
                }
            )
        };
    }

    fn save(&mut self, choose: bool) -> bool {
        let path = if choose || self.path.is_none() {
            rfd::FileDialog::new()
                .add_filter("Encore score", &["encore"])
                .set_file_name(format!("{}.encore", safe_filename(&self.score.title)))
                .save_file()
        } else {
            self.path.clone()
        };
        if let Some(path) = path {
            match self.score.save(&path) {
                Ok(()) => {
                    self.status = format!("Saved {}", path.display());
                    self.path = Some(path);
                    self.saved = self.score.clone();
                    self.needs_native_save = false;
                    true
                }
                Err(e) => {
                    self.status = format!("Save failed: {e}");
                    false
                }
            }
        } else {
            false
        }
    }

    fn export(&mut self, ext: &str) {
        if ext == "wav" && synth::duration_seconds(&self.score) > synth::MAX_RENDER_SECONDS {
            self.status="Audio export supports scores up to 10 minutes. Other exports support longer scores.".into();
            return;
        }
        if self.worker.is_some() {
            self.status = "Please wait for the current render to finish.".into();
            return;
        }
        let mut score = self.score.clone();
        let mut layout = None;
        if self.part_view && ["pdf", "svg"].contains(&ext) {
            match self.selected_part_projection() {
                Ok(Some(projection)) => {
                    score = projection.score;
                    layout = Some(projection.layout);
                }
                Ok(None) => {}
                Err(error) => {
                    self.status = error;
                    return;
                }
            }
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(format!("{}.{}", safe_filename(&score.title), ext))
            .save_file()
        {
            let (tx, rx) = mpsc::channel();
            self.worker = Some(rx);
            self.status = format!("Exporting {}…", path.display());
            let engine = self.audio_engine.clone();
            std::thread::spawn(move || {
                let result =
                    crate::export_to_with_layout(&score, &path, &engine, layout).map(|()| path);
                let _ = tx.send(JobResult::Export(result));
            });
        }
    }

    fn stop_audition(&mut self) {
        self.audition_worker = None;
        if let Some(p) = self.audition_playback.as_mut() {
            p.stop();
        }
    }

    fn set_audio_engine(&mut self, engine: AudioEngine) {
        self.stop();
        if let AudioEngine::Sampled(bank) = &engine {
            self.sampled_bank = Some(bank.clone());
        }
        self.status = format!("{} selected. Press Play to hear the score.", engine.name());
        self.audio_engine = engine;
    }

    fn load_soundfont(&mut self) {
        if self.bank_worker.is_some() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SoundFont 2", &["sf2"])
            .pick_file()
        {
            let (tx, rx) = mpsc::channel();
            self.bank_worker = Some(rx);
            self.status = format!("Loading SoundFont {}…", path.display());
            std::thread::spawn(move || {
                let _ = tx.send(SampleBank::load(&path));
            });
        }
    }

    fn audition(&mut self, instrument: Instrument, midi: Option<u8>, velocity: u8, hold: f32) {
        self.stop_audition();
        if self.audition_playback.is_none() {
            match synth::Playback::new() {
                Ok(p) => self.audition_playback = Some(p),
                Err(e) => {
                    self.status = format!(
                        "Audio unavailable: {e}. You can still export instrument WAV files."
                    );
                    return;
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        self.audition_worker = Some(rx);
        let label = match midi {
            Some(m) => format!("{} · {}", instrument.name(), Pitch::from_midi(m).name()),
            None => format!("{} · test phrase", instrument.name()),
        };
        self.status = format!("Preparing {label}…");
        let engine = self.audio_engine.clone();
        std::thread::spawn(move || {
            let audio = if let Some(m) = midi {
                engine.render_note(instrument, m, velocity, hold)
            } else {
                engine.render(&instruments::audition_phrase(instrument, velocity))
            };
            let _ = tx.send(audio.map(|samples| (samples, label)));
        });
    }

    fn export_audition(&mut self) {
        if self.worker.is_some() {
            self.status = "Please wait for the current export to finish.".into();
            return;
        }
        let score = instruments::audition_phrase(self.lab_instrument, self.lab_velocity);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV audio", &["wav"])
            .set_file_name(format!("{} audition.wav", self.lab_instrument.name()))
            .save_file()
        {
            let (tx, rx) = mpsc::channel();
            self.worker = Some(rx);
            self.status = "Exporting instrument audition…".into();
            let engine = self.audio_engine.clone();
            std::thread::spawn(move || {
                let result = crate::export_to_with_engine(&score, &path, &engine).map(|()| path);
                let _ = tx.send(JobResult::Export(result));
            });
        }
    }

    fn stop(&mut self) {
        self.stop_audition();
        if self.rendering_audio {
            self.worker = None;
            self.rendering_audio = false;
        }
        if let Some(p) = self.playback.as_mut() {
            p.stop();
        }
    }

    fn toggle_play(&mut self) {
        if self.worker.is_some() {
            return;
        }
        self.stop_audition();
        if let Err(e) = self.score.validate() {
            self.status = format!("Cannot play: {e}");
            return;
        }
        if synth::duration_seconds(&self.score) > synth::MAX_RENDER_SECONDS {
            self.status = "Audio rendering supports scores up to 10 minutes. PDF, SVG, MusicXML and MIDI exports support longer scores.".into();
            return;
        }
        if let Some(p) = self.playback.as_mut() {
            if p.is_paused() {
                p.resume();
                return;
            }
            if p.is_playing() {
                p.pause();
                return;
            }
        }
        if self.playback.is_none() {
            match synth::Playback::new() {
                Ok(p) => self.playback = Some(p),
                Err(e) => {
                    self.status = format!("Audio unavailable: {e}. WAV export is still available.");
                    return;
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        self.worker = Some(rx);
        let score = self.score.clone();
        self.rendering_audio = true;
        self.status = "Rendering orchestra…".into();
        let engine = self.audio_engine.clone();
        std::thread::spawn(move || {
            let audio = engine.render(&score);
            let _ = tx.send(JobResult::Audio(audio));
        });
    }

    fn poll_worker(&mut self) {
        if let Some(rx) = &self.bank_worker {
            match rx.try_recv() {
                Ok(Ok(bank)) => {
                    self.bank_worker = None;
                    self.set_audio_engine(AudioEngine::Sampled(bank));
                }
                Ok(Err(error)) => {
                    self.bank_worker = None;
                    self.status = format!("SoundFont load failed: {error}");
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.bank_worker = None;
                    self.status = "SoundFont loader stopped unexpectedly.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.audition_worker {
            match rx.try_recv() {
                Ok(Ok((samples, label))) => {
                    self.audition_worker = None;
                    if let Some(p) = self.audition_playback.as_mut() {
                        p.set_volume(self.master);
                        p.play(samples);
                        self.status = format!("Auditioning {label}");
                    }
                }
                Ok(Err(error)) => {
                    self.audition_worker = None;
                    self.status = format!("Audition failed: {error}");
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.audition_worker = None;
                    self.status = "Instrument audition failed to render.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    self.rendering_audio = false;
                    match result {
                        JobResult::Audio(Ok(audio)) => {
                            if let Some(p) = self.playback.as_mut() {
                                p.set_volume(self.master);
                                p.play(audio);
                                self.status = "Playing score • Space to pause".into();
                            }
                        }
                        JobResult::Audio(Err(error)) => {
                            self.status = format!("Audio render failed: {error}");
                        }
                        JobResult::Export(Ok(path)) => {
                            self.status = format!("Exported {}", path.display())
                        }
                        JobResult::Export(Err(e)) => self.status = format!("Export failed: {e}"),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.rendering_audio = false;
                    self.status = "Audio/export worker stopped unexpectedly.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    fn clamp_cursor(&mut self) {
        self.active_part = self
            .active_part
            .min(self.score.parts.len().saturating_sub(1));
        self.cursor_measure = self
            .cursor_measure
            .min(self.score.measure_count().saturating_sub(1));
        if self.cursor_tick >= self.score.measure_ticks() {
            self.cursor_tick = 0;
        }
    }

    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.stop();
            self.redo.push(std::mem::replace(&mut self.score, s));
            self.selected = None;
            self.clamp_cursor();
            self.skip_history = true;
            self.status = "Undo".into();
        }
    }
    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.stop();
            self.undo.push(std::mem::replace(&mut self.score, s));
            self.selected = None;
            self.clamp_cursor();
            self.skip_history = true;
            self.status = "Redo".into();
        }
    }

    fn delete_selected(&mut self) {
        if let Some(anchor) = self
            .selected
            .and_then(|selection| encore::document::Anchor::from_selection(&self.score, selection))
        {
            encore::document::remove_anchor(&mut self.score, &anchor);
        }
        if let Some(sel) = self.selected.take()
            && let Some(m) = self
                .score
                .parts
                .get_mut(sel.part)
                .and_then(|p| p.measures.get_mut(sel.measure))
            && sel.note < m.notes.len()
        {
            let n = m.notes.remove(sel.note);
            self.cursor_measure = sel.measure;
            self.cursor_tick = n.start;
            self.status = "Note removed".into();
        }
    }

    fn trim_last_empty_bar(&mut self) -> Result<(), String> {
        if self.score.measure_count() <= 1
            || self
                .score
                .parts
                .iter()
                .any(|part| part.measures.last().is_none_or(|bar| !bar.notes.is_empty()))
        {
            return Err("Only a final empty measure can be removed here.".into());
        }
        let mut candidate = self.score.clone();
        for staff in &mut candidate.parts {
            staff.measures.pop();
        }
        encore::document::truncate_measures(&mut candidate)
            .map_err(|error| format!("Cannot remove this bar: {error}"))?;
        self.score = candidate;
        self.selected = None;
        self.clamp_cursor();
        Ok(())
    }

    /// Follow deliberate staff selection while preserving an explicit part
    /// switch made in the header during this frame.
    fn reconcile_part_staff(&mut self) {
        if !self.part_view || self.cached_filter != Some(self.selected_linked_part) {
            return;
        }
        let definitions = encore::parts::definitions(&self.score);
        if definitions
            .get(self.selected_linked_part)
            .is_some_and(|part| part.staves.contains(&self.active_part))
        {
            return;
        }
        if let Some(index) = definitions
            .iter()
            .position(|part| part.staves.contains(&self.active_part))
        {
            self.selected_linked_part = index;
        } else {
            self.part_view = false;
        }
        self.selected = None;
    }

    fn enter_note(
        &mut self,
        part: usize,
        measure: usize,
        start: u32,
        mut pitch: Pitch,
        chord: bool,
    ) {
        if part >= self.score.parts.len() || measure >= self.score.measure_count() {
            return;
        }
        if let Some(alter) = self.accidental {
            pitch.alter = alter;
        }
        let raw_midi = (pitch.octave as i32 + 1) * 12 + pitch.step.semitone() + pitch.alter as i32;
        if !(0..=127).contains(&raw_midi) {
            self.status = "Pitch is outside the supported MIDI range (C−1 to G9).".into();
            return;
        }
        let proposed = Note {
            start,
            duration: self.duration,
            dotted: self.dotted,
            tuplet: self.tuplet,
            layer: self.active_layer,
            pitches: if self.rest { vec![] } else { vec![pitch] },
            velocity: 88,
            staccato: false,
            articulations: vec![],
            tied: false,
        };
        let ticks = match proposed.checked_ticks() {
            Ok(ticks) => ticks,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let measure_ticks = self.score.measure_ticks();
        let m = &mut self.score.parts[part].measures[measure];
        if chord
            && !self.rest
            && let Some(i) = m
                .notes
                .iter()
                .position(|n| n.start == start && n.layer == self.active_layer)
        {
            if !m.notes[i].pitches.iter().any(|p| p.midi() == pitch.midi())
                && m.notes[i].pitches.len() < 16
            {
                m.notes[i].pitches.push(pitch);
            }
            self.selected = Some(Selection {
                part,
                measure,
                note: i,
            });
            self.status = "Added chord tone".into();
            return;
        }
        if start + ticks > measure_ticks {
            self.status = "That duration crosses the barline. Choose a shorter note or enter tied notes in the next measure.".into();
            return;
        }
        if m.notes.iter().any(|n| {
            n.layer == self.active_layer
                && n.start != start
                && start < n.start + n.ticks()
                && start + ticks > n.start
        }) {
            self.status =
                "This note overlaps another event. Delete that event or choose a shorter duration."
                    .into();
            return;
        }
        m.notes
            .retain(|n| n.start != start || n.layer != self.active_layer);
        m.notes.push(proposed);
        m.notes.sort_by_key(|n| n.start);
        let i = m
            .notes
            .iter()
            .position(|n| n.start == start && n.layer == self.active_layer)
            .unwrap();
        self.selected = Some(Selection {
            part,
            measure,
            note: i,
        });
        self.active_part = part;
        self.cursor_tick = start + ticks;
        self.cursor_measure = measure;
        if self.cursor_tick >= self.score.measure_ticks() {
            self.cursor_tick = 0;
            self.cursor_measure += 1;
        }
        if self.cursor_measure >= self.score.measure_count() {
            self.score.add_measures(1);
            if self.cursor_measure >= self.score.measure_count() {
                self.cursor_measure = self.score.measure_count() - 1;
                self.cursor_tick = self.score.measure_ticks();
            }
        }
        self.entry_pitch = pitch;
        self.octave = self
            .entry_written_pitch(pitch)
            .map_or(pitch.octave, |written| written.octave);
        if let Some(remaining) = self.tuplet_remaining {
            if remaining <= 1 {
                self.tuplet = None;
                self.tuplet_remaining = None;
            } else {
                self.tuplet_remaining = Some(remaining - 1);
            }
        }
        if self.rest {
            encore::document::remove_anchor(
                &mut self.score,
                &encore::document::Anchor {
                    part,
                    measure,
                    tick: start,
                    layer: self.active_layer,
                },
            );
        }
        self.status = format!(
            "{} entered • next: measure {}, beat {:.2}",
            if self.rest {
                "Rest".into()
            } else {
                pitch.name()
            },
            self.cursor_measure + 1,
            self.cursor_tick as f32 / PPQ as f32 + 1.0
        );
    }

    fn transpose(&mut self, semitones: i8) {
        if let Some(s) = self.selected {
            let n = &mut self.score.parts[s.part].measures[s.measure].notes[s.note];
            if n.pitches
                .iter()
                .any(|p| !(0..=127).contains(&(p.midi() as i16 + semitones as i16)))
            {
                self.status = "Transposition would move this chord outside the MIDI range.".into();
                return;
            }
            for p in &mut n.pitches {
                *p = Pitch::from_midi((p.midi() as i16 + semitones as i16) as u8);
            }
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.stop();
            self.properties = false;
            self.help = false;
            self.show_history = false;
            self.show_import_report = false;
            self.show_parts = false;
            self.notation_editor = Default::default();
            if self.midi_import.take().is_some() {
                self.midi_import_error = None;
                return;
            }
            if !self.lab_open && self.pending.is_none() {
                self.selected = None;
                self.write_mode = false;
            }
            return;
        }
        if self.pending.is_some() || self.lab_open || self.midi_import.is_some() {
            return;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save(ctx.input(|i| i.modifiers.shift));
            return;
        }
        if ctx.wants_keyboard_input()
            || self.properties
            || self.help
            || self.show_history
            || self.show_import_report
            || self.show_parts
        {
            return;
        }
        if ctx.input(|input| {
            input.modifiers.command && input.modifiers.alt && input.key_pressed(egui::Key::Slash)
        }) {
            if let Some(previous) = self.previous_part_view {
                self.part_view = previous.is_some();
                if let Some(index) = previous {
                    self.selected_linked_part = index;
                }
            }
            return;
        }
        if self.annotation_keyboard(ctx) {
            return;
        }
        if ctx.input(|input| {
            input.modifiers.command
                && input.modifiers.alt
                && (input.key_pressed(egui::Key::Period) || input.key_pressed(egui::Key::Comma))
        }) {
            let count = encore::parts::definitions(&self.score).len();
            if count > 0 {
                self.selected_linked_part =
                    if ctx.input(|input| input.key_pressed(egui::Key::Comma)) {
                        (self.selected_linked_part + count - 1) % count
                    } else {
                        (self.selected_linked_part + 1) % count
                    };
                self.part_view = true;
            }
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        let mac = cfg!(target_os = "macos");
        for event in events {
            if self.pending.is_some()
                || self.midi_import.is_some()
                || self.lab_open
                || self.properties
                || self.help
                || self.show_history
                || self.show_import_report
            {
                break;
            }
            match &event {
                egui::Event::Copy => {
                    self.copy_selection(ctx, false);
                    continue;
                }
                egui::Event::Cut => {
                    self.copy_selection(ctx, true);
                    continue;
                }
                egui::Event::Paste(text) => {
                    self.paste_selection(text);
                    continue;
                }
                _ => {}
            }
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if modifiers.command && !modifiers.alt {
                let handled = match key {
                    egui::Key::N => {
                        self.request(Action::New, ctx);
                        true
                    }
                    egui::Key::O => {
                        self.request(Action::Open, ctx);
                        true
                    }
                    egui::Key::Q => {
                        self.request(Action::Quit, ctx);
                        true
                    }
                    egui::Key::Z if modifiers.shift && !mac => {
                        self.show_history = true;
                        true
                    }
                    egui::Key::Z if modifiers.shift => {
                        self.redo();
                        true
                    }
                    egui::Key::Z => {
                        self.undo();
                        true
                    }
                    egui::Key::Y if mac => {
                        self.show_history = true;
                        true
                    }
                    egui::Key::Y => {
                        self.redo();
                        true
                    }
                    egui::Key::C => {
                        self.copy_selection(ctx, false);
                        true
                    }
                    egui::Key::X => {
                        self.copy_selection(ctx, true);
                        true
                    }
                    _ => false,
                };
                if handled {
                    if self.skip_history {
                        return;
                    }
                    continue;
                }
            }
            if key == egui::Key::Space && !modifiers.command && !modifiers.alt {
                self.toggle_play();
                continue;
            }
            if key == egui::Key::F1 {
                self.help = true;
                continue;
            }
            if !self.write_mode {
                if key == egui::Key::Delete {
                    self.delete_selected();
                }
                continue;
            }
            if let Some(command) = shortcuts::entry_command(self.entry_profile, key, modifiers, mac)
            {
                self.apply_entry_command(command);
                ctx.input_mut(|input| {
                    input.consume_key(modifiers, key);
                });
                ctx.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
            }
        }
    }

    fn top(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        let primary = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        let redo_key = if cfg!(target_os = "macos") {
            "Cmd+Shift+Z"
        } else {
            "Ctrl+Y"
        };
        let history_key = if cfg!(target_os = "macos") {
            "Cmd+Y"
        } else {
            "Ctrl+Shift+Z"
        };
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Frame::new()
                        .fill(colors.accent)
                        .corner_radius(10)
                        .inner_margin(egui::Margin::symmetric(10, 1))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("e")
                                    .size(28.0)
                                    .strong()
                                    .color(colors.workspace),
                            );
                        });
                    ui.label(RichText::new("encore").size(23.0).strong());
                    ui.add_space(18.0);
                    egui::MenuBar::new().ui(ui, |ui| {
                        ui.menu_button("File", |ui| {
                            if ui
                                .add(
                                    egui::Button::new("New score")
                                        .shortcut_text(format!("{primary}+N")),
                                )
                                .clicked()
                            {
                                ui.close();
                                self.request(Action::New, ctx);
                            }
                            if ui
                                .add(
                                    egui::Button::new("Open…")
                                        .shortcut_text(format!("{primary}+O")),
                                )
                                .clicked()
                            {
                                ui.close();
                                self.request(Action::Open, ctx);
                            }
                            if ui
                                .add(
                                    egui::Button::new("Save").shortcut_text(format!("{primary}+S")),
                                )
                                .clicked()
                            {
                                ui.close();
                                self.save(false);
                            }
                            if ui
                                .add(
                                    egui::Button::new("Save as…")
                                        .shortcut_text(format!("{primary}+Shift+S")),
                                )
                                .clicked()
                            {
                                ui.close();
                                self.save(true);
                            }
                            ui.separator();
                            if ui.button("Demo: First Light").clicked() {
                                ui.close();
                                self.request(Action::Demo, ctx);
                            }
                            if ui.button("Demo: Carol of the Bells — Epic").clicked() {
                                ui.close();
                                self.request(Action::CarolDemo, ctx);
                            }
                            ui.separator();
                            if ui.button("Quit").clicked() {
                                ui.close();
                                self.request(Action::Quit, ctx);
                            }
                        });
                        ui.menu_button("Edit", |ui| {
                            if ui
                                .add_enabled(
                                    !self.undo.is_empty(),
                                    egui::Button::new("Undo").shortcut_text(format!("{primary}+Z")),
                                )
                                .clicked()
                            {
                                self.undo();
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    !self.redo.is_empty(),
                                    egui::Button::new("Redo").shortcut_text(redo_key),
                                )
                                .clicked()
                            {
                                self.redo();
                                ui.close();
                            }
                            if ui
                                .add(
                                    egui::Button::new("Undo/Redo list…").shortcut_text(history_key),
                                )
                                .clicked()
                            {
                                self.show_history = true;
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    self.selected.is_some(),
                                    egui::Button::new("Cut event")
                                        .shortcut_text(format!("{primary}+X")),
                                )
                                .clicked()
                            {
                                self.copy_selection(ctx, true);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    self.selected.is_some(),
                                    egui::Button::new("Copy event")
                                        .shortcut_text(format!("{primary}+C")),
                                )
                                .clicked()
                            {
                                self.copy_selection(ctx, false);
                                ui.close();
                            }
                            if ui
                                .add(
                                    egui::Button::new("Paste event at cursor")
                                        .shortcut_text(format!("{primary}+V")),
                                )
                                .clicked()
                            {
                                ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    self.selected.is_some(),
                                    egui::Button::new("Delete selection"),
                                )
                                .clicked()
                            {
                                self.delete_selected();
                                ui.close();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    !self.import_warnings.is_empty(),
                                    egui::Button::new("Import notes…"),
                                )
                                .clicked()
                            {
                                self.show_import_report = true;
                                ui.close();
                            }
                            if ui.button("Score properties…").clicked() {
                                self.properties = true;
                                ui.close();
                            }
                        });
                        ui.menu_button("Notation", |ui| {
                            if ui.button("Lyrics…").clicked() {
                                self.open_lyrics();
                                ui.close();
                            }
                            if ui.button("Slurs…").clicked() {
                                self.open_slurs();
                                ui.close();
                            }
                            if ui.button("Repeats and endings…").clicked() {
                                self.open_repeats();
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Manage linked parts…").clicked() {
                                self.show_parts = true;
                                ui.close();
                            }
                            if ui.button("Export all linked parts as PDF…").clicked() {
                                self.export_all_linked_parts();
                                ui.close();
                            }
                        });
                        ui.menu_button("Export", |ui| {
                            for (label, ext) in [
                                ("PDF sheet music", "pdf"),
                                ("SVG sheet music", "svg"),
                                ("MusicXML interchange", "musicxml"),
                                ("MIDI performance", "mid"),
                                ("WAV audio", "wav"),
                            ] {
                                if ui.button(label).clicked() {
                                    ui.close();
                                    self.export(ext);
                                }
                            }
                        });
                        if ui.button("Instruments").clicked() {
                            self.lab_open = true;
                            self.library_reveal = true;
                            self.lab_instrument = self.score.parts[self.active_part].instrument;
                            self.lab_midi = instruments::profile(self.lab_instrument).audition;
                        }
                        if ui.button("Guide").clicked() {
                            self.help = true;
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Export PDF").color(colors.workspace).strong(),
                                )
                                .fill(colors.accent),
                            )
                            .clicked()
                        {
                            self.export("pdf");
                        }
                        let toggle = if self.theme == Theme::Dark {
                            "Light mode"
                        } else {
                            "Dark mode"
                        };
                        if ui
                            .button(toggle)
                            .on_hover_text("Switch appearance; score paper stays white")
                            .clicked()
                        {
                            self.set_theme(
                                ctx,
                                if self.theme == Theme::Dark {
                                    Theme::Light
                                } else {
                                    Theme::Dark
                                },
                            );
                        }
                        if ui.button("Save").clicked() {
                            self.save(false);
                        }
                        ui.label(
                            RichText::new(if self.dirty() {
                                "●  Unsaved changes"
                            } else if self.path.is_none() {
                                "●  Not saved to disk"
                            } else {
                                "●  All changes saved"
                            })
                            .size(12.0)
                            .color(colors.muted),
                        );
                    });
                });
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    let compact = ui.available_width() < 1150.0;
                    let title_width = ui
                        .fonts_mut(|fonts| {
                            fonts
                                .layout_no_wrap(
                                    self.score.title.clone(),
                                    FontId::proportional(18.0),
                                    colors.text,
                                )
                                .size()
                                .x
                        })
                        .min(if compact { 250.0 } else { 430.0 });
                    ui.add_sized(
                        [title_width, 26.0],
                        egui::Label::new(RichText::new(&self.score.title).size(18.0).strong())
                            .truncate(),
                    )
                    .on_hover_text(&self.score.title);
                    if ui.small_button("Edit details").clicked() {
                        self.properties = true;
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(if compact {
                            format!(
                                "{} staves · {} measures",
                                self.score.parts.len(),
                                self.score.measure_count()
                            )
                        } else {
                            format!(
                                "{} staves  ·  {} measures  ·  {}",
                                self.score.parts.len(),
                                self.score.measure_count(),
                                key_name(self.score.key_fifths)
                            )
                        })
                        .size(12.0)
                        .color(colors.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("+").clicked() {
                            self.zoom = (self.zoom + 0.1).min(2.0);
                        }
                        ui.label(format!("{:.0}%", self.zoom * 100.0));
                        if ui.button("−").clicked() {
                            self.zoom = (self.zoom - 0.1).max(0.4);
                        }
                        ui.separator();
                        if ui.small_button("Parts…").clicked() {
                            self.show_parts = true;
                        }
                        let definitions = encore::parts::definitions(&self.score);
                        self.selected_linked_part = self
                            .selected_linked_part
                            .min(definitions.len().saturating_sub(1));
                        if self.part_view {
                            egui::ComboBox::from_id_salt("view-part")
                                .selected_text(
                                    definitions
                                        .get(self.selected_linked_part)
                                        .map(|part| part.name.as_str())
                                        .unwrap_or("Part"),
                                )
                                .width(135.0)
                                .show_ui(ui, |ui| {
                                    for (index, part) in definitions.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut self.selected_linked_part,
                                            index,
                                            &part.name,
                                        );
                                    }
                                });
                        }
                        ui.selectable_value(&mut self.part_view, true, "Part");
                        ui.selectable_value(&mut self.part_view, false, "Full score");
                    });
                });
            });
        egui::TopBottomPanel::top("notation-tools")
            .frame(
                egui::Frame::new()
                    .fill(colors.raised)
                    .inner_margin(egui::Margin::symmetric(20, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(
                            !self.write_mode && self.notation_editor.tool == notation::Tool::Select,
                            "Select",
                        )
                        .clicked()
                    {
                        self.write_mode = false;
                        self.notation_editor.tool = notation::Tool::Select;
                    }
                    let write = ui
                        .selectable_value(&mut self.write_mode, true, "Write")
                        .on_hover_text(
                            "Click a staff to insert; Shift-click an event to add a chord tone",
                        );
                    if write.clicked() {
                        self.notation_editor.tool = notation::Tool::Select;
                        write.surrender_focus();
                    }
                    if ui
                        .selectable_label(
                            self.notation_editor.tool == notation::Tool::Lyrics,
                            "Lyrics",
                        )
                        .clicked()
                    {
                        self.open_lyrics();
                    }
                    if ui
                        .selectable_label(
                            self.notation_editor.tool == notation::Tool::Slurs,
                            "Slurs",
                        )
                        .clicked()
                    {
                        self.open_slurs();
                    }
                    if ui.button("Repeats…").clicked() {
                        self.open_repeats();
                    }
                    ui.separator();
                    egui::ComboBox::from_id_salt("active-layer")
                        .selected_text(format!("Layer {}", self.active_layer + 1))
                        .show_ui(ui, |ui| {
                            for layer in 0..4 {
                                if ui
                                    .selectable_value(
                                        &mut self.active_layer,
                                        layer,
                                        format!("Layer {}", layer + 1),
                                    )
                                    .changed()
                                {
                                    self.selected = None;
                                }
                            }
                        });
                    for d in Duration::ALL {
                        ui.selectable_value(
                            &mut self.duration,
                            *d,
                            format!(
                                "{} {}",
                                shortcuts::duration_digit(*d),
                                match d {
                                    Duration::DoubleWhole => "Breve",
                                    Duration::Eighth => "8th",
                                    Duration::Sixteenth => "16th",
                                    _ => d.name(),
                                }
                            ),
                        )
                        .on_hover_text("Duration for new notes");
                    }
                    ui.separator();
                    ui.toggle_value(&mut self.dotted, "Dotted");
                    ui.toggle_value(&mut self.rest, "Rest · Alt+R");
                    let mut triplet = self.tuplet.is_some();
                    if ui.toggle_value(&mut triplet, "Triplet 3:2").changed() {
                        self.tuplet = triplet.then_some(Tuplet {
                            actual: 3,
                            normal: 2,
                        });
                        self.tuplet_remaining = None;
                    }
                    if ui.available_size_before_wrap().x < 165.0 {
                        ui.end_row();
                    }
                    egui::ComboBox::from_id_salt("entry-profile")
                        .width(150.0)
                        .selected_text(match self.entry_profile {
                            EntryProfile::SimpleLaptop => "Simple · Laptop",
                            EntryProfile::Speedy => "Speedy Entry",
                        })
                        .show_ui(ui, |ui| {
                            for profile in [EntryProfile::SimpleLaptop, EntryProfile::Speedy] {
                                ui.selectable_value(
                                    &mut self.entry_profile,
                                    profile,
                                    profile.name(),
                                );
                            }
                        });
                    ui.separator();
                    for (a, name) in [
                        (None, "Key"),
                        (Some(-1), "♭"),
                        (Some(0), "♮"),
                        (Some(1), "♯"),
                    ] {
                        ui.selectable_value(&mut self.accidental, a, name);
                    }
                });
            });
    }

    fn instruments(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        egui::SidePanel::left("instruments")
            .resizable(true)
            .default_width(217.0)
            .width_range(185.0..=300.0)
            .frame(egui::Frame::new().fill(colors.surface).inner_margin(16))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("INSTRUMENTS")
                            .size(11.0)
                            .strong()
                            .color(colors.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("+")
                            .on_hover_text("Browse 128 instruments and add a staff")
                            .clicked()
                        {
                            self.lab_open = true;
                            self.library_reveal = true;
                        }
                    });
                });
                ui.add_space(10.0);
                egui::ScrollArea::vertical()
                    .id_salt("part-list")
                    .max_height((ui.available_height() - 245.0).max(100.0))
                    .show(ui, |ui| {
                        for (index, part) in self.score.parts.iter_mut().enumerate() {
                            let active = self.active_part == index;
                            egui::Frame::new()
                                .fill(if active {
                                    colors.selected
                                } else {
                                    colors.surface
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if active {
                                        colors.accent.gamma_multiply(0.5)
                                    } else {
                                        colors.border
                                    },
                                ))
                                .inner_margin(10)
                                .corner_radius(7)
                                .show(ui, |ui| {
                                    if ui
                                        .selectable_label(
                                            active,
                                            RichText::new(&part.name).strong().color(if active {
                                                colors.accent
                                            } else {
                                                colors.text
                                            }),
                                        )
                                        .clicked()
                                    {
                                        self.active_part = index;
                                        self.selected = None;
                                        self.cursor_tick = 0;
                                    }
                                    ui.label(
                                        RichText::new(part.instrument.name())
                                            .size(11.0)
                                            .color(colors.muted),
                                    );
                                    ui.horizontal(|ui| {
                                        ui.toggle_value(&mut part.muted, "M")
                                            .on_hover_text("Mute staff");
                                        ui.toggle_value(&mut part.solo, "S")
                                            .on_hover_text("Solo staff");
                                        ui.add(
                                            egui::Slider::new(&mut part.volume, 0.0..=1.0)
                                                .show_value(false)
                                                .text(""),
                                        )
                                        .on_hover_text("Staff volume");
                                    });
                                });
                            ui.add_space(5.0);
                        }
                    });
                ui.add_space(15.0);
                ui.separator();
                ui.add_space(10.0);
                ui.label(
                    RichText::new("SCORE")
                        .size(11.0)
                        .strong()
                        .color(colors.muted),
                );
                ui.horizontal(|ui| {
                    ui.label("Tempo");
                    ui.add(
                        egui::DragValue::new(&mut self.score.tempo)
                            .range(20..=400)
                            .suffix(" BPM"),
                    );
                });
                if ui.button("+  Add 4 measures").clicked() {
                    self.score.add_measures(4);
                }
                let can_trim = self.score.measure_count() > 1
                    && self
                        .score
                        .parts
                        .iter()
                        .all(|p| p.measures.last().is_some_and(|m| m.notes.is_empty()));
                if ui
                    .add_enabled(can_trim, egui::Button::new("Remove last empty bar"))
                    .clicked()
                    && let Err(error) = self.trim_last_empty_bar()
                {
                    self.status = error;
                }
                if ui.button("Score properties").clicked() {
                    self.properties = true;
                }
                ui.add_space(18.0);
                ui.label(
                    RichText::new(if matches!(self.audio_engine, AudioEngine::Sampled(_)) {
                        "128 sampled instruments\nReady to play, entirely offline"
                    } else {
                        "Original procedural instruments\nNo sound library required"
                    })
                    .size(11.0)
                    .color(colors.muted),
                );
            });
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        let written_interval = self.entry_transposition();
        let previous_anchor = self
            .selected
            .and_then(|selection| encore::document::Anchor::from_selection(&self.score, selection));
        let previous_score = self.score.clone();
        egui::SidePanel::right("inspector").resizable(true).default_width(211.0).width_range(180.0..=300.0).frame(egui::Frame::new().fill(colors.surface).inner_margin(16)).show(ctx, |ui| {
            egui::ScrollArea::vertical().id_salt("inspector-scroll").show(ui,|ui| {
            ui.label(RichText::new("INSPECTOR").size(11.0).strong().color(colors.muted)); ui.add_space(15.0);
            let mut remove = false;
            if let Some(s) = self.selected {
                if let Some(n) = self.score.parts.get_mut(s.part).and_then(|p| p.measures.get_mut(s.measure)).and_then(|m| m.notes.get_mut(s.note)) {
                    ui.label(RichText::new(if n.pitches.is_empty() { "Rest".into() } else { n.pitches.iter().map(|p| p.name()).collect::<Vec<_>>().join(" · ") }).size(21.0).strong());
                    ui.label(RichText::new(format!("Measure {}  ·  Beat {:.2}", s.measure+1,n.start as f32 / PPQ as f32 + 1.0)).size(12.0).color(colors.muted));
                    ui.add_space(14.0);
                    let original = n.clone();
                    if n.pitches.len()>1 {egui::ComboBox::from_id_salt("chord-pitch").selected_text(format!("Edit pitch: {}",self.entry_pitch.name())).show_ui(ui,|ui|{for pitch in &n.pitches{if ui.selectable_value(&mut self.entry_pitch,*pitch,pitch.name()).changed(){self.octave=written_interval.written_pitch(*pitch).map_or(pitch.octave,|written|written.octave);}}});}
                    egui::ComboBox::from_label("Duration").selected_text(n.duration.name()).show_ui(ui, |ui| { for &d in Duration::ALL { ui.selectable_value(&mut n.duration, d, d.name()); } });
                    egui::ComboBox::from_id_salt("note-layer").selected_text(format!("Layer {}",n.layer+1)).show_ui(ui,|ui|{for layer in 0..4{ui.selectable_value(&mut n.layer,layer,format!("Layer {}",layer+1));}});
                    ui.checkbox(&mut n.dotted, "Dotted");
                    let mut is_tuplet=n.tuplet.is_some();
                    if ui.checkbox(&mut is_tuplet,"Tuplet").changed() {n.tuplet=is_tuplet.then_some(Tuplet{actual:3,normal:2});}
                    if let Some(ratio)=&mut n.tuplet {ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut ratio.actual).range(2..=16));ui.label("in the time of");ui.add(egui::DragValue::new(&mut ratio.normal).range(1..=16));
                    });}
                    if let Err(error)=n.checked_ticks() {*n=original.clone();self.status=error;}
                    ui.checkbox(&mut n.staccato, "Staccato");
                    for articulation in [Articulation::Accent,Articulation::Marcato,Articulation::Tenuto,Articulation::Staccatissimo] {
                        let mut enabled=n.articulations.contains(&articulation);
                        if ui.checkbox(&mut enabled,articulation.name()).changed() {
                            if enabled {n.articulations.push(articulation);} else {n.articulations.retain(|a|*a!=articulation);}
                        }
                    }
                    ui.add_enabled(!n.pitches.is_empty(), egui::Checkbox::new(&mut n.tied, "Tie to next note"));
                    ui.add_space(10.0); ui.label("Dynamics"); ui.add(egui::Slider::new(&mut n.velocity, 1..=127).text("velocity"));
                    ui.horizontal_wrapped(|ui| { for (name,v) in [("pp",32),("p",48),("mp",64),("mf",80),("f",100),("ff",116)] { if ui.small_button(name).clicked() { n.velocity=v; } } });
                    if n.ticks() + n.start > self.score.measure_ticks() { self.score.parts[s.part].measures[s.measure].notes[s.note] = original.clone(); self.status = "That duration extends past the barline.".into(); }
                    let m = &self.score.parts[s.part].measures[s.measure];
                    let n = &m.notes[s.note];
                    if m.notes.iter().enumerate().any(|(i,x)| i!=s.note && n.layer==x.layer && n.start < x.start+x.ticks() && n.start+n.ticks()>x.start) { self.score.parts[s.part].measures[s.measure].notes[s.note] = original; self.status = "That duration overlaps the next event.".into(); }
                    ui.add_space(12.0); ui.horizontal(|ui| { if ui.button("− semitone").clicked() { self.transpose(-1); }
 if ui.button("+").clicked() { self.transpose(1); } });
                    ui.horizontal(|ui| { if ui.button("− octave").clicked() { self.transpose(-12); }
 if ui.button("+").clicked() { self.transpose(12); } });
                    if ui.button("Delete note").clicked() { remove = true; }
                }
            } else {
                ui.label(RichText::new("Make your mark.").size(19.0).strong());
                ui.add_space(8.0); ui.label("Choose Write, then click a staff to enter a note. Select a note to edit its expression here.");
                ui.add_space(12.0); ui.label(RichText::new("Shift-click adds a chord tone.\nA–G enters notes at the cursor.\n0–8 chooses duration (5 = quarter).\n↑ / ↓ moves by a diatonic step.").size(12.0).color(colors.muted));
            }
            if remove { self.delete_selected(); }
            ui.add_space(20.0); ui.separator(); ui.add_space(14.0);
            let part = &mut self.score.parts[self.active_part];
            ui.label(RichText::new("STAFF").size(11.0).strong().color(colors.muted));
            ui.text_edit_singleline(&mut part.name);
            egui::ComboBox::from_id_salt("instrument-select").selected_text(part.instrument.name()).show_ui(ui, |ui| { for &i in Instrument::ALL { ui.selectable_value(&mut part.instrument, i, i.name()); } });
            egui::ComboBox::from_label("Clef").selected_text(format!("{:?}",part.clef)).show_ui(ui, |ui| { for c in [Clef::Treble,Clef::Bass,Clef::Alto] { ui.selectable_value(&mut part.clef,c,format!("{c:?}")); } });
            ui.add(egui::Slider::new(&mut part.pan, -1.0..=1.0).text("pan"));
            if self.score.parts.len()>1 && ui.small_button("Remove this staff").clicked() { encore::document::remove_staff(&mut self.score, self.active_part); self.score.parts.remove(self.active_part); self.active_part=0; self.selected=None; }
            ui.add_space(20.0); ui.label(RichText::new(format!("ENTRY CURSOR\nMeasure {}  ·  Beat {:.2}",self.cursor_measure+1,self.cursor_tick as f32 / PPQ as f32 +1.0)).size(11.0).color(colors.muted));
            ui.add(egui::DragValue::new(&mut self.octave).range(0..=8).prefix("Octave "));
            });
        });
        if let (Some(from), Some(to)) = (
            previous_anchor,
            self.selected.and_then(|selection| {
                encore::document::Anchor::from_selection(&self.score, selection)
            }),
        ) && from != to
            && let Err(error) = encore::document::move_anchor(&mut self.score, from, to)
        {
            self.score = previous_score;
            self.status = error;
        }
    }

    fn transport(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        egui::TopBottomPanel::bottom("transport")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let playing = self
                        .playback
                        .as_ref()
                        .is_some_and(|p| p.is_playing() && !p.is_paused());
                    if ui
                        .add_enabled(
                            self.worker.is_none(),
                            egui::Button::new(
                                RichText::new(if playing { "Ⅱ  Pause" } else { "▶  Play" })
                                    .color(colors.workspace)
                                    .strong(),
                            )
                            .fill(colors.accent)
                            .min_size(Vec2::new(90.0, 32.0)),
                        )
                        .clicked()
                    {
                        self.toggle_play();
                    }
                    if ui.button("■  Stop").clicked() {
                        self.stop();
                    }
                    let seconds = self.playback.as_ref().map_or(0.0, |p| p.position_seconds());
                    ui.label(
                        RichText::new(format!(
                            "{:02}:{:02} / {:02}:{:02}",
                            seconds as u32 / 60,
                            seconds as u32 % 60,
                            synth::duration_seconds(&self.score) as u32 / 60,
                            synth::duration_seconds(&self.score) as u32 % 60
                        ))
                        .monospace()
                        .size(13.0),
                    );
                    ui.separator();
                    ui.label(format!("♩ = {}", self.score.tempo));
                    ui.label(format!(
                        "{}/{}",
                        self.score.time_numerator, self.score.time_denominator
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(egui::Slider::new(&mut self.master, 0.0..=1.0).show_value(false));
                        ui.label("Master");
                        if self.worker.is_some() {
                            ui.spinner();
                        }
                    });
                });
                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.status).size(11.0).color(colors.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let source = if matches!(self.audio_engine, AudioEngine::Sampled(_)) {
                            "●  Sampled · 128 sounds"
                        } else {
                            "Procedural synthesis"
                        };
                        if ui
                            .small_button(source)
                            .on_hover_text("Choose sounds in Instrument lab")
                            .clicked()
                        {
                            self.lab_open = true;
                            self.library_reveal = true;
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("keyboard")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::symmetric(20, 9)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("AUDITION")
                            .size(10.0)
                            .strong()
                            .color(colors.muted),
                    );
                    ui.label(
                        RichText::new(self.score.parts[self.active_part].instrument.name())
                            .size(11.0)
                            .color(colors.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Octave +").clicked() {
                            self.keyboard_base = (self.keyboard_base + 12).min(72);
                        }
                        if ui.small_button("Octave −").clicked() {
                            self.keyboard_base = self.keyboard_base.saturating_sub(12);
                        }
                        if ui.small_button("Instrument lab").clicked() {
                            self.lab_open = true;
                            self.library_reveal = true;
                            self.lab_instrument = self.score.parts[self.active_part].instrument;
                            self.lab_midi = instruments::profile(self.lab_instrument).audition;
                        }
                        ui.label(
                            RichText::new("Click to listen · Shift-click to enter")
                                .size(11.0)
                                .color(colors.muted),
                        );
                    });
                });
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), Sense::hover());
                let w = rect.width() / 28.0;
                let mut white = 0;
                let mut pressed = None;
                for midi in self.keyboard_base..self.keyboard_base + 48 {
                    if ![1, 3, 6, 8, 10].contains(&(midi % 12)) {
                        let r = Rect::from_min_size(
                            Pos2::new(rect.left() + white as f32 * w, rect.top()),
                            Vec2::new(w - 1.0, 57.0),
                        );
                        let response = ui.interact(r, ui.id().with(("key", midi)), Sense::click());
                        ui.painter().rect_filled(
                            r,
                            2.0,
                            if response.hovered() {
                                colors.key_hover
                            } else {
                                Color32::WHITE
                            },
                        );
                        ui.painter().rect_stroke(
                            r,
                            2.0,
                            Stroke::new(0.6_f32, Color32::from_gray(199)),
                            egui::StrokeKind::Inside,
                        );
                        if midi % 12 == 0 {
                            ui.painter().text(
                                Pos2::new(r.left() + 5.0, r.bottom() - 5.0),
                                egui::Align2::LEFT_BOTTOM,
                                format!("C{}", midi as i16 / 12 - 1),
                                FontId::proportional(9.0),
                                colors.key_ink.gamma_multiply(0.65),
                            );
                        }
                        if response.clicked() {
                            pressed = Some(midi);
                        }
                        white += 1;
                    }
                }
                white = 0;
                for midi in self.keyboard_base..self.keyboard_base + 48 {
                    if [1, 3, 6, 8, 10].contains(&(midi % 12)) {
                        let r = Rect::from_min_size(
                            Pos2::new(rect.left() + white as f32 * w - w * 0.31, rect.top()),
                            Vec2::new(w * 0.62, 35.0),
                        );
                        let response = ui.interact(r, ui.id().with(("key", midi)), Sense::click());
                        ui.painter().rect_filled(
                            r,
                            2.0,
                            if response.hovered() {
                                colors.accent
                            } else {
                                colors.key_ink
                            },
                        );
                        if response.clicked() {
                            pressed = Some(midi);
                        }
                    } else {
                        white += 1;
                    }
                }
                if let Some(midi) = pressed {
                    if ctx.input(|i| i.modifiers.shift) {
                        self.enter_note(
                            self.active_part,
                            self.cursor_measure,
                            self.cursor_tick,
                            Pitch::from_midi(midi),
                            false,
                        );
                    } else {
                        self.preview_sample =
                            Some((self.score.parts[self.active_part].instrument, midi));
                    }
                }
            });
    }

    fn score_canvas(&mut self, ctx: &egui::Context) {
        self.reconcile_part_staff();
        let colors = Palette::for_context(ctx);
        let projection = match self.selected_part_projection() {
            Ok(projection) => projection,
            Err(error) => {
                self.status = error;
                self.part_view = false;
                None
            }
        };
        let filter = self.part_view.then_some(self.selected_linked_part);
        if self.cached_filter != filter {
            self.previous_part_view = Some(self.cached_filter);
            if let Some(projection) = &projection
                && !projection.source_staves.contains(&self.active_part)
            {
                self.active_part = projection.source_staves[0];
                self.selected = None;
            }
            self.octave = self
                .entry_written_pitch(self.entry_pitch)
                .map_or(self.entry_pitch.octave, |written| written.octave);
        }
        if self.cached_score.as_ref() != Some(&self.score)
            || self.cached_selection != self.selected
            || self.cached_filter != filter
        {
            let display_score = projection
                .as_ref()
                .map_or(&self.score, |projection| &projection.score);
            self.cached_pages = engraving::engrave(
                display_score,
                &EngravingOptions {
                    selected: projection
                        .as_ref()
                        .and_then(|projection| {
                            self.selected
                                .and_then(|selected| projection.project_selection(selected))
                        })
                        .or(self.selected.filter(|_| projection.is_none())),
                    layout: projection.as_ref().map(|projection| projection.layout),
                    ..Default::default()
                },
            );
            if let Some(projection) = &projection {
                for page in &mut self.cached_pages {
                    for staff in &mut page.staves {
                        staff.part = projection.source_staves[staff.part];
                    }
                    for hit in &mut page.hits {
                        if let Some(source) = projection.source_selection(hit.selection) {
                            hit.selection = source;
                        }
                    }
                }
            }
            self.cached_score = Some(self.score.clone());
            self.cached_selection = self.selected;
            self.cached_filter = filter;
        }
        let playback_order = self.score.playback_order().unwrap_or_default();
        let mut click = None;
        let mut slur_drag_start = None;
        let slur_tool = self.notation_editor.tool == notation::Tool::Slurs;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(colors.workspace))
            .show(ctx, |ui| {
                egui::ScrollArea::both()
                    .id_salt("score-scroll")
                    .show(ui, |ui| {
                        ui.add_space(25.0);
                        for (page_index, page) in self.cached_pages.iter().enumerate() {
                            let size = Vec2::new(page.width * self.zoom, page.height * self.zoom);
                            let margin = ((ui.available_width() - size.x) / 2.0).max(24.0);
                            ui.horizontal(|ui| {
                                ui.add_space(margin);
                                let (rect, response) = ui.allocate_exact_size(
                                    size,
                                    if slur_tool {
                                        Sense::click_and_drag()
                                    } else {
                                        Sense::click()
                                    },
                                );
                                let shadow = rect.translate(Vec2::new(3.0, 5.0));
                                ui.painter().rect_filled(
                                    shadow,
                                    1.0,
                                    Color32::from_black_alpha(100),
                                );
                                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                                if ui.is_rect_visible(rect) {
                                    paint_page(ui.painter(), page, rect.min, self.zoom);
                                }
                                if let Some(p) = self
                                    .playback
                                    .as_ref()
                                    .filter(|p| p.is_playing() || p.is_paused())
                                {
                                    let tick = p.position_seconds() * self.score.tempo as f32
                                        / 60.0
                                        * PPQ as f32;
                                    let performed_measure =
                                        (tick / self.score.measure_ticks() as f32) as usize;
                                    let measure = playback_order
                                        .get(performed_measure)
                                        .copied()
                                        .unwrap_or(usize::MAX);
                                    for staff in page.staves.iter().filter(|s| s.measure == measure)
                                    {
                                        let x = rect.left()
                                            + staff.x_at_tick(
                                                tick % self.score.measure_ticks() as f32,
                                            ) * self.zoom;
                                        ui.painter().line_segment(
                                            [
                                                Pos2::new(
                                                    x,
                                                    rect.top() + (staff.y - 14.0) * self.zoom,
                                                ),
                                                Pos2::new(
                                                    x,
                                                    rect.top()
                                                        + (staff.y + staff.spacing * 4.0 + 14.0)
                                                            * self.zoom,
                                                ),
                                            ],
                                            Stroke::new(2.0_f32, colors.playhead),
                                        );
                                    }
                                }
                                if slur_tool
                                    && response.drag_started()
                                    && let Some(origin) =
                                        ctx.input(|input| input.pointer.press_origin())
                                {
                                    let point = (origin - rect.min) / self.zoom;
                                    slur_drag_start = page
                                        .hits
                                        .iter()
                                        .min_by(|a, b| {
                                            Vec2::new(a.x - point.x, a.y - point.y)
                                                .length_sq()
                                                .total_cmp(
                                                    &Vec2::new(b.x - point.x, b.y - point.y)
                                                        .length_sq(),
                                                )
                                        })
                                        .filter(|hit| {
                                            Vec2::new(hit.x - point.x, hit.y - point.y).length()
                                                < 18.0
                                        })
                                        .map(|hit| hit.selection);
                                }
                                if (response.clicked() || slur_tool && response.drag_stopped())
                                    && let Some(pos) = response
                                        .interact_pointer_pos()
                                        .or_else(|| ctx.input(|input| input.pointer.latest_pos()))
                                {
                                    response.surrender_focus();
                                    click = Some((
                                        page_index,
                                        (pos - rect.min) / self.zoom,
                                        ctx.input(|i| i.modifiers.shift),
                                    ));
                                }
                                if self.write_mode {
                                    response.on_hover_cursor(egui::CursorIcon::Crosshair);
                                }
                            });
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                ui.add_space(margin);
                                ui.label(
                                    RichText::new(format!(
                                        "PAGE {} / {}",
                                        page_index + 1,
                                        self.cached_pages.len()
                                    ))
                                    .size(10.0)
                                    .color(colors.muted),
                                );
                            });
                            ui.add_space(22.0);
                        }
                    });
            });
        if let Some(selection) = slur_drag_start {
            self.begin_slur_at(selection);
        }
        if let Some((page_index, pos, chord)) = click {
            let page = &self.cached_pages[page_index];
            let nearest = page
                .hits
                .iter()
                .map(|h| (h, Vec2::new(h.x - pos.x, h.y - pos.y).length()))
                .filter(|(_, d)| *d < 14.0)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if !self.write_mode && !chord {
                if let Some((h, _)) = nearest {
                    let sel = h.selection;
                    if self.annotation_click(sel) {
                        return;
                    }
                    self.selected = Some(sel);
                    self.active_part = sel.part;
                    self.active_layer =
                        self.score.parts[sel.part].measures[sel.measure].notes[sel.note].layer;
                    if let Some(pitch) = self.score.parts[sel.part].measures[sel.measure].notes
                        [sel.note]
                        .pitches
                        .first()
                        .copied()
                    {
                        self.entry_pitch = pitch;
                        self.octave = self
                            .entry_written_pitch(pitch)
                            .map_or(pitch.octave, |written| written.octave);
                    }
                    self.cursor_measure = sel.measure;
                    self.cursor_tick =
                        self.score.parts[sel.part].measures[sel.measure].notes[sel.note].start;
                } else if let Some(staff) = page
                    .staves
                    .iter()
                    .filter(|s| pos.x >= s.content_x && pos.x <= s.x + s.width)
                    .min_by(|a, b| {
                        (pos.y - a.y - a.spacing * 2.0)
                            .abs()
                            .total_cmp(&(pos.y - b.y - b.spacing * 2.0).abs())
                    })
                    && (pos.y - staff.y - staff.spacing * 2.0).abs() < staff.spacing * 7.0
                {
                    self.active_part = staff.part;
                    self.cursor_measure = staff.measure;
                    self.cursor_tick =
                        ((staff.tick_at_x(pos.x) / self.entry_ticks() as f32).floor() as u32
                            * self.entry_ticks())
                        .min(
                            self.score
                                .measure_ticks()
                                .saturating_sub(self.entry_ticks()),
                        );
                    self.selected = None;
                }
            } else {
                let hit = nearest.map(|(h, _)| h.selection);
                let staff = page
                    .staves
                    .iter()
                    .filter(|s| pos.x >= s.content_x - 10.0 && pos.x <= s.x + s.width)
                    .min_by(|a, b| {
                        (pos.y - a.y - a.spacing * 2.0)
                            .abs()
                            .total_cmp(&(pos.y - b.y - b.spacing * 2.0).abs())
                    });
                if let Some(staff) =
                    staff.filter(|s| (pos.y - s.y - s.spacing * 2.0).abs() < s.spacing * 9.0)
                {
                    let part = staff.part;
                    let measure = staff.measure;
                    let step0 = match self.score.parts[part].clef {
                        Clef::Treble => 30,
                        Clef::Bass => 18,
                        Clef::Alto => 24,
                    };
                    let step = step0
                        + ((staff.y + staff.spacing * 4.0 - pos.y) / (staff.spacing / 2.0)).round()
                            as i32;
                    let written_key = projection
                        .as_ref()
                        .map_or(self.score.key_fifths, |projection| {
                            projection.score.key_fifths
                        });
                    let mut pitch = Pitch::from_staff_step(step, written_key);
                    if let Some(alter) = self.accidental {
                        pitch.alter = alter;
                    }
                    if let Some(projection) = &projection {
                        match projection.transposition.concert_pitch(pitch) {
                            Ok(source) => pitch = source,
                            Err(error) => {
                                self.status = error;
                                return;
                            }
                        }
                    }
                    let start =
                        if let Some(s) = hit.filter(|s| s.part == part && s.measure == measure) {
                            self.score.parts[part].measures[measure].notes[s.note].start
                        } else {
                            ((staff.tick_at_x(pos.x) / self.entry_ticks() as f32).round() as u32
                                * self.entry_ticks())
                            .min(
                                self.score
                                    .measure_ticks()
                                    .saturating_sub(self.entry_ticks()),
                            )
                        };
                    let accidental = self.accidental.take();
                    self.enter_note(part, measure, start, pitch, chord);
                    self.accidental = accidental;
                }
            }
        }
    }

    fn sound_controls(&mut self, ui: &mut egui::Ui) {
        let colors = Palette::for_context(ui.ctx());
        ui.label(RichText::new("Playback sounds").strong());
        let sampled = matches!(self.audio_engine, AudioEngine::Sampled(_));
        let loading = self.bank_worker.is_some();
        ui.add_enabled_ui(!loading, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(!sampled, "Procedural").clicked() && sampled {
                    self.set_audio_engine(AudioEngine::Procedural);
                }
                if ui
                    .add_enabled(
                        self.sampled_bank.is_some(),
                        egui::Button::selectable(sampled, "Sampled library"),
                    )
                    .clicked()
                    && !sampled
                    && let Some(bank) = self.sampled_bank.clone()
                {
                    self.set_audio_engine(AudioEngine::Sampled(bank));
                }
                if ui.button("Load SoundFont…").clicked() {
                    self.load_soundfont();
                }
            });
        });
        if loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Loading sound bank…");
            });
        }
        let label = ui.label(
            RichText::new(self.audio_engine.name())
                .size(12.0)
                .color(colors.accent),
        );
        if let Some(path) = self.audio_engine.bank_path() {
            label.on_hover_text(path.display().to_string());
        }
        ui.label(
            RichText::new(if sampled {
                "All 128 instruments use SoundFont samples. Choose a family or search below."
            } else {
                "Procedural mode uses approximations. Select Sampled library to hear the SoundFont instruments."
            })
            .size(12.0)
            .color(colors.muted),
        );
        ui.add_space(5.0);
        ui.separator();
    }

    fn library_results(&self) -> Vec<Instrument> {
        let query = self.library_search.trim().to_lowercase();
        Instrument::ALL
            .iter()
            .copied()
            .filter(|instrument| {
                self.library_family
                    .is_none_or(|family| instrument.family() == family)
                    && (query.is_empty()
                        || instrument.name().to_lowercase().contains(&query)
                        || instrument.family().to_lowercase().contains(&query)
                        || self
                            .audio_engine
                            .preset_name(*instrument)
                            .is_some_and(|name| name.to_lowercase().contains(&query)))
            })
            .collect()
    }

    fn select_library_instrument(&mut self, instrument: Instrument) {
        if instrument != self.lab_instrument {
            self.stop_audition();
            self.lab_instrument = instrument;
            self.lab_midi = instruments::profile(instrument).audition;
        }
    }

    fn add_library_staff(&mut self) {
        if self.score.parts.len() >= MAX_PARTS {
            return;
        }
        self.score.add_part(self.lab_instrument);
        self.active_part = self.score.parts.len() - 1;
        self.selected = None;
        self.status = format!("Added {}", self.lab_instrument.name());
    }

    fn replace_library_staff(&mut self) {
        let part = &mut self.score.parts[self.active_part];
        if part.name == part.instrument.name() {
            part.name = self.lab_instrument.name().into();
        }
        part.instrument = self.lab_instrument;
        part.clef = self.lab_instrument.default_clef();
        self.status = format!("{} now uses {}", part.name, self.lab_instrument.name());
    }

    fn instrument_lab(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        if !self.lab_open {
            return;
        }
        let mut open = true;
        let viewport = ctx.content_rect();
        let list_height = (viewport.height() - 475.0).clamp(140.0, 405.0);
        egui::Window::new("Instrument library")
            .open(&mut open)
            .default_width(880.0)
            .min_width(740.0)
            .max_width((viewport.width() - 48.0).max(740.0))
            .max_height((viewport.height() - 48.0).max(480.0))
            .default_pos(Pos2::new((viewport.width() - 920.0).max(24.0) * 0.5, 95.0_f32.min(viewport.height() * 0.08)))
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Find your sound.").size(25.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        sound_badge(ui, if matches!(self.audio_engine, AudioEngine::Sampled(_)) { "128 SAMPLED SOUNDS" } else { "PROCEDURAL MODE" });
                    });
                });
                ui.label(RichText::new("From an intimate solo to the full orchestra. Explore, audition, and add a voice.").color(colors.muted));
                ui.add_space(10.0);
                self.sound_controls(ui);
                ui.add_space(8.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(Vec2::new(275.0, list_height + 90.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(275.0);
                        ui.add(egui::TextEdit::singleline(&mut self.library_search).hint_text("Search instruments…").desired_width(f32::INFINITY));
                        egui::ComboBox::from_id_salt("library-family").width(255.0)
                            .selected_text(self.library_family.unwrap_or("All families"))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.library_family, None, "All families");
                                let mut families = Vec::new();
                                for instrument in Instrument::ALL {
                                    if !families.contains(&instrument.family()) {
                                        families.push(instrument.family());
                                        ui.selectable_value(&mut self.library_family, Some(instrument.family()), instrument.family());
                                    }
                                }
                            });
                        let results = self.library_results();
                        ui.label(RichText::new(format!("{} instruments", results.len())).size(11.0).color(colors.muted));
                        egui::ScrollArea::vertical().id_salt("library-results").max_height(list_height).auto_shrink([false, false]).show(ui, |ui| {
                            if results.is_empty() {
                                ui.label(RichText::new("No matching instruments. Try another name or family.").color(colors.muted));
                            }
                            for instrument in results {
                                let active = self.lab_instrument == instrument;
                                let response = ui.add_sized([ui.available_width(), 29.0], egui::Button::selectable(active, RichText::new(instrument.name()).size(13.0).color(if active { colors.accent } else { colors.text })).right_text(""));
                                if active && self.library_reveal { response.scroll_to_me(Some(egui::Align::Center)); }
                                if response.clicked() { self.select_library_instrument(instrument); }
                            }
                            self.library_reveal = false;
                        });
                    });
                    ui.separator();
                    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), list_height + 90.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().id_salt("library-details").max_height(list_height + 90.0).auto_shrink([false, false]).show(ui, |ui| {
                            let profile = instruments::profile(self.lab_instrument);
                            ui.label(RichText::new(self.lab_instrument.family().to_uppercase()).size(11.0).color(colors.accent));
                            ui.label(RichText::new(self.lab_instrument.name()).size(27.0).strong());
                            let preset = self.audio_engine.preset_name(self.lab_instrument);
                            ui.label(RichText::new(preset.map(|name| format!("SoundFont preset · {name}")).unwrap_or_else(|| "Procedural approximation".into())).size(12.0).color(colors.muted));

                            ui.horizontal_wrapped(|ui| {
                                if ui.add_enabled(self.score.parts.len() < MAX_PARTS, egui::Button::new("+  Add staff")).clicked() { self.add_library_staff(); }
                                if ui.button("Use on active staff").on_hover_text("Change the current staff's instrument and clef; keep its notes").clicked() { self.replace_library_staff(); }
                                if ui.button("Export phrase…").clicked() { self.export_audition(); }
                            });
                            ui.add_space(8.0);
                            ui.label(profile.character);
                            ui.label(RichText::new(format!("Audition range {}–{} · concert pitch", Pitch::from_midi(profile.low).name(), Pitch::from_midi(profile.high).name())).size(12.0).color(colors.muted));
                            ui.add_space(8.0);
                            egui::Frame::new().fill(colors.raised).corner_radius(10).inner_margin(14).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal_wrapped(|ui| {
                                    if ui.add(egui::Button::new(RichText::new("▶  Play note").color(colors.workspace).strong()).fill(colors.accent)).clicked() {
                                        self.audition(self.lab_instrument, Some(self.lab_midi), self.lab_velocity, self.lab_hold);
                                    }
                                    if ui.button("▶  Test phrase").clicked() {
                                        self.stop(); self.audition(self.lab_instrument, None, self.lab_velocity, self.lab_hold);
                                    }
                                    if ui.button("■  Stop").clicked() { self.stop_audition(); }
                                    if self.audition_worker.is_some() { ui.spinner(); }
                                });
                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    ui.label("Pitch");
                                    ui.add(egui::Slider::new(&mut self.lab_midi, profile.low..=profile.high).show_value(false));
                                    ui.label(RichText::new(Pitch::from_midi(self.lab_midi).name()).strong().color(colors.accent));
                                });
                                ui.add(egui::Slider::new(&mut self.lab_velocity, 1..=127).text("Velocity"));
                                ui.horizontal(|ui| { for (name, velocity) in [("pp",32),("p",48),("mp",64),("mf",80),("f",100),("ff",116)] {
                                    if ui.selectable_label(self.lab_velocity == velocity, name).clicked() { self.lab_velocity = velocity; }
                                }});
                                ui.add(egui::Slider::new(&mut self.lab_hold, 0.15..=5.0).text("Hold (seconds)"));
                                ui.add_space(6.0);
                            });
                            ui.add_space(8.0);
                            ui.add_space(8.0);
                            ui.label(RichText::new(profile.technique).size(12.0).color(colors.muted));
                        });
                    });
                });
            });
        self.lab_open = open;
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
        if self.show_import_report {
            let mut open = true;
            egui::Window::new("Import notes").open(&mut open).default_width(560.0).show(ctx, |ui| {
                ui.label("Your score is open. Some source features need review in Encore:");
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for warning in &self.import_warnings { ui.label(format!("• {warning}")); }
                });
                ui.add_space(8.0);
                ui.label(RichText::new("The source file is unchanged. Save this edition as an .encore project.").color(colors.muted));
                if ui.button("Continue editing").clicked() { self.show_import_report = false; }
            });
            if !open {
                self.show_import_report = false;
            }
        }
        if self.pending.is_some() {
            egui::Window::new("Save your score?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("This score has unsaved changes.");
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save and continue").clicked()
                            && self.save(false)
                            && let Some(a) = self.pending.take()
                        {
                            self.perform(a, ctx);
                        }
                        if ui.button("Discard changes").clicked()
                            && let Some(a) = self.pending.take()
                        {
                            self.perform(a, ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending = None;
                        }
                    });
                });
        }
        if let Some(path) = self.midi_import.clone() {
            let mut open = true;
            let mut import = false;
            let mut cancel = false;
            egui::Window::new("Import MIDI performance").open(&mut open).resizable(false).default_width(440.0).show(ctx,|ui| {
                ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                ui.label("Choose how performance timing becomes written rhythm. Exact timing preserves the source; a rhythmic grid snaps notes to that precision.");
                ui.radio_value(&mut self.midi_grid,None,"Exact timing · reject unsupported rhythms");
                egui::ComboBox::from_label("Quantize to").selected_text(self.midi_grid.and_then(|grid|crate::midi_options::GRIDS.iter().find(|(_,_,ticks)|*ticks==grid).map(|(_,label,_)|*label)).unwrap_or("Choose a grid…")).show_ui(ui,|ui| {
                    for (_,label,grid) in crate::midi_options::GRIDS {ui.selectable_value(&mut self.midi_grid,Some(*grid),*label);}
                });
                if self.midi_grid.is_some(){ui.label("Timing changes are listed in the import notes. Use MusicXML to retain written rhythms and articulation marks.");}
                if let Some(error)=&self.midi_import_error {ui.separator();ui.label(error);}
                ui.horizontal(|ui|{import=ui.button("Import").clicked();cancel=ui.button("Cancel").clicked();});
            });
            if cancel || !open {
                self.midi_import = None;
                self.midi_import_error = None;
            } else if import {
                let result = match self.midi_grid {
                    Some(grid) => encore::import::load_midi_quantized(&path, grid),
                    None => encore::import::load(&path),
                };
                match result {
                    Ok(imported) => self.accept_import(path, imported),
                    Err(error) => self.midi_import_error = Some(error),
                }
            }
        }
        if self.show_history {
            let mut open = true;
            let mut undo_count = 0;
            let mut redo_count = 0;
            egui::Window::new("Undo/Redo list")
                .open(&mut open)
                .default_width(380.0)
                .show(ctx, |ui| {
                    ui.label("Choose how many score changes to undo or redo.");
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for count in 1..=self.undo.len() {
                                if ui
                                    .button(format!(
                                        "Undo {count} {}",
                                        if count == 1 { "change" } else { "changes" }
                                    ))
                                    .clicked()
                                {
                                    undo_count = count;
                                }
                            }
                            if !self.undo.is_empty() && !self.redo.is_empty() {
                                ui.separator();
                            }
                            for count in 1..=self.redo.len() {
                                if ui
                                    .button(format!(
                                        "Redo {count} {}",
                                        if count == 1 { "change" } else { "changes" }
                                    ))
                                    .clicked()
                                {
                                    redo_count = count;
                                }
                            }
                            if self.undo.is_empty() && self.redo.is_empty() {
                                ui.label("No score changes yet.");
                            }
                        });
                });
            for _ in 0..undo_count {
                self.undo();
            }
            for _ in 0..redo_count {
                self.redo();
            }
            self.show_history = open && undo_count == 0 && redo_count == 0;
        }
        if self.properties {
            let mut open = true;
            egui::Window::new("Score properties").open(&mut open).resizable(false).show(ctx,|ui| {
                egui::Grid::new("score-properties").spacing([15.0,13.0]).show(ui,|ui| {
                    ui.label("Title"); ui.text_edit_singleline(&mut self.score.title); ui.end_row();
                    ui.label("Composer"); ui.text_edit_singleline(&mut self.score.composer); ui.end_row();
                    ui.label("Tempo"); ui.add(egui::DragValue::new(&mut self.score.tempo).range(20..=400)); ui.end_row();
                    ui.label("Key signature"); egui::ComboBox::from_id_salt("key-signature").selected_text(key_name(self.score.key_fifths)).show_ui(ui,|ui|{for f in -7..=7{ui.selectable_value(&mut self.score.key_fifths,f,key_name(f));}}); ui.end_row();
                    ui.label("Time signature"); ui.horizontal(|ui| {
                        let previous=(self.score.time_numerator,self.score.time_denominator);
                        ui.add(egui::DragValue::new(&mut self.score.time_numerator).range(1..=32)); ui.label("/");
                        egui::ComboBox::from_id_salt("denominator").selected_text(self.score.time_denominator.to_string()).show_ui(ui,|ui|{for d in [1,2,4,8,16,32,64,128]{ui.selectable_value(&mut self.score.time_denominator,d,d.to_string());}});
                        let limit=self.score.measure_ticks();
                        if self.score.parts.iter().any(|p|p.measures.iter().any(|m|m.notes.iter().any(|n|n.start+n.ticks()>limit))) { self.score.time_numerator=previous.0; self.score.time_denominator=previous.1; self.status="Existing notes extend past this time signature. Shorten those measures first.".into(); }
                    }); ui.end_row();
                });
                self.clamp_cursor();
                ui.add_space(12.0); ui.label(RichText::new("The full score uses concert pitch. Linked parts use their written transposition. Key changes update the signature; existing pitches retain their spelling.").size(12.0).color(colors.muted));
                if ui.button("Done").clicked(){self.properties=false;}
            });
            if !open {
                self.properties = false;
            }
        }
        egui::Window::new("Getting started with Encore").open(&mut self.help).default_width(520.0).show(ctx,|ui| {
            ui.heading("From an idea to a printed score"); ui.add_space(8.0);
            for text in ["1. File → New score creates a piano grand staff. Add orchestral staves with + in Instruments.","2. Select Write, choose a duration (0–8; 5 is a quarter note), and click a staff. Shift-click an existing event to add a chord tone. Tab enters a rest; Alt+R toggles the rest tool. Period dots the selected entry; Shift+Period toggles dotted entry.","3. A–G enters notes at the cursor and advances it. Set the octave in Inspector. Click a blank staff in Select mode to move the cursor. Left/Right selects the previous or next entry.","4. Select a note to edit its duration, dynamics, tie, or articulation. In Simple Entry, Up/Down moves by a diatonic step; Shift adds an octave. Delete removes the event. Ctrl/Command+Z undoes.","5. Space plays or pauses the score; Escape stops. Use the M/S mixer buttons to mute or solo staves. Click piano keys to audition the active instrument; Shift-click enters a note.","6. Open MusicXML, MIDI, MuseScore, or supported Finale .musx files with File → Open. Review import notes and save a new Encore project.","7. Notation adds lyrics, phrase slurs, repeats and endings. Lyrics mode uses Space/hyphen to advance and underscore for word extensions. Slurs join two clicked notes, with editable curves.","8. Parts opens linked instrumental parts with written transposition and independent page formats. Edit a part to update the shared score. Notation → Manage linked parts also exports every part as PDF.","9. Save your .encore project, then export PDF for printing, SVG for vector art, MusicXML for notation interchange, MIDI for a sequencer, or WAV for audio."] { ui.label(text); ui.add_space(9.0); }
            ui.separator(); ui.label("Four independent rhythmic layers can share each staff, with chords and explicit or automatic rests. Select the active layer in the notation toolbar. Playback uses concert pitches; linked part views show written pitches. Instrument library offers 128 SoundFont instruments, with searchable families and note or phrase auditions. Procedural mode provides optional approximations.");
        });
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.poll_worker();
        self.skip_history = false;
        let before = self.score.clone();
        self.active_part = self
            .active_part
            .min(self.score.parts.len().saturating_sub(1));
        if let Some(s) = self.selected
            && self
                .score
                .parts
                .get(s.part)
                .and_then(|p| p.measures.get(s.measure))
                .and_then(|m| m.notes.get(s.note))
                .is_none()
        {
            self.selected = None;
        }
        self.cursor_measure = self
            .cursor_measure
            .min(self.score.measure_count().saturating_sub(1));
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Action::Quit);
        }
        let instrument = self.score.parts[self.active_part].instrument;
        if self.keyboard_instrument != instrument {
            self.keyboard_instrument = instrument;
            self.keyboard_base = keyboard_base_for(instrument);
        }
        if let Some(s) = self.selected
            && let Some(note) = self
                .score
                .parts
                .get(s.part)
                .and_then(|p| p.measures.get(s.measure))
                .and_then(|m| m.notes.get(s.note))
        {
            self.active_layer = note.layer;
        }
        self.reconcile_part_staff();
        self.keyboard(ctx);
        self.top(ctx);
        self.transport(ctx);
        self.instruments(ctx);
        self.reconcile_part_staff();
        self.inspector(ctx);
        self.score_canvas(ctx);
        self.dialogs(ctx);
        self.notation_dialog(ctx);
        self.linked_parts_dialog(ctx);
        self.instrument_lab(ctx);
        if self.score != before && !self.skip_history {
            if self.rendering_audio
                || self
                    .playback
                    .as_ref()
                    .is_some_and(|p| p.is_playing() || p.is_paused())
            {
                self.stop();
                self.status = "Score changed. Press Play to hear the updated score.".into();
            }
            self.undo.push(before);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        if let Some(p) = self.playback.as_mut() {
            p.set_volume(self.master);
        }
        if let Some(p) = self.audition_playback.as_mut() {
            p.set_volume(self.master);
        }
        if let Some((instrument, midi)) = self.preview_sample.take() {
            self.audition(instrument, Some(midi), self.lab_velocity, 0.9);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "{}{} — Encore",
            if self.dirty() { "• " } else { "" },
            self.score.title
        )));
        if self.worker.is_some()
            || self.bank_worker.is_some()
            || self.audition_worker.is_some()
            || self.playback.as_ref().is_some_and(|p| p.is_playing())
            || self
                .audition_playback
                .as_ref()
                .is_some_and(|p| p.is_playing())
        {
            ctx.request_repaint_after(StdDuration::from_millis(30));
        }
    }
}

impl eframe::App for EncoreApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("encore.theme", self.theme.key().into());
        storage.set_string("encore.entry-profile", self.entry_profile.key().into());
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

fn sound_badge(ui: &mut egui::Ui, label: &str) {
    let colors = Palette::for_context(ui.ctx());
    egui::Frame::new()
        .fill(colors.selected)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(9, 5))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(10.0)
                    .strong()
                    .color(colors.accent),
            );
        });
}

fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let colors = Palette::for_theme(theme);
    let mut style = (*ctx.style()).clone();
    style.visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    style.visuals.panel_fill = colors.surface;
    style.visuals.override_text_color = Some(colors.text);
    style.visuals.weak_text_color = Some(colors.muted);
    style.visuals.window_fill = colors.surface;
    style.visuals.window_stroke = Stroke::new(1.0_f32, colors.border);
    style.visuals.window_corner_radius = 12.into();
    style.visuals.menu_corner_radius = 8.into();
    style.visuals.extreme_bg_color = colors.workspace;
    style.visuals.text_edit_bg_color = Some(colors.workspace);
    style.visuals.faint_bg_color = colors.raised;
    style.visuals.selection.bg_fill = colors.selected;
    style.visuals.selection.stroke.color = colors.accent;
    style.visuals.hyperlink_color = colors.accent;
    if theme == Theme::Dark {
        style.visuals.warn_fg_color = colors.accent;
        style.visuals.error_fg_color = colors.text;
    }
    style.visuals.slider_trailing_fill = true;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors.border);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = 6.into();
        widget.bg_stroke = Stroke::new(1.0_f32, colors.border);
        widget.fg_stroke.color = colors.text;
    }
    style.visuals.widgets.inactive.bg_fill = colors.raised;
    style.visuals.widgets.inactive.weak_bg_fill = colors.raised;
    style.visuals.widgets.hovered.bg_fill = colors.hovered;
    style.visuals.widgets.hovered.weak_bg_fill = colors.hovered;
    style.visuals.widgets.hovered.bg_stroke.color = colors.accent;
    style.visuals.widgets.active.bg_fill = colors.selected;
    style.visuals.widgets.active.weak_bg_fill = colors.selected;
    style.visuals.widgets.active.fg_stroke.color = colors.text;
    style.visuals.widgets.open.bg_fill = colors.selected;
    style.visuals.widgets.open.weak_bg_fill = colors.selected;
    style.spacing.item_spacing = Vec2::new(9.0, 8.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.window_margin = 16.into();
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    ctx.set_style(style);
}

fn configure_context(ctx: &egui::Context) {
    apply_theme(ctx, Theme::Dark);
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        ("ui-sans", include_bytes!("../assets/Sans.ttf").as_slice()),
        (
            "music-symbols",
            include_bytes!("../assets/Music.ttf").as_slice(),
        ),
    ] {
        fonts.font_data.insert(
            name.into(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
        if name == "ui-sans" {
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, name.into());
        } else {
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .push(name.into());
        }
        fonts
            .families
            .get_mut(&FontFamily::Monospace)
            .unwrap()
            .push(name.into());
    }
    fonts.font_data.insert(
        "score-serif".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/Serif.ttf"
        ))),
    );
    fonts.families.insert(
        FontFamily::Name("score-serif".into()),
        vec!["score-serif".into(), "music-symbols".into()],
    );
    ctx.set_fonts(fonts);
}

fn safe_filename(s: &str) -> String {
    let s = s
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>();
    if s.trim().is_empty() {
        "Untitled score".into()
    } else {
        s
    }
}
fn key_name(f: i8) -> &'static str {
    match f {
        -7 => "C♭ major / A♭ minor",
        -6 => "G♭ major / E♭ minor",
        -5 => "D♭ major / B♭ minor",
        -4 => "A♭ major / F minor",
        -3 => "E♭ major / C minor",
        -2 => "B♭ major / G minor",
        -1 => "F major / D minor",
        0 => "C major / A minor",
        1 => "G major / E minor",
        2 => "D major / B minor",
        3 => "A major / F♯ minor",
        4 => "E major / C♯ minor",
        5 => "B major / G♯ minor",
        6 => "F♯ major / D♯ minor",
        _ => "C♯ major / A♯ minor",
    }
}

fn paint_page(painter: &egui::Painter, page: &Page, origin: Pos2, scale: f32) {
    let pos = |x: f32, y: f32| Pos2::new(origin.x + x * scale, origin.y + y * scale);
    let dark = painter.ctx().style().visuals.dark_mode;
    let color = |c| page_preview_color(c, dark);
    for item in &page.items {
        match item {
            Primitive::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                color: c,
            } => {
                painter.line_segment(
                    [pos(*x1, *y1), pos(*x2, *y2)],
                    Stroke::new(width * scale, color(*c)),
                );
            }
            Primitive::Ellipse {
                cx,
                cy,
                rx,
                ry,
                fill,
                stroke,
                width,
            } => {
                let points = (0..40)
                    .map(|i| {
                        let a = i as f32 / 40.0 * std::f32::consts::TAU;
                        pos(cx + rx * a.cos(), cy + ry * a.sin())
                    })
                    .collect::<Vec<_>>();
                painter.add(egui::Shape::convex_polygon(
                    points,
                    fill.map_or(Color32::TRANSPARENT, color),
                    stroke.map_or(Stroke::NONE, |c| Stroke::new(width * scale, color(c))),
                ));
            }
            Primitive::Path {
                points,
                closed,
                fill,
                stroke,
                width,
            } => {
                let points = points.iter().map(|(x, y)| pos(*x, *y)).collect::<Vec<_>>();
                if let Some(fill) = fill {
                    // A single mesh avoids antialiasing each internal triangle;
                    // feathering acute triangle corners produces long spikes.
                    let mut mesh = egui::Mesh::default();
                    for [a, b, c] in triangulate(&points) {
                        let base = mesh.vertices.len() as u32;
                        for p in [a, b, c] {
                            mesh.colored_vertex(p, color(*fill));
                        }
                        mesh.add_triangle(base, base + 1, base + 2);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    if stroke.is_none() && *closed {
                        let middle = points.len() / 2;
                        let ribbon = middle > 2
                            && points.len().is_multiple_of(2)
                            && points[0].distance_sq(points[points.len() - 1]) < 0.01
                            && points[middle - 1].distance_sq(points[middle]) < 0.01;
                        if ribbon {
                            // Tapered phrase curves have two coincident tips.
                            // Outline each side separately so an acute closing
                            // join cannot form an antialiasing spike.
                            for side in points.chunks(middle) {
                                painter.add(egui::Shape::line(
                                    side.to_vec(),
                                    Stroke::new(0.4 * scale, color(*fill)),
                                ));
                            }
                        } else {
                            painter.add(egui::Shape::closed_line(
                                points.clone(),
                                Stroke::new(0.35 * scale, color(*fill)),
                            ));
                        }
                    }
                }
                if let Some(c) = stroke {
                    let s = Stroke::new(width * scale, color(*c));
                    if *closed {
                        painter.add(egui::Shape::closed_line(points, s));
                    } else {
                        painter.add(egui::Shape::line(points, s));
                    }
                }
            }
            Primitive::Text {
                x,
                y,
                text,
                size,
                color: c,
                serif,
            } => {
                let family = if *serif {
                    FontFamily::Name("score-serif".into())
                } else {
                    FontFamily::Proportional
                };
                // Glyph baseline is approximately 80% of the em size.
                painter.text(
                    pos(*x, *y - size * 0.82),
                    egui::Align2::LEFT_TOP,
                    text,
                    FontId::new(size * scale, family),
                    color(*c),
                );
            }
        }
    }
}

fn page_preview_color(color: [u8; 3], dark: bool) -> Color32 {
    if dark {
        // Keep selection and engraving neutral on the white score without
        // changing colors in the document or exported files.
        let gray =
            (u32::from(color[0]) * 2126 + u32::from(color[1]) * 7152 + u32::from(color[2]) * 722)
                / 10_000;
        Color32::from_gray(gray as u8)
    } else {
        Color32::from_rgb(color[0], color[1], color[2])
    }
}

fn triangulate(points: &[Pos2]) -> Vec<[Pos2; 3]> {
    if points.len() < 3 {
        return vec![];
    }
    let cross = |a: Pos2, b: Pos2, c: Pos2| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let area: f32 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut idx = (0..points.len()).collect::<Vec<_>>();
    let mut out = Vec::new();
    let mut guard = 0;
    while idx.len() > 2 && guard < points.len() * points.len() {
        guard += 1;
        let mut found = false;
        for j in 0..idx.len() {
            let a = points[idx[(j + idx.len() - 1) % idx.len()]];
            let b = points[idx[j]];
            let c = points[idx[(j + 1) % idx.len()]];
            if cross(a, b, c) * sign < 0.001 {
                continue;
            }
            if idx.iter().enumerate().any(|(k, &i)| {
                k != j
                    && k != (j + idx.len() - 1) % idx.len()
                    && k != (j + 1) % idx.len()
                    && cross(a, b, points[i]) * sign > 0.0001
                    && cross(b, c, points[i]) * sign > 0.0001
                    && cross(c, a, points[i]) * sign > 0.0001
            }) {
                continue;
            }
            out.push([a, b, c]);
            idx.remove(j);
            found = true;
            break;
        }
        if !found {
            break;
        }
    }
    out
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

fn keyboard_base_for(instrument: Instrument) -> u8 {
    if instrument.gm_program() == 0 {
        48
    } else {
        (instruments::profile(instrument).low / 12 * 12).min(72)
    }
}
