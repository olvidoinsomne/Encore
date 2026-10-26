//! Interchange exporters written from the public MusicXML and Standard MIDI formats.
pub(crate) mod percussion;
use crate::model::{Clef, Duration, Note, PPQ, Part, Pitch, Score};
use std::collections::HashMap;
use std::fmt::Write as _;

pub(crate) fn xml(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            c == '\t'
                || c == '\n'
                || c == '\r'
                || ('\u{20}'..='\u{d7ff}').contains(&c)
                || ('\u{e000}'..='\u{fffd}').contains(&c)
                || c >= '\u{10000}'
        })
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn expression_directions(score: &Score, part: usize, measure: usize) -> String {
    use crate::expressions::{ExpressionKind, HairpinKind, Placement};
    let mut out = String::new();
    let direction = |out: &mut String,
                     at: u32,
                     layer: u8,
                     placement: Placement,
                     body: String,
                     sound: String| {
        let _ = writeln!(
            out,
            "      <direction placement=\"{}\"><direction-type>{body}</direction-type><offset>{at}</offset><voice>{}</voice>{sound}</direction>",
            if placement == Placement::Above {
                "above"
            } else {
                "below"
            },
            layer + 1
        );
    };
    for mark in
        score.notation.expressions.iter().filter(|mark| {
            mark.anchor.measure == measure && (mark.anchor.part == part || mark.system)
        })
    {
        let body = match mark.kind {
            ExpressionKind::Dynamic => {
                let symbol = if [
                    "pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "sfz", "fp",
                ]
                .contains(&mark.text.as_str())
                {
                    format!("<{}/>", mark.text)
                } else {
                    format!("<other-dynamics>{}</other-dynamics>", xml(&mark.text))
                };
                format!("<dynamics>{symbol}</dynamics>")
            }
            ExpressionKind::Rehearsal => format!("<rehearsal>{}</rehearsal>", xml(&mark.text)),
            _ => format!("<words>{}</words>", xml(&mark.text)),
        };
        let hidden = serde_json::to_string(mark)
            .ok()
            .map_or_else(String::new, |json| {
                format!(
                    "<other-direction print-object=\"no\">encore-expression:{}</other-direction>",
                    xml(&json)
                )
            });
        let sound = mark.velocity.map_or_else(String::new, |velocity| {
            format!(
                "<sound dynamics=\"{:.5}\"/>",
                f64::from(velocity) * 100.0 / 90.0
            )
        });
        direction(
            &mut out,
            mark.anchor.tick,
            mark.anchor.layer,
            mark.placement,
            body + &hidden,
            sound,
        );
    }
    for line in score
        .notation
        .spanners
        .iter()
        .filter(|line| line.start.part == part)
    {
        let number = line.start.layer + 1;
        if line.end.measure == measure {
            direction(
                &mut out,
                line.end.tick,
                line.end.layer,
                line.placement,
                format!("<wedge type=\"stop\" number=\"{number}\"/>"),
                String::new(),
            );
        }
        if line.start.measure == measure {
            let metadata = serde_json::to_string(line)
                .ok()
                .map_or_else(String::new, |json| {
                    format!(
                        "<other-direction print-object=\"no\">encore-hairpin:{}</other-direction>",
                        xml(&json)
                    )
                });
            direction(
                &mut out,
                line.start.tick,
                line.start.layer,
                line.placement,
                format!(
                    "<wedge type=\"{}\" number=\"{number}\"/>{metadata}",
                    if line.kind == HairpinKind::Crescendo {
                        "crescendo"
                    } else {
                        "diminuendo"
                    }
                ),
                String::new(),
            );
        }
    }
    for change in &score.notation.tempo {
        if let Some(ramp) = &change.ramp
            && ramp.end.measure == measure
        {
            direction(
                &mut out,
                ramp.end.tick,
                0,
                Placement::Above,
                "<dashes type=\"stop\" number=\"1\"/>".into(),
                format!("<sound tempo=\"{}\"/>", ramp.bpm),
            );
        }
    }
    for change in score
        .notation
        .tempo
        .iter()
        .filter(|change| change.at.measure == measure)
    {
        let mut body = if change.text.is_empty() {
            String::new()
        } else {
            format!("<words>{}</words>", xml(&change.text))
        };
        body.push_str(&format!(
            "<metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome>",
            change.bpm
        ));
        if change.ramp.is_some() {
            body.push_str("<dashes type=\"start\" number=\"1\"/>");
        }
        direction(
            &mut out,
            change.at.tick,
            0,
            Placement::Above,
            body,
            format!("<sound tempo=\"{}\"/>", change.bpm),
        );
    }
    out
}
fn channel(index: usize) -> u8 {
    let n = (index % 15) as u8;
    if n >= 9 { n + 1 } else { n }
}
fn flattened(part: &Part, measure_ticks: u32) -> Vec<(u32, &Note)> {
    part.measures
        .iter()
        .enumerate()
        .flat_map(|(bar, measure)| {
            measure
                .notes
                .iter()
                .map(move |note| (bar as u32 * measure_ticks + note.start, note))
        })
        .collect()
}
struct Timeline<'a> {
    notes: Vec<(u32, &'a Note)>,
    starts: HashMap<(u32, u8), usize>,
    ends: HashMap<(u32, u8), usize>,
}
impl<'a> Timeline<'a> {
    fn new(part: &'a Part, measure_ticks: u32) -> Self {
        let notes = flattened(part, measure_ticks);
        let starts = notes
            .iter()
            .enumerate()
            .map(|(i, (t, note))| ((*t, note.lane()), i))
            .collect();
        let ends = notes
            .iter()
            .enumerate()
            .map(|(i, (t, n))| ((*t + n.ticks(), n.lane()), i))
            .collect();
        Self {
            notes,
            starts,
            ends,
        }
    }
    fn tie_in(
        &self,
        index: &crate::entry_style::Index<'_>,
        score: &Score,
        part: usize,
        start: u32,
        layer: u8,
        midi: u8,
    ) -> bool {
        self.ends.get(&(start, layer)).is_some_and(|i| {
            index.ties_forward(
                &crate::document::Anchor {
                    voice: self.notes[*i].1.voice,
                    part,
                    measure: (self.notes[*i].0 / score.measure_ticks()) as usize,
                    tick: self.notes[*i].1.start,
                    layer: self.notes[*i].1.layer,
                },
                self.notes[*i].1,
                midi,
            ) && self.notes[*i].1.pitches.iter().any(|p| p.midi() == midi)
        })
    }
    fn tie_out(
        &self,
        index: &crate::entry_style::Index<'_>,
        score: &Score,
        part: usize,
        start: u32,
        note: &Note,
        midi: u8,
    ) -> bool {
        index.ties_forward(
            &crate::document::Anchor {
                voice: note.voice,
                part,
                measure: (start / score.measure_ticks()) as usize,
                tick: note.start,
                layer: note.layer,
            },
            note,
            midi,
        ) && self
            .starts
            .get(&(start + note.ticks(), note.lane()))
            .is_some_and(|i| self.notes[*i].1.pitches.iter().any(|p| p.midi() == midi))
    }
}

