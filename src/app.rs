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

const INK: Color32 = Color32::from_rgb(35, 44, 48);
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
}

impl Palette {
    fn for_theme(theme: Theme) -> Self {
        let rgb = Color32::from_rgb;
        match theme {
            Theme::Dark => Self {
                text: rgb(231, 236, 245),
                muted: rgb(151, 165, 187),
                accent: rgb(121, 224, 194),
                surface: rgb(24, 31, 44),
                raised: rgb(32, 42, 58),
                border: rgb(48, 61, 81),
                selected: rgb(31, 65, 66),
                workspace: rgb(14, 20, 30),
                hovered: rgb(48, 65, 85),
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
    cursor_measure: usize,
    cursor_tick: u32,
    duration: Duration,
    dotted: bool,
    rest: bool,
    accidental: Option<i8>,
    octave: i8,
    write_mode: bool,
    zoom: f32,
    part_view: bool,
    master: f32,
    theme: Theme,
    needs_native_save: bool,
    import_warnings: Vec<String>,
    show_import_report: bool,
    playback: Option<synth::Playback>,
    worker: Option<mpsc::Receiver<JobResult>>,
    rendering_audio: bool,
    pending: Option<Action>,
    help: bool,
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
            && let Err(error) = app.open_document(path)
        {
            app.status = format!("Could not open score: {error}");
        }
        app.theme = Theme::from_storage(cc.storage);
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
            cursor_measure: 0,
            cursor_tick: 0,
            duration: Duration::Quarter,
            dotted: false,
            rest: false,
            accidental: None,
            octave: 4,
            write_mode: false,
            zoom: 0.9,
            part_view: false,
            master: 0.8,
            theme: Theme::Dark,
            needs_native_save: false,
            import_warnings: Vec::new(),
            show_import_report: false,
            playback: None,
            worker: None,
            rendering_audio: false,
            pending: None,
            help: false,
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
                            "mus",
                        ],
                    )
                    .add_filter("Encore projects", &["encore", "json"])
                    .add_filter("MusicXML", &["musicxml", "xml", "mxl"])
                    .add_filter("MuseScore", &["mscx", "mscz"])
                    .add_filter("Finale", &["musx", "mus"])
                    .pick_file()
                    && let Err(error) = self.open_document(path)
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
        self.selected = None;
        self.active_part = 0;
        self.cursor_measure = 0;
        self.cursor_tick = 0;
        self.undo.clear();
        self.redo.clear();
        self.skip_history = true;
        self.status = "Ready. Choose Write and click a staff, or enter notes with A–G.".into();
    }

