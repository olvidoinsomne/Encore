//! Measured repeated attacks over unchanged written entry durations.
use crate::{
    document::Anchor,
    model::{Duration, Note, Score},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_TREMOLOS: usize = 20_000;
fn playback() -> bool {
    true
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tremolo {
    pub anchor: Anchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<Anchor>,
    pub strokes: u8,
    #[serde(default = "playback")]
    pub playback: bool,
    #[serde(default)]
    pub offset: [f32; 2],
}
impl Tremolo {
    pub fn new(anchor: Anchor) -> Self {
        Self {
            anchor,
            end: None,
            strokes: 3,
            playback: true,
            offset: [0.; 2],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=8).contains(&self.strokes)
            || self.anchor.layer > 3
            || self.anchor.voice > 1
            || self.end.is_some_and(|a| a.layer > 3 || a.voice > 1)
            || self.offset.iter().any(|v| !v.is_finite() || v.abs() > 160.)
        {
            return Err("Measured tremolos need 1–8 strokes, a valid voice, and finite offsets within 160 points.".into());
        }
        Ok(())
    }
}
/// Endpoint flag is true only on the second principal of a two-note tremolo.
pub struct Index<'a> {
    marks: BTreeMap<Anchor, (&'a Tremolo, bool)>,
}
impl<'a> Index<'a> {
    pub fn new(score: &'a Score) -> Self {
        let mut marks = BTreeMap::new();
        for mark in &score.notation.tremolos {
            marks.insert(mark.anchor, (mark, false));
            if let Some(end) = mark.end {
                marks.insert(end, (mark, true));
            }
        }
        Self { marks }
    }
    pub fn at(&self, anchor: &Anchor) -> Option<(&'a Tremolo, bool)> {
        self.marks.get(anchor).copied()
    }
    pub fn display_duration(&self, anchor: &Anchor, note: &Note) -> Duration {
        if self.at(anchor).is_some_and(|(m, _)| m.end.is_some()) {
            match note.duration {
                Duration::Quarter => Duration::Half,
                Duration::Half => Duration::Whole,
                _ => note.duration,
            }
        } else {
            note.duration
        }
    }
    pub fn notations(&self, anchor: &Anchor) -> String {
        let Some((mark, end)) = self.at(anchor) else {
            return String::new();
        };
        let kind = if mark.end.is_none() {
            "single"
        } else if end {
            "stop"
        } else {
            "start"
        };
        let mut text = format!(
            "<ornaments><tremolo type=\"{kind}\" relative-x=\"{}\" relative-y=\"{}\">{}</tremolo></ornaments>",
            mark.offset[0], -mark.offset[1], mark.strokes
        );
        if !end {
            text.push_str(&format!("<other-notation type=\"single\" print-object=\"no\">encore-tremolo:{}</other-notation>",crate::export::xml(&serde_json::to_string(mark).expect("validated tremolo"))));
        }
        text
    }
}
/// Extra slashes add to the ordinary beams of short single-note values.
pub fn pulse_ticks(note: &Note, strokes: u8) -> f64 {
    let flags = match note.duration {
        Duration::Eighth => 1,
        Duration::Sixteenth => 2,
        Duration::ThirtySecond => 3,
        Duration::SixtyFourth => 4,
        Duration::OneTwentyEighth => 5,
        _ => 0,
    };
    let tuplet = note
        .tuplet
        .map_or(1., |t| f64::from(t.normal) / f64::from(t.actual));
    480. / 2_f64.powi(i32::from(strokes) + flags) * tuplet
}
pub fn validate(score: &Score) -> Result<(), String> {
    if score.notation.tremolos.is_empty() {
        return Ok(());
    }
    if score.notation.tremolos.len() > MAX_TREMOLOS {
        return Err("Score exceeds the 20000 measured tremolo limit.".into());
    }
    let notes = crate::ornaments::principal_index(score);
    let styles = crate::entry_style::Index::new(score);
    let mut incoming = BTreeSet::new();
    for (a, n) in &notes {
        for pitch in &n.pitches {
            if styles.ties_forward(a, n, pitch.midi()) {
                incoming.insert((
                    a.part,
                    a.lane(),
                    tick(score, *a) + u64::from(n.ticks()),
                    pitch.midi(),
                ));
            }
        }
    }
    let mut seen = BTreeSet::new();
    let graces: BTreeSet<_> = score
        .notation
        .grace
        .iter()
        .map(|g| (g.anchor, g.placement))
        .collect();
    let ornaments: BTreeSet<_> = score
        .notation
        .ornaments
        .iter()
        .filter(|o| o.playback && o.kind != crate::ornaments::OrnamentKind::NonArpeggiate)
        .map(|o| o.anchor)
        .collect();
    let mut lines = BTreeMap::<(usize, u8), Vec<(u64, u64)>>::new();
    for l in &score.notation.lines {
        if matches!(
            l.kind,
            crate::lines::LineKind::Trill { .. } | crate::lines::LineKind::Glissando
        ) {
            lines
                .entry((l.start.part, l.start.lane()))
                .or_default()
                .push((tick(score, l.start), tick(score, l.end)));
        }
    }
    for intervals in lines.values_mut() {
        intervals.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::new();
        for &(a, b) in intervals.iter() {
            if let Some(last) = merged.last_mut().filter(|last| last.1 >= a) {
                last.1 = last.1.max(b);
            } else {
                merged.push((a, b));
            }
        }
        *intervals = merged;
    }
    for mark in &score.notation.tremolos {
        mark.validate()?;
        let first = *notes
            .get(&mark.anchor)
            .ok_or("A tremolo references a missing principal entry.")?;
        if first.pitches.is_empty() {
            return Err("Measured tremolos require sounding entries, not rests.".into());
        }
        if !seen.insert(mark.anchor) {
            return Err("An entry can belong to only one tremolo.".into());
        }
        let mut endtick = tick(score, mark.anchor) + u64::from(first.ticks());
        if let Some(end) = mark.end {
            let second = *notes
                .get(&end)
                .ok_or("A paired tremolo references a missing second entry.")?;
            if end.part != mark.anchor.part
                || end.measure != mark.anchor.measure
                || end.lane() != mark.anchor.lane()
                || mark.anchor.tick.checked_add(first.ticks()) != Some(end.tick)
                || first.duration != second.duration
                || first.dotted
                || second.dotted
                || first.tuplet.is_some()
                || second.tuplet.is_some()
                || !matches!(first.duration, Duration::Quarter | Duration::Half)
                || second.pitches.is_empty()
            {
                return Err("Paired tremolos need consecutive equal undotted, non-tuplet quarters or halves in one staff, bar, layer and voice.".into());
            }
            if !seen.insert(end) {
                return Err("An entry can belong to only one tremolo.".into());
            }
            for (a, n) in [(mark.anchor, first), (end, second)] {
                if n.pitches
                    .iter()
                    .any(|p| styles.ties_forward(&a, n, p.midi()))
                {
                    return Err("Paired tremolo principals cannot have ties.".into());
                }
                // Incoming ties use source identity, including independent chord-pitch ties.
                if n.pitches
                    .iter()
                    .any(|p| incoming.contains(&(a.part, a.lane(), tick(score, a), p.midi())))
                {
                    return Err("Paired tremolo principals cannot receive ties.".into());
                }
            }
            if graces.contains(&(mark.anchor, crate::grace::GracePlacement::After))
                || graces.contains(&(end, crate::grace::GracePlacement::Before))
            {
                return Err("Put grace groups before the first or after the second paired tremolo principal; internal grace windows are ambiguous.".into());
            }
            endtick = tick(score, end) + u64::from(second.ticks());
        }
        if mark.playback {
            if pulse_ticks(first, mark.strokes) < 1. {
                return Err("Played tremolo subdivisions must be at least one tick; reduce strokes or use a longer note value.".into());
            }
            if ornaments.contains(&mark.anchor) || mark.end.is_some_and(|a| ornaments.contains(&a))
            {
                return Err(
                    "A played tremolo cannot share a principal with a played ornament.".into(),
                );
            }
            if lines
                .get(&(mark.anchor.part, mark.anchor.lane()))
                .is_some_and(|v| {
                    let i = v.partition_point(|(_, b)| *b <= tick(score, mark.anchor));
                    v.get(i).is_some_and(|(a, _)| *a < endtick)
                })
            {
                return Err("A played tremolo cannot overlap a trill or glissando line.".into());
            }
        }
    }
    Ok(())
}
fn tick(score: &Score, a: Anchor) -> u64 {
    (a.measure as u64)
        .saturating_mul(u64::from(score.measure_ticks()))
        .saturating_add(u64::from(a.tick))
}
pub fn assign(score: &mut Score, mark: Tremolo) -> Result<(), String> {
    mark.validate()?;
    let mut candidate = score.clone();
    candidate
        .notation
        .tremolos
        .retain(|m| m.anchor != mark.anchor);
    candidate.notation.tremolos.push(mark);
    candidate.validate()?;
    *score = candidate;
    Ok(())
}
pub fn remove_anchor(score: &mut Score, a: &Anchor) {
    score
        .notation
        .tremolos
        .retain(|m| m.anchor != *a && m.end != Some(*a));
}
pub fn remove_part(score: &mut Score, part: usize) {
    score.notation.tremolos.retain(|m| m.anchor.part != part);
}
pub fn reindex_parts(score: &mut Score, removed: usize) {
    for m in &mut score.notation.tremolos {
        if m.anchor.part > removed {
            m.anchor.part -= 1;
        }
        if let Some(a) = &mut m.end
            && a.part > removed
        {
            a.part -= 1;
        }
    }
}
pub fn prune(score: &mut Score) {
    let notes = crate::ornaments::principal_index(score);
    let keep: BTreeSet<_> = score
        .notation
        .tremolos
        .iter()
        .filter(|m| {
            notes.get(&m.anchor).is_some_and(|n| !n.pitches.is_empty())
                && m.end
                    .is_none_or(|a| notes.get(&a).is_some_and(|n| !n.pitches.is_empty()))
        })
        .map(|m| m.anchor)
        .collect();
    score.notation.tremolos.retain(|m| keep.contains(&m.anchor));
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        grace::{GraceGroup, GraceNote, GracePlacement, GraceTiming},
        model::{Pitch, Tuplet},
    };
    pub(crate) fn fixture() -> (Score, Anchor) {
        let mut s = Score::blank();
        s.parts.truncate(1);
        s.parts[0].measures.truncate(1);
        s.parts[0].measures[0].notes = vec![note(0, Duration::Quarter, &[60])];
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        (s, a)
    }
    pub(crate) fn note(start: u32, duration: Duration, pitches: &[u8]) -> Note {
        Note {
            start,
            duration,
            pitches: pitches.iter().map(|p| Pitch::from_midi(*p)).collect(),
            voice: 0,
            voice_launch: None,
            layer: 0,
            dotted: false,
            tuplet: None,
            velocity: 80,
            staccato: false,
            tied: false,
            articulations: vec![],
        }
    }
    #[test]
    fn measured_tremolo_native_roundtrip_defaults_and_display_rhythm() {
        let (mut s, a) = fixture();
        let mut m = Tremolo::new(a);
        m.end = Some(Anchor { tick: 480, ..a });
        s.parts[0].measures[0]
            .notes
            .push(note(480, Duration::Quarter, &[67]));
        s.notation.tremolos.push(m);
        s.validate().unwrap();
        let i = Index::new(&s);
        assert_eq!(
            i.display_duration(&a, &s.parts[0].measures[0].notes[0]),
            Duration::Half
        );
        assert!(i.at(&Anchor { tick: 480, ..a }).unwrap().1);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(s, serde_json::from_str::<Score>(&json).unwrap());
        let old: Tremolo = serde_json::from_str(&format!(
            "{{\"anchor\":{},\"strokes\":2}}",
            serde_json::to_string(&a).unwrap()
        ))
        .unwrap();
        assert!(old.playback);
        assert!(old.end.is_none());
    }
    #[test]
    fn dots_short_note_beams_and_tuplets_set_the_pulse_rate() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes[0].duration = Duration::Eighth;
        s.parts[0].measures[0].notes[0].dotted = true;
        let mut mark = Tremolo::new(a);
        mark.strokes = 1;
        s.notation.tremolos.push(mark);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes.iter().map(|n| n.start).collect::<Vec<_>>(),
            vec![0., 120., 240.]
        );
        assert_eq!(t.notes.last().unwrap().end, 360.);
        s.parts[0].measures[0].notes[0].dotted = false;
        s.parts[0].measures[0].notes[0].tuplet = Some(Tuplet {
            actual: 3,
            normal: 2,
        });
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes.iter().map(|n| n.start).collect::<Vec<_>>(),
            vec![0., 80.]
        );
    }
    #[test]
    fn paired_tremolo_alternates_whole_chords_over_both_stored_entries() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes = vec![
            note(0, Duration::Quarter, &[60, 64]),
            note(480, Duration::Quarter, &[67, 71]),
        ];
        let mut mark = Tremolo::new(a);
        mark.end = Some(Anchor { tick: 480, ..a });
        mark.strokes = 2;
        s.notation.tremolos.push(mark);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(t.notes.len(), 16);
        assert_eq!(
            t.notes
                .iter()
                .filter(|n| n.start == 120.)
                .map(|n| n.midi)
                .collect::<Vec<_>>(),
            vec![67, 71]
        );
        assert_eq!(
            t.notes
                .iter()
                .filter(|n| n.start == 240.)
                .map(|n| n.midi)
                .collect::<Vec<_>>(),
            vec![60, 64]
        );
        assert_eq!(t.notes.last().unwrap().end, 960.);
        assert_eq!(s.parts[0].measures[0].notes[0].duration, Duration::Quarter);
    }
    #[test]
    fn single_roll_breaks_incoming_tie_but_final_pulse_can_tie_out() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes = vec![
            note(0, Duration::Quarter, &[60]),
            note(480, Duration::Quarter, &[60]),
            note(960, Duration::Quarter, &[60]),
        ];
        s.parts[0].measures[0].notes[0].tied = true;
        s.parts[0].measures[0].notes[1].tied = true;
        let mut m = Tremolo::new(Anchor { tick: 480, ..a });
        m.strokes = 1;
        s.notation.tremolos.push(m);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes.iter().map(|n| (n.start, n.end)).collect::<Vec<_>>(),
            vec![(0., 480.), (480., 720.), (720., 1440.)]
        );
    }
    #[test]
    fn grace_windows_are_borrowed_before_single_and_paired_pulses() {
        let (mut s, a) = fixture();
        let mut g = GraceGroup::new(
            a,
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Sixteenth)],
        );
        g.percent = 25.;
        g.timing = GraceTiming::StealMain;
        s.notation.grace.push(g);
        let mut mark = Tremolo::new(a);
        mark.strokes = 2;
        s.notation.tremolos.push(mark);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes
                .iter()
                .map(|n| (n.midi, n.start))
                .collect::<Vec<_>>(),
            vec![(72, 0.), (60, 120.), (60, 240.), (60, 360.)]
        );
        s.parts[0].measures[0]
            .notes
            .push(note(480, Duration::Quarter, &[67]));
        s.notation.tremolos[0].end = Some(Anchor { tick: 480, ..a });
        let mut tail = s.notation.grace[0].clone();
        tail.anchor.tick = 480;
        tail.placement = GracePlacement::After;
        s.notation.grace.push(tail);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(t.notes.iter().find(|n| !n.grace).unwrap().start, 120.);
        assert_eq!(
            t.notes
                .iter()
                .filter(|n| !n.grace)
                .map(|n| n.end)
                .fold(0., f64::max),
            840.
        );
        assert_eq!(t.notes.last().unwrap().start, 840.);
    }
    #[test]
    fn repeated_voice_local_rolls_reach_midi_and_procedural_audio() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes[0].voice = 1;
        let mut m = Tremolo::new(Anchor { voice: 1, ..a });
        m.strokes = 1;
        s.notation.tremolos.push(m);
        s.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 0,
            times: 2,
            endings: vec![],
        });
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes.iter().map(|n| n.start).collect::<Vec<_>>(),
            vec![0., 240., 1920., 2160.]
        );
        assert!(t.notes.iter().all(|n| n.layer == 4));
        let bytes = crate::export::midi(&s).unwrap();
        assert_eq!(bytes.windows(3).filter(|b| *b == [0x94, 60, 80]).count(), 4);
        let samples = crate::audio::AudioEngine::Procedural.render(&s).unwrap();
        assert!(samples.iter().any(|v| v.abs() > 0.001));
        assert!(samples.iter().all(|v| v.is_finite()));
    }
    #[test]
    fn percussion_roll_tokens_map_after_the_shared_pulse_expansion() {
        let mut s = crate::performance::percussion_tests::fixture();
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        let mut m = Tremolo::new(a);
        m.strokes = 2;
        s.notation.tremolos.push(m);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(t.notes.len(), 4);
        assert!(t.notes.iter().all(|n| n.midi == 38));
        let e = crate::performance::drum_events(&t, 0, |v| v.round() as u64);
        assert_eq!(e.iter().filter(|e| e.velocity.is_some()).count(), 4);
        assert_eq!(
            crate::export::midi(&s)
                .unwrap()
                .windows(3)
                .filter(|b| *b == [0x99, 38, 80])
                .count(),
            4
        );
    }
    #[test]
    fn drum_tremolo_hairpin_velocities_follow_each_attack_and_reset_on_repeats() {
        let mut s = crate::performance::percussion_tests::fixture();
        let a = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
            voice: 0,
        };
        s.parts[0].measures[0]
            .notes
            .push(crate::performance::percussion_tests::note(
                480,
                Duration::Quarter,
                7,
                0,
                0,
            ));
        let mut mark = Tremolo::new(a);
        mark.end = Some(Anchor { tick: 480, ..a });
        mark.strokes = 2;
        s.notation.tremolos.push(mark);
        let mut hairpin = crate::expressions::Hairpin::new(a, Anchor { tick: 960, ..a });
        hairpin.start_velocity = Some(40);
        hairpin.end_velocity = Some(120);
        s.notation.spanners.push(hairpin);
        s.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 0,
            times: 2,
            endings: vec![],
        });
        let t = crate::performance::Timeline::new(&s).unwrap();
        let velocities: Vec<_> = t.notes.iter().map(|n| n.velocity).collect();
        assert_eq!(
            velocities,
            [
                vec![40, 50, 60, 70, 80, 90, 100, 110],
                vec![40, 50, 60, 70, 80, 90, 100, 110]
            ]
            .concat()
        );
        assert!(t.controls.iter().all(|c| c.controller != 11));
        assert_eq!(t.notes[1].anchor.tick, 480); // Identity is stable; evaluation time is 120.
        assert_eq!(
            crate::export::midi(&s)
                .unwrap()
                .windows(3)
                .filter(|b| *b == [0x99, 38, 50])
                .count(),
            2
        );
        s.notation.repeats.clear();
        s.notation.tremolos[0].end = None;
        let mut group = GraceGroup::new(
            a,
            vec![GraceNote::new(
                crate::percussion::token(3),
                Duration::Sixteenth,
            )],
        );
        group.percent = 25.;
        group.timing = GraceTiming::StealMain;
        s.notation.grace.push(group);
        let t = crate::performance::Timeline::new(&s).unwrap();
        assert_eq!(
            t.notes
                .iter()
                .filter(|n| !n.grace && n.anchor.tick == 0)
                .map(|n| (n.start, n.velocity))
                .collect::<Vec<_>>(),
            vec![(120., 50), (240., 60), (360., 70)]
        );
    }
    #[test]
    fn malformed_and_ambiguous_tremolos_fail_atomically() {
        let (mut s, a) = fixture();
        let original = s.clone();
        let mut m = Tremolo::new(a);
        m.strokes = 9;
        assert!(assign(&mut s, m).is_err());
        assert_eq!(s, original);
        let mut m = Tremolo::new(a);
        m.end = Some(Anchor { tick: 480, ..a });
        s.parts[0].measures[0]
            .notes
            .push(note(480, Duration::Quarter, &[67]));
        s.notation.tremolos.push(m);
        s.parts[0].measures[0].notes[0].tied = true;
        assert!(s.validate().unwrap_err().contains("ties"));
        s.parts[0].measures[0].notes[0].tied = false;
        let mut g = GraceGroup::new(
            a,
            vec![GraceNote::new(Pitch::from_midi(72), Duration::Eighth)],
        );
        g.placement = GracePlacement::After;
        s.notation.grace.push(g);
        assert!(s.validate().unwrap_err().contains("internal grace"));
        s.notation.grace.clear();
        s.notation.tremolos[0].end = Some(Anchor {
            voice: 1,
            tick: 480,
            ..a
        });
        assert!(s.validate().is_err());
    }
    #[test]
    fn subdivision_and_performed_expansion_are_checked_before_allocation() {
        let (mut s, a) = fixture();
        s.parts[0].measures[0].notes[0].duration = Duration::Eighth;
        let mut m = Tremolo::new(a);
        m.strokes = 8;
        s.notation.tremolos.push(m);
        assert!(s.validate().unwrap_err().contains("at least one tick"));
        s.notation.tremolos[0].playback = false;
        s.validate().unwrap();
        s.notation.tremolos[0].playback = true;
        s.parts[0].measures[0].notes[0].duration = Duration::Whole;
        s.parts[0].measures[0].notes[0].pitches = (48..64).map(Pitch::from_midi).collect();
        s.notation.repeats.push(crate::repeats::RepeatRegion {
            start: 0,
            end: 0,
            times: 16,
            endings: vec![],
        });
        let bar = s.parts[0].measures[0].clone();
        s.parts[0].measures.resize(16, bar);
        for measure in 1..16 {
            s.notation.tremolos.push(Tremolo {
                anchor: Anchor { measure, ..a },
                end: None,
                strokes: 8,
                playback: true,
                offset: [0.; 2],
            });
        }
        s.notation.repeats[0].end = 15;
        assert!(
            crate::performance::Timeline::new(&s)
                .unwrap_err()
                .contains("two million")
        );
    }
    #[test]
    fn disabled_roll_preserves_existing_procedural_samples() {
        let (mut s, a) = fixture();
        let before = crate::synth::render_dry(&s);
        let mut m = Tremolo::new(a);
        m.playback = false;
        s.notation.tremolos.push(m);
        assert!(!crate::performance::needed(&s));
        assert_eq!(before, crate::synth::render_dry(&s));
    }
}
