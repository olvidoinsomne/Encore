//! Bounded shared performance decoration for grace notes, ornaments and pedals.
//! Written rhythm and concert pitch remain untouched. Empty metadata keeps the
//! legacy schedulers, preserving existing audio renders exactly.
use crate::{
    document::Anchor,
    expressions::{ExpressionKind, ExpressionMap, TempoMap},
    grace::{GracePlacement, GraceTiming},
    lines::{LineKind, PedalKind},
    model::{Instrument, Note, Score},
};
use std::collections::{BTreeMap, HashSet};
pub const MAX_NOTES: usize = 2_000_000;
pub const MAX_CONTROLS: usize = 1_000_000;
#[derive(Clone, Debug)]
pub struct PerformedNote {
    pub anchor: Anchor,
    /// 0–3 notation layers; 4–11 independent chord stack slots.
    pub layer: u8,
    pub grace: bool,
    pub start: f64,
    pub end: f64,
    pub sounding_end: f64,
    pub sampler_end: f64,
    pub midi: u8,
    pub velocity: u8,
    pub instrument: Instrument,
}
#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub tick: f64,
    pub part: usize,
    pub layer: u8,
    pub controller: u8,
    pub value: u8,
    pub priority: u8,
}
#[derive(Clone, Debug)]
pub struct Timeline {
    pub notes: Vec<PerformedNote>,
    pub controls: Vec<Control>,
    pub tempo: TempoMap,
    pub ticks: f64,
}
impl Timeline {
    pub fn seconds(&self, tick: f64) -> f64 {
        self.tempo.seconds_at_tick(tick)
    }
    pub fn expression(&self, part: usize, layer: u8, tick: f64) -> f32 {
        let lane = self
            .controls
            .iter()
            .filter(|c| c.part == part && c.layer == layer && c.controller == 11);
        let mut value = 127;
        for c in lane {
            if c.tick > tick {
                break;
            }
            value = c.value;
        }
        f32::from(value) / 127.0
    }
}
pub fn needed(score: &Score) -> bool {
    score.notation.chords.iter().any(|chord| chord.play)
        || !score.notation.grace.is_empty()
        || score
            .notation
            .lines
            .iter()
            .any(|line| !matches!(line.kind, LineKind::Ottava { .. }))
        || !score.notation.spanners.is_empty()
        || score
            .notation
            .expressions
            .iter()
            .any(|e| e.kind == ExpressionKind::Technique && technique(&e.text).is_some())
}
#[derive(Clone, Copy)]
enum Technique {
    Ordinary,
    Pizzicato,
    Tremolo,
    Muted,
}
fn technique(text: &str) -> Option<Technique> {
    let s = text.to_ascii_lowercase().replace(['.', ' '], "");
    match s.as_str() {
        "pizz" | "pizzicato" => Some(Technique::Pizzicato),
        "arco" | "ord" | "ordinario" | "senzasord" | "senzasordino" | "open" => {
            Some(Technique::Ordinary)
        }
        "trem" | "tremolo" => Some(Technique::Tremolo),
        "consord" | "consordino" | "mute" | "muted" => Some(Technique::Muted),
        _ => None,
    }
}
fn preset(base: Instrument, t: Technique) -> Instrument {
    let program = base.gm_program();
    let target = match t {
        Technique::Ordinary => program,
        Technique::Pizzicato if (40..=51).contains(&program) => {
            if program == 43 {
                32
            } else {
                45
            }
        }
        Technique::Tremolo if (40..=51).contains(&program) => 44,
        Technique::Muted if matches!(program, 56 | 59) => 59,
        _ => program,
    };
    Instrument::from_gm_program(target).unwrap_or(base)
}
#[derive(Clone)]
struct Event {
    anchor: Anchor,
    start: f64,
    end: f64,
    original_start: u64,
    original_end: u64,
    note: Note,
    grace: bool,
    grace_velocity: Option<u8>,
}
fn source(score: &Score, a: Anchor) -> u64 {
    a.measure as u64 * u64::from(score.measure_ticks()) + u64::from(a.tick)
}
fn adjacent(a: &Event, b: &Event, score: &Score) -> bool {
    !a.grace
        && !b.grace
        && a.anchor.part == b.anchor.part
        && a.anchor.layer == b.anchor.layer
        && source(score, a.anchor) + u64::from(a.note.ticks()) == source(score, b.anchor)
        && a.original_end == b.original_start
}
/// Grace windows are borrowed before tie merging. A tie across a displaced
/// principal onset or trimmed donor tail is rearticulated, never stretched over
/// an intervening ornament. At jumps, previous-note borrowing falls back to main.
fn events(
    score: &Score,
    part: usize,
    layer: u8,
    order: &[usize],
    groups: &BTreeMap<(Anchor, GracePlacement), &crate::grace::GraceGroup>,
) -> Result<Vec<Event>, String> {
    let mut main: Vec<_> = crate::repeats::events(&score.parts[part], score.measure_ticks(), order)
        .into_iter()
        .filter(|e| e.note.layer == layer)
        .map(|e| Event {
            anchor: Anchor {
                part,
                measure: e.measure,
                tick: e.note.start,
                layer,
            },
            start: e.tick as f64,
            end: (e.tick + u64::from(e.note.ticks())) as f64,
            original_start: e.tick,
            original_end: e.tick + u64::from(e.note.ticks()),
            note: e.note.clone(),
            grace: false,
            grace_velocity: None,
        })
        .collect();
    let mut added = Vec::new();
    for i in 0..main.len() {
        for placement in [GracePlacement::Before, GracePlacement::After] {
            let Some(group) = groups.get(&(main[i].anchor, placement)) else {
                continue;
            };
            if group.percent == 0.0 {
                continue;
            }
            let previous = placement == GracePlacement::Before
                && group.timing == GraceTiming::StealPrevious
                && i > 0
                && adjacent(&main[i - 1], &main[i], score)
                && (main[i - 1].end - main[i].start).abs() < 1e-8;
            let donor = if previous { i - 1 } else { i };
            let available = (main[donor].end - main[donor].start).max(0.0);
            let amount = available * f64::from(group.percent) / 100.0;
            if amount <= 0.0 {
                continue;
            }
            let start = if previous || placement == GracePlacement::After {
                main[donor].end -= amount;
                main[donor].end
            } else {
                let start = main[i].start;
                main[i].start += amount;
                start
            };
            let weight = group
                .notes
                .iter()
                .map(|n| u64::from(n.weight()))
                .sum::<u64>() as f64;
            let mut cursor = start;
            for grace in &group.notes {
                let end = cursor + amount * f64::from(grace.weight()) / weight;
                let mut note = main[i].note.clone();
                note.pitches = grace.pitches.clone();
                note.velocity = grace.velocity.unwrap_or(main[i].note.velocity);
                note.tied = false;
                note.staccato = false;
                note.articulations.clear();
                if main.len() + added.len() >= MAX_NOTES {
                    return Err("Performed grace expansion exceeds two million events.".into());
                }
                added.push(Event {
                    anchor: main[i].anchor,
                    start: cursor,
                    end: if group.slur_to_main {
                        end
                    } else {
                        cursor + (end - cursor) * 0.94
                    },
                    original_start: main[i].original_start,
                    original_end: main[i].original_end,
                    note,
                    grace: true,
                    grace_velocity: grace.velocity,
                });
                cursor = end;
            }
        }
    }
    main.extend(added);
    main.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.grace.cmp(&b.grace)));
    Ok(main)
}
/// Contiguous written runs reset all line/technique state at repeat and D.C./D.S.
/// jumps. Intersections clip sustained pedals and hairpins at those boundaries.
fn runs(score: &Score, order: &[usize]) -> Vec<(f64, f64, f64)> {
    let width = f64::from(score.measure_ticks());
    let mut out = Vec::new();
    let mut first = 0;
    while first < order.len() {
        let mut end = first + 1;
        while end < order.len() && order[end] == order[end - 1] + 1 {
            end += 1;
        }
        out.push((
            first as f64 * width,
            order[first] as f64 * width,
            (order[end - 1] + 1) as f64 * width,
        ));
        first = end;
    }
    out
}
fn intervals(score: &Score, order: &[usize], start: Anchor, end: Anchor) -> Vec<(f64, f64)> {
    let a = source(score, start) as f64;
    let b = source(score, end) as f64;
    runs(score, order)
        .into_iter()
        .filter_map(|(performed, lo, hi)| {
            let left = a.max(lo);
            let right = b.min(hi);
            (left < right).then_some((performed + left - lo, performed + right - lo))
        })
        .collect()
}
fn to_anchor(score: &Score, part: usize, layer: u8, source_tick: u64) -> Anchor {
    let width = u64::from(score.measure_ticks());
    Anchor {
        part,
        layer,
        measure: (source_tick / width) as usize,
        tick: (source_tick % width) as u32,
    }
}
impl Timeline {
    pub fn new(score: &Score) -> Result<Self, String> {
        score.validate()?;
        let order = crate::repeats::playback_order(score)?;
        let tempo = TempoMap::new(score)?;
        let ticks = order.len() as f64 * f64::from(score.measure_ticks());
        let expressions = ExpressionMap::new(score);
        let styles = crate::entry_style::Index::new(score);
        let groups: BTreeMap<_, _> = score
            .notation
            .grace
            .iter()
            .map(|g| ((g.anchor, g.placement), g))
            .collect();
        let mut notes = Vec::new();
        let mut controls = Vec::new();
        let mut techniques = BTreeMap::<(usize, u8), Vec<(u64, Technique)>>::new();
        for mark in &score.notation.expressions {
            if mark.kind == ExpressionKind::Technique
                && let Some(t) = technique(&mark.text)
            {
                for part in 0..score.parts.len() {
                    if mark.system || part == mark.anchor.part {
                        techniques
                            .entry((part, mark.anchor.layer))
                            .or_default()
                            .push((source(score, mark.anchor), t));
                    }
                }
            }
        }
        for lane in techniques.values_mut() {
            lane.sort_by_key(|(t, _)| *t);
        }
        for (part, p) in score.parts.iter().enumerate() {
            for layer in 0..4 {
                let timeline = events(score, part, layer, &order, &groups)?;
                let mut consumed = HashSet::new();
                for (i, e) in timeline.iter().enumerate() {
                    if e.note.velocity == 0 {
                        continue;
                    }
                    for pitch in &e.note.pitches {
                        let midi = pitch.midi();
                        if consumed.contains(&(i, midi)) {
                            continue;
                        }
                        let mut last = i;
                        let mut end = e.end;
                        while !timeline[last].grace
                            && styles.ties_forward(
                                &timeline[last].anchor,
                                &timeline[last].note,
                                midi,
                            )
                        {
                            let Some(next) = timeline.get(last + 1) else {
                                break;
                            };
                            if !adjacent(&timeline[last], next, score)
                                || (end - next.start).abs() > 1e-8
                                || !next.note.pitches.iter().any(|p| p.midi() == midi)
                                || next.note.velocity == 0
                            {
                                break;
                            }
                            last += 1;
                            consumed.insert((last, midi));
                            end = next.end;
                        }
                        let last_event = &timeline[last];
                        let shortening = if last_event.grace {
                            0.0
                        } else {
                            (last_event.end - last_event.start)
                                * last_event.note.playback_shortening()
                        };
                        let legato = !last_event.grace
                            && crate::slurs::continues(score, &last_event.anchor)
                            && timeline.get(last + 1).is_some_and(|next| {
                                adjacent(last_event, next, score) && next.start == end
                            })
                            && shortening == 0.0;
                        let end = (end - shortening + if legato { 15.0 } else { 0.0 })
                            .max(e.start + 0.01);
                        let lane = techniques.get(&(part, layer));
                        let instrument = lane.map_or(p.instrument, |lane| {
                            let count =
                                lane.partition_point(|(at, _)| *at <= source(score, e.anchor));
                            if count == 0 {
                                p.instrument
                            } else {
                                preset(p.instrument, lane[count - 1].1)
                            }
                        });
                        notes.push(PerformedNote {
                            anchor: e.anchor,
                            layer,
                            grace: e.grace,
                            start: e.start,
                            end,
                            sounding_end: end,
                            sampler_end: end,
                            midi,
                            velocity: if e.grace {
                                e.grace_velocity
                                    .unwrap_or_else(|| expressions.velocity(e.anchor, &e.note))
                            } else {
                                expressions.velocity(e.anchor, &e.note)
                            },
                            instrument,
                        });
                        if notes.len() > MAX_NOTES {
                            return Err(
                                "Performed pitch expansion exceeds two million notes.".into()
                            );
                        }
                    }
                }
            }
        }
        // Trills and chromatic glissandi are discrete note sequences on every
        // backend. Continuous portamento and keyswitch dictionaries are separate.
        for line in &score.notation.lines {
            match line.kind {
                LineKind::Trill { interval, rate } => {
                    let spans = intervals(score, &order, line.start, line.end);
                    let mut decorated = Vec::new();
                    let mut overflow = false;
                    notes.retain(|n| {
                        if overflow {
                            return true;
                        }
                        if n.anchor.part != line.start.part
                            || n.anchor.layer != line.start.layer
                            || n.grace
                        {
                            return true;
                        }
                        let Some((a, b)) = spans.iter().find(|(a, b)| n.start < *b && n.end > *a)
                        else {
                            return true;
                        };
                        if n.start < *a {
                            let mut head = n.clone();
                            head.end = *a;
                            head.sounding_end = *a;
                            head.sampler_end = *a;
                            decorated.push(head);
                        }
                        if n.end > *b {
                            let mut tail = n.clone();
                            tail.start = *b;
                            decorated.push(tail);
                        }
                        let end = n.end.min(*b);
                        let mut at = n.start.max(*a);
                        let mut upper = false;
                        while at < end {
                            let stop = (at + f64::from(rate)).min(end);
                            let mut note = n.clone();
                            note.start = at;
                            note.end = stop;
                            note.sounding_end = stop;
                            note.sampler_end = stop;
                            note.midi = if upper {
                                (i16::from(n.midi) + i16::from(interval)).min(127) as u8
                            } else {
                                n.midi
                            };
                            if decorated.len() >= MAX_NOTES {
                                overflow = true;
                                break;
                            }
                            decorated.push(note);
                            upper = !upper;
                            at = stop;
                            if decorated.len() > MAX_NOTES {
                                break;
                            }
                        }
                        false
                    });
                    if overflow {
                        return Err("Trill expansion exceeds two million notes.".into());
                    }
                    notes.extend(decorated);
                }
                LineKind::Glissando => {
                    let target = crate::document::note_at(score, &line.end)
                        .and_then(|n| n.pitches.first())
                        .map(|p| p.midi());
                    if let Some(target) = target {
                        let spans: Vec<_> = intervals(score, &order, line.start, line.end)
                            .into_iter()
                            .filter(|(_, b)| {
                                notes
                                    .iter()
                                    .any(|n| n.anchor == line.end && (n.start - *b).abs() < 1e-8)
                            })
                            .collect();
                        let mut decorated = Vec::new();
                        let mut overflow = false;
                        notes.retain(|n| {
                            if overflow {
                                return true;
                            }
                            if n.anchor != line.start || n.grace {
                                return true;
                            }
                            let Some((a, b)) =
                                spans.iter().find(|(a, _)| (n.start - *a).abs() < 1e-8)
                            else {
                                return true;
                            };
                            let distance = i16::from(target) - i16::from(n.midi);
                            let count = distance.unsigned_abs().max(1) as usize;
                            if decorated.len() + count > MAX_NOTES {
                                overflow = true;
                                return true;
                            }
                            for step in 0..count {
                                let mut note = n.clone();
                                note.midi =
                                    (i16::from(n.midi) + distance.signum() * step as i16) as u8;
                                note.start = *a + (*b - *a) * step as f64 / count as f64;
                                note.end = *a + (*b - *a) * (step + 1) as f64 / count as f64;
                                note.sounding_end = note.end;
                                note.sampler_end = note.end;
                                decorated.push(note);
                            }
                            false
                        });
                        if overflow {
                            return Err("Glissando expansion exceeds two million notes.".into());
                        }
                        notes.extend(decorated);
                    }
                }
                _ => {}
            }
            if notes.len() > MAX_NOTES {
                return Err("Ornament expansion exceeds two million notes.".into());
            }
        }
        for line in &score.notation.lines {
            if let LineKind::Pedal { kind } = line.kind {
                for (a, b) in intervals(score, &order, line.start, line.end) {
                    if controls.len() + 8 > MAX_CONTROLS {
                        return Err("Pedal controller expansion exceeds one million events.".into());
                    }
                    for layer in 0..4 {
                        let controller = if kind == PedalKind::Damper { 64 } else { 66 };
                        controls.push(Control {
                            tick: a,
                            part: line.start.part,
                            layer,
                            controller,
                            value: 127,
                            priority: if kind == PedalKind::Sostenuto { 4 } else { 1 },
                        });
                        controls.push(Control {
                            tick: b,
                            part: line.start.part,
                            layer,
                            controller,
                            value: 0,
                            priority: 1,
                        });
                    }
                    for note in notes
                        .iter_mut()
                        .filter(|n| n.anchor.part == line.start.part)
                    {
                        let held = match kind {
                            PedalKind::Damper => note.end >= a && note.start < b,
                            PedalKind::Sostenuto => note.start <= a && note.end > a,
                        };
                        if held && note.end < b {
                            note.sounding_end = note.sounding_end.max(b);
                            if kind == PedalKind::Sostenuto {
                                note.sampler_end = note.sampler_end.max(b);
                            }
                        }
                    }
                }
            }
        }
        // A fixed attack reference leaves headroom for CC11 to swell a held SF2
        // voice. This is GM expression loudness, not sample-layer crossfading.
        for part in 0..score.parts.len() {
            for layer in 0..4 {
                let mut hairpins: Vec<_> = score
                    .notation
                    .spanners
                    .iter()
                    .filter(|h| h.start.part == part && h.start.layer == layer)
                    .collect();
                if hairpins.is_empty() {
                    continue;
                }
                hairpins.sort_by_key(|h| source(score, h.start));
                let mut mark_positions = BTreeMap::<usize, Vec<u32>>::new();
                for mark in score
                    .notation
                    .expressions
                    .iter()
                    .filter(|mark| mark.anchor.part == part && mark.anchor.layer == layer)
                {
                    mark_positions
                        .entry(mark.anchor.measure)
                        .or_default()
                        .push(mark.anchor.tick);
                }
                let reference = notes
                    .iter()
                    .filter(|n| n.anchor.part == part && n.anchor.layer == layer)
                    .map(|n| n.velocity)
                    .chain(
                        score
                            .notation
                            .spanners
                            .iter()
                            .filter(|h| h.start.part == part && h.start.layer == layer)
                            .flat_map(|h| {
                                [
                                    h.start_velocity.unwrap_or(127),
                                    h.end_velocity.unwrap_or(127),
                                ]
                            }),
                    )
                    .max()
                    .unwrap_or(127)
                    .max(1);
                let fallback = Note {
                    start: 0,
                    duration: crate::model::Duration::Whole,
                    dotted: false,
                    tuplet: None,
                    layer,
                    pitches: Vec::new(),
                    velocity: 80,
                    staccato: false,
                    tied: false,
                    articulations: Vec::new(),
                };
                let mut contexts: Vec<_> = score.parts[part]
                    .measures
                    .iter()
                    .enumerate()
                    .flat_map(|(m, bar)| {
                        bar.notes
                            .iter()
                            .filter(move |n| {
                                n.layer == layer && n.velocity > 0 && !n.pitches.is_empty()
                            })
                            .map(move |n| {
                                (
                                    m as u64 * u64::from(score.measure_ticks())
                                        + u64::from(n.start),
                                    n,
                                )
                            })
                    })
                    .collect();
                contexts.sort_by_key(|(at, _)| *at);
                let mut previous = None;
                for (bar, &source_bar) in order.iter().enumerate() {
                    let start = bar as f64 * f64::from(score.measure_ticks());
                    let source_start = source_bar as u64 * u64::from(score.measure_ticks());
                    let step = ((f64::from(crate::model::PPQ) * f64::from(score.tempo) / 60.0
                        * 0.02)
                        .round() as usize)
                        .clamp(4, 30);
                    let mut positions: Vec<u32> = vec![0];
                    let source_end = source_start + u64::from(score.measure_ticks());
                    let first = hairpins.partition_point(|h| source(score, h.end) <= source_start);
                    let last = hairpins.partition_point(|h| source(score, h.start) < source_end);
                    for h in &hairpins[first..last] {
                        let a = source(score, h.start);
                        let b = source(score, h.end);
                        let stride = step.max(((b - a) / 512).max(1) as usize);
                        let lo = a.max(source_start);
                        let hi = b.min(source_end);
                        positions.push((lo - source_start) as u32);
                        positions.push((hi - source_start) as u32);
                        let first = lo + (stride as u64 - (lo - a) % stride as u64) % stride as u64;
                        for at in (first..hi).step_by(stride) {
                            positions.push((at - source_start) as u32);
                        }
                    }

                    positions.extend(
                        score.parts[part].measures[source_bar]
                            .notes
                            .iter()
                            .filter(|n| n.layer == layer)
                            .map(|n| n.start),
                    );
                    if bar + 1 == order.len() {
                        positions.push(score.measure_ticks());
                    }
                    if let Some(marks) = mark_positions.get(&source_bar) {
                        positions.extend(marks);
                    }
                    positions.sort_unstable();
                    positions.dedup();
                    for at in positions {
                        let now = source_start + u64::from(at);
                        let position = contexts.partition_point(|(at, _)| *at <= now);
                        let lane_note =
                            position.checked_sub(1).map_or(&fallback, |i| contexts[i].1);
                        let value =
                            expressions.velocity(to_anchor(score, part, layer, now), lane_note);
                        let value = (f64::from(value) * 127.0 / f64::from(reference))
                            .round()
                            .clamp(0.0, 127.0) as u8;
                        if previous != Some(value) {
                            controls.push(Control {
                                tick: start + f64::from(at),
                                part,
                                layer,
                                controller: 11,
                                value,
                                priority: 1,
                            });
                            previous = Some(value);

                            if controls.len() > MAX_CONTROLS {
                                return Err(
                                    "Performance controller expansion exceeds one million events."
                                        .into(),
                                );
                            }
                        }
                    }
                }
                let envelope: Vec<_> = controls
                    .iter()
                    .filter(|c| c.part == part && c.layer == layer && c.controller == 11)
                    .map(|c| (c.tick, c.value))
                    .collect();
                for note in notes
                    .iter_mut()
                    .filter(|n| n.anchor.part == part && n.layer == layer)
                {
                    if note.grace {
                        let i = envelope.partition_point(|(at, _)| *at <= note.start);
                        let value = i.checked_sub(1).map_or(127, |i| envelope[i].1);
                        note.velocity = if value == 0 {
                            0
                        } else {
                            (f64::from(note.velocity) * 127.0 / f64::from(value))
                                .round()
                                .clamp(0.0, 127.0) as u8
                        };
                    } else {
                        note.velocity = if note.velocity == 0 { 0 } else { reference };
                    }
                }
            }
        }
        for (part, slot, start, end, pitches) in crate::chords::playback_events(score) {
            let a = to_anchor(score, part, 0, start);
            let b = if end == score.measure_count() as u64 * u64::from(score.measure_ticks()) {
                Anchor {
                    measure: score.measure_count() - 1,
                    tick: score.measure_ticks(),
                    ..a
                }
            } else {
                to_anchor(score, part, 0, end)
            };
            for (start, end) in intervals(score, &order, a, b) {
                for &midi in &pitches {
                    notes.push(PerformedNote {
                        anchor: a,
                        layer: 4 + slot,
                        grace: false,
                        start,
                        end,
                        sounding_end: end,
                        sampler_end: end,
                        midi,
                        velocity: 80,
                        instrument: score.parts[part].instrument,
                    });
                    if notes.len() > MAX_NOTES {
                        return Err("Chord accompaniment exceeds the performed pitch limit.".into());
                    }
                }
            }
        }
        if controls.len() > MAX_CONTROLS {
            return Err("Performance controller expansion exceeds one million events.".into());
        }
        notes.sort_by(|a, b| {
            a.start
                .total_cmp(&b.start)
                .then(a.anchor.cmp(&b.anchor))
                .then(a.midi.cmp(&b.midi))
        });
        controls.sort_by(|a, b| a.tick.total_cmp(&b.tick).then(a.priority.cmp(&b.priority)));
        Ok(Self {
            notes,
            controls,
            tempo,
            ticks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        grace::{GraceGroup, GraceNote},
        lines::PerformanceLine,
        model::{Duration, Pitch},
    };
    fn fixture() -> Score {
        let mut s = Score::blank();
        s.tempo = 120;
        s.parts.truncate(1);
        s.parts[0].measures.truncate(2);
        for m in &mut s.parts[0].measures {
            m.notes.clear();
        }
        s
    }
    fn note(start: u32, midi: u8) -> Note {
        Note {
            start,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: vec![Pitch::from_midi(midi)],
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        }
    }
    fn anchor(tick: u32) -> Anchor {
        Anchor {
            part: 0,
            measure: 0,
            tick,
            layer: 0,
        }
    }
    #[test]
    fn weighted_grace_borrows_previous_and_rearticulates_displaced_ties() {
        let mut s = fixture();
        let mut first = note(0, 60);
        first.tied = true;
        s.parts[0].measures[0].notes = vec![first, note(480, 60)];
        let mut g = GraceGroup::new(
            anchor(480),
            vec![
                GraceNote::new(Pitch::from_midi(65), Duration::Sixteenth),
                GraceNote::new(Pitch::from_midi(67), Duration::Eighth),
            ],
        );
        g.percent = 25.0;
        s.notation.grace.push(g);
        let t = Timeline::new(&s).unwrap();
        let notes: Vec<_> = t
            .notes
            .iter()
            .map(|n| (n.midi, n.start, n.end, n.grace))
            .collect();
        assert_eq!(
            notes,
            vec![
                (60, 0.0, 360.0, false),
                (65, 360.0, 397.6, true),
                (67, 400.0, 475.2, true),
                (60, 480.0, 960.0, false)
            ]
        );
        assert_eq!(s.parts[0].measures[0].notes[0].ticks(), 480);
    }
    #[test]
    fn grace_previous_falls_back_to_main_after_repeat_jump() {
        let mut s = fixture();
        s.parts[0].measures[0].notes = vec![note(0, 60)];
        s.parts[0].measures[1].notes = vec![note(1440, 62)];
        s.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 1,
            times: 2,
            endings: vec![],
        });
        let mut g = GraceGroup::new(
            anchor(0),
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Eighth)],
        );
        g.percent = 25.;
        s.notation.grace.push(g);
        let t = Timeline::new(&s).unwrap();
        let main: Vec<_> = t
            .notes
            .iter()
            .filter(|n| n.midi == 60)
            .map(|n| n.start)
            .collect();
        assert_eq!(main, vec![120., 3960.]);
        assert!(
            t.notes
                .iter()
                .any(|n| n.midi == 62 && n.start == 3360. && n.end == 3840.)
        );
    }
    #[test]
    fn pedal_damper_extends_all_layers_but_sostenuto_only_captures_held_notes() {
        for kind in [PedalKind::Damper, PedalKind::Sostenuto] {
            let mut s = fixture();
            let mut later = note(480, 64);
            later.layer = 1;
            s.parts[0].measures[0].notes = vec![note(0, 60), later];
            s.notation.lines.push(PerformanceLine::new(
                anchor(240),
                anchor(1440),
                LineKind::Pedal { kind },
            ));
            let t = Timeline::new(&s).unwrap();
            assert_eq!(t.notes[0].end, 480.);
            assert_eq!(t.notes[0].sounding_end, 1440.);
            assert_eq!(
                t.notes[1].sounding_end,
                if kind == PedalKind::Damper {
                    1440.
                } else {
                    960.
                }
            );
            assert_eq!(t.controls.iter().filter(|c| c.value == 127).count(), 4);
        }
    }
    #[test]
    fn trill_glissando_and_program_switches_are_real_performed_events() {
        let mut s = fixture();
        s.parts[0].instrument = Instrument::Violin;
        s.parts[0].measures[0].notes = vec![note(0, 60), note(480, 64), note(960, 67)];
        s.notation.lines.push(PerformanceLine::new(
            anchor(0),
            anchor(480),
            LineKind::Trill {
                interval: 1,
                rate: 120,
            },
        ));
        s.notation.lines.push(PerformanceLine::new(
            anchor(480),
            anchor(960),
            LineKind::Glissando,
        ));
        let mut technique = crate::expressions::Expression::text(anchor(480), "pizz.");
        technique.kind = ExpressionKind::Technique;
        s.notation.expressions.push(technique);
        let t = Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes.iter().map(|n| n.midi).collect::<Vec<_>>(),
            vec![60, 61, 60, 61, 64, 65, 66, 67]
        );
        assert_eq!(t.notes[4].instrument.gm_program(), 45);
        assert_eq!(t.notes[4].end - t.notes[4].start, 160.);
    }
    #[test]
    fn hairpin_controllers_swell_a_single_held_note_and_reset_on_repeat() {
        let mut s = fixture();
        let mut n = note(0, 60);
        n.duration = Duration::Whole;
        s.parts[0].measures[0].notes = vec![n];
        let mut h = crate::expressions::Hairpin::new(anchor(0), anchor(1920));
        h.start_velocity = Some(40);
        h.end_velocity = Some(120);
        s.notation.spanners.push(h);
        s.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 0,
            times: 2,
            endings: vec![],
        });
        let t = Timeline::new(&s).unwrap();
        assert_eq!(t.notes.len(), 2);
        assert_eq!(t.notes[0].velocity, 120);
        let values: Vec<_> = t.controls.iter().filter(|c| c.controller == 11).collect();
        assert_eq!(values[0].value, 42);
        assert_eq!((t.expression(0, 0, 960.) * 127.).round() as u8, 85);
        assert!(values.iter().any(|c| c.tick == 1920. && c.value == 42));
        assert!(t.expression(0, 0, 1800.) > t.expression(0, 0, 120.));
    }
    #[test]
    fn all_backends_consume_same_fractional_grace_ornament_and_pedal_plan() {
        let mut s = fixture();
        s.parts[0].measures[0].notes = vec![note(0, 60), note(480, 64)];
        let mut grace = GraceGroup::new(
            anchor(0),
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Eighth)],
        );
        grace.percent = 12.5;
        s.notation.grace.push(grace);
        s.notation.lines.push(PerformanceLine::new(
            anchor(0),
            anchor(1440),
            LineKind::Pedal {
                kind: PedalKind::Damper,
            },
        ));
        s.validate().unwrap();
        let t = Timeline::new(&s).unwrap();
        assert_eq!(t.notes[1].start, 60.);
        assert_eq!(t.notes[1].sounding_end, 1440.);
        let bytes = crate::export::midi(&s);
        assert!(bytes.windows(3).any(|b| b == [0xb0, 64, 127]));
        assert!(bytes.windows(3).any(|b| b == [0x90, 72, 80]));
        let audio = crate::synth::render_dry(&s);
        assert!(!audio.is_empty());
        assert!(audio.iter().all(|v| v.is_finite()));
        assert!(audio.iter().any(|v| v.abs() > 0.001));
    }
}

