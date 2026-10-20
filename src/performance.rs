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
    /// 0–7 independent notation voices; 8–15 chord stack slots.
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
    score
        .parts
        .iter()
        .flat_map(|part| &part.measures)
        .flat_map(|measure| &measure.notes)
        .any(|note| note.voice != 0)
        || score.notation.chords.iter().any(|chord| chord.play)
        || score
            .notation
            .ornaments
            .iter()
            .any(|o| o.playback && o.kind != crate::ornaments::OrnamentKind::NonArpeggiate)
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
        && a.anchor.lane() == b.anchor.lane()
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
        .filter(|e| e.note.lane() == layer)
        .map(|e| Event {
            anchor: Anchor {
                part,
                measure: e.measure,
                tick: e.note.start,
                layer: layer % 4,
                voice: layer / 4,
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
        voice: layer / 4,
        layer: layer % 4,
        measure: (source_tick / width) as usize,
        tick: (source_tick % width) as u32,
    }
}
#[derive(Clone)]
struct OrnamentPlan<'a> {
    mark: &'a crate::ornaments::Ornament,
    start: f64,
    end: f64,
    principal_start: f64,
    pitches: Vec<u8>,
    upper: u8,
    lower: u8,
    velocity: u8,
    instrument: Instrument,
}
fn clipped(note: &PerformedNote, start: f64, end: f64) -> PerformedNote {
    let mut result = note.clone();
    result.start = start;
    result.end = end;
    result.sounding_end = end;
    result.sampler_end = end;
    result
}
fn append_ornament_segment(output: &mut Vec<PerformedNote>, segment: PerformedNote, floor: usize) {
    if output.len() > floor
        && let Some(previous) = output.last_mut()
        && previous.midi == segment.midi
        && previous.velocity == segment.velocity
        && previous.instrument == segment.instrument
        && (previous.end - segment.start).abs() < 1e-8
    {
        previous.end = segment.end;
        previous.sounding_end = segment.end;
        previous.sampler_end = segment.end;
    } else {
        output.push(segment);
    }
}
fn decorate_ornaments(
    notes: Vec<PerformedNote>,
    plans: &BTreeMap<(usize, u8), Vec<OrnamentPlan<'_>>>,
) -> Result<Vec<PerformedNote>, String> {
    use crate::ornaments::OrnamentKind as K;
    let mut output = Vec::with_capacity(notes.len());
    for note in notes {
        let output_start = output.len();
        let Some(lane) = plans
            .get(&(note.anchor.part, note.layer))
            .filter(|_| !note.grace)
        else {
            output.push(note);
            continue;
        };
        let mut cursor = note.start;
        let mut state = note.clone();
        let first = lane.partition_point(|p| p.end <= note.start);
        for plan in lane[first..].iter().take_while(|p| p.start < note.end) {
            if !plan.pitches.contains(&note.midi) {
                continue;
            }
            if plan.mark.kind.chord() {
                // A pitch already held by a tie is not attacked again by a roll.
                if (note.start - plan.principal_start).abs() > 1e-8 {
                    continue;
                }
                let mut pitches = plan.pitches.clone();
                pitches.sort_unstable();
                if plan.mark.kind == K::ArpeggioDown {
                    pitches.reverse();
                }
                let rank = pitches.iter().position(|p| *p == note.midi).unwrap_or(0);
                cursor = plan.start
                    + (plan.end - plan.start) * rank as f64 / pitches.len().max(1) as f64;
                cursor = cursor.min(note.end - 0.01).max(note.start);
                continue;
            }
            let a = plan.start.max(cursor);
            let b = plan.end.min(note.end);
            if b <= a {
                continue;
            }
            if a > cursor {
                append_ornament_segment(&mut output, clipped(&state, cursor, a), output_start);
            }
            let sequence: Vec<u8> = match plan.mark.kind {
                K::Turn => vec![plan.upper, note.midi, plan.lower, note.midi],
                K::InvertedTurn => vec![plan.lower, note.midi, plan.upper, note.midi],
                K::Mordent => vec![note.midi, plan.lower, note.midi],
                K::UpperMordent => vec![note.midi, plan.upper, note.midi],
                _ => Vec::new(),
            };
            if output.len() + sequence.len() + 1 > MAX_NOTES {
                return Err("Ornament expansion exceeds two million notes.".into());
            }
            state.velocity = plan.velocity;
            state.instrument = plan.instrument;
            state.anchor = plan.mark.anchor;
            for (index, midi) in sequence.iter().enumerate() {
                let mut segment = clipped(
                    &state,
                    a + (b - a) * index as f64 / sequence.len() as f64,
                    a + (b - a) * (index + 1) as f64 / sequence.len() as f64,
                );
                segment.midi = *midi;
                segment.anchor = plan.mark.anchor;
                append_ornament_segment(&mut output, segment, output_start);
            }
            cursor = b;
        }
        if cursor < note.end {
            append_ornament_segment(&mut output, clipped(&state, cursor, note.end), output_start);
        }
        if output.len() > MAX_NOTES {
            return Err("Ornament expansion exceeds two million notes.".into());
        }
    }
    Ok(output)
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
        let ornament_marks: BTreeMap<_, _> = score
            .notation
            .ornaments
            .iter()
            .filter(|o| o.playback && o.kind != crate::ornaments::OrnamentKind::NonArpeggiate)
            .map(|o| (o.anchor, o))
            .collect();
        let mut ornament_plans = BTreeMap::<(usize, u8), Vec<OrnamentPlan<'_>>>::new();
        let mut plan_count = 0_usize;
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
            for layer in 0..8 {
                let timeline = events(score, part, layer, &order, &groups)?;
                for event in timeline.iter().filter(|e| !e.grace) {
                    if let Some(&mark) = ornament_marks.get(&event.anchor) {
                        if plan_count >= MAX_NOTES {
                            return Err(
                                "Performed ornaments exceed two million principal windows.".into(),
                            );
                        }
                        let (upper, lower) =
                            crate::ornaments::auxiliary_pitches_for_note(score, mark, &event.note)?;
                        let start = event.start
                            + (event.end - event.start) * f64::from(mark.delay_percent) / 100.;
                        let end = (start + f64::from(mark.window_ticks)).min(event.end);
                        ornament_plans
                            .entry((part, layer))
                            .or_default()
                            .push(OrnamentPlan {
                                mark,
                                start,
                                end,
                                principal_start: event.start,
                                pitches: event.note.pitches.iter().map(|p| p.midi()).collect(),
                                upper: upper.midi(),
                                lower: lower.midi(),
                                velocity: expressions.velocity(event.anchor, &event.note),
                                instrument: techniques.get(&(part, layer % 4)).map_or(
                                    p.instrument,
                                    |lane| {
                                        let i = lane.partition_point(|(at, _)| {
                                            *at <= source(score, event.anchor)
                                        });
                                        i.checked_sub(1).map_or(p.instrument, |i| {
                                            preset(p.instrument, lane[i].1)
                                        })
                                    },
                                ),
                            });
                        plan_count += 1;
                    }
                }
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
                        let lane = techniques.get(&(part, layer % 4));
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
                            || n.anchor.lane() != line.start.lane()
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
        notes = decorate_ornaments(notes, &ornament_plans)?;
        let mut performed_attacks = BTreeMap::<(usize, u8, u8), Vec<f64>>::new();
        if score.notation.lines.iter().any(|line| {
            matches!(
                line.kind,
                LineKind::Pedal {
                    kind: PedalKind::Sostenuto
                }
            )
        }) {
            for note in &notes {
                performed_attacks
                    .entry((note.anchor.part, note.layer, note.midi))
                    .or_default()
                    .push(note.start);
            }
            for attacks in performed_attacks.values_mut() {
                attacks.sort_by(f64::total_cmp);
            }
        }
        for line in &score.notation.lines {
            if let LineKind::Pedal { kind } = line.kind {
                for (a, b) in intervals(score, &order, line.start, line.end) {
                    if kind == PedalKind::Sostenuto {
                        for note in notes.iter().filter(|n| {
                            n.anchor.part == line.start.part && n.start <= a && n.end > a
                        }) {
                            if let Some(attacks) =
                                performed_attacks.get(&(note.anchor.part, note.layer, note.midi))
                            {
                                let next = attacks.partition_point(|at| *at <= a);
                                if attacks.get(next).is_some_and(|at| *at < b) {
                                    return Err("Sostenuto cannot reattack a captured performed pitch in the same voice before release; use damper pedal or another voice.".into());
                                }
                            }
                        }
                    }
                    if controls.len() + 16 > MAX_CONTROLS {
                        return Err("Pedal controller expansion exceeds one million events.".into());
                    }
                    for layer in 0..8 {
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
            for layer in 0..8 {
                let mut hairpins: Vec<_> = score
                    .notation
                    .spanners
                    .iter()
                    .filter(|h| h.start.part == part && h.start.layer == layer % 4)
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
                    .filter(|mark| mark.anchor.part == part && mark.anchor.layer == layer % 4)
                {
                    mark_positions
                        .entry(mark.anchor.measure)
                        .or_default()
                        .push(mark.anchor.tick);
                }
                let reference = notes
                    .iter()
                    .filter(|n| n.anchor.part == part && n.layer == layer)
                    .map(|n| n.velocity)
                    .chain(
                        score
                            .notation
                            .spanners
                            .iter()
                            .filter(|h| h.start.part == part && h.start.layer == layer % 4)
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
                    voice_launch: None,
                    start: 0,
                    duration: crate::model::Duration::Whole,
                    dotted: false,
                    tuplet: None,
                    layer: layer % 4,
                    voice: layer / 4,
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
                                n.lane() == layer && n.velocity > 0 && !n.pitches.is_empty()
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
                            .filter(|n| n.lane() == layer)
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
                        layer: 8 + slot,
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
            voice: 0,
            voice_launch: None,
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
            voice: 0,
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
            assert_eq!(t.controls.iter().filter(|c| c.value == 127).count(), 8);
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
            voice: 0,
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
        assert_eq!(timeline.notes.iter().filter(|n| n.layer == 8).count(), 10);
        assert_eq!(timeline.notes.iter().filter(|n| n.layer == 9).count(), 10);
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
            voice: 0,
            voice_launch: None,
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
                voice: 0,
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

#[cfg(test)]
mod voice_two_tests {
    use super::*;
    use crate::{
        expressions::{Expression, Hairpin},
        grace::{GraceGroup, GraceNote},
        lines::PerformanceLine,
        model::{Duration, Pitch},
    };
    pub(super) fn fixture() -> Score {
        let mut score = Score::blank();
        score.tempo = 120;
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        let first = Note {
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
        let mut second = first.clone();
        second.voice = 1;
        second.voice_launch = Some(0);
        second.duration = Duration::Quarter;
        second.tied = true;
        let mut continuation = second.clone();
        continuation.start = 480;
        continuation.tied = false;
        score.parts[0].measures[0].notes = vec![first, second, continuation];
        score
    }
    fn a(voice: u8, tick: u32) -> Anchor {
        Anchor {
            part: 0,
            measure: 0,
            tick,
            layer: 0,
            voice,
        }
    }
    #[test]
    fn simultaneous_identical_pitches_have_voice_local_tie_lifetimes() {
        let score = fixture();
        score.validate().unwrap();
        let t = Timeline::new(&score).unwrap();
        assert_eq!(t.notes.len(), 2);
        let first = t.notes.iter().find(|n| n.layer == 0).unwrap();
        let second = t.notes.iter().find(|n| n.layer == 4).unwrap();
        assert_eq!((first.start, first.end, first.anchor.voice), (0., 1920., 0));
        assert_eq!(
            (second.start, second.end, second.anchor.voice),
            (0., 960., 1)
        );
        assert!(needed(&score));
    }
    #[test]
    fn layer_dynamics_hairpins_and_techniques_reach_both_voices() {
        let mut score = fixture();
        score.parts[0].instrument = Instrument::Violin;
        score
            .notation
            .expressions
            .push(Expression::dynamic(a(0, 0), "p", 40));
        let mut technique = Expression::text(a(1, 0), "pizz.");
        technique.kind = ExpressionKind::Technique;
        score.notation.expressions.push(technique);
        let mut hairpin = Hairpin::new(a(0, 0), a(0, 1920));
        hairpin.start_velocity = Some(40);
        hairpin.end_velocity = Some(120);
        score.notation.spanners.push(hairpin);
        let t = Timeline::new(&score).unwrap();
        for lane in [0, 4] {
            let n = t.notes.iter().find(|n| n.layer == lane).unwrap();
            assert_eq!(n.instrument.gm_program(), 45);
            assert_eq!(n.velocity, 120);
            assert!((t.expression(0, lane, 0.) - 40. / 120.).abs() < 0.01);
            assert!(t.expression(0, lane, 960.) > 0.6);
        }
        let mut duplicate = Expression::dynamic(a(1, 0), "f", 100);
        duplicate.anchor.voice = 1;
        score.notation.expressions.push(duplicate);
        assert!(score.validate().is_err());
    }
    #[test]
    fn grace_donor_and_glissando_are_local_to_secondary_voice() {
        let mut score = fixture();
        score.parts[0].measures[0].notes[1].tied = false;
        score.notation.grace.push(GraceGroup::new(
            a(1, 480),
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Eighth)],
        ));
        let t = Timeline::new(&score).unwrap();
        let primary = t.notes.iter().find(|n| n.layer == 0).unwrap();
        assert_eq!(primary.end, 1920.);
        assert!(
            t.notes
                .iter()
                .any(|n| n.layer == 4 && n.grace && n.start == 384.)
        );
        assert!(
            t.notes
                .iter()
                .any(|n| n.layer == 4 && !n.grace && n.end == 384.)
        );
        score.notation.grace.clear();
        score.parts[0].measures[0].notes[1].pitches = vec![Pitch::from_midi(64)];
        score.parts[0].measures[0].notes[2].pitches = vec![Pitch::from_midi(67)];
        score.notation.lines.push(PerformanceLine::new(
            a(1, 0),
            a(1, 480),
            LineKind::Glissando,
        ));
        let t = Timeline::new(&score).unwrap();
        assert_eq!(t.notes.iter().filter(|n| n.layer == 0).count(), 1);
        assert_eq!(
            t.notes
                .iter()
                .filter(|n| n.layer == 4)
                .map(|n| n.midi)
                .collect::<Vec<_>>(),
            vec![64, 65, 66, 67]
        );
    }
    #[test]
    fn layer_ottava_reaches_both_voices_while_pedal_isolates_equal_keys() {
        let mut score = fixture();
        score.parts[0].measures[0].notes[1].tied = false;
        score.notation.lines.push(PerformanceLine::new(
            a(0, 0),
            a(0, 1920),
            LineKind::Ottava { semitones: 12 },
        ));
        let index = crate::lines::PitchIndex::new(&score);
        for voice in 0..2 {
            assert_eq!(
                index
                    .written_pitch(a(voice, 0), Pitch::from_midi(72))
                    .midi(),
                60
            );
        }
        score.notation.lines.push(PerformanceLine::new(
            a(0, 0),
            a(0, 1440),
            LineKind::Pedal {
                kind: PedalKind::Sostenuto,
            },
        ));
        assert!(score.validate().is_err());
        score.parts[0].measures[0].notes[2].pitches[0] = Pitch::from_midi(64);
        score.validate().unwrap();
        let t = Timeline::new(&score).unwrap();
        assert_eq!(
            t.notes
                .iter()
                .find(|n| n.layer == 4 && n.midi == 60)
                .unwrap()
                .sounding_end,
            1440.
        );
        assert_eq!(
            t.notes
                .iter()
                .find(|n| n.layer == 4 && n.midi == 64)
                .unwrap()
                .sounding_end,
            960.
        );
    }
}

#[cfg(test)]
mod discrete_ornament_tests {
    use super::*;
    use crate::{
        grace::{GraceGroup, GraceNote, GraceTiming},
        model::{Duration, Pitch},
        ornaments::{Ornament, OrnamentKind as K},
    };
    fn fixture() -> (Score, Anchor) {
        let mut s = Score::blank();
        s.parts.truncate(1);
        s.parts[0].measures.truncate(1);
        s.parts[0].measures[0].notes = vec![Note {
            layer: 0,
            voice: 0,
            voice_launch: None,
            start: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(60)],
            velocity: 80,
            staccato: false,
            articulations: vec![],
            tied: false,
        }];
        (
            s,
            Anchor {
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
                voice: 0,
            },
        )
    }
    #[test]
    fn turn_window_preserves_plain_prefix_tail_and_both_voices() {
        let (mut s, a) = fixture();
        let mut n = s.parts[0].measures[0].notes[0].clone();
        n.voice = 1;
        n.pitches = vec![Pitch::from_midi(48)];
        s.parts[0].measures[0].notes.push(n);
        let mut m = Ornament::new(a, K::Turn);
        m.delay_percent = 25.;
        s.notation.ornaments.push(m);
        let t = Timeline::new(&s).unwrap();
        let lane: Vec<_> = t
            .notes
            .iter()
            .filter(|n| n.layer == 0)
            .map(|n| (n.midi, n.start, n.end))
            .collect();
        assert_eq!(
            lane,
            vec![
                (60, 0., 120.),
                (62, 120., 150.),
                (60, 150., 180.),
                (59, 180., 210.),
                (60, 210., 480.)
            ]
        );
        assert_eq!(
            t.notes
                .iter()
                .find(|n| n.layer == 4)
                .map(|n| (n.midi, n.start, n.end)),
            Some((48, 0., 480.))
        );
    }
    #[test]
    fn tied_continuation_ornament_decorates_the_held_principal() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes[0].tied = true;
        let mut next = s.parts[0].measures[0].notes[0].clone();
        next.start = 480;
        next.tied = false;
        s.parts[0].measures[0].notes.push(next);
        s.notation
            .ornaments
            .push(Ornament::new(Anchor { tick: 480, ..a }, K::Mordent));
        let t = Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes
                .iter()
                .map(|n| (n.midi, n.start, n.end))
                .collect::<Vec<_>>(),
            vec![(60, 0., 520.), (59, 520., 560.), (60, 560., 960.)]
        );
    }
    #[test]
    fn rolls_sort_chord_pitches_and_leave_tied_members_held() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes[0].pitches = vec![
            Pitch::from_midi(67),
            Pitch::from_midi(60),
            Pitch::from_midi(64),
        ];
        s.notation.ornaments.push(Ornament::new(a, K::ArpeggioDown));
        let t = Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes
                .iter()
                .map(|n| (n.midi, n.start, n.end))
                .collect::<Vec<_>>(),
            vec![(67, 0., 480.), (64, 40., 480.), (60, 80., 480.)]
        );
        let mut continuation = s.parts[0].measures[0].notes[0].clone();
        continuation.start = 480;
        continuation.tied = false;
        s.parts[0].measures[0].notes[0].tied = true;
        s.parts[0].measures[0].notes.push(continuation);
        s.notation.ornaments[0].anchor.tick = 480;
        let t = Timeline::new(&s).unwrap();
        assert!(t.notes.iter().all(|n| n.start == 0. && n.end == 960.));
    }
    #[test]
    fn grace_and_repeat_decoration_share_midi_and_procedural_plan() {
        let (mut s, a) = fixture();
        let mut grace = GraceGroup::new(
            a,
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Sixteenth)],
        );
        grace.percent = 25.;
        grace.timing = GraceTiming::StealMain;
        s.notation.grace.push(grace);
        s.notation.ornaments.push(Ornament::new(a, K::UpperMordent));
        s.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 0,
            times: 2,
            endings: vec![],
        });
        let t = Timeline::new(&s).unwrap();
        assert_eq!(t.notes[0].midi, 72);
        assert_eq!(t.notes[1].start, 120.);
        assert!(t.notes.iter().any(|n| n.midi == 62 && n.start == 160.));
        assert!(t.notes.iter().any(|n| n.midi == 62 && n.start == 2080.));
        let midi = crate::export::midi(&s);
        assert!(midi.windows(3).any(|b| b == [0x90, 62, 80]));
        let audio = crate::synth::render_dry(&s);
        assert!(!audio.is_empty());
        assert!(audio.iter().all(|n| n.is_finite()));
    }
    #[test]
    fn disabled_marks_and_non_arpeggiate_preserve_legacy_path() {
        let (mut s, a) = fixture();
        let mut mark = Ornament::new(a, K::Turn);
        mark.playback = false;
        s.notation.ornaments.push(mark);
        assert!(!needed(&s));
        let baseline = crate::synth::render_dry(&s);
        s.notation.ornaments.clear();
        assert_eq!(baseline, crate::synth::render_dry(&s));
    }
    #[test]
    fn ornament_generated_sostenuto_reattacks_are_rejected_before_backend_divergence() {
        let (mut s, a) = fixture();
        s.notation.ornaments.push(Ornament::new(a, K::Mordent));
        s.notation.lines.push(crate::lines::PerformanceLine::new(
            a,
            Anchor { tick: 960, ..a },
            LineKind::Pedal {
                kind: PedalKind::Sostenuto,
            },
        ));
        s.validate().unwrap();
        assert!(
            Timeline::new(&s)
                .unwrap_err()
                .contains("captured performed pitch")
        );
        s.notation.ornaments[0].kind = K::Turn;
        assert!(Timeline::new(&s).is_ok());
    }
    #[test]
    fn a_roll_does_not_hide_a_later_ornament_on_its_tied_pitch() {
        let (mut s, a) = fixture();
        let first = &mut s.parts[0].measures[0].notes[0];
        first.pitches = vec![
            Pitch::from_midi(60),
            Pitch::from_midi(64),
            Pitch::from_midi(67),
        ];
        first.tied = true;
        let mut continuation = first.clone();
        continuation.start = 480;
        continuation.pitches = vec![Pitch::from_midi(60)];
        continuation.tied = false;
        s.parts[0].measures[0].notes.push(continuation);
        s.notation.ornaments.push(Ornament::new(a, K::ArpeggioDown));
        s.notation
            .ornaments
            .push(Ornament::new(Anchor { tick: 480, ..a }, K::Mordent));
        let t = Timeline::new(&s).unwrap();
        assert!(
            t.notes
                .iter()
                .any(|n| n.midi == 59 && n.start == 520. && n.end == 560.)
        );
        assert!(t.notes.iter().any(|n| n.midi == 60 && n.start == 80.));
    }
    #[test]
    fn an_ornament_on_a_tied_continuation_uses_current_dynamics_and_technique() {
        let (mut s, a) = fixture();
        s.parts[0].instrument = Instrument::Violin;
        s.parts[0].measures[0].notes[0].tied = true;
        let mut n = s.parts[0].measures[0].notes[0].clone();
        n.start = 480;
        n.tied = false;
        n.velocity = 110;
        s.parts[0].measures[0].notes.push(n);
        let at = Anchor { tick: 480, ..a };
        s.notation.ornaments.push(Ornament::new(at, K::Turn));
        let mut technique = crate::expressions::Expression::text(at, "pizz.");
        technique.kind = ExpressionKind::Technique;
        s.notation.expressions.push(technique);
        let t = Timeline::new(&s).unwrap();
        assert_eq!(t.notes[0].instrument, Instrument::Violin);
        assert!(
            t.notes
                .iter()
                .filter(|n| n.start >= 480.)
                .all(|n| n.velocity == 110 && n.instrument.gm_program() == 45)
        );
        assert_eq!(t.notes.last().unwrap().end, 960.);
    }
}
