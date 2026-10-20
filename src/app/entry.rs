//! Entry actions shared by the toolbar and Finale keyboard profiles.
use super::*;

impl EncoreApp {
    pub(super) fn set_entry_voice(&mut self, voice: u8) {
        if voice == self.active_voice || voice > 1 {
            return;
        }
        let selected = self
            .selected
            .and_then(|s| encore::document::Anchor::from_selection(&self.score, s));
        if voice == 1 {
            let launch = selected
                .filter(|a| {
                    a.part == self.active_part && a.layer == self.active_layer && a.voice == 0
                })
                .unwrap_or_else(|| self.caret_anchor());
            let mut launch = launch;
            launch.voice = 0;
            if encore::document::note_at(&self.score, &launch).is_none() {
                self.status = "Voice 2 must launch from an existing Voice 1 note or rest. Select that entry first.".into();
                return;
            }
            self.cursor_measure = launch.measure;
            self.cursor_tick = launch.tick;
            self.active_voice_launch = Some(launch);
        } else {
            self.active_voice_launch = None;
        }
        self.active_voice = voice;
        let anchor = self.caret_anchor();
        self.selected = anchor.selection(&self.score);
        self.status = format!("Layer {} · Voice {}", self.active_layer + 1, voice + 1);
    }

    pub(super) fn entry_voice_launch(
        &self,
        part: usize,
        measure: usize,
        start: u32,
    ) -> Result<Option<u32>, String> {
        if self.active_voice == 0 {
            return Ok(None);
        }
        let notes = &self.score.parts[part].measures[measure].notes;
        if let Some(existing) = notes
            .iter()
            .find(|n| n.layer == self.active_layer && n.voice == 1 && n.start == start)
        {
            return Ok(existing.voice_launch);
        }
        let launch = self
            .active_voice_launch
            .filter(|a| {
                a.part == part
                    && a.measure == measure
                    && a.layer == self.active_layer
                    && a.tick <= start
            })
            .map(|a| a.tick)
            .or_else(|| {
                notes
                    .iter()
                    .find(|n| n.layer == self.active_layer && n.voice == 0 && n.start == start)
                    .map(|n| n.start)
            });
        match launch {
            Some(tick) if notes.iter().any(|n| n.voice == 0 && n.layer == self.active_layer && n.start == tick) => Ok(Some(tick)),
            _ => Err("Voice 2 needs a Voice 1 launch in this measure. Select a primary entry and switch to Voice 2.".into()),
        }
    }
    /// The caret stores a concert pitch; its octave control is displayed in the
    /// active part's written pitch. Keep this conversion out of source notation.
    pub(super) fn entry_transposition(&self) -> encore::parts::Transposition {
        if self.part_view {
            let definitions = encore::parts::definitions(&self.score);
            definitions
                .get(self.selected_linked_part)
                .or_else(|| definitions.first())
                .map(|part| part.transposition)
                .unwrap_or_default()
        } else {
            encore::parts::Transposition::CONCERT
        }
    }

    fn caret_anchor(&self) -> encore::document::Anchor {
        encore::document::Anchor {
            part: self.active_part,
            measure: self.cursor_measure,
            tick: self.cursor_tick,
            layer: self.active_layer,
            voice: self.active_voice,
        }
    }
    fn selected_or_caret_anchor(&self) -> encore::document::Anchor {
        self.selected
            .and_then(|selection| encore::document::Anchor::from_selection(&self.score, selection))
            .unwrap_or_else(|| self.caret_anchor())
    }
    pub(super) fn written_pitch_at(
        &self,
        pitch: Pitch,
        anchor: encore::document::Anchor,
    ) -> Result<Pitch, String> {
        self.entry_transposition()
            .written_pitch(encore::lines::written_pitch(&self.score, anchor, pitch))
    }
    pub(super) fn concert_pitch_at(
        &self,
        written: Pitch,
        anchor: encore::document::Anchor,
    ) -> Result<Pitch, String> {
        let mut source = self.entry_transposition().concert_pitch(written)?;
        let display = encore::lines::written_pitch(&self.score, anchor, source);
        source.octave = source.octave.saturating_add(source.octave - display.octave);
        // Recheck the sounding range after inverting the display-only octave line.
        encore::parts::Transposition::CONCERT.written_pitch(source)
    }
    pub(super) fn entry_written_pitch(&self, pitch: Pitch) -> Result<Pitch, String> {
        self.written_pitch_at(pitch, self.selected_or_caret_anchor())
    }
    fn entry_concert_pitch(&self, pitch: Pitch) -> Result<Pitch, String> {
        self.concert_pitch_at(pitch, self.selected_or_caret_anchor())
    }

    fn written_key(&self) -> Result<i8, String> {
        self.entry_transposition()
            .written_key(self.score.key_fifths)
    }