/// A concert-pitch MusicXML 4.0 score. Gaps are filled by explicit rests, so
/// every part has an identical measured timeline in other notation programs.
pub fn musicxml(score: &Score) -> String {
    if score.validate().is_err() {
        return String::new();
    }
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n<score-partwise version=\"4.0\">\n",
    );
    let _ = writeln!(
        out,
        "  <work><work-title>{}</work-title></work>\n  <movement-title>{}</movement-title>\n  <identification><creator type=\"composer\">{}</creator><encoding><software>Encore</software></encoding>{}</identification>",
        xml(&score.title),
        xml(&score.title),
        xml(&score.composer),
        crate::groups::native_metadata(score)
    );
    out.push_str("  <defaults><scaling><millimeters>7</millimeters><tenths>40</tenths></scaling></defaults>\n  <part-list>\n");
    for (pi, part) in score.parts.iter().enumerate() {
        out.push_str(&crate::groups::musicxml_start(score, pi));
        let id = pi + 1;
        let volume = if part.muted || (score.parts.iter().any(|p| p.solo && !p.muted) && !part.solo)
        {
            0.0
        } else {
            (part.volume * 100.0).clamp(0.0, 100.0)
        };
        if part.percussion.is_some() {
            out.push_str(&percussion::part_list(pi, part, volume));
            out.push_str(&crate::groups::musicxml_stop(score, pi));
            continue;
        }
        let _ = writeln!(
            out,
            "    <score-part id=\"P{id}\"><part-name>{}</part-name><part-abbreviation>{}</part-abbreviation><score-instrument id=\"I{id}\"><instrument-name>{}</instrument-name></score-instrument>{}<midi-instrument id=\"I{id}\"><midi-channel>{}</midi-channel><midi-program>{}</midi-program><volume>{volume:.2}</volume><pan>{:.2}</pan></midi-instrument></score-part>",
            xml(&part.name),
            xml(part.instrument.name()),
            xml(part.instrument.name()),
            percussion::device(pi),
            1,
            part.instrument.gm_program() + 1,
            part.pan * 90.0
        );
        out.push_str(&crate::groups::musicxml_stop(score, pi));
    }
    out.push_str("  </part-list>\n");
    let style_index = crate::entry_style::Index::new(score);
    let grace = crate::grace::MusicXmlIndex::new(score);
    let lines = crate::lines::MusicXmlIndex::new(score);
    let ornaments = crate::ornaments::MusicXmlIndex::new(score);
    let tremolos = crate::tremolos::Index::new(score);
    let notation = xml_annotations(score, &grace, &lines);
    let measure_ticks = score.measure_ticks();
    for (pi, part) in score.parts.iter().enumerate() {
        let _ = writeln!(out, "  <part id=\"P{}\">", pi + 1);
        let timeline = Timeline::new(part, measure_ticks);
        let mut previous_dynamic: [Option<&str>; 4] = [None; 4];
        let layers = (0..8)
            .filter(|layer| {
                part.measures
                    .iter()
                    .flat_map(|measure| &measure.notes)
                    .any(|note| note.lane() == *layer)
            })
            .collect::<Vec<_>>();
        let layers = if layers.is_empty() { vec![0] } else { layers };
        for (mi, measure) in part.measures.iter().enumerate() {
            let _ = writeln!(out, "    <measure number=\"{}\">", mi + 1);
            if mi == 0 {
                if let Some(staff) = &part.percussion {
                    let _ = writeln!(
                        out,
                        "<attributes><divisions>{PPQ}</divisions><time><beats>{}</beats><beat-type>{}</beat-type></time><clef><sign>percussion</sign></clef><staff-details><staff-lines>{}</staff-lines></staff-details></attributes>",
                        score.time_numerator, score.time_denominator, staff.lines
                    );
                    out.push_str(&percussion::native(part));
                } else {
                    let (sign, line) = match part.clef {
                        Clef::Treble => ("G", 2),
                        Clef::Bass => ("F", 4),
                        Clef::Alto => ("C", 3),
                    };
                    let _ = writeln!(
                        out,
                        "      <attributes><divisions>{PPQ}</divisions><key><fifths>{}</fifths></key><time><beats>{}</beats><beat-type>{}</beat-type></time><clef><sign>{sign}</sign><line>{line}</line></clef></attributes>",
                        score.key_fifths, score.time_numerator, score.time_denominator
                    );
                }
                let _ = writeln!(
                    out,
                    "      <direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome><other-direction print-object=\"no\">encore-initial-tempo</other-direction></direction-type><sound tempo=\"{}\"/></direction>",
                    score.tempo, score.tempo
                );
            }
            out.push_str(&expression_directions(score, pi, mi));
            out.push_str(lines.directions(pi, mi));
            out.push_str(&crate::chords::musicxml(score, pi, mi));
            out.push_str(&crate::repeats::musicxml_before_measure(score, mi));
            out.push_str(&crate::repeats::musicxml_navigation_before_measure(
                score, mi,
            ));
            for (layer_index, &layer) in layers.iter().enumerate() {
                if layer_index > 0 {
                    let _ = writeln!(
                        out,
                        "      <backup><duration>{measure_ticks}</duration></backup>"
                    );
                }
                let layer_notes = measure
                    .notes
                    .iter()
                    .filter(|note| note.lane() == layer)
                    .collect::<Vec<_>>();
                if layer_notes.is_empty() {
                    if layer >= 4 {
                        write_gap(&mut out, measure_ticks, layer);
                        continue;
                    }
                    let _ = writeln!(
                        out,
                        "      <note><rest measure=\"yes\"/><duration>{measure_ticks}</duration><voice>{}</voice></note>",
                        layer + 1
                    );
                    continue;
                }
                let mut cursor = 0;
                let tuplet_positions = tuplet_boundaries(&layer_notes);
                for (note_index, note) in layer_notes.into_iter().enumerate() {
                    let tuplet_bounds = tuplet_positions[note_index];
                    if note.start > cursor {
                        write_gap(&mut out, note.start - cursor, layer);
                    }
                    let start = mi as u32 * measure_ticks + note.start;
                    let anchor = crate::document::Anchor {
                        voice: note.voice,
                        part: pi,
                        measure: mi,
                        tick: note.start,
                        layer: note.layer,
                    };
                    out.push_str(grace.before(&anchor));
                    let marks = notation
                        .get(&anchor)
                        .map(|(s, l)| (s.as_str(), l.as_str()))
                        .unwrap_or(("", ""));
                    if note.pitches.is_empty() {
                        write_xml_note(
                            &mut out,
                            note,
                            None,
                            false,
                            false,
                            false,
                            tuplet_bounds,
                            ("", ""),
                            style_index.get(&anchor),
                            None,
                            false,
                        );
                    } else {
                        let dynamic = crate::model::dynamic_name(note.velocity);
                        if !score.notation.expressions.iter().any(|mark| {
                            mark.anchor.part == pi
                                && mark.kind == crate::expressions::ExpressionKind::Dynamic
                        }) && note.velocity > 0
                            && previous_dynamic[usize::from(layer % 4)] != Some(dynamic)
                        {
                            let _ = writeln!(
                                out,
                                "      <direction placement=\"below\"><direction-type><dynamics><{dynamic}/></dynamics><other-direction print-object=\"no\">encore-derived-dynamic</other-direction></direction-type><voice>{}</voice></direction>",
                                layer + 1
                            );
                            previous_dynamic[usize::from(layer % 4)] = Some(dynamic);
                        }
                        for (chord_index, pitch) in note.pitches.iter().enumerate() {
                            let ornament = ornaments.notations_for_pitch(
                                &anchor,
                                Some(pitch.midi()),
                                chord_index > 0,
                            );
                            let notation_marks = if chord_index == 0 {
                                marks.0.to_owned() + &ornament + &tremolos.notations(&anchor)
                            } else {
                                ornament
                            };
                            write_xml_note(
                                &mut out,
                                note,
                                Some(*pitch),
                                chord_index > 0,
                                timeline.tie_in(
                                    &style_index,
                                    score,
                                    pi,
                                    start,
                                    note.lane(),
                                    pitch.midi(),
                                ),
                                timeline.tie_out(
                                    &style_index,
                                    score,
                                    pi,
                                    start,
                                    note,
                                    pitch.midi(),
                                ),
                                if chord_index == 0 {
                                    tuplet_bounds
                                } else {
                                    (false, false)
                                },
                                (&notation_marks, if chord_index == 0 { marks.1 } else { "" }),
                                style_index.get(&anchor),
                                part.percussion
                                    .as_ref()
                                    .and_then(|staff| staff.sound(pitch.midi()))
                                    .map(|sound| (pi, sound)),
                                tremolos.at(&anchor).is_some_and(|(m, _)| m.end.is_some()),
                            );
                        }
                    }
                    out.push_str(grace.after(&anchor));
                    cursor = note.start + note.ticks();
                }
                if cursor < measure_ticks {
                    write_gap(&mut out, measure_ticks - cursor, layer);
                }
            }
            out.push_str(&crate::repeats::musicxml_navigation_after_measure(
                score, mi,
            ));
            out.push_str(&crate::repeats::musicxml_after_measure(score, mi));
            if mi + 1 == part.measures.len() && !score.notation.repeats.iter().any(|r| r.end == mi)
            {
                out.push_str("      <barline location=\"right\"><bar-style>light-heavy</bar-style></barline>\n");
            }
            out.push_str("    </measure>\n");
        }
        out.push_str("  </part>\n");
    }
    out.push_str("</score-partwise>\n");
    out
}