    fn open_document(&mut self, path: PathBuf) -> Result<(), String> {
        let result = encore::import::load(&path)?;
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
        Ok(())
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
        if self.part_view && ["pdf", "svg"].contains(&ext) {
            let part = score.parts[self.active_part].clone();
            score.title = format!("{} — {}", score.title, part.name);
            score.parts = vec![part];
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
                let result = crate::export_to_with_engine(&score, &path, &engine).map(|()| path);
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
        let ticks = self.duration.ticks() * if self.dotted { 3 } else { 2 } / 2;
        let measure_ticks = self.score.measure_ticks();
        let m = &mut self.score.parts[part].measures[measure];
        if chord
            && !self.rest
            && let Some(i) = m.notes.iter().position(|n| n.start == start)
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
        if m.notes
            .iter()
            .any(|n| n.start != start && start < n.start + n.ticks() && start + ticks > n.start)
        {
            self.status =
                "This note overlaps another event. Delete that event or choose a shorter duration."
                    .into();
            return;
        }
        m.notes.retain(|n| n.start != start);
        m.notes.push(Note {
            start,
            duration: self.duration,
            dotted: self.dotted,
            pitches: if self.rest { vec![] } else { vec![pitch] },
            velocity: 88,
            staccato: false,
            tied: false,
        });
        m.notes.sort_by_key(|n| n.start);
        let i = m.notes.iter().position(|n| n.start == start).unwrap();
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
            if !self.lab_open && self.pending.is_none() {
                self.selected = None;
            }
        }
        if self.pending.is_some() || self.lab_open {
            return;
        }
        let command = ctx.input(|i| i.modifiers.command);
        if command && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.save(ctx.input(|i| i.modifiers.shift));
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        if command {
            if ctx.input(|i| i.key_pressed(egui::Key::N)) {
                self.request(Action::New, ctx);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::O)) {
                self.request(Action::Open, ctx);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Z)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.redo();
                } else {
                    self.undo();
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Y)) {
                self.redo();
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.toggle_play();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            self.delete_selected();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            self.transpose(if ctx.input(|i| i.modifiers.shift) {
                12
            } else {
                1
            });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            self.transpose(if ctx.input(|i| i.modifiers.shift) {
                -12
            } else {
                -1
            });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.cursor_tick += self.duration.ticks();
            if self.cursor_tick >= self.score.measure_ticks() {
                self.cursor_tick = 0;
                self.cursor_measure = (self.cursor_measure + 1).min(self.score.measure_count() - 1);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            if self.cursor_tick == 0 && self.cursor_measure > 0 {
                self.cursor_measure -= 1;
                self.cursor_tick = self
                    .score
                    .measure_ticks()
                    .saturating_sub(self.duration.ticks());
            } else {
                self.cursor_tick = self.cursor_tick.saturating_sub(self.duration.ticks());
            }
        }
        for (key, d) in [
            (egui::Key::Num1, Duration::Whole),
            (egui::Key::Num2, Duration::Half),
            (egui::Key::Num3, Duration::Quarter),
            (egui::Key::Num4, Duration::Eighth),
            (egui::Key::Num5, Duration::Sixteenth),
        ] {
            if ctx.input(|i| i.key_pressed(key)) {
                self.duration = d;
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.rest = !self.rest;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Period)) {
            self.dotted = !self.dotted;
        }
        if self.write_mode {
            for (key, step) in [
                (egui::Key::A, Step::A),
                (egui::Key::B, Step::B),
                (egui::Key::C, Step::C),
                (egui::Key::D, Step::D),
                (egui::Key::E, Step::E),
                (egui::Key::F, Step::F),
                (egui::Key::G, Step::G),
            ] {
                if ctx.input(|i| i.key_pressed(key)) {
                    let natural = Pitch {
                        step,
                        octave: self.octave,
                        alter: 0,
                    };
                    let pitch = Pitch::from_staff_step(natural.staff_step(), self.score.key_fifths);
                    self.enter_note(
                        self.active_part,
                        self.cursor_measure,
                        self.cursor_tick,
                        pitch,
                        false,
                    );
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.enter_note(
                    self.active_part,
                    self.cursor_measure,
                    self.cursor_tick,
                    Pitch {
                        step: Step::C,
                        octave: self.octave,
                        alter: 0,
                    },
                    false,
                );
            }
        }
    }

    fn top(&mut self, ctx: &egui::Context) {
        let colors = Palette::for_context(ctx);
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
                            if ui.button("New score          Ctrl+N").clicked() {
                                ui.close();
                                self.request(Action::New, ctx);
                            }
                            if ui.button("Open…                Ctrl+O").clicked() {
                                ui.close();
                                self.request(Action::Open, ctx);
                            }
                            if ui.button("Save                    Ctrl+S").clicked() {
                                ui.close();
                                self.save(false);
                            }
                            if ui.button("Save as…").clicked() {
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
                                    egui::Button::new("Undo    Ctrl+Z"),
                                )
                                .clicked()
                            {
                                self.undo();
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    !self.redo.is_empty(),
                                    egui::Button::new("Redo    Ctrl+Shift+Z"),
                                )
                                .clicked()
                            {
                                self.redo();
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
                    ui.selectable_value(&mut self.write_mode, false, "Select")
                        .on_hover_text("Click a note to inspect or edit it");
                    ui.selectable_value(&mut self.write_mode, true, "Write")
                        .on_hover_text(
                            "Click a staff to insert; Shift-click an event to add a chord tone",
                        );
                    ui.separator();
                    for (i, d) in Duration::ALL.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.duration,
                            *d,
                            format!("{} {}", i + 1, d.name()),
                        )
                        .on_hover_text("Duration for new notes");
                    }
                    ui.separator();
                    ui.toggle_value(&mut self.dotted, "Dotted");
                    ui.toggle_value(&mut self.rest, "Rest  R");
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
                {
                    for p in &mut self.score.parts {
                        p.measures.pop();
                    }
                    self.selected = None;
                    self.clamp_cursor();
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
                    egui::ComboBox::from_label("Duration").selected_text(n.duration.name()).show_ui(ui, |ui| { for &d in Duration::ALL { ui.selectable_value(&mut n.duration, d, d.name()); } });
                    ui.checkbox(&mut n.dotted, "Dotted"); ui.checkbox(&mut n.staccato, "Staccato"); ui.add_enabled(!n.pitches.is_empty(), egui::Checkbox::new(&mut n.tied, "Tie to next note"));
                    ui.add_space(10.0); ui.label("Dynamics"); ui.add(egui::Slider::new(&mut n.velocity, 1..=127).text("velocity"));
                    ui.horizontal_wrapped(|ui| { for (name,v) in [("pp",32),("p",48),("mp",64),("mf",80),("f",100),("ff",116)] { if ui.small_button(name).clicked() { n.velocity=v; } } });
                    if n.ticks() + n.start > self.score.measure_ticks() { self.score.parts[s.part].measures[s.measure].notes[s.note] = original.clone(); self.status = "That duration extends past the barline.".into(); }
                    let m = &self.score.parts[s.part].measures[s.measure];
                    let n = &m.notes[s.note];
                    if m.notes.iter().enumerate().any(|(i,x)| i!=s.note && n.start < x.start+x.ticks() && n.start+n.ticks()>x.start) { self.score.parts[s.part].measures[s.measure].notes[s.note] = original; self.status = "That duration overlaps the next event.".into(); }
                    ui.add_space(12.0); ui.horizontal(|ui| { if ui.button("− semitone").clicked() { self.transpose(-1); }
 if ui.button("+").clicked() { self.transpose(1); } });
                    ui.horizontal(|ui| { if ui.button("− octave").clicked() { self.transpose(-12); }
 if ui.button("+").clicked() { self.transpose(12); } });
                    if ui.button("Delete note").clicked() { remove = true; }
                }
            } else {
                ui.label(RichText::new("Make your mark.").size(19.0).strong());
                ui.add_space(8.0); ui.label("Choose Write, then click a staff to enter a note. Select a note to edit its expression here.");
                ui.add_space(12.0); ui.label(RichText::new("Shift-click adds a chord tone.\nA–G enters notes at the cursor.\n1–5 chooses note duration.\n↑ / ↓ changes selected pitch.").size(12.0).color(colors.muted));
            }
            if remove { self.delete_selected(); }
            ui.add_space(20.0); ui.separator(); ui.add_space(14.0);
            let part = &mut self.score.parts[self.active_part];
            ui.label(RichText::new("STAFF").size(11.0).strong().color(colors.muted));
            ui.text_edit_singleline(&mut part.name);
            egui::ComboBox::from_id_salt("instrument-select").selected_text(part.instrument.name()).show_ui(ui, |ui| { for &i in Instrument::ALL { ui.selectable_value(&mut part.instrument, i, i.name()); } });
            egui::ComboBox::from_label("Clef").selected_text(format!("{:?}",part.clef)).show_ui(ui, |ui| { for c in [Clef::Treble,Clef::Bass,Clef::Alto] { ui.selectable_value(&mut part.clef,c,format!("{c:?}")); } });
            ui.add(egui::Slider::new(&mut part.pan, -1.0..=1.0).text("pan"));
            if self.score.parts.len()>1 && ui.small_button("Remove this staff").clicked() { self.score.parts.remove(self.active_part); self.active_part=0; self.selected=None; }
            ui.add_space(20.0); ui.label(RichText::new(format!("ENTRY CURSOR\nMeasure {}  ·  Beat {:.2}",self.cursor_measure+1,self.cursor_tick as f32 / PPQ as f32 +1.0)).size(11.0).color(colors.muted));
            ui.add(egui::DragValue::new(&mut self.octave).range(0..=8).prefix("Octave "));
            });
        });
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
                                Color32::from_rgb(211, 234, 224)
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
                                INK.gamma_multiply(0.65),
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
                                INK
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
        let colors = Palette::for_context(ctx);
        let filter = if self.part_view {
            Some(self.active_part)
        } else {
            None
        };
        if self.cached_score.as_ref() != Some(&self.score)
            || self.cached_selection != self.selected
            || self.cached_filter != filter
        {
            self.cached_pages = engraving::engrave(
                &self.score,
                &EngravingOptions {
                    selected: self.selected,
                    playhead: None,
                    part_filter: filter,
                },
            );
            self.cached_score = Some(self.score.clone());
            self.cached_selection = self.selected;
            self.cached_filter = filter;
        }
        let mut click = None;
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
                                let (rect, response) = ui.allocate_exact_size(size, Sense::click());
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
                                    let measure =
                                        (tick / self.score.measure_ticks() as f32) as usize;
                                    for staff in page.staves.iter().filter(|s| s.measure == measure)
                                    {
                                        let x = rect.left()
                                            + (staff.content_x
                                                + (tick % self.score.measure_ticks() as f32)
                                                    / self.score.measure_ticks() as f32
                                                    * staff.content_width)
                                                * self.zoom;
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
                                            Stroke::new(
                                                2.0_f32,
                                                Color32::from_rgba_unmultiplied(40, 150, 120, 180),
                                            ),
                                        );
                                    }
                                }
                                if response.clicked()
                                    && let Some(pos) = response.interact_pointer_pos()
                                {
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
                    self.selected = Some(sel);
                    self.active_part = sel.part;
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
                    self.cursor_tick = (((pos.x - staff.content_x) / staff.content_width
                        * self.score.measure_ticks() as f32
                        / self.duration.ticks() as f32)
                        .floor() as u32
                        * self.duration.ticks())
                    .min(
                        self.score
                            .measure_ticks()
                            .saturating_sub(self.duration.ticks()),
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
                    let pitch = Pitch::from_staff_step(step, self.score.key_fifths);
                    let start =
                        if let Some(s) = hit.filter(|s| s.part == part && s.measure == measure) {
                            self.score.parts[part].measures[measure].notes[s.note].start
                        } else {
                            (((pos.x - staff.content_x).max(0.0) / staff.content_width
                                * self.score.measure_ticks() as f32
                                / self.duration.ticks() as f32)
                                .round() as u32
                                * self.duration.ticks())
                            .min(
                                self.score
                                    .measure_ticks()
                                    .saturating_sub(self.duration.ticks()),
                            )
                        };
                    self.enter_note(part, measure, start, pitch, chord);
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
                        ui.add(egui::DragValue::new(&mut self.score.time_numerator).range(1..=12)); ui.label("/");
                        egui::ComboBox::from_id_salt("denominator").selected_text(self.score.time_denominator.to_string()).show_ui(ui,|ui|{for d in [2,4,8,16]{ui.selectable_value(&mut self.score.time_denominator,d,d.to_string());}});
                        let limit=self.score.measure_ticks();
                        if self.score.parts.iter().any(|p|p.measures.iter().any(|m|m.notes.iter().any(|n|n.start+n.ticks()>limit))) { self.score.time_numerator=previous.0; self.score.time_denominator=previous.1; self.status="Existing notes extend past this time signature. Shorten those measures first.".into(); }
                    }); ui.end_row();
                });
                self.clamp_cursor();
                ui.add_space(12.0); ui.label(RichText::new("All instruments use concert pitch. Key changes update the signature; existing pitches retain their spelling.").size(12.0).color(colors.muted));
                if ui.button("Done").clicked(){self.properties=false;}
            });
            if !open {
                self.properties = false;
            }
        }
        egui::Window::new("Getting started with Encore").open(&mut self.help).default_width(520.0).show(ctx,|ui| {
            ui.heading("From an idea to a printed score"); ui.add_space(8.0);
            for text in ["1. File → New score creates a piano grand staff. Add orchestral staves with + in Instruments.","2. Select Write, choose a duration (1–5), and click a staff. Shift-click an existing event to add a chord tone. R toggles rests; the dot button adds half the duration.","3. A–G enters notes at the cursor and advances it. Set the octave in Inspector. Click a blank staff in Select mode to move the cursor. Left/Right moves by the current duration.","4. Select a note to edit its duration, dynamics, tie, or articulation. Up/Down transposes by a semitone; Shift adds an octave. Delete removes the event. Ctrl+Z undoes.","5. Space plays or pauses the score; Escape stops. M and S mute or solo staves. Click piano keys to audition the active instrument; Shift-click enters a note.","6. Open MusicXML, MuseScore, or supported Finale .musx files with File → Open. Review import notes and save a new Encore project.","7. Save your .encore project, then export PDF for printing, SVG for vector art, MusicXML for notation interchange, MIDI for a sequencer, or WAV for audio."] { ui.label(text); ui.add_space(9.0); }
            ui.separator(); ui.label("This version supports one rhythmic voice per staff, with chords and explicit or automatic rests. Add another staff for an independent voice. Instruments play concert pitch. Instrument library offers 128 SoundFont instruments, with searchable families and note or phrase auditions. Procedural mode provides optional approximations.");
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
        self.keyboard(ctx);
        self.top(ctx);
        self.transport(ctx);
        self.instruments(ctx);
        self.inspector(ctx);
        self.score_canvas(ctx);
        self.dialogs(ctx);
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
        vec!["score-serif".into()],
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
    let color = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
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
                        painter.add(egui::Shape::closed_line(
                            points.clone(),
                            Stroke::new(0.35 * scale, color(*fill)),
                        ));
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