    fn set_pitch_accidental(&mut self, value: i8, relative: bool) {
        let Some(index) = self.active_pitch_index() else {
            self.accidental = Some(if relative {
                self.accidental
                    .unwrap_or(0)
                    .saturating_add(value)
                    .clamp(-2, 2)
            } else {
                value.clamp(-2, 2)
            });
            return;
        };
        let source = self.selected_entry().unwrap().pitches[index];
        let result = self.entry_written_pitch(source).and_then(|mut written| {
            written.alter = if relative {
                written.alter.saturating_add(value)
            } else {
                value
            };
            if !(-2..=2).contains(&written.alter) {
                return Err(
                    "Written accidentals must be within double-flat and double-sharp.".into(),
                );
            }
            self.entry_concert_pitch(written)
                .map(|source| (source, written))
        });
        match result {
            Ok((source, written)) => {
                if self.edit_entry(|note| note.pitches[index] = source) {
                    self.entry_pitch = source;
                    self.octave = written.octave;
                }
            }
            Err(error) => self.status = format!("Cannot change written pitch: {error}"),
        }
    }

    pub(super) fn entry_ticks(&self) -> u32 {
        Note {
            voice: self.active_voice,
            voice_launch: None,
            start: 0,
            duration: self.duration,
            dotted: self.dotted,
            tuplet: self.tuplet,
            layer: self.active_layer,
            pitches: vec![],
            velocity: 88,
            staccato: false,
            articulations: vec![],
            tied: false,
        }
        .ticks()
        .max(1)
    }

    pub(super) fn edit_entry(&mut self, edit: impl FnOnce(&mut Note)) -> bool {
        let Some(s) = self.selected else { return false };
        let old_anchor = encore::document::Anchor::from_selection(&self.score, s);
        let bar_ticks = self.score.measure_ticks();
        let old_end = self
            .selected_entry()
            .map(|n| s.measure as u32 * bar_ticks + n.start + n.ticks());
        let cursor_at_end =
            old_end == Some(self.cursor_measure as u32 * bar_ticks + self.cursor_tick);
        let mut candidate = self.score.clone();
        let original_note = self.selected_entry().cloned();
        let Some(note) = candidate
            .parts
            .get_mut(s.part)
            .and_then(|p| p.measures.get_mut(s.measure))
            .and_then(|m| m.notes.get_mut(s.note))
        else {
            return false;
        };
        edit(note);
        let now_rest = note.pitches.is_empty();
        if now_rest {
            note.tied = false;
        }
        let new_end = s.measure as u32 * bar_ticks + note.start + note.ticks();
        // Ordinary entry extends a score when the caret fills its last bar.
        // Resizing the entry just typed must preserve that continuation, too.
        // A cursor the user moved elsewhere must not cause a structural edit.
        if cursor_at_end
            && new_end / bar_ticks >= candidate.measure_count() as u32
            && candidate.measure_count() < encore::model::MAX_MEASURES
        {
            candidate.add_measures(1);
        }
        if now_rest && let Some(anchor) = old_anchor {
            encore::document::remove_anchor(&mut candidate, &anchor);
        }
        if let (Some(anchor), Some(original)) = (old_anchor, original_note) {
            encore::entry_style::synchronize_note(&mut candidate, &anchor, &original);
        }
        if let Err(error) = candidate.validate() {
            self.status = format!("Cannot change entry: {error}");
            return false;
        }
        self.score = candidate;
        if cursor_at_end {
            self.cursor_measure = (new_end / bar_ticks) as usize;
            self.cursor_tick = new_end % bar_ticks;
            if self.cursor_measure >= self.score.measure_count() {
                self.cursor_measure = self.score.measure_count() - 1;
                self.cursor_tick = bar_ticks;
            }
        }
        self.status = "Entry updated".into();
        true
    }

    pub(super) fn selected_entry(&self) -> Option<&Note> {
        let s = self.selected?;
        self.score
            .parts
            .get(s.part)?
            .measures
            .get(s.measure)?
            .notes
            .get(s.note)
    }

    pub(super) fn active_pitch_index(&self) -> Option<usize> {
        let note = self.selected_entry()?;
        note.pitches
            .iter()
            .position(|p| *p == self.entry_pitch)
            .or_else(|| note.pitches.len().checked_sub(1))
    }