/// MusicXML uses matched numbered start/stop marks. Reuse free numbers as
/// phrases end; distinct lyric names keep verse/chorus/section lines separate.
fn xml_annotations(
    score: &Score,
    grace: &crate::grace::MusicXmlIndex,
    lines: &crate::lines::MusicXmlIndex,
) -> std::collections::BTreeMap<crate::document::Anchor, (String, String)> {
    let mut result: std::collections::BTreeMap<crate::document::Anchor, (String, String)> =
        Default::default();
    if let Ok(numbering) = crate::slurs::MusicXmlNumbering::new(score) {
        for (index, slur) in score.notation.slurs.iter().enumerate() {
            let number = numbering.number(index);
            result
                .entry(slur.start)
                .or_default()
                .0
                .push_str(&crate::slurs::musicxml_start(slur, number));
            result
                .entry(slur.end)
                .or_default()
                .0
                .insert_str(0, &crate::slurs::musicxml_stop(slur, number));
        }
    }
    for lyric in &score.notation.lyrics {
        let output = &mut result.entry(lyric.anchor).or_default().1;
        let _ = write!(
            output,
            "<lyric number=\"{}\" name=\"{}\" relative-y=\"{}\"><syllabic>{}</syllabic>",
            lyric.verse,
            lyric.kind.name().to_ascii_lowercase(),
            -lyric.baseline,
            lyric.syllabic.musicxml()
        );
        for (index, piece) in lyric.text.split('‿').enumerate() {
            if index > 0 {
                output.push_str("<elision>‿</elision>");
            }
            let _ = write!(output, "<text>{}</text>", xml(piece));
        }
        if lyric.extend_to.is_some() {
            output.push_str("<extend type=\"start\"/>");
        }
        output.push_str("</lyric>");
        if let Some(end) = lyric.extend_to {
            let _ = write!(
                result.entry(end).or_default().1,
                "<lyric number=\"{}\" name=\"{}\"><extend type=\"stop\"/></lyric>",
                lyric.verse,
                lyric.kind.name().to_ascii_lowercase()
            );
        }
    }
    for (part, p) in score.parts.iter().enumerate() {
        for (measure, m) in p.measures.iter().enumerate() {
            for note in &m.notes {
                let anchor = crate::document::Anchor {
                    voice: note.voice,
                    part,
                    measure,
                    tick: note.start,
                    layer: note.layer,
                };
                let extra = lines.notations(&anchor).to_owned() + grace.main_notations(&anchor);
                if !extra.is_empty() {
                    result.entry(anchor).or_default().0.push_str(&extra);
                }
            }
        }
    }
    result
}

fn tuplet_boundaries(notes: &[&Note]) -> Vec<(bool, bool)> {
    let mut boundaries = vec![(false, false); notes.len()];
    let mut first = 0;
    while first < notes.len() {
        let Some(ratio) = notes[first].tuplet else {
            first += 1;
            continue;
        };
        let mut end = first + 1;
        while end < notes.len()
            && end - first < usize::from(ratio.actual.max(1))
            && notes[end].tuplet == Some(ratio)
            && notes[end - 1].start + notes[end - 1].ticks() == notes[end].start
        {
            end += 1;
        }
        boundaries[first].0 = true;
        boundaries[end - 1].1 = true;
        first = end;
    }
    boundaries
}

fn write_gap(out: &mut String, mut ticks: u32, layer: u8) {
    // Secondary voices contain only their branch entries. Forward cursor motion
    // preserves implicit gaps without inventing visible secondary rests.
    if layer >= 4 {
        let _ = writeln!(
            out,
            "      <forward><duration>{ticks}</duration><voice>{}</voice></forward>",
            layer + 1
        );
        return;
    }
    for duration in Duration::ALL {
        for dotted in [true, false] {
            if dotted && !duration.ticks().is_multiple_of(2) {
                continue;
            }
            let size = duration.ticks() * if dotted { 3 } else { 2 } / 2;
            while ticks >= size {
                let note = Note {
                    voice: layer / 4,
                    voice_launch: None,
                    articulations: Vec::new(),
                    start: 0,
                    duration: *duration,
                    dotted,
                    tuplet: None,
                    layer: layer % 4,
                    pitches: Vec::new(),
                    velocity: 64,
                    staccato: false,
                    tied: false,
                };
                write_xml_note(
                    out,
                    &note,
                    None,
                    false,
                    false,
                    false,
                    (false, false),
                    ("", ""),
                    None,
                    None,
                    false,
                );
                ticks -= size;
            }
        }
    }
    // Unquantized imported/edit positions remain exact even without a conventional glyph.
    if ticks > 0 {
        let _ = writeln!(
            out,
            "      <note><rest/><duration>{ticks}</duration><voice>{}</voice></note>",
            layer + 1
        );
    }
}
#[allow(clippy::too_many_arguments)]
fn write_xml_note(
    out: &mut String,
    note: &Note,
    pitch: Option<Pitch>,
    chord: bool,
    tie_in: bool,
    tie_out: bool,
    tuplet_bounds: (bool, bool),
    annotations: (&str, &str),
    style: Option<&crate::entry_style::EntryStyle>,
    drum: Option<(usize, &crate::percussion::Sound)>,
    paired_tremolo: bool,
) {
    let attributes = crate::entry_style::xml_attributes(style);
    if pitch.is_some() {
        let _ = write!(
            out,
            "      <note dynamics=\"{:.3}\"{}>",
            f32::from(note.velocity) * 100.0 / 90.0,
            attributes
        );
    } else {
        let _ = write!(out, "      <note{attributes}>");
    }

    if chord {
        out.push_str("<chord/>");
    }
    if let Some((_, sound)) = drum {
        out.push_str(&percussion::pitch(sound));
    } else if let Some(pitch) = pitch {
        let _ = write!(
            out,
            "<pitch><step>{}</step><alter>{}</alter><octave>{}</octave></pitch>",
            pitch.step.name(),
            pitch.alter,
            pitch.octave
        );
    } else {
        out.push_str("<rest/>");
    }
    let _ = write!(out, "<duration>{}</duration>", note.ticks());
    if tie_in {
        out.push_str("<tie type=\"stop\"/>");
    }
    if tie_out {
        out.push_str("<tie type=\"start\"/>");
    }
    if let Some((part, sound)) = drum {
        out.push_str(&percussion::instrument(part, sound.id));
    }
    let _ = write!(
        out,
        "<voice>{}</voice><type>{}</type>",
        note.lane() + 1,
        if paired_tremolo {
            match note.duration {
                Duration::Quarter => Duration::Half,
                Duration::Half => Duration::Whole,
                d => d,
            }
        } else {
            note.duration
        }
        .musicxml_type()
    );
    if note.dotted {
        out.push_str("<dot/>");
    }
    if drum.is_none() {
        out.push_str(&crate::entry_style::xml_accidental(
            style,
            Some(note),
            pitch.map(|pitch| pitch.midi()),
        ));
    }
    if paired_tremolo {
        out.push_str("<time-modification><actual-notes>2</actual-notes><normal-notes>1</normal-notes></time-modification>");
    } else if let Some(tuplet) = note.tuplet {
        let _ = write!(
            out,
            "<time-modification><actual-notes>{}</actual-notes><normal-notes>{}</normal-notes></time-modification>",
            tuplet.actual, tuplet.normal
        );
    }
    let mut display_style = style.cloned();
    if let Some((_, sound)) = drum {
        let style = display_style.get_or_insert_with(|| {
            crate::entry_style::EntryStyle::new(crate::document::Anchor {
                part: 0,
                measure: 0,
                tick: 0,
                layer: note.layer,
                voice: note.voice,
            })
        });
        if style.notehead == crate::entry_style::Notehead::Normal {
            style.notehead = sound.notehead;
        }
    }
    out.push_str(&crate::entry_style::xml_elements(
        display_style.as_ref(),
        Some(note),
        pitch.map(|pitch| pitch.midi()),
        chord,
    ));
    let style_notations = crate::entry_style::xml_notations(style, chord);
    // Pitch alter is authoritative; accidental display is left to the importer.
    let has_articulations =
        pitch.is_some() && (note.staccato || (!chord && !note.articulations.is_empty()));
    if tie_in
        || tie_out
        || has_articulations
        || tuplet_bounds.0
        || tuplet_bounds.1
        || !annotations.0.is_empty()
        || !style_notations.is_empty()
        || (!chord && note.voice_launch.is_some())
    {
        out.push_str("<notations>");
        let orientation = style.map_or("", |style| match style.tie_direction {
            crate::slurs::SlurDirection::Above => " orientation=\"over\"",
            crate::slurs::SlurDirection::Below => " orientation=\"under\"",
            _ => "",
        });
        if tie_in {
            let _ = write!(out, "<tied type=\"stop\"{orientation}/>");
        }
        if tie_out {
            let _ = write!(out, "<tied type=\"start\"{orientation}/>");
        }
        if has_articulations {
            out.push_str("<articulations>");
            if note.staccato {
                out.push_str("<staccato/>");
            }
            if !chord {
                for articulation in &note.articulations {
                    let _ = write!(out, "<{}/>", articulation.musicxml_name());
                }
            }
            out.push_str("</articulations>");
        }
        if tuplet_bounds.0 {
            out.push_str("<tuplet type=\"start\" bracket=\"yes\"/>");
        }
        if tuplet_bounds.1 {
            out.push_str("<tuplet type=\"stop\"/>");
        }
        out.push_str(&style_notations);
        if !chord && let Some(launch) = note.voice_launch {
            let _ = write!(
                out,
                "<other-notation type=\"single\" print-object=\"no\">encore-voice-launch:{launch}</other-notation>"
            );
        }
        out.push_str(annotations.0);
        out.push_str("</notations>");
    }
    out.push_str(annotations.1);
    out.push_str("</note>\n");
}