#[cfg(test)]
mod accompaniment_tests {
    use super::*;
    #[test]
    fn chord_stack_slots_have_separate_realized_lanes_and_follow_repeats() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes.clear();
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let mut first =
            crate::chords::ChordSymbol::new(anchor, crate::chords::parse("Cmaj7/E").unwrap());
        first.play = true;
        let mut second = first.clone();
        second.slot = 1;
        score.notation.chords = vec![first, second];
        score.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 0,
            times: 2,
            endings: vec![],
        });
        score.validate().unwrap();
        let timeline = Timeline::new(&score).unwrap();
        assert_eq!(timeline.notes.iter().filter(|n| n.layer == 4).count(), 10);
        assert_eq!(timeline.notes.iter().filter(|n| n.layer == 5).count(), 10);
        assert!(timeline.notes.iter().all(|n| n.end - n.start == 1920.));
        let midi = crate::export::midi(&score);
        assert_eq!(u16::from_be_bytes([midi[10], midi[11]]), 4);
        assert!(!crate::synth::render_dry(&score).is_empty());
    }
}

#[cfg(test)]
mod review_regressions {
    use super::*;
    use crate::{
        expressions::Hairpin,
        grace::{GraceGroup, GraceNote, GraceTiming},
        lines::PerformanceLine,
        model::{Duration, Pitch},
    };
    fn score() -> (Score, Anchor) {
        let mut s = Score::blank();
        s.parts.truncate(1);
        s.parts[0].measures.truncate(1);
        s.parts[0].measures[0].notes = vec![Note {
            start: 0,
            duration: Duration::Whole,
            dotted: false,
            tuplet: None,
            layer: 0,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        }];
        (
            s,
            Anchor {
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
            },
        )
    }
    #[test]
    fn explicit_grace_velocity_survives_unrelated_later_hairpin() {
        let (mut s, a) = score();
        let mut g = GraceGroup::new(
            a,
            vec![GraceNote::new(Pitch::from_midi(74), Duration::Eighth)],
        );
        g.timing = GraceTiming::StealMain;
        g.percent = 25.;
        g.notes[0].velocity = Some(20);
        s.notation.grace.push(g);
        let mut h = Hairpin::new(Anchor { tick: 960, ..a }, Anchor { tick: 1440, ..a });
        h.start_velocity = Some(80);
        h.end_velocity = Some(100);
        s.notation.spanners.push(h);
        let t = Timeline::new(&s).unwrap();
        let grace = t.notes.iter().find(|n| n.grace).unwrap();
        assert_eq!(grace.velocity, 25);
        let effective = f32::from(grace.velocity) * t.expression(0, 0, grace.start);
        assert!((effective - 20.).abs() < 0.2);
    }
    #[test]
    fn a_partial_held_note_trill_preserves_head_and_tail() {
        let (mut s, a) = score();
        s.notation.lines.push(PerformanceLine::new(
            Anchor { tick: 240, ..a },
            Anchor { tick: 720, ..a },
            LineKind::Trill {
                interval: 2,
                rate: 120,
            },
        ));
        let t = Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes
                .iter()
                .map(|n| (n.start, n.end, n.midi))
                .collect::<Vec<_>>(),
            vec![
                (0., 240., 60),
                (240., 360., 60),
                (360., 480., 62),
                (480., 600., 60),
                (600., 720., 62),
                (720., 1920., 60)
            ]
        );
    }
    #[test]
    fn mixed_pedals_and_captured_same_layer_reattacks_reject_exactly() {
        let (mut s, a) = score();
        s.parts[0].measures[0].notes[0].duration = Duration::Quarter;
        let mut later = s.parts[0].measures[0].notes[0].clone();
        later.start = 960;
        s.parts[0].measures[0].notes.push(later);
        s.notation.lines.push(PerformanceLine::new(
            a,
            Anchor { tick: 1440, ..a },
            LineKind::Pedal {
                kind: PedalKind::Sostenuto,
            },
        ));
        assert!(s.validate().unwrap_err().contains("reattack"));
        s.parts[0].measures[0].notes[1].layer = 1;
        s.validate().unwrap();
        s.notation.lines.push(PerformanceLine::new(
            Anchor { tick: 480, ..a },
            Anchor { tick: 960, ..a },
            LineKind::Pedal {
                kind: PedalKind::Damper,
            },
        ));
        assert!(s.validate().is_err());
    }
}