    fn enter_caret(&mut self, rest: bool, chord: bool) {
        let anchor = if chord {
            self.selected_or_caret_anchor()
        } else {
            self.caret_anchor()
        };
        let written = match self.written_pitch_at(self.entry_pitch, anchor) {
            Ok(mut written) => {
                written.octave = self.octave;
                if let Some(alter) = self.accidental {
                    written.alter = alter;
                }
                written
            }
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let source = match self.concert_pitch_at(written, anchor) {
            Ok(source) => source,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let previous_rest = self.rest;
        let previous_accidental = self.accidental.take();
        self.rest = rest;
        let (part, measure, start) = if chord {
            self.selected
                .map(|selection| {
                    (
                        selection.part,
                        selection.measure,
                        self.score.parts[selection.part].measures[selection.measure].notes
                            [selection.note]
                            .start,
                    )
                })
                .unwrap_or((self.active_part, self.cursor_measure, self.cursor_tick))
        } else {
            (self.active_part, self.cursor_measure, self.cursor_tick)
        };
        self.entry_pitch = source;
        self.enter_note(part, measure, start, source, chord);
        self.rest = previous_rest;
        self.accidental = previous_accidental;
        if let Ok(written) = self.written_pitch_at(self.entry_pitch, anchor) {
            self.octave = written.octave;
        }
    }

    fn navigate_entry(&mut self, direction: i8) {
        let current = self.selected.map(|s| (s.measure, s.note));
        let mut entries: Vec<_> = self.score.parts[self.active_part]
            .measures
            .iter()
            .enumerate()
            .flat_map(|(measure, m)| {
                m.notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.layer == self.active_layer && n.voice == self.active_voice)
                    .map(move |(note, n)| (measure, note, n.start))
            })
            .collect();
        if direction < 0 {
            entries.reverse();
        }
        let target = entries.into_iter().find(|&(measure, note, start)| {
            if let Some((cm, cn)) = current {
                if direction < 0 {
                    (measure, note) < (cm, cn)
                } else {
                    (measure, note) > (cm, cn)
                }
            } else if direction < 0 {
                (measure, start) < (self.cursor_measure, self.cursor_tick)
            } else {
                (measure, start) >= (self.cursor_measure, self.cursor_tick)
            }
        });
        if let Some((measure, note, start)) = target {
            self.selected = Some(Selection {
                part: self.active_part,
                measure,
                note,
            });
            self.cursor_measure = measure;
            self.cursor_tick = start;
            if let Some(pitch) = self.score.parts[self.active_part].measures[measure].notes[note]
                .pitches
                .first()
            {
                self.entry_pitch = *pitch;
                self.octave = self
                    .entry_written_pitch(*pitch)
                    .map_or(pitch.octave, |written| written.octave);
            }
        } else if direction > 0
            && let Some(n) = self.selected_entry()
        {
            let end = n.start + n.ticks();
            self.cursor_tick = end;
            self.selected = None;
            if end >= self.score.measure_ticks()
                && self.cursor_measure + 1 < self.score.measure_count()
            {
                self.cursor_measure += 1;
                self.cursor_tick = 0;
            }
        }
    }

    fn change_entry_pitch(&mut self, steps: i32, octave: bool) {
        let fifths = match self.written_key() {
            Ok(fifths) => fifths,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let index = self.active_pitch_index();
        let base = self
            .selected_entry()
            .filter(|_| self.entry_profile == EntryProfile::SimpleLaptop)
            .and_then(|note| index.map(|index| note.pitches[index]))
            .unwrap_or(self.entry_pitch);
        let mut written = match self.entry_written_pitch(base) {
            Ok(pitch) => pitch,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if self.entry_profile == EntryProfile::Speedy || self.selected.is_none() {
            written.octave = self.octave;
        }
        let caret = if octave {
            Pitch {
                octave: written.octave.saturating_add(steps as i8),
                ..written
            }
        } else {
            Pitch::from_staff_step(written.staff_step() + steps, fifths)
        };
        let anchor = if self.entry_profile == EntryProfile::SimpleLaptop {
            self.selected_or_caret_anchor()
        } else {
            self.caret_anchor()
        };
        let source = match self.concert_pitch_at(caret, anchor) {
            Ok(pitch) => pitch,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if self.entry_profile == EntryProfile::SimpleLaptop && self.selected.is_some() {
            if self.edit_entry(|note| {
                if let Some(index) = index {
                    note.pitches[index] = source;
                }
            }) {
                self.entry_pitch = source;
                self.octave = caret.octave;
            }
        } else {
            self.entry_pitch = source;
            self.octave = caret.octave;
        }
    }

    fn triplet_command(&mut self) {
        if self.tuplet.is_some() {
            self.tuplet = None;
            self.tuplet_remaining = None;
            self.status = "Regular entry timing selected; existing tuplets are unchanged.".into();
            return;
        }
        let triplet = Tuplet {
            actual: 3,
            normal: 2,
        };
        if self.entry_profile == EntryProfile::SimpleLaptop {
            let Some(selection) = self.selected else {
                self.status =
                    "Enter the first note or rest, then press 9 to create its triplet.".into();
                return;
            };
            let mut candidate = self.score.clone();
            let original = candidate.parts[selection.part].measures[selection.measure].notes
                [selection.note]
                .clone();
            let mut first = original.clone();
            first.tuplet = Some(triplet);
            let ticks = match first.checked_ticks() {
                Ok(ticks) => ticks,
                Err(error) => {
                    self.status = error;
                    return;
                }
            };
            let end = first.start + 3 * ticks;
            if end > candidate.measure_ticks() {
                self.status = "The triplet group would cross the barline.".into();
                return;
            }
            let notes = &mut candidate.parts[selection.part].measures[selection.measure].notes;
            if notes.iter().enumerate().any(|(i, n)| {
                i != selection.note
                    && n.lane() == first.lane()
                    && n.start < end
                    && n.start + n.ticks() > first.start
            }) {
                self.status="The triplet would overlap existing entries. Move or remove those entries first.".into();
                return;
            }
            notes[selection.note] = first.clone();
            for index in 1..3 {
                let mut rest = first.clone();
                rest.start = first.start + index * ticks;
                rest.pitches.clear();
                rest.tied = false;
                rest.staccato = false;
                notes.push(rest);
            }
            notes.sort_by_key(|n| n.start);
            if let Err(error) = candidate.validate() {
                self.status = format!("Cannot create triplet: {error}");
                return;
            }
            self.score = candidate;
            let note = self.score.parts[selection.part].measures[selection.measure]
                .notes
                .iter()
                .position(|n| n.start == first.start && n.lane() == first.lane())
                .unwrap();
            self.selected = Some(Selection { note, ..selection });
            self.duration = original.duration;
            self.dotted = original.dotted;
            self.active_part = selection.part;
            self.cursor_measure = selection.measure;
            self.cursor_tick = first.start + ticks;
            self.tuplet = Some(triplet);
            self.tuplet_remaining = Some(2);
            self.status = "Triplet created. Enter two more notes to replace its rests.".into();
        } else {
            self.tuplet = Some(triplet);
            self.tuplet_remaining = Some(3);
            self.status = "Triplet prepared. The next three entries use 3:2 timing.".into();
        }
    }

    pub(super) fn apply_entry_command(&mut self, command: EntryCommand) {
        use EntryCommand::*;
        match command {
            Style(command) => self.apply_style_command(command),
            EditExpressions => self.open_expressions(),
            EditArticulations => self.show_entry_style = true,
            Duration(duration, selected) => {
                if selected {
                    self.edit_entry(|n| n.duration = duration);
                } else {
                    self.duration = duration;
                }
            }
            SpeedyDuration(duration) => {
                self.duration = duration;
                let selected_at_caret = self.selected.is_some_and(|s| {
                    s.part == self.active_part
                        && s.measure == self.cursor_measure
                        && self.score.parts[s.part].measures[s.measure].notes[s.note].lane()
                            == self.active_layer + 4 * self.active_voice
                        && self.score.parts[s.part].measures[s.measure].notes[s.note].start
                            == self.cursor_tick
                });
                if selected_at_caret {
                    if self.edit_entry(|n| n.duration = duration) {
                        let n = self.selected_entry().unwrap();
                        self.cursor_tick = n.start + n.ticks();
                        self.selected = None;
                    }
                } else {
                    self.enter_caret(self.rest, false);
                }
            }
            Pitch(step, chord) => {
                let fifths = match self.written_key() {
                    Ok(fifths) => fifths,
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                };
                let mut written = encore::model::Pitch::from_staff_step(
                    i32::from(self.octave) * 7 + step.index(),
                    fifths,
                );
                if let Some(alter) = self.accidental {
                    written.alter = alter;
                }
                let pitch = match self.concert_pitch_at(written, self.caret_anchor()) {
                    Ok(pitch) => pitch,
                    Err(error) => {
                        self.status = error;
                        return;
                    }
                };
                let at_selected = self.selected.is_some_and(|selection| {
                    selection.part == self.active_part
                        && selection.measure == self.cursor_measure
                        && self.score.parts[selection.part].measures[selection.measure].notes
                            [selection.note]
                            .lane()
                            == self.active_layer + 4 * self.active_voice
                        && self.score.parts[selection.part].measures[selection.measure].notes
                            [selection.note]
                            .start
                            == self.cursor_tick
                });
                if !chord && at_selected {
                    let index = self.active_pitch_index();
                    if self.edit_entry(|note| {
                        if let Some(index) = index {
                            note.pitches[index] = pitch;
                        } else {
                            note.pitches.push(pitch);
                        }
                    }) {
                        self.entry_pitch = pitch;
                        self.octave = written.octave;
                    }
                } else {
                    self.entry_pitch = pitch;
                    self.enter_caret(self.rest, chord);
                }
            }
            Enter => {
                let at_selected = self.selected.is_some_and(|selection| {
                    selection.part == self.active_part
                        && selection.measure == self.cursor_measure
                        && self.score.parts[selection.part].measures[selection.measure].notes
                            [selection.note]
                            .start
                            == self.cursor_tick
                        && self.score.parts[selection.part].measures[selection.measure].notes
                            [selection.note]
                            .lane()
                            == self.active_layer + 4 * self.active_voice
                });
                if self.entry_profile == EntryProfile::SimpleLaptop && at_selected {
                    let note = self.selected_entry().unwrap();
                    let end = note.start + note.ticks();
                    if let Some(pitch) = note.pitches.first().copied() {
                        self.entry_pitch = pitch;
                        self.octave = self
                            .entry_written_pitch(pitch)
                            .map_or(pitch.octave, |written| written.octave);
                    }
                    self.cursor_tick = end;
                    if end == self.score.measure_ticks()
                        && self.cursor_measure + 1 < self.score.measure_count()
                    {
                        self.cursor_measure += 1;
                        self.cursor_tick = 0;
                    }
                    self.selected = None;
                    self.status = "Entry caret activated after the selected event.".into();
                } else {
                    self.enter_caret(self.rest, false);
                }
            }
            Chord => self.enter_caret(false, true),
            Interval(interval) => {
                let Some(base) = self
                    .active_pitch_index()
                    .and_then(|index| self.selected_entry().map(|note| note.pitches[index]))
                else {
                    return;
                };
                let result = self.entry_written_pitch(base).and_then(|written| {
                    let steps = i32::from(interval.signum()) * (i32::from(interval.abs()) - 1);
                    let pitch = encore::model::Pitch::from_staff_step(
                        written.staff_step() + steps,
                        self.written_key()?,
                    );
                    self.entry_concert_pitch(pitch)
                        .map(|source| (source, pitch.octave))
                });
                match result {
                    Ok((source, octave)) => {
                        self.entry_pitch = source;
                        self.octave = octave;
                        self.enter_caret(false, true);
                    }
                    Err(error) => self.status = error,
                }
            }
            SpeedyRest(duration) => {
                self.duration = duration;
                self.enter_caret(true, false);
            }
            EnterRest => self.enter_caret(true, false),
            ToggleRestTool => self.rest = !self.rest,
            ToggleRest => {
                let pitch = self.entry_pitch;
                if self.selected.is_some() {
                    self.edit_entry(|n| {
                        if n.pitches.is_empty() {
                            n.pitches.push(pitch)
                        } else {
                            n.pitches.clear();
                            n.tied = false;
                        }
                    });
                }
            }
            Dot(tool) => {
                if tool {
                    self.dotted = !self.dotted;
                } else if self.selected.is_some() {
                    self.edit_entry(|n| n.dotted = !n.dotted);
                } else {
                    self.dotted = !self.dotted;
                }
            }
            Tie(previous) => {
                let location = (
                    self.selected,
                    self.cursor_measure,
                    self.cursor_tick,
                    self.entry_pitch,
                    self.octave,
                );
                if previous {
                    self.navigate_entry(-1);
                }
                if !previous || self.selected != location.0 {
                    self.toggle_selected_pitch_tie();
                }
                if previous {
                    (
                        self.selected,
                        self.cursor_measure,
                        self.cursor_tick,
                        self.entry_pitch,
                        self.octave,
                    ) = location;
                }
            }
            Accidental(change) => self.set_pitch_accidental(change, change != 0),
            SetAccidental(alter) => self.set_pitch_accidental(alter, false),
            ToolAccidental(alter) => self.accidental = Some(alter),
            Boundary(direction) => {
                let measure = &self.score.parts[self.active_part].measures[self.cursor_measure];
                if direction < 0 {
                    if let Some((note, n)) = measure
                        .notes
                        .iter()
                        .enumerate()
                        .filter(|(_, n)| {
                            n.layer == self.active_layer && n.voice == self.active_voice
                        })
                        .min_by_key(|(_, n)| n.start)
                    {
                        self.selected = Some(Selection {
                            part: self.active_part,
                            measure: self.cursor_measure,
                            note,
                        });
                        self.cursor_tick = n.start;
                    } else {
                        self.selected = None;
                        self.cursor_tick = 0;
                    }
                } else {
                    self.cursor_tick = measure
                        .notes
                        .iter()
                        .filter(|n| n.layer == self.active_layer && n.voice == self.active_voice)
                        .map(|n| n.start + n.ticks())
                        .max()
                        .unwrap_or(0);
                    self.selected = None;
                }
            }
            Diatonic(steps) => self.change_entry_pitch(steps, false),
            Octave(octaves) => self.change_entry_pitch(i32::from(octaves), true),
            Navigate(direction) => self.navigate_entry(direction),
            Measure(direction) => {
                self.cursor_measure = (self.cursor_measure as isize + isize::from(direction))
                    .clamp(0, self.score.measure_count() as isize - 1)
                    as usize;
                self.cursor_tick = 0;
                self.selected = None;
            }
            Layer(direction) => {
                self.active_layer =
                    (i16::from(self.active_layer) + i16::from(direction)).rem_euclid(4) as u8;
                self.selected = None;
                self.active_voice_launch = None;
                self.status = format!("Layer {}", self.active_layer + 1);
            }
            ToggleVoice => self.set_entry_voice(1 - self.active_voice),
            Staff(direction) => {
                if self.part_view {
                    let definitions = encore::parts::definitions(&self.score);
                    if let Some(part) = definitions
                        .get(self.selected_linked_part)
                        .or_else(|| definitions.first())
                    {
                        let current = part
                            .staves
                            .iter()
                            .position(|staff| *staff == self.active_part)
                            .unwrap_or(0);
                        let next = (current as isize + isize::from(direction))
                            .clamp(0, part.staves.len() as isize - 1)
                            as usize;
                        self.active_part = part.staves[next];
                    }
                } else {
                    self.active_part = (self.active_part as isize + isize::from(direction))
                        .clamp(0, self.score.parts.len() as isize - 1)
                        as usize;
                }
                self.selected = None;
            }
            Delete => self.delete_selected(),
            RemoveTone => {
                let index = self.active_pitch_index();
                self.edit_entry(|n| {
                    if let Some(index) = index {
                        n.pitches.remove(index);
                    }
                    if n.pitches.is_empty() {
                        n.tied = false;
                    }
                });
            }
            ClearSelection => {
                self.selected = None;
            }
            Triplet => self.triplet_command(),
            ExitEntry => {
                self.write_mode = false;
                self.selected = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clarinet() -> EncoreApp {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].instrument = Instrument::Clarinet;
        score.parts[0].name = "Clarinet in B♭".into();
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.part_view = true;
        app
    }

    #[test]
    fn linked_clarinet_letters_and_chord_intervals_store_concert_pitches() {
        let mut app = clarinet();
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            60
        );
        assert_eq!(app.entry_pitch.midi(), 60);
        assert_eq!(app.octave, 4);
        app.apply_entry_command(EntryCommand::Interval(3));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0]
                .pitches
                .iter()
                .map(|pitch| pitch.midi())
                .collect::<Vec<_>>(),
            vec![60, 64]
        );
        let projection = app.selected_part_projection().unwrap().unwrap();
        assert_eq!(
            projection.score.parts[0].measures[0].notes[0]
                .pitches
                .iter()
                .map(|pitch| pitch.midi())
                .collect::<Vec<_>>(),
            vec![62, 66]
        );
        app.score.validate().unwrap();
    }

    #[test]
    fn linked_clarinet_natural_and_diatonic_edit_apply_to_written_note_name() {
        let mut app = clarinet();
        app.apply_entry_command(EntryCommand::Pitch(Step::F, false));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            64
        );
        app.apply_entry_command(EntryCommand::SetAccidental(0));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0],
            encore::model::Pitch {
                step: Step::E,
                octave: 4,
                alter: -1
            }
        );
        let written = app.entry_written_pitch(app.entry_pitch).unwrap();
        assert_eq!((written.step, written.alter), (Step::F, 0));
        app.apply_entry_command(EntryCommand::Diatonic(1));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            65
        );
        assert_eq!(
            app.entry_written_pitch(app.entry_pitch).unwrap().step,
            Step::G
        );
        app.apply_entry_command(EntryCommand::Octave(1));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            77
        );
        assert_eq!(app.octave, 5);
        app.score.validate().unwrap();
    }

    #[test]
    fn linked_speedy_caret_and_pending_accidental_convert_only_once() {
        let mut app = clarinet();
        app.entry_profile = EntryProfile::Speedy;
        app.entry_pitch = encore::model::Pitch::from_midi(60);
        app.octave = 5;
        app.accidental = Some(1);
        app.apply_entry_command(EntryCommand::SpeedyDuration(Duration::Quarter));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            73
        );
        assert_eq!(app.octave, 5);
        assert_eq!(app.accidental, Some(1));
        app.score.validate().unwrap();
    }

    #[test]
    fn octave_transposing_part_navigation_keeps_written_octave() {
        let mut app = clarinet();
        app.score.parts[0].instrument = Instrument::DoubleBass;
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            48
        );
        app.apply_entry_command(EntryCommand::Navigate(-1));
        assert_eq!(app.entry_pitch.midi(), 48);
        assert_eq!(app.octave, 4);
    }

    #[test]
    fn rest_conversion_clears_anchored_lyrics_and_slurs_in_one_edit() {
        let mut app = clarinet();
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::E, false));
        let start = encore::document::Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = encore::document::Anchor { tick: 480, ..start };
        encore::lyrics::assign(&mut app.score, encore::lyrics::Lyric::new(start, "Sing")).unwrap();
        encore::slurs::add(&mut app.score, encore::slurs::Slur::new(start, end)).unwrap();
        app.apply_entry_command(EntryCommand::Navigate(-1));
        app.apply_entry_command(EntryCommand::ToggleRest);
        assert!(app.score.parts[0].measures[0].notes[0].pitches.is_empty());
        assert!(app.score.notation.lyrics.is_empty());
        assert!(app.score.notation.slurs.is_empty());
        app.score.validate().unwrap();
    }

    #[test]
    fn simple_enter_activates_caret_without_replacing_a_selected_chord() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Duration(Duration::Half, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::E, true));
        app.apply_entry_command(EntryCommand::Boundary(-1));
        app.apply_entry_command(EntryCommand::Duration(Duration::Quarter, false));
        let original = app.score.clone();
        app.apply_entry_command(EntryCommand::Enter);
        assert_eq!(app.score, original);
        assert_eq!(app.cursor_tick, 960);
        assert!(app.selected.is_none());
    }

    fn app() -> EncoreApp {
        EncoreApp::from_score(Score::blank(), None, "Ready".into())
    }

    #[test]
    fn simple_triplet_converts_entered_note_and_replaces_two_placeholder_rests() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Duration(Duration::Eighth, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        assert_eq!(app.cursor_tick, 240);
        app.apply_entry_command(EntryCommand::Triplet);
        assert_eq!(app.score.parts[0].measures[0].notes.len(), 3);
        assert!(
            app.score.parts[0].measures[0].notes[1..]
                .iter()
                .all(|n| n.pitches.is_empty())
        );
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::E, false));
        let notes = &app.score.parts[0].measures[0].notes;
        assert_eq!(
            notes
                .iter()
                .map(|n| (n.start, n.ticks(), n.pitches[0].midi()))
                .collect::<Vec<_>>(),
            vec![(0, 160, 60), (160, 160, 62), (320, 160, 64)]
        );
        assert_eq!(app.cursor_tick, 480);
        assert!(app.tuplet.is_none() && app.tuplet_remaining.is_none());
        app.score.validate().unwrap();
    }

    #[test]
    fn speedy_triplet_precedes_entry_and_stops_after_three_notes() {
        let mut app = app();
        app.entry_profile = EntryProfile::Speedy;
        app.apply_entry_command(EntryCommand::Triplet);
        for _ in 0..3 {
            app.apply_entry_command(EntryCommand::SpeedyDuration(Duration::Eighth));
        }
        assert_eq!(app.cursor_tick, 480);
        assert!(app.tuplet.is_none());
        app.apply_entry_command(EntryCommand::SpeedyDuration(Duration::Quarter));
        assert_eq!(app.score.parts[0].measures[0].notes[3].ticks(), 480);
        app.score.validate().unwrap();
    }

    #[test]
    fn dot_and_selected_duration_changes_keep_an_advancing_caret_at_the_new_end() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.apply_entry_command(EntryCommand::Dot(false));
        assert_eq!(app.cursor_tick, 720);
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        assert_eq!(app.score.parts[0].measures[0].notes[1].start, 720);
        app.apply_entry_command(EntryCommand::Duration(Duration::Eighth, true));
        assert_eq!(app.cursor_tick, 960);
        app.apply_entry_command(EntryCommand::Pitch(Step::E, false));
        assert_eq!(app.score.parts[0].measures[0].notes[2].start, 960);
        app.score.validate().unwrap();
    }

    #[test]
    fn chord_pitch_accidentals_and_diatonic_motion_target_one_tone() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::E, true));
        app.apply_entry_command(EntryCommand::Accidental(1));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0]
                .pitches
                .iter()
                .map(|p| p.midi())
                .collect::<Vec<_>>(),
            vec![60, 65]
        );
        app.apply_entry_command(EntryCommand::Diatonic(1));
        let pitches = &app.score.parts[0].measures[0].notes[0].pitches;
        assert_eq!(pitches[0].midi(), 60);
        assert_eq!(pitches[1].step, Step::F);
        assert_eq!(pitches[1].alter, 0);
        app.score.validate().unwrap();
    }

    #[test]
    fn tie_previous_preserves_the_current_selection_and_caret() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        let location = (app.selected, app.cursor_measure, app.cursor_tick);
        app.apply_entry_command(EntryCommand::Tie(true));
        assert!(app.score.parts[0].measures[0].notes[0].tied);
        assert_eq!(
            (app.selected, app.cursor_measure, app.cursor_tick),
            location
        );
    }

    #[test]
    fn independent_layer_entry_and_navigation_do_not_replace_or_select_other_layers() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.active_layer = 1;
        app.cursor_tick = 0;
        app.selected = None;
        app.apply_entry_command(EntryCommand::Pitch(Step::E, false));
        let notes = &app.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].layer, 0);
        assert_eq!(notes[1].layer, 1);
        app.apply_entry_command(EntryCommand::Boundary(-1));
        let selection = app.selected.unwrap();
        assert_eq!(
            app.score.parts[selection.part].measures[selection.measure].notes[selection.note].layer,
            1
        );
        app.score.validate().unwrap();
    }

    #[test]
    fn resizing_the_last_entered_note_to_a_barline_extends_the_advancing_caret() {
        let mut app = app();
        for part in &mut app.score.parts {
            part.measures.truncate(1);
        }
        app.cursor_tick = 960;
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        assert_eq!((app.cursor_measure, app.cursor_tick), (0, 1440));
        app.apply_entry_command(EntryCommand::Duration(Duration::Half, true));
        assert_eq!(app.score.measure_count(), 2);
        assert_eq!((app.cursor_measure, app.cursor_tick), (1, 0));
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        assert_eq!(
            app.score.parts[0].measures[1].notes[0].pitches[0].step,
            Step::D
        );
        app.score.validate().unwrap();
    }

    #[test]
    fn resizing_an_entry_does_not_extend_when_the_cursor_was_moved_elsewhere() {
        let mut app = app();
        for part in &mut app.score.parts {
            part.measures.truncate(1);
        }
        app.cursor_tick = 960;
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.cursor_tick = 0;
        app.apply_entry_command(EntryCommand::Duration(Duration::Half, true));
        assert_eq!(app.score.measure_count(), 1);
        assert_eq!((app.cursor_measure, app.cursor_tick), (0, 0));
        app.score.validate().unwrap();
    }

    #[test]
    fn changing_active_layer_does_not_repitch_the_old_layers_selection() {
        let mut app = app();
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.cursor_tick = 0;
        app.active_layer = 1;
        app.apply_entry_command(EntryCommand::Pitch(Step::E, false));
        let notes = &app.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].pitches[0].step, Step::C);
        assert_eq!(notes[1].pitches[0].step, Step::E);
        assert_eq!((notes[0].layer, notes[1].layer), (0, 1));
        app.score.validate().unwrap();
    }

    #[test]
    fn resizing_at_the_measure_limit_keeps_a_valid_bounded_cursor() {
        let mut app = app();
        app.score.add_measures(encore::model::MAX_MEASURES);
        app.cursor_measure = encore::model::MAX_MEASURES - 1;
        app.cursor_tick = 960;
        app.apply_entry_command(EntryCommand::Pitch(Step::C, false));
        app.apply_entry_command(EntryCommand::Duration(Duration::Half, true));
        assert_eq!(app.score.measure_count(), encore::model::MAX_MEASURES);
        assert_eq!(app.cursor_measure, encore::model::MAX_MEASURES - 1);
        assert_eq!(app.cursor_tick, app.score.measure_ticks());
        app.score.validate().unwrap();
    }
}