#[derive(Debug)]
struct Event {
    tick: u32,
    priority: u8,
    bytes: Vec<u8>,
}
fn meta(kind: u8, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0xff, kind];
    variable_length(bytes.len() as u32, &mut out);
    out.extend_from_slice(bytes);
    out
}
fn variable_length(mut value: u32, out: &mut Vec<u8>) {
    let mut buffer = [0u8; 5];
    let mut i = 4;
    buffer[i] = (value & 0x7f) as u8;
    value >>= 7;
    while value > 0 {
        i -= 1;
        buffer[i] = (value & 0x7f) as u8 | 0x80;
        value >>= 7;
    }
    out.extend_from_slice(&buffer[i..]);
}
fn track(mut events: Vec<Event>, end: u32) -> Vec<u8> {
    events.sort_by_key(|event| (event.tick, event.priority));
    let mut data = Vec::new();
    let mut previous = 0;
    for event in events {
        while event.tick.saturating_sub(previous) > 0x0fff_ffff {
            variable_length(0x0fff_ffff, &mut data);
            data.extend_from_slice(&[0xff, 0x7f, 0]);
            previous += 0x0fff_ffff;
        }
        variable_length(event.tick.saturating_sub(previous), &mut data);
        data.extend_from_slice(&event.bytes);
        previous = event.tick;
    }
    while end.saturating_sub(previous) > 0x0fff_ffff {
        variable_length(0x0fff_ffff, &mut data);
        data.extend_from_slice(&[0xff, 0x7f, 0]);
        previous += 0x0fff_ffff;
    }
    variable_length(end.saturating_sub(previous), &mut data);
    data.extend_from_slice(&[0xff, 0x2f, 0]);
    let mut out = b"MTrk".to_vec();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend(data);
    out
}

