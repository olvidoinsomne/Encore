//! Grace collectors use source anchors, then resolve after voice/staff mapping.
use super::annotations::SourceAnchor;
use super::{Warnings, child, number, value};
use crate::document::Anchor;
use crate::entry_style::BeamStyle;
use crate::grace::{GraceGroup, GraceNote, GracePlacement, GraceTiming};
use crate::model::{Duration, Pitch, Score};
use roxmltree::Node;
use std::collections::{BTreeMap, BTreeSet};

struct Pending {
    at: SourceAnchor,
    group: GraceGroup,
    slurs: BTreeSet<String>,
    own: bool,
}
#[derive(Default)]
pub(super) struct GraceCollector {
    pending: BTreeMap<(u32, String, GracePlacement), Pending>,
    last: BTreeMap<(u32, String), SourceAnchor>,
    groups: Vec<Pending>,
    after: BTreeMap<(u32, String), usize>,
    count: usize,
}
fn dummy(at: &SourceAnchor) -> Anchor {
    Anchor {
        part: 0,
        measure: at.measure,
        tick: at.tick,
        layer: 0,
    }
}
fn metadata(node: Node<'_, '_>) -> Result<Option<GraceGroup>, String> {
    for element in node
        .descendants()
        .filter(|n| n.has_tag_name("other-notation"))
    {
        if let Some(json) = element.text().unwrap_or("").strip_prefix("encore-grace:") {
            if json.len() > 64 * 1024 {
                return Err("Grace metadata exceeds the import limit.".into());
            }
            let group: GraceGroup = serde_json::from_str(json)
                .map_err(|e| format!("Invalid Encore grace metadata: {e}"))?;
            group.validate()?;
            return Ok(Some(group));
        }
    }
    Ok(None)
}
impl GraceCollector {
    pub(super) fn xml(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        pitch: Pitch,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        let grace = child(node, "grace").ok_or("Missing MusicXML grace element.")?;
        if grace.attribute("make-time").is_some() {
            return Err("MusicXML grace make-time inserts global time and is not supported; use steal-time grace notes.".into());
        }
        let duration = value(node, "type")
            .and_then(Duration::from_musicxml_type)
            .unwrap_or(Duration::Eighth);
        let dots = node.children().filter(|n| n.has_tag_name("dot")).count();
        if dots > 1 {
            return Err("Grace notes support at most one augmentation dot.".into());
        }
        let mut note = GraceNote::new(pitch, duration);
        note.dotted = dots == 1;
        if let Some(beam) = node
            .children()
            .find(|n| n.has_tag_name("beam") && n.attribute("number").unwrap_or("1") == "1")
        {
            note.beam = match beam.text().unwrap_or("").trim() {
                "begin" => BeamStyle::Break,
                "continue" | "end" => BeamStyle::Join,
                _ => BeamStyle::Auto,
            };
        }
        let own = metadata(node)?;
        let mut group = own
            .clone()
            .unwrap_or_else(|| GraceGroup::new(dummy(&at), Vec::new()));
        group.notes.clear();
        if own.is_none() {
            group.slashed = grace.attribute("slash") == Some("yes");
            let previous = grace.attribute("steal-time-previous");
            let following = grace.attribute("steal-time-following");
            if previous.is_some() && following.is_some() {
                return Err(
                    "A grace group borrowing both neighboring notes is not supported.".into(),
                );
            }
            group.timing = if following.is_some() {
                GraceTiming::StealMain
            } else {
                GraceTiming::StealPrevious
            };
            if let Some(percent) = previous.or(following) {
                group.percent = percent
                    .parse()
                    .map_err(|_| "Invalid grace steal-time percentage.")?;
            }
        }
        for element in node
            .descendants()
            .filter(|n| n.has_tag_name("other-notation"))
        {
            let text = element.text().unwrap_or("");
            if text == "encore-grace-side:after" {
                group.placement = GracePlacement::After;
                group.timing = GraceTiming::StealMain;
            }
            if let Some(json) = text.strip_prefix("encore-grace-note:") {
                if json.len() > 4096 {
                    return Err("Grace-note metadata exceeds the import limit.".into());
                }
                let native: GraceNote = serde_json::from_str(json)
                    .map_err(|e| format!("Invalid Encore grace-note metadata: {e}"))?;
                note.beam = native.beam;
                note.velocity = native.velocity;
            }
        }
        let chord = child(node, "chord").is_some();
        if !chord
            && own.is_none()
            && (grace.attribute("steal-time-previous").is_some()
                || grace.attribute("steal-time-following").is_some())
            && let Some(raw) =
                self.pending
                    .get_mut(&(at.staff, at.voice.clone(), GracePlacement::Before))
            && !raw.own
        {
            if raw.group.timing != group.timing {
                return Err(
                    "A grace group mixing previous- and following-note donors is not supported."
                        .into(),
                );
            }
            raw.group.percent += group.percent;
        }
        let starts = node
            .descendants()
            .filter(|n| n.has_tag_name("slur") && n.attribute("type") == Some("start"))
            .map(|n| n.attribute("number").unwrap_or("1").to_owned())
            .collect();
        if node.descendants().any(|n| {
            n.has_tag_name("lyric")
                || n.has_tag_name("time-modification")
                || n.has_tag_name("tie")
                || n.has_tag_name("articulations")
        }) {
            warnings.add("Grace-note lyrics, tuplets, ties, and articulations are not retained; pitches and visual note values are preserved.");
        }
        self.push(at, group, note, chord, starts, own.is_some())
    }
    fn push(
        &mut self,
        at: SourceAnchor,
        group: GraceGroup,
        note: GraceNote,
        chord: bool,
        slurs: BTreeSet<String>,
        own: bool,
    ) -> Result<(), String> {
        self.count += usize::from(!chord);
        if self.count > crate::grace::MAX_GRACE_NOTES {
            return Err("Imported grace-note count exceeds the document limit.".into());
        }
        let key = (at.staff, at.voice.clone());
        if group.placement == GracePlacement::After {
            let prior = self
                .last
                .get(&key)
                .cloned()
                .ok_or("An after-grace group has no preceding principal note.")?;
            if let Some(raw) = self.after.get(&key).and_then(|i| self.groups.get_mut(*i)) {
                if chord {
                    raw.group
                        .notes
                        .last_mut()
                        .ok_or("Grace chord has no preceding note.")?
                        .pitches
                        .extend(note.pitches);
                } else {
                    raw.group.notes.push(note);
                }
                return check_pending(raw);
            }
            if chord {
                return Err("Grace chord has no preceding note.".into());
            }
            let mut group = group;
            group.notes.push(note);
            let raw = Pending {
                at: prior,
                group,
                slurs,
                own,
            };
            check_pending(&raw)?;
            self.after.insert(key, self.groups.len());
            self.groups.push(raw);
            return Ok(());
        }
        self.push_waiting(at, group, note, chord, slurs, own)
    }
    fn push_waiting(
        &mut self,
        at: SourceAnchor,
        group: GraceGroup,
        note: GraceNote,
        chord: bool,
        slurs: BTreeSet<String>,
        own: bool,
    ) -> Result<(), String> {
        let key = (at.staff, at.voice.clone(), group.placement);
        if let Some(raw) = self.pending.get_mut(&key) {
            if chord {
                raw.group
                    .notes
                    .last_mut()
                    .ok_or("Grace chord has no preceding note.")?
                    .pitches
                    .extend(note.pitches);
            } else {
                raw.group.notes.push(note);
            }
            raw.slurs.extend(slurs);
            check_pending(raw)
        } else {
            if chord {
                return Err("Grace chord has no preceding note.".into());
            }
            let mut group = group;
            group.notes.push(note);
            let raw = Pending {
                at,
                group,
                slurs,
                own,
            };
            check_pending(&raw)?;
            self.pending.insert(key, raw);
            Ok(())
        }
    }
    pub(super) fn muse(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        pitches: Vec<Pitch>,
        warnings: &mut Warnings,
    ) -> Result<bool, String> {
        let kind = node.children().find_map(|n| {
            matches!(
                n.tag_name().name(),
                "acciaccatura"
                    | "appoggiatura"
                    | "grace4"
                    | "grace16"
                    | "grace32"
                    | "grace8after"
                    | "grace16after"
                    | "grace32after"
            )
            .then(|| n.tag_name().name())
        });
        let Some(kind) = kind else {
            return Ok(false);
        };
        let duration = match kind {
            "grace4" => Duration::Quarter,
            "grace16" | "grace16after" => Duration::Sixteenth,
            "grace32" | "grace32after" => Duration::ThirtySecond,
            _ => Duration::Eighth,
        };
        let mut group = GraceGroup::new(dummy(&at), Vec::new());
        group.slashed = kind == "acciaccatura";
        if kind.ends_with("after") {
            group.placement = GracePlacement::After;
            group.timing = GraceTiming::StealMain;
        }
        let dots = number::<u8>(node, "dots")?.unwrap_or(0);
        if dots > 1 {
            return Err("Grace notes support at most one dot.".into());
        }
        let note = GraceNote {
            pitches,
            duration,
            dotted: dots == 1,
            velocity: None,
            beam: match value(node, "BeamMode").or_else(|| value(node, "beamMode")) {
                Some("begin") | Some("1") => BeamStyle::Break,
                Some("mid") | Some("2") => BeamStyle::Join,
                _ => BeamStyle::Auto,
            },
        };
        if node
            .descendants()
            .any(|n| n.has_tag_name("Slur") || n.has_tag_name("Spanner"))
        {
            warnings.add("MuseScore grace-note spanners are re-engraved without their custom endpoints; grace groups remain editable.");
        }
        if group.placement == GracePlacement::After {
            self.count += 1;
            if self.count > crate::grace::MAX_GRACE_NOTES {
                return Err("Imported grace-note count exceeds the document limit.".into());
            }
            self.push_waiting(at, group, note, false, BTreeSet::new(), false)?;
        } else {
            self.push(at, group, note, false, BTreeSet::new(), false)?;
        }
        Ok(true)
    }
    /// Called for a pitched principal onset, once per chord rather than per tone.
    pub(super) fn main(
        &mut self,
        node: Node<'_, '_>,
        at: SourceAnchor,
        _warnings: &mut Warnings,
    ) -> Result<BTreeSet<String>, String> {
        let mut consumed = BTreeSet::new();
        for element in node
            .descendants()
            .filter(|n| n.has_tag_name("other-notation"))
        {
            if let Some(numbers) = element
                .text()
                .unwrap_or("")
                .strip_prefix("encore-grace-connectors:")
            {
                consumed.extend(numbers.split(',').map(str::to_owned));
            }
        }
        let key = (at.staff, at.voice.clone());
        self.after.remove(&key);
        for placement in GracePlacement::ALL {
            if let Some(mut raw) = self.pending.remove(&(key.0, key.1.clone(), placement)) {
                for slur in node
                    .descendants()
                    .filter(|n| n.has_tag_name("slur") && n.attribute("type") == Some("stop"))
                {
                    let number = slur.attribute("number").unwrap_or("1");
                    if raw.slurs.contains(number) {
                        raw.group.slur_to_main = true;
                        consumed.insert(number.to_owned());
                    }
                }
                if placement == GracePlacement::After {
                    raw.group.notes.reverse();
                }
                raw.at = at.clone();
                self.groups.push(raw);
            }
        }
        self.last.insert(key, at);
        Ok(consumed)
    }
    pub(super) fn shift(&mut self, measure: usize, ticks: u32) {
        for raw in self.groups.iter_mut().chain(self.pending.values_mut()) {
            if raw.at.measure == measure {
                raw.at.tick = raw.at.tick.saturating_add(ticks);
            }
        }
        for at in self.last.values_mut() {
            if at.measure == measure {
                at.tick = at.tick.saturating_add(ticks);
            }
        }
    }
    pub(super) fn finish(
        self,
        score: &mut Score,
        mapping: &BTreeMap<(u32, String), (usize, u8)>,
        warnings: &mut Warnings,
    ) -> Result<(), String> {
        if !self.pending.is_empty() {
            warnings.add("Grace notes with no following principal entry were omitted; explicit after-grace groups are retained.");
        }
        let mt = score.measure_ticks();
        for raw in self.groups {
            let Some(&(part, layer)) = mapping.get(&(raw.at.staff, raw.at.voice.clone())) else {
                return Err("Grace notes refer to an unknown source voice.".into());
            };
            let mut group = raw.group;
            group.anchor = Anchor {
                part,
                layer,
                measure: raw.at.measure + raw.at.tick as usize / mt as usize,
                tick: raw.at.tick % mt,
            };
            if raw.own && !group.slur_to_main && !raw.slurs.is_empty() {
                warnings.add("A grace slur to an unsupported endpoint was omitted.");
            }
            crate::grace::assign(score, group)?;
        }
        crate::grace::validate(score)
    }
}
fn check_pending(raw: &Pending) -> Result<(), String> {
    raw.group.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn note(start: u32, midi: u8) -> crate::model::Note {
        crate::model::Note {
            start,
            layer: 0,
            duration: Duration::Quarter,
            dotted: false,
            tuplet: None,
            pitches: vec![Pitch::from_midi(midi)],
            velocity: 80,
            staccato: false,
            articulations: Vec::new(),
            tied: false,
        }
    }
    #[test]
    fn grace_musicxml_roundtrip_preserves_both_sides_chords_and_native_options() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        score.parts[0].measures.truncate(1);
        score.parts[0].measures[0].notes = vec![note(0, 60), note(480, 62)];
        let anchor = Anchor {
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let mut before = GraceGroup::new(
            anchor,
            vec![
                GraceNote::new(Pitch::from_midi(61), Duration::Sixteenth),
                GraceNote::new(Pitch::from_midi(63), Duration::OneTwentyEighth),
            ],
        );
        before.notes[1].dotted = true;
        before.notes[0].pitches.push(Pitch::from_midi(65));
        before.notes[0].velocity = Some(47);
        before.notes[1].beam = BeamStyle::Flat;
        before.scale = 0.7;
        before.percent = 17.5;
        before.slashed = true;
        before.slur_to_main = true;
        before.timing = GraceTiming::StealMain;
        let mut after = before.clone();
        after.placement = GracePlacement::After;
        after.notes.reverse();
        crate::grace::assign(&mut score, before).unwrap();
        crate::grace::assign(&mut score, after).unwrap();
        score.validate().unwrap();
        let xml = crate::export::musicxml(&score);
        let document = super::super::parse_xml(&xml).unwrap();
        let grace_notes = document
            .descendants()
            .filter(|n| n.has_tag_name("note") && child(*n, "grace").is_some())
            .collect::<Vec<_>>();
        assert!(
            grace_notes.iter().all(|n| n
                .children()
                .find(|c| c.is_element())
                .is_some_and(|c| c.has_tag_name("grace"))),
            "MusicXML requires grace before an optional chord marker"
        );
        let borrowed = grace_notes
            .iter()
            .filter(|n| child(**n, "chord").is_none())
            .take(2)
            .map(|n| {
                child(*n, "grace")
                    .unwrap()
                    .attribute("steal-time-following")
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
            })
            .sum::<f64>();
        assert!(
            (borrowed - 17.5).abs() < 0.000001,
            "per-note weights must sum to the group window"
        );
        let imported = super::super::musicxml::parse(&document).unwrap();
        assert_eq!(imported.score.notation.grace, score.notation.grace);
        assert_eq!(
            imported.score.parts[0].measures[0]
                .notes
                .iter()
                .filter(|n| !n.pitches.is_empty())
                .collect::<Vec<_>>(),
            score.parts[0].measures[0].notes.iter().collect::<Vec<_>>()
        );
        assert!(
            !imported
                .warnings
                .iter()
                .any(|s| s.contains("slur stop without")),
            "{:?}",
            imported.warnings
        );
    }
    #[test]
    fn musescore_before_and_after_groups_share_following_principal_and_pickup_shift() {
        // MuseScore serializes both sides ahead of their principal chord;
        // after-grace notes are stored in reverse visual order.
        let xml = "<museScore version='3.6'><Score><Staff id='1'><Measure len='1/4'><voice><TimeSig><sigN>4</sigN><sigD>4</sigD></TimeSig><Chord><acciaccatura/><Note><pitch>61</pitch></Note></Chord><Chord><grace16after/><Note><pitch>65</pitch></Note></Chord><Chord><grace16after/><Note><pitch>64</pitch></Note></Chord><Chord><durationType>quarter</durationType><Note><pitch>60</pitch></Note></Chord></voice></Measure></Staff></Score></museScore>";
        let document = super::super::parse_xml(xml).unwrap();
        let imported = super::super::musescore::parse(&document).unwrap();
        assert_eq!(imported.score.notation.grace.len(), 2);
        let before = crate::grace::at(
            &imported.score,
            &Anchor {
                part: 0,
                measure: 0,
                tick: 1440,
                layer: 0,
            },
            GracePlacement::Before,
        )
        .unwrap();
        assert!(before.slashed);
        let after =
            crate::grace::at(&imported.score, &before.anchor, GracePlacement::After).unwrap();
        assert_eq!(
            after
                .notes
                .iter()
                .map(|n| n.pitches[0].midi())
                .collect::<Vec<_>>(),
            vec![64, 65]
        );
        assert_eq!(imported.score.parts[0].measures[0].notes[0].start, 1440);
    }
    #[test]
    fn make_time_is_explicitly_rejected_instead_of_shifting_written_events() {
        let document = roxmltree::Document::parse(
            "<note><grace make-time='120'/><pitch><step>C</step><octave>4</octave></pitch></note>",
        )
        .unwrap();
        let mut c = GraceCollector::default();
        let error = c
            .xml(
                document.root_element(),
                SourceAnchor {
                    staff: 1,
                    voice: "1".into(),
                    measure: 0,
                    tick: 0,
                },
                Pitch::from_midi(60),
                &mut Warnings::default(),
            )
            .unwrap_err();
        assert!(error.contains("make-time"));
    }
    #[test]
    fn foreign_grace_percentages_are_added_without_consuming_score_time() {
        let xml = "<score-partwise><part-list><score-part id='P1'><part-name>Piano</part-name><midi-instrument><midi-program>1</midi-program></midi-instrument></score-part></part-list><part id='P1'><measure><attributes><divisions>480</divisions></attributes><note><grace steal-time-following='10'/><pitch><step>D</step><octave>4</octave></pitch><type>16th</type></note><note><grace steal-time-following='20'/><pitch><step>E</step><octave>4</octave></pitch><type>eighth</type></note><note><pitch><step>C</step><octave>4</octave></pitch><duration>480</duration><type>quarter</type></note></measure></part></score-partwise>";
        let document = super::super::parse_xml(xml).unwrap();
        let imported = super::super::musicxml::parse(&document).unwrap();
        let g = &imported.score.notation.grace[0];
        assert_eq!(g.percent, 30.0);
        assert_eq!(g.timing, GraceTiming::StealMain);
        assert_eq!(g.anchor.tick, 0);
        assert_eq!(imported.score.parts[0].measures[0].notes[0].start, 0);
    }
}