#[cfg(test)]
mod ottava_tests {
    use super::*;
    #[test]
    fn written_keyboard_entry_inverts_ottava_and_linked_part_transposition() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].instrument = Instrument::Clarinet;
        let a = encore::document::Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        score
            .notation
            .lines
            .push(encore::lines::PerformanceLine::new(
                a,
                encore::document::Anchor { tick: 960, ..a },
                encore::lines::LineKind::Ottava { semitones: 12 },
            ));
        let mut app = EncoreApp::from_score(score, None, "Ready".into());
        app.part_view = true;
        app.octave = 4;
        app.apply_entry_command(EntryCommand::Pitch(Step::D, false));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            72
        );
        let written = app.entry_written_pitch(app.entry_pitch).unwrap();
        assert_eq!((written.step, written.octave), (Step::D, 4));
        app.apply_entry_command(EntryCommand::Octave(1));
        assert_eq!(
            app.score.parts[0].measures[0].notes[0].pitches[0].midi(),
            84
        );
        app.score.validate().unwrap();
    }
}

#[cfg(test)]
mod voice_tests {
    use super::*;
    fn app() -> EncoreApp {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.duration = Duration::Half;
        app.enter_note(0, 0, 0, Pitch::from_midi(60), false);
        app.cursor_measure = 0;
        app.cursor_tick = 0;
        app
    }
    #[test]
    fn speedy_second_voice_launches_once_and_preserves_primary_rhythm() {
        let mut app = app();
        app.entry_profile = EntryProfile::Speedy;
        app.apply_entry_command(EntryCommand::ToggleVoice);
        assert_eq!((app.active_voice, app.cursor_tick), (1, 0));
        for _ in 0..4 {
            app.apply_entry_command(EntryCommand::SpeedyDuration(Duration::Eighth));
        }
        let notes = &app.score.parts[0].measures[0].notes;
        assert_eq!(notes.len(), 5);
        assert_eq!(notes.iter().find(|n| n.voice == 0).unwrap().ticks(), 960);
        assert_eq!(
            notes
                .iter()
                .filter(|n| n.voice == 1)
                .map(|n| (n.start, n.voice_launch))
                .collect::<Vec<_>>(),
            vec![(0, Some(0)), (240, Some(0)), (480, Some(0)), (720, Some(0))]
        );
        app.score.validate().unwrap();
        app.apply_entry_command(EntryCommand::ToggleVoice);
        assert_eq!(app.active_voice, 0);
    }
    #[test]
    fn primary_launcher_deletion_is_atomic_and_rest_conversion_keeps_branch() {
        let mut app = app();
        let primary = app.selected.unwrap();
        app.set_entry_voice(1);
        app.duration = Duration::Quarter;
        app.enter_note(0, 0, 0, Pitch::from_midi(67), false);
        app.selected = Some(primary);
        let before = app.score.clone();
        app.delete_selected();
        assert_eq!(app.score, before);
        assert!(app.status.contains("secondary branch"));
        assert!(app.edit_entry(|n| n.pitches.clear()));
        app.score.validate().unwrap();
        assert!(
            app.score.parts[0].measures[0]
                .notes
                .iter()
                .any(|n| n.voice == 0 && n.pitches.is_empty())
        );
        assert_eq!(
            app.score.parts[0].measures[0]
                .notes
                .iter()
                .find(|n| n.voice == 1)
                .unwrap()
                .voice_launch,
            Some(0)
        );
    }
    #[test]
    fn second_voice_requires_primary_and_navigation_stays_in_voice() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.set_entry_voice(1);
        assert_eq!(app.active_voice, 0);
        assert!(app.status.contains("launch"));
        app = self::app();
        app.set_entry_voice(1);
        app.duration = Duration::Quarter;
        app.enter_note(0, 0, 0, Pitch::from_midi(67), false);
        app.enter_note(0, 0, 480, Pitch::from_midi(69), false);
        app.navigate_entry(-1);
        assert_eq!(app.selected_entry().unwrap().voice, 1);
        assert_eq!(app.selected_entry().unwrap().start, 0);
        assert!(!app.edit_entry(|n| n.start = 480));
        app.score.validate().unwrap();
    }
}