/// Standard MIDI format 1, 480 ticks/quarter. Each staff has its own track;
/// ports extend beyond the 15 melodic channels without using percussion channel 10.
pub fn midi(score: &Score) -> Result<Vec<u8>, String> {
    score.validate()?;
    let decorated = crate::performance::needed(score)
        .then(|| crate::performance::Timeline::new(score))
        .transpose()?;
    let expression_map = crate::expressions::ExpressionMap::new(score);
    let style_index = crate::entry_style::Index::new(score);
    let measure_ticks = score.measure_ticks();
    let order = crate::repeats::playback_order(score)?;
    let end = measure_ticks * order.len() as u32;
    let tempo = 60_000_000u32 / u32::from(score.tempo);
    let denominator_power = score.time_denominator.trailing_zeros() as u8;
    let mut conductor = vec![
        Event {
            tick: 0,
            priority: 0,
            bytes: meta(0x03, score.title.as_bytes()),
        },
        Event {
            tick: 0,
            priority: 0,
            bytes: meta(0x01, score.composer.as_bytes()),
        },
        Event {
            tick: 0,
            priority: 0,
            bytes: meta(0x51, &tempo.to_be_bytes()[1..]),
        },
        Event {
            tick: 0,
            priority: 0,
            bytes: meta(0x58, &[score.time_numerator, denominator_power, 24, 8]),
        },
        Event {
            tick: 0,
            priority: 0,
            bytes: meta(0x59, &[score.key_fifths as u8, 0]),
        },
    ];
    if !score.notation.tempo.is_empty() {
        conductor.retain(|event| !event.bytes.starts_with(&[0xff, 0x51]));
        {
            let map = crate::expressions::TempoMap::new(score)?;
            for (tick, tempo) in map.midi_tempos() {
                conductor.push(Event {
                    tick,
                    priority: 0,
                    bytes: meta(0x51, &tempo.to_be_bytes()[1..]),
                });
            }
        }
    }
    let mut tracks = vec![track(conductor, end)];
    let has_solo = score.parts.iter().any(|part| part.solo && !part.muted);
    let mut lyric_index: HashMap<crate::document::Anchor, Vec<&crate::lyrics::Lyric>> =
        HashMap::new();
    for lyric in &score.notation.lyrics {
        lyric_index.entry(lyric.anchor).or_default().push(lyric);
    }
    for lyrics in lyric_index.values_mut() {
        lyrics.sort_by_key(|lyric| (lyric.kind, lyric.verse));
    }
    for (part_index, part) in score.parts.iter().enumerate() {
        if let Some(staff) = &part.percussion {
            let channel = 9_u8;
            let port = (part_index * 2) as u8;
            let mut events = vec![
                Event {
                    tick: 0,
                    priority: 0,
                    bytes: meta(0x03, part.name.as_bytes()),
                },
                Event {
                    tick: 0,
                    priority: 0,
                    bytes: meta(0x21, &[port]),
                },
                Event {
                    tick: 0,
                    priority: 1,
                    bytes: vec![0xc0 | channel, staff.kit],
                },
                Event {
                    tick: 0,
                    priority: 1,
                    bytes: vec![
                        0xb0 | channel,
                        7,
                        (part.volume * 127.).round().clamp(0., 127.) as u8,
                    ],
                },
                Event {
                    tick: 0,
                    priority: 1,
                    bytes: vec![0xb0 | channel, 10, ((part.pan + 1.) * 63.5).round() as u8],
                },
            ];
            if !part.muted && (!has_solo || part.solo) && part.volume > 0. {
                let timeline = decorated
                    .as_ref()
                    .ok_or("Percussion requires the shared performance timeline.")?;
                for event in crate::performance::drum_events(timeline, part_index, |tick| {
                    tick.round() as u64
                }) {
                    let tick = u32::try_from(event.tick)
                        .map_err(|_| "Percussion MIDI tick exceeds the output range.")?;
                    events.push(Event {
                        tick,
                        priority: if event.velocity.is_some() { 3 } else { 0 },
                        bytes: vec![
                            if event.velocity.is_some() {
                                0x90 | channel
                            } else {
                                0x80 | channel
                            },
                            event.key,
                            event.velocity.unwrap_or(0),
                        ],
                    });
                }
                for event in crate::repeats::events(part, measure_ticks, &order) {
                    let anchor = crate::document::Anchor {
                        part: part_index,
                        measure: event.measure,
                        tick: event.note.start,
                        layer: event.note.layer,
                        voice: event.note.voice,
                    };
                    if let Some(lyrics) = lyric_index.get(&anchor) {
                        for lyric in lyrics {
                            let mut text = lyric.text.clone();
                            if matches!(
                                lyric.syllabic,
                                crate::lyrics::Syllabic::Begin | crate::lyrics::Syllabic::Middle
                            ) {
                                text.push('-');
                            }
                            events.push(Event {
                                tick: event.tick as u32,
                                priority: 0,
                                bytes: meta(0x05, text.as_bytes()),
                            });
                        }
                    }
                }
            }
            tracks.push(track(events, end));
            continue;
        }
        let layers = (0..8)
            .filter(|layer| {
                part.measures
                    .iter()
                    .flat_map(|measure| &measure.notes)
                    .any(|note| note.lane() == *layer)
            })
            .collect::<Vec<_>>();
        let mut layers = if layers.is_empty() { vec![0] } else { layers };
        if let Some(timeline) = &decorated {
            for lane in 8..16 {
                if timeline
                    .notes
                    .iter()
                    .any(|n| n.anchor.part == part_index && n.layer == lane)
                {
                    layers.push(lane);
                }
            }
        }
        for layer in layers {
            let channel = channel(usize::from(layer));
            let port = (part_index * 2 + usize::from(layer >= 15)) as u8;
            let mut events = vec![
                Event {
                    tick: 0,
                    priority: 0,
                    bytes: meta(0x03, part.name.as_bytes()),
                },
                Event {
                    tick: 0,
                    priority: 0,
                    bytes: meta(0x21, &[port]),
                },
                Event {
                    tick: 0,
                    priority: 1,
                    bytes: vec![0xc0 | channel, part.instrument.gm_program()],
                },
                Event {
                    tick: 0,
                    priority: 1,
                    bytes: vec![
                        0xb0 | channel,
                        7,
                        (part.volume * 127.0).round().clamp(0.0, 127.0) as u8,
                    ],
                },
                Event {
                    tick: 0,
                    priority: 1,
                    bytes: vec![0xb0 | channel, 10, ((part.pan + 1.0) * 63.5).round() as u8],
                },
            ];
            if !part.muted && (!has_solo || part.solo) {
                if let Some(timeline) = &decorated {
                    let mut program = Some(part.instrument);
                    for note in timeline
                        .notes
                        .iter()
                        .filter(|n| n.anchor.part == part_index && n.layer == layer)
                    {
                        let start = note.start.round() as u32;
                        if program != Some(note.instrument) {
                            events.push(Event {
                                tick: start,
                                priority: 1,
                                bytes: vec![0xc0 | channel, note.instrument.gm_program()],
                            });
                            program = Some(note.instrument);
                        }
                        events.push(Event {
                            tick: start,
                            priority: 3,
                            bytes: vec![0x90 | channel, note.midi, note.velocity],
                        });
                        events.push(Event {
                            tick: (note.end.round() as u32).max(start + 1),
                            priority: 0,
                            bytes: vec![0x80 | channel, note.midi, 0],
                        });
                    }
                    for c in timeline
                        .controls
                        .iter()
                        .filter(|c| c.part == part_index && c.layer == layer)
                    {
                        events.push(Event {
                            tick: c.tick.round() as u32,
                            priority: c.priority,
                            bytes: vec![0xb0 | channel, c.controller, c.value],
                        });
                    }
                    for event in crate::repeats::events(part, measure_ticks, &order)
                        .into_iter()
                        .filter(|e| e.note.lane() == layer)
                    {
                        let anchor = crate::document::Anchor {
                            voice: event.note.voice,
                            part: part_index,
                            measure: event.measure,
                            tick: event.note.start,
                            layer: event.note.layer,
                        };
                        if let Some(lyrics) = lyric_index.get(&anchor) {
                            for lyric in lyrics {
                                let mut text = lyric.text.clone();
                                if matches!(
                                    lyric.syllabic,
                                    crate::lyrics::Syllabic::Begin
                                        | crate::lyrics::Syllabic::Middle
                                ) {
                                    text.push('-');
                                }
                                events.push(Event {
                                    tick: event.tick as u32,
                                    priority: 0,
                                    bytes: meta(0x05, text.as_bytes()),
                                });
                            }
                        }
                    }
                } else {
                    let timeline = crate::repeats::events(part, measure_ticks, &order);
                    let starts: HashMap<_, _> = timeline
                        .iter()
                        .enumerate()
                        .map(|(index, event)| ((event.tick, event.note.lane()), index))
                        .collect();
                    let ends: HashMap<_, _> = timeline
                        .iter()
                        .enumerate()
                        .map(|(index, event)| {
                            (
                                (
                                    event.tick + u64::from(event.note.ticks()),
                                    event.note.lane(),
                                ),
                                index,
                            )
                        })
                        .collect();
                    for event in timeline.iter().filter(|event| event.note.lane() == layer) {
                        let note = event.note;
                        let start = event.tick as u32;
                        let anchor = crate::document::Anchor {
                            voice: note.voice,
                            part: part_index,
                            measure: event.measure,
                            tick: note.start,
                            layer: note.layer,
                        };
                        if let Some(lyrics) = lyric_index.get(&anchor) {
                            for lyric in lyrics {
                                let mut text = lyric.text.clone();
                                if matches!(
                                    lyric.syllabic,
                                    crate::lyrics::Syllabic::Begin
                                        | crate::lyrics::Syllabic::Middle
                                ) {
                                    text.push('-');
                                }
                                events.push(Event {
                                    tick: start,
                                    priority: 0,
                                    bytes: meta(0x05, text.as_bytes()),
                                });
                            }
                        }
                        let previous = ends
                            .get(&(event.tick, layer))
                            .map(|index| &timeline[*index]);
                        let following = starts
                            .get(&(event.tick + u64::from(note.ticks()), layer))
                            .map(|index| &timeline[*index]);
                        for pitch in &note.pitches {
                            let midi = pitch.midi();
                            let tie_in = previous.is_some_and(|previous| {
                                style_index.ties_forward(
                                    &crate::document::Anchor {
                                        voice: previous.note.voice,
                                        part: part_index,
                                        measure: previous.measure,
                                        tick: previous.note.start,
                                        layer: previous.note.layer,
                                    },
                                    previous.note,
                                    midi,
                                ) && crate::repeats::written_adjacent(
                                    previous,
                                    event,
                                    measure_ticks,
                                ) && previous
                                    .note
                                    .pitches
                                    .iter()
                                    .any(|pitch| pitch.midi() == midi)
                            });
                            let tie_out = style_index.ties_forward(&anchor, note, midi)
                                && following.is_some_and(|next| {
                                    crate::repeats::written_adjacent(event, next, measure_ticks)
                                        && next
                                            .note
                                            .pitches
                                            .iter()
                                            .any(|pitch| pitch.midi() == midi)
                                });
                            if !tie_in {
                                events.push(Event {
                                    tick: start,
                                    priority: 3,
                                    bytes: vec![
                                        0x90 | channel,
                                        midi,
                                        expression_map.velocity(anchor, note),
                                    ],
                                });
                            }
                            if !tie_out {
                                let ticks = note.playback_ticks()
                                    + crate::repeats::legato_extension(
                                        score, part_index, event, following,
                                    );
                                events.push(Event {
                                    tick: start + ticks.max(1),
                                    priority: 2,
                                    bytes: vec![0x80 | channel, midi, 0],
                                });
                            }
                        }
                    }
                }
            }
            tracks.push(track(events, end));
        }
    }
    let mut out = b"MThd".to_vec();
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&(PPQ as u16).to_be_bytes());
    for track in tracks {
        out.extend(track);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instrument;
    fn note(start: u32, pitches: &[u8]) -> Note {
        Note {
            voice: 0,
            voice_launch: None,
            articulations: Vec::new(),
            start,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: pitches.iter().map(|p| Pitch::from_midi(*p)).collect(),
            velocity: 80,
            staccato: false,
            tied: false,
        }
    }
    #[test]
    fn xml_contains_instruments_tempo_and_escaped_metadata() {
        let mut score = Score::demo();
        score.title = "A < B & C".into();
        let out = musicxml(&score);
        assert!(out.contains("A &lt; B &amp; C"));
        assert!(out.contains("<fifths>2</fifths>"));
        assert!(out.contains("<midi-program>74</midi-program>"));
        assert!(out.contains("tempo=\"108\""));
        assert!(out.contains("<tied type=\"start\"/>"));
        assert!(out.contains("<tied type=\"stop\"/>"));
        assert!(out.contains("<staccato/>"));
    }

    #[test]
    fn articulation_musicxml_marks_once_per_chord_and_midi_performs_the_marks() {
        use crate::model::Articulation;
        let mut score = Score::blank();
        score.parts.truncate(1);
        let mut event = note(0, &[60, 64]);
        event.articulations = Articulation::ALL.to_vec();
        score.parts[0].measures[0].notes = vec![event];
        let xml = musicxml(&score);
        for marker in [
            "<accent/>",
            "<strong-accent/>",
            "<tenuto/>",
            "<staccatissimo/>",
        ] {
            assert_eq!(xml.matches(marker).count(), 1, "{marker}");
        }
        let bytes = midi(&score).unwrap();
        assert!(bytes.windows(3).any(|bytes| bytes == [0x90, 60, 98]));
        assert_eq!(
            midi_notes(&bytes),
            vec![
                (0, 0x90, 60),
                (0, 0x90, 64),
                (144, 0x80, 60),
                (144, 0x80, 64)
            ]
        );
        score.parts[0].measures[0].notes[0].articulations = vec![Articulation::Tenuto];
        let bytes = midi(&score).unwrap();
        assert!(bytes.windows(3).any(|bytes| bytes == [0x90, 60, 83]));
        assert_eq!(midi_notes(&bytes)[2].0, 480);
    }
    #[test]
    fn xml_fills_gaps_and_writes_chords() {
        let mut score = Score::blank();
        score.parts[0].measures[0].notes = vec![note(480, &[60, 64, 67])];
        let out = musicxml(&score);
        assert_eq!(out.matches("<chord/>").count(), 2);
        assert!(
            out.contains("<rest/><duration>480</duration><voice>1</voice><type>quarter</type>")
        );
        assert!(out.contains("<rest/><duration>960</duration><voice>1</voice><type>half</type>"));
    }

    #[test]
    fn xml_prints_dynamic_changes_without_repeating_each_note() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, &[60]), note(480, &[64])];
        let mut louder = note(0, &[67]);
        louder.velocity = 116;
        score.parts[0].measures[1].notes = vec![louder];
        let xml = musicxml(&score);
        assert_eq!(xml.matches("<mf/>").count(), 1);
        assert_eq!(xml.matches("<ff/>").count(), 1);
    }
    fn read_variable(bytes: &[u8], pos: &mut usize) -> u32 {
        let mut n = 0;
        loop {
            let b = bytes[*pos];
            *pos += 1;
            n = (n << 7) | u32::from(b & 0x7f);
            if b & 0x80 == 0 {
                return n;
            }
        }
    }
    fn midi_notes(bytes: &[u8]) -> Vec<(u32, u8, u8)> {
        let mut out = Vec::new();
        let mut p = 14;
        while p < bytes.len() {
            assert_eq!(&bytes[p..p + 4], b"MTrk");
            let n = u32::from_be_bytes(bytes[p + 4..p + 8].try_into().unwrap()) as usize;
            p += 8;
            let finish = p + n;
            let mut tick = 0;
            while p < finish {
                tick += read_variable(bytes, &mut p);
                let status = bytes[p];
                p += 1;
                if status == 0xff {
                    p += 1;
                    let size = read_variable(bytes, &mut p) as usize;
                    p += size;
                } else if status & 0xf0 == 0xc0 {
                    p += 1;
                } else {
                    let a = bytes[p];
                    let b = bytes[p + 1];
                    p += 2;
                    if status & 0xf0 == 0x90 || status & 0xf0 == 0x80 {
                        out.push((tick, status & 0xf0, a));
                    }
                    let _ = b;
                }
            }
        }
        out
    }
    #[test]
    fn midi_honors_gap_chord_tie_staccato_and_program() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].instrument = Instrument::Violin;
        let mut first = note(480, &[60, 64]);
        first.tied = true;
        let mut next = note(960, &[60, 64]);
        next.staccato = true;
        score.parts[0].measures[0].notes = vec![first, next];
        let bytes = midi(&score).unwrap();
        assert_eq!(&bytes[..4], b"MThd");
        assert!(bytes.windows(2).any(|pair| pair == [0xc0, 40]));
        assert_eq!(
            midi_notes(&bytes),
            vec![
                (480, 0x90, 60),
                (480, 0x90, 64),
                (1200, 0x80, 60),
                (1200, 0x80, 64)
            ]
        );
        score.parts[0].muted = true;
        assert!(midi_notes(&midi(&score).unwrap()).is_empty());
    }
    #[test]
    fn every_catalog_program_survives_midi_and_musicxml_export() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, &[60])];
        for &instrument in Instrument::ALL {
            score.parts[0].instrument = instrument;
            let program = instrument.gm_program();
            assert!(
                midi(&score)
                    .unwrap()
                    .windows(2)
                    .any(|pair| pair == [0xc0, program]),
                "{}",
                instrument.name()
            );
            assert!(
                musicxml(&score).contains(&format!(
                    "<midi-program>{}</midi-program>",
                    u16::from(program) + 1
                )),
                "{}",
                instrument.name()
            );
        }
    }

    #[test]
    fn midi_honors_cross_bar_tie_and_solo() {
        let mut score = Score::blank();
        let mut first = note(1440, &[60]);
        first.tied = true;
        score.parts[0].measures[0].notes = vec![first];
        score.parts[0].measures[1].notes = vec![note(0, &[60])];
        score.parts[1].measures[0].notes = vec![note(0, &[48])];
        score.parts[0].solo = true;
        assert_eq!(
            midi_notes(&midi(&score).unwrap()),
            vec![(1440, 0x90, 60), (2400, 0x80, 60)]
        );
    }
    #[test]
    fn midi_partial_chord_ties_do_not_retrigger_or_stick() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        let mut first = note(480, &[60, 64]);
        first.tied = true;
        score.parts[0].measures[0].notes = vec![first, note(960, &[60, 67])];
        assert_eq!(
            midi_notes(&midi(&score).unwrap()),
            vec![
                (480, 0x90, 60),
                (480, 0x90, 64),
                (960, 0x80, 64),
                (960, 0x90, 67),
                (1440, 0x80, 60),
                (1440, 0x80, 67),
            ]
        );
    }
    #[test]
    fn xml_sixteenth_type_and_velocity_follow_musicxml() {
        let mut score = Score::blank();
        let mut sixteenth = note(0, &[60]);
        sixteenth.duration = Duration::Sixteenth;
        sixteenth.velocity = 90;
        score.parts[0].measures[0].notes = vec![sixteenth];
        let out = musicxml(&score);
        assert!(out.contains("<type>16th</type>"));
        assert!(out.contains("dynamics=\"100.000\""));
        assert!(!out.contains("sixteenth"));
    }
    #[test]
    fn malformed_midi_input_is_safe() {
        let mut score = Score::blank();
        score.tempo = 0;
        assert!(midi(&score).is_err());
    }
    #[test]
    fn midi_tuplets_have_exact_note_on_and_off_times() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes = (0..3)
            .map(|index| {
                let mut value = note(index * 160, &[60 + index as u8]);
                value.duration = Duration::Eighth;
                value.tuplet = Some(crate::model::Tuplet::TRIPLET);
                value
            })
            .collect();
        assert_eq!(
            midi_notes(&midi(&score).unwrap()),
            vec![
                (0, 0x90, 60),
                (160, 0x80, 60),
                (160, 0x90, 61),
                (320, 0x80, 61),
                (320, 0x90, 62),
                (480, 0x80, 62),
            ]
        );
    }

    #[test]
    fn midi_layers_have_independent_tracks_and_channels_for_overlapping_unisons() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes = (0..4)
            .map(|layer| {
                let mut note = note(0, &[60]);
                note.layer = layer;
                note
            })
            .collect();
        let bytes = midi(&score).unwrap();
        assert_eq!(u16::from_be_bytes([bytes[10], bytes[11]]), 5);
        for layer in 0..4 {
            assert!(
                bytes
                    .windows(3)
                    .any(|event| event == [0x90 | layer, 60, 80])
            );
            assert!(bytes.windows(3).any(|event| event == [0x80 | layer, 60, 0]));
        }
    }
    #[test]
    fn midi_performs_voltas_and_does_not_join_ties_across_repeat_jumps() {
        use crate::repeats::{Ending, RepeatRegion};
        let mut score = Score::blank();
        score.parts.truncate(1);
        for (bar, measure) in score.parts[0].measures.iter_mut().enumerate() {
            let mut value = note(0, &[60 + bar as u8]);
            value.duration = Duration::Whole;
            value.tied = true;
            measure.notes = vec![value];
        }
        score.notation.repeats.push(RepeatRegion {
            start: 0,
            end: 3,
            times: 2,
            endings: vec![
                Ending {
                    start: 2,
                    end: 2,
                    passes: vec![1],
                },
                Ending {
                    start: 3,
                    end: 3,
                    passes: vec![2],
                },
            ],
        });
        let result = midi_notes(&midi(&score).unwrap());
        let ons: Vec<_> = result
            .iter()
            .filter(|(_, status, _)| *status == 0x90)
            .map(|(tick, _, pitch)| (*tick, *pitch))
            .collect();
        assert_eq!(
            ons,
            [
                (0, 60),
                (1920, 61),
                (3840, 62),
                (5760, 60),
                (7680, 61),
                (9600, 63)
            ]
        );
        // A tied repeat-end C must reattack when returning to the initial C.
        score.notation.repeats = vec![RepeatRegion::simple(0, 0)];
        let ons: Vec<_> = midi_notes(&midi(&score).unwrap())
            .into_iter()
            .filter(|(_, status, _)| *status == 0x90)
            .collect();
        assert_eq!(&ons[..2], &[(0, 0x90, 60), (1920, 0x90, 60)]);
    }

    #[test]
    fn midi_slur_overlap_respects_detached_marks_and_unison_reattacks() {
        use crate::{document::Anchor, slurs::Slur};
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, &[60]), note(480, &[62])];
        let start = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor { tick: 480, ..start };
        score.notation.slurs.push(Slur::new(start, end));
        assert!(midi_notes(&midi(&score).unwrap()).contains(&(495, 0x80, 60)));
        score.parts[0].measures[0].notes[0].staccato = true;
        assert!(midi_notes(&midi(&score).unwrap()).contains(&(240, 0x80, 60)));
        score.parts[0].measures[0].notes[0].staccato = false;
        score.parts[0].measures[0].notes[1].pitches = vec![Pitch::from_midi(60)];
        let events = midi_notes(&midi(&score).unwrap());
        assert_eq!(events[1], (480, 0x80, 60));
        assert_eq!(events[2], (480, 0x90, 60));
    }

    #[test]
    fn long_repeat_midi_uses_legal_four_byte_delta_times() {
        let bytes = track(Vec::new(), 0x3000_0000);
        let mut index = 8;
        let mut tick = 0;
        while index < bytes.len() {
            let begin = index;
            tick += read_variable(&bytes, &mut index);
            assert!(index - begin <= 4);
            assert_eq!(bytes[index], 0xff);
            let kind = bytes[index + 1];
            index += 2;
            let length = read_variable(&bytes, &mut index) as usize;
            index += length;
            if kind == 0x2f {
                break;
            }
        }
        assert_eq!(tick, 0x3000_0000);
    }

    #[test]
    fn midi_repeats_utf8_lyric_meta_at_every_performed_source_onset() {
        use crate::{
            document::Anchor,
            lyrics::{Lyric, Syllabic},
            repeats::RepeatRegion,
        };
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, &[60])];
        let mut lyric = Lyric::new(
            Anchor {
                voice: 0,
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
            },
            "Gló",
        );
        lyric.syllabic = Syllabic::Begin;
        score.notation.lyrics.push(lyric);
        score.notation.repeats.push(RepeatRegion::simple(0, 0));
        let expected = [0xff, 0x05, 5, b'G', b'l', 0xc3, 0xb3, b'-'];
        assert_eq!(
            midi(&score)
                .unwrap()
                .windows(expected.len())
                .filter(|window| *window == expected)
                .count(),
            2
        );
    }
    #[test]
    fn midi_da_capo_al_fine_uses_the_shared_performed_route() {
        use crate::repeats::{NavigationKind, NavigationMark};
        let mut score = Score::blank();
        score.tempo = 120;
        score.tempo = 120;
        score.parts.truncate(1);
        for (bar, measure) in score.parts[0].measures.iter_mut().enumerate() {
            measure.notes = vec![note(0, &[60 + bar as u8])];
        }
        score.notation.navigation = vec![
            NavigationMark {
                measure: 1,
                kind: NavigationKind::Fine,
                play_repeats_after_jump: false,
            },
            NavigationMark {
                measure: 3,
                kind: NavigationKind::DaCapoAlFine,
                play_repeats_after_jump: false,
            },
        ];
        let ons: Vec<_> = midi_notes(&midi(&score).unwrap())
            .into_iter()
            .filter(|(_, status, _)| *status == 0x90)
            .map(|(tick, _, pitch)| (tick, pitch))
            .collect();
        assert_eq!(
            ons,
            [
                (0, 60),
                (1920, 61),
                (3840, 62),
                (5760, 63),
                (7680, 60),
                (9600, 61)
            ]
        );
        assert_eq!(crate::synth::duration_seconds(&score), 12.0);
    }
}

#[cfg(test)]
mod solo_regression {
    use super::*;
    #[test]
    fn a_muted_solo_does_not_silence_unmuted_midi_staves() {
        let mut score = Score::blank();
        score.parts[0].solo = true;
        score.parts[0].muted = true;
        if score.parts.len() < 2 {
            let mut part = score.parts[0].clone();
            part.solo = false;
            part.muted = false;
            score.parts.push(part);
        }
        score.parts[1].solo = false;
        score.parts[1].muted = false;
        score.parts[1].measures[0].notes = vec![Note {
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        }];
        let bytes = midi(&score).unwrap();
        assert!(bytes.windows(3).any(|b| b == [0x90, 60, 80]));
        assert!(bytes.windows(4).any(|b| b == [0xff, 0x21, 1, 2]));
    }
}

#[cfg(test)]
mod voice_two_tests {
    use super::*;
    fn score() -> Score {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        let note = Note {
            layer: 0,
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: Duration::Whole,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        };
        let mut second = note.clone();
        second.voice = 1;
        second.voice_launch = Some(0);
        second.duration = Duration::Quarter;
        second.tied = true;
        let mut tail = second.clone();
        tail.start = 480;
        tail.tied = false;
        score.parts[0].measures[0].notes = vec![note, second, tail];
        score
    }
    fn vlq(bytes: &[u8], at: &mut usize) -> u32 {
        let mut n = 0;
        loop {
            let b = bytes[*at];
            *at += 1;
            n = (n << 7) | u32::from(b & 127);
            if b & 128 == 0 {
                return n;
            }
        }
    }
    fn events(bytes: &[u8]) -> Vec<(u32, u8, u8, u8)> {
        let mut all = Vec::new();
        let mut at = 14;
        while at < bytes.len() {
            assert_eq!(&bytes[at..at + 4], b"MTrk");
            let length = u32::from_be_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            at += 8;
            let end = at + length;
            let mut tick = 0;
            while at < end {
                tick += vlq(bytes, &mut at);
                let status = bytes[at];
                at += 1;
                if status == 255 {
                    at += 1;
                    let count = vlq(bytes, &mut at) as usize;
                    at += count;
                } else {
                    let one = status & 240 == 192 || status & 240 == 208;
                    let key = bytes[at];
                    let value = if one { 0 } else { bytes[at + 1] };
                    if status & 240 == 128 || status & 240 == 144 {
                        all.push((tick, status, key, value));
                    }
                    at += if one { 1 } else { 2 };
                }
            }
        }
        all
    }
    #[test]
    fn midi_keeps_same_pitch_voices_on_independent_lanes_and_ties() {
        let score = score();
        score.validate().unwrap();
        let events = events(&midi(&score).unwrap());
        assert!(events.contains(&(0, 0x90, 60, 80)));
        assert!(events.contains(&(1920, 0x80, 60, 0)));
        assert!(events.contains(&(0, 0x94, 60, 80)));
        assert!(events.contains(&(960, 0x84, 60, 0)));
        assert_eq!(events.len(), 4);
    }
    #[test]
    fn musicxml_writes_independent_voice_passes_and_launch_metadata() {
        let score = score();
        let xml = musicxml(&score);
        assert_eq!(
            xml.matches("<backup><duration>1920</duration></backup>")
                .count(),
            1
        );
        assert!(xml.contains("<voice>1</voice><type>whole</type>"));
        assert!(xml.contains("<voice>5</voice><type>quarter</type>"));
        assert_eq!(xml.matches("encore-voice-launch:0").count(), 2);
        assert!(xml.contains("<forward><duration>960</duration><voice>5</voice></forward>"));
        let document = roxmltree::Document::parse_with_options(
            &xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let voice_five: Vec<_> = document
            .descendants()
            .filter(|n| {
                n.has_tag_name("note")
                    && n.children()
                        .find(|n| n.has_tag_name("voice"))
                        .and_then(|n| n.text())
                        == Some("5")
            })
            .collect();
        assert_eq!(voice_five.len(), 2);
        assert!(
            voice_five[0]
                .descendants()
                .any(|n| n.has_tag_name("tie") && n.attribute("type") == Some("start"))
        );
        assert!(
            voice_five[1]
                .descendants()
                .any(|n| n.has_tag_name("tie") && n.attribute("type") == Some("stop"))
        );
    }
}

#[cfg(test)]
mod percussion_tests {
    use super::*;
    use crate::performance::percussion_tests::{fixture, note};
    #[test]
    fn drum_xml_uses_unpitched_ids_default_heads_and_one_based_midi_keys() {
        let mut s = fixture();
        s.parts[0].measures[0].notes = vec![
            note(0, Duration::Quarter, 1, 0, 0),
            note(480, Duration::Quarter, 3, 1, 0),
        ];
        let x = musicxml(&s);
        let d = roxmltree::Document::parse_with_options(
            &x,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            d.descendants().filter(|n| n.has_tag_name("pitch")).count(),
            0
        );
        assert_eq!(
            d.descendants()
                .filter(|n| n.has_tag_name("unpitched"))
                .count(),
            2
        );
        assert!(x.contains("<midi-unpitched>47</midi-unpitched>"));
        assert!(x.contains("<instrument id=\"P1-S1\"/>"));
        assert!(x.contains(">x</notehead>"));
        assert!(x.contains("<sign>percussion</sign>"));
        assert!(x.contains("<staff-lines>5</staff-lines>"));
        assert!(!x.contains("<accidental>"));
        assert!(x.contains("encore-percussion:"));
    }
    fn ports_and_channels(bytes: &[u8]) -> Vec<(u8, u8)> {
        let mut offset = 14;
        let mut output = vec![];
        while offset < bytes.len() {
            let len =
                u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let data = &bytes[offset + 8..offset + 8 + len];
            if let Some(port) = data
                .windows(4)
                .find(|b| b[..3] == [0xff, 0x21, 1])
                .map(|b| b[3])
            {
                let channel = data
                    .windows(3)
                    .find(|b| b[0] & 0xf0 == 0x90)
                    .map(|b| b[0] & 0xf)
                    .unwrap_or(255);
                output.push((port, channel));
            }
            offset += 8 + len;
        }
        output
    }
    #[test]
    fn midi_drums_merge_voices_on_channel_ten_and_staff_ports_never_collide() {
        let mut s = fixture();
        s.parts[0].measures[0]
            .notes
            .push(note(240, Duration::Quarter, 2, 1, 0));
        let melodic = Score::blank().parts[0].clone();
        s.parts.push(melodic);
        s.parts[1].measures.truncate(1);
        s.parts[1].measures[0].notes = vec![note(0, Duration::Quarter, 60, 0, 0)];
        let b = midi(&s).unwrap();
        assert_eq!(ports_and_channels(&b), vec![(0, 9), (2, 0)]);
        assert!(b.windows(3).any(|b| b == [0x99, 38, 80]));
        assert!(b.windows(3).any(|b| b == [0x99, 42, 80]));
        let part = s.parts[0].clone();
        s.parts = vec![part; 64];
        let b = midi(&s).unwrap();
        let ports = ports_and_channels(&b);
        assert_eq!(ports.len(), 64);
        assert_eq!(ports.last(), Some(&(126, 9)));
        let x = musicxml(&s);
        let d = roxmltree::Document::parse_with_options(
            &x,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            d.descendants()
                .filter(|n| n.has_tag_name("midi-device"))
                .filter_map(|n| n.attribute("port"))
                .all(|p| p.parse::<u8>().unwrap() <= 16)
        );
    }
    #[test]
    fn sixty_fourth_melodic_staff_with_all_sixteen_lanes_uses_last_valid_port() {
        let mut s = fixture();
        let drum = s.parts[0].clone();
        s.parts = vec![drum; 64];
        let mut last = Score::blank().parts[0].clone();
        last.measures.truncate(1);
        last.measures[0].notes = (0..8)
            .map(|lane| note(0, Duration::Quarter, 60, lane / 4, lane % 4))
            .collect();
        s.parts[63] = last;
        let at = crate::document::Anchor {
            part: 63,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        for slot in 0..8 {
            let mut chord = crate::chords::ChordSymbol::new(at, crate::chords::parse("C").unwrap());
            chord.slot = slot;
            chord.play = true;
            s.notation.chords.push(chord);
        }
        let ports = ports_and_channels(&midi(&s).unwrap());
        assert_eq!(ports.len(), 79);
        assert_eq!(ports.last(), Some(&(127, 0)));
        let unique: std::collections::BTreeSet<_> = ports.iter().copied().collect();
        assert_eq!(unique.len(), ports.len());
        assert!(ports.iter().all(|(port, _)| *port <= 127));
    }
}