#[cfg(test)]
mod transactional_voice_tests {
    use super::*;
    #[test]
    fn adding_chord_to_secondary_glissando_endpoint_rolls_back_entry_and_cursor() {
        let mut app = EncoreApp::from_score(Score::blank(), None, "Ready".into());
        app.duration = Duration::Half;
        app.enter_note(0, 0, 0, Pitch::from_midi(72), false);
        app.cursor_tick = 0;
        app.set_entry_voice(1);
        app.duration = Duration::Quarter;
        app.enter_note(0, 0, 0, Pitch::from_midi(60), false);
        app.enter_note(0, 0, 480, Pitch::from_midi(67), false);
        let start = encore::document::Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 1,
        };
        app.score
            .notation
            .lines
            .push(encore::lines::PerformanceLine::new(
                start,
                encore::document::Anchor { tick: 480, ..start },
                encore::lines::LineKind::Glissando,
            ));
        app.score.validate().unwrap();
        let before = app.score.clone();
        let selected = app.selected;
        let cursor = (app.cursor_measure, app.cursor_tick);
        app.enter_note(0, 0, 480, Pitch::from_midi(71), true);
        assert_eq!(app.score, before);
        assert_eq!(app.selected, selected);
        assert_eq!((app.cursor_measure, app.cursor_tick), cursor);
        assert!(app.status.contains("single-pitch"));
    }
}
