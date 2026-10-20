//! Conventional, measure-boundary navigation. Targets are deliberately unique.
use super::MAX_PERFORMED_MEASURES;
use crate::model::Score;
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NavigationKind {
    Segno,
    Coda,
    Fine,
    ToCoda,
    DaCapo,
    DaCapoAlFine,
    DaCapoAlCoda,
    DalSegno,
    DalSegnoAlFine,
    DalSegnoAlCoda,
}
impl NavigationKind {
    pub const ALL: [Self; 10] = [
        Self::Segno,
        Self::Coda,
        Self::Fine,
        Self::ToCoda,
        Self::DaCapo,
        Self::DaCapoAlFine,
        Self::DaCapoAlCoda,
        Self::DalSegno,
        Self::DalSegnoAlFine,
        Self::DalSegnoAlCoda,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Segno => "Segno",
            Self::Coda => "Coda",
            Self::Fine => "Fine",
            Self::ToCoda => "To Coda",
            Self::DaCapo => "D.C.",
            Self::DaCapoAlFine => "D.C. al Fine",
            Self::DaCapoAlCoda => "D.C. al Coda",
            Self::DalSegno => "D.S.",
            Self::DalSegnoAlFine => "D.S. al Fine",
            Self::DalSegnoAlCoda => "D.S. al Coda",
        }
    }
    pub fn is_jump(self) -> bool {
        matches!(
            self,
            Self::DaCapo
                | Self::DaCapoAlFine
                | Self::DaCapoAlCoda
                | Self::DalSegno
                | Self::DalSegnoAlFine
                | Self::DalSegnoAlCoda
        )
    }
    pub fn is_start(self) -> bool {
        matches!(self, Self::Segno | Self::Coda)
    }
    fn is_segno_jump(self) -> bool {
        matches!(
            self,
            Self::DalSegno | Self::DalSegnoAlFine | Self::DalSegnoAlCoda
        )
    }
    fn is_fine(self) -> bool {
        matches!(self, Self::DaCapoAlFine | Self::DalSegnoAlFine)
    }
    fn is_coda(self) -> bool {
        matches!(self, Self::DaCapoAlCoda | Self::DalSegnoAlCoda)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigationMark {
    pub measure: usize,
    pub kind: NavigationKind,
    #[serde(default)]
    pub play_repeats_after_jump: bool,
}

pub(super) fn validate(score: &Score) -> Result<(), String> {
    use NavigationKind::*;
    let marks = &score.notation.navigation;
    if marks.len() > 128 {
        return Err("A score supports at most 128 navigation marks.".into());
    }
    let mut occupied = BTreeSet::new();
    let mut repeat_setting = None;
    for mark in marks {
        if mark.measure >= score.measure_count() {
            return Err("A navigation mark extends outside the score.".into());
        }
        if !occupied.insert((mark.measure, mark.kind)) {
            return Err("Duplicate navigation mark.".into());
        }
        if mark.kind.is_jump() {
            if marks.iter().any(|other| {
                other.measure == mark.measure && other.kind != mark.kind && other.kind.is_jump()
            }) {
                return Err("Only one navigation jump may occur at a barline.".into());
            }
            if repeat_setting.is_some_and(|setting| setting != mark.play_repeats_after_jump) {
                return Err(
                    "Navigation jumps in one score must share the same play-repeats setting."
                        .into(),
                );
            }
            repeat_setting = Some(mark.play_repeats_after_jump);
        }
    }
    for kind in [Segno, Coda, Fine, ToCoda] {
        if marks.iter().filter(|mark| mark.kind == kind).count() > 1 {
            return Err(format!(
                "Use one unambiguous {} target per score.",
                kind.name()
            ));
        }
    }
    let target = |kind| {
        marks
            .iter()
            .find(|mark| mark.kind == kind)
            .map(|mark| mark.measure)
    };
    for jump in marks.iter().filter(|mark| mark.kind.is_jump()) {
        let start = if jump.kind.is_segno_jump() {
            target(Segno).ok_or("D.S. needs a segno target.")?
        } else {
            0
        };
        if start > jump.measure {
            return Err("A D.S. target must precede its jump.".into());
        }
        if jump.kind.is_fine() {
            let fine = target(Fine).ok_or("An al Fine jump needs a Fine mark.")?;
            if fine < start || fine >= jump.measure {
                return Err("Fine must fall between the jump target and its jump barline.".into());
            }
        }
        if jump.kind.is_coda() {
            let to = target(ToCoda).ok_or("An al Coda jump needs a To Coda mark.")?;
            let coda = target(Coda).ok_or("An al Coda jump needs a coda target.")?;
            if to < start || to >= jump.measure || coda <= to {
                return Err("To Coda must lie on the return passage before its jump, with Coda later in the score.".into());
            }
        }
    }
    Ok(())
}

pub(super) fn perform(
    score: &Score,
    normal: &[usize],
    final_pass: &[usize],
) -> Result<Vec<usize>, String> {
    use NavigationKind::*;
    if score.notation.navigation.is_empty() {
        return Ok(normal.to_vec());
    }
    let marks = &score.notation.navigation;
    let target = |kind| {
        marks
            .iter()
            .find(|mark| mark.kind == kind)
            .map(|mark| mark.measure)
    };
    let mut stream = normal;
    let mut cursor = 0;
    let mut output = Vec::new();
    let mut jumps = BTreeSet::new();
    let mut active = None;
    let mut coda_taken = false;
    while let Some(&bar) = stream.get(cursor) {
        if output.len() >= MAX_PERFORMED_MEASURES {
            return Err(format!(
                "Navigation exceeds the {MAX_PERFORMED_MEASURES} performed-measure limit."
            ));
        }
        output.push(bar);
        if active.is_some_and(NavigationKind::is_fine) && target(Fine) == Some(bar) {
            active = None;
            break;
        }
        if active.is_some_and(NavigationKind::is_coda) && !coda_taken && target(ToCoda) == Some(bar)
        {
            let coda = target(Coda).ok_or("Missing coda target.")?;
            cursor = stream
                .iter()
                .position(|source| *source == coda)
                .ok_or("The coda target is skipped by the selected final ending.")?;
            coda_taken = true;
            active = None;
            continue;
        }
        if let Some((index, jump)) = marks.iter().enumerate().find(|(index, mark)| {
            mark.measure == bar && mark.kind.is_jump() && !jumps.contains(index)
        }) {
            jumps.insert(index);
            active = Some(jump.kind);
            coda_taken = false;
            stream = if jump.play_repeats_after_jump {
                normal
            } else {
                final_pass
            };
            let destination = if jump.kind.is_segno_jump() {
                target(Segno).ok_or("Missing segno target.")?
            } else {
                0
            };
            cursor = stream
                .iter()
                .position(|source| *source == destination)
                .ok_or("The navigation target is skipped by the selected final ending.")?;
            continue;
        }
        cursor += 1;
    }
    if active.is_some_and(NavigationKind::is_fine) {
        return Err("The selected return route never reaches Fine.".into());
    }
    if active.is_some_and(NavigationKind::is_coda) && !coda_taken {
        return Err("The selected return route never reaches To Coda.".into());
    }
    Ok(output)
}

pub fn musicxml_navigation_before_measure(score: &Score, measure: usize) -> String {
    musicxml_marks(score, measure, true)
}
pub fn musicxml_navigation_after_measure(score: &Score, measure: usize) -> String {
    musicxml_marks(score, measure, false)
}
fn musicxml_marks(score: &Score, measure: usize, start: bool) -> String {
    use NavigationKind::*;
    let mut out = String::new();
    for mark in score
        .notation
        .navigation
        .iter()
        .filter(|mark| mark.measure == measure && mark.kind.is_start() == start)
    {
        let (glyph, sound): (String, String) = match mark.kind {
            Segno => ("<segno/>".into(), "segno=\"encore-segno\"".into()),
            Coda => ("<coda/>".into(), "coda=\"encore-coda\"".into()),
            Fine => ("<words>Fine</words>".into(), "fine=\"yes\"".into()),
            ToCoda => (
                "<words>To Coda</words>".into(),
                "tocoda=\"encore-coda\"".into(),
            ),
            kind => (
                format!(
                    "<words>{}{}</words>",
                    kind.name(),
                    if mark.play_repeats_after_jump {
                        " (with repeats)"
                    } else {
                        ""
                    }
                ),
                if kind.is_segno_jump() {
                    "dalsegno=\"encore-segno\"".into()
                } else {
                    "dacapo=\"yes\"".into()
                },
            ),
        };
        out.push_str(&format!("      <direction placement=\"above\"><direction-type>{glyph}</direction-type><sound {sound}/></direction>\n"));
    }
    out
}

pub fn parse_musicxml_navigation(
    part: Node<'_, '_>,
    measure_count: usize,
) -> Result<Vec<NavigationMark>, String> {
    use NavigationKind::*;
    let repeat_settings: BTreeSet<_> = part
        .descendants()
        .filter(|n| n.has_tag_name("repeat") && n.attribute("direction") == Some("backward"))
        .filter_map(|n| n.attribute("after-jump"))
        .collect();
    if repeat_settings
        .iter()
        .any(|setting| !matches!(*setting, "yes" | "no"))
        || repeat_settings.len() > 1
    {
        return Err(
            "Mixed repeat-after-jump settings cannot be represented as one navigation route."
                .into(),
        );
    }
    let play = repeat_settings.contains("yes");
    let mut marks = Vec::new();
    let mut implicit_purpose_bars = BTreeSet::new();
    let mut segno_labels = BTreeSet::new();
    let mut coda_labels = BTreeSet::new();
    let mut segno_jumps = BTreeSet::new();
    let mut coda_jumps = BTreeSet::new();
    for (bar, measure) in part
        .children()
        .filter(|n| n.has_tag_name("measure"))
        .enumerate()
    {
        if bar >= measure_count {
            return Err("Navigation extends outside the score.".into());
        }
        for sound in measure.descendants().filter(|n| n.has_tag_name("sound")) {
            let has_nav = ["segno", "coda", "fine", "tocoda", "dacapo", "dalsegno"]
                .iter()
                .any(|name| sound.attribute(*name).is_some());
            if !has_nav {
                continue;
            }
            let direction = sound.parent().filter(|n| n.has_tag_name("direction"));
            if sound.attribute("time-only").is_some()
                || sound.children().any(|n| {
                    n.has_tag_name("offset") && n.text().is_some_and(|text| text.trim() != "0")
                })
                || direction.is_some_and(|direction| {
                    direction.children().any(|n| {
                        n.has_tag_name("offset") && n.text().is_some_and(|text| text.trim() != "0")
                    })
                })
            {
                return Err(
                    "Navigation with custom passes or offsets must be expanded before importing."
                        .into(),
                );
            }
            let words = direction
                .map(|d| {
                    d.descendants()
                        .filter(|n| n.has_tag_name("words"))
                        .filter_map(|n| n.text())
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                })
                .unwrap_or_default();
            let mut push = |kind| {
                marks.push(NavigationMark {
                    measure: bar,
                    kind,
                    play_repeats_after_jump: kind.is_jump()
                        && (play || words.contains("with repeats")),
                })
            };
            if let Some(label) = sound.attribute("segno") {
                segno_labels.insert(label.to_owned());
                push(Segno);
            }
            if let Some(label) = sound.attribute("coda") {
                coda_labels.insert(label.to_owned());
                push(Coda);
            }
            if sound.attribute("fine").is_some() {
                push(Fine);
            }
            if let Some(label) = sound.attribute("tocoda") {
                coda_jumps.insert(label.to_owned());
                push(ToCoda);
            }
            if sound.attribute("dacapo").is_some() || sound.attribute("dalsegno").is_some() {
                let segno = sound.attribute("dalsegno");
                if sound
                    .attribute("dacapo")
                    .is_some_and(|value| value != "yes")
                    || segno.is_some() && sound.attribute("dacapo").is_some()
                {
                    return Err("Invalid or ambiguous MusicXML navigation jump.".into());
                }
                if let Some(label) = segno {
                    segno_jumps.insert(label.to_owned());
                }
                let fine = words.contains("fine");
                let coda = words.contains("coda");
                if fine && coda {
                    return Err(
                        "A navigation jump cannot target Fine and Coda simultaneously.".into(),
                    );
                }
                if !["d.c.", "d.s.", "da capo", "dal segno"]
                    .iter()
                    .any(|name| words.contains(name))
                {
                    implicit_purpose_bars.insert(bar);
                }
                push(match (segno.is_some(), fine, coda) {
                    (false, true, _) => DaCapoAlFine,
                    (false, _, true) => DaCapoAlCoda,
                    (false, _, _) => DaCapo,
                    (true, true, _) => DalSegnoAlFine,
                    (true, _, true) => DalSegnoAlCoda,
                    (true, _, _) => DalSegno,
                });
            }
        }
    }
    if segno_labels.len() > 1
        || coda_labels.len() > 1
        || segno_jumps
            .iter()
            .any(|label| !segno_labels.contains(label))
        || coda_jumps.iter().any(|label| !coda_labels.contains(label))
    {
        return Err("Navigation labels require one matching segno and coda target.".into());
    }
    // Some exporters encode the jump purpose only through sound targets.
    let fine = marks.iter().any(|mark| mark.kind == Fine);
    let coda = marks.iter().any(|mark| mark.kind == ToCoda);
    if fine && coda && !implicit_purpose_bars.is_empty() {
        return Err("A navigation jump with both Fine and Coda needs an explicit al Fine or al Coda instruction.".into());
    }
    if !fine || !coda {
        for mark in marks
            .iter_mut()
            .filter(|mark| implicit_purpose_bars.contains(&mark.measure))
        {
            mark.kind = match (mark.kind, fine, coda) {
                (DaCapo, true, false) => DaCapoAlFine,
                (DalSegno, true, false) => DalSegnoAlFine,
                (DaCapo, false, true) => DaCapoAlCoda,
                (DalSegno, false, true) => DalSegnoAlCoda,
                (kind, _, _) => kind,
            };
        }
    }
    marks.sort_by_key(|mark| (mark.measure, mark.kind));
    marks.dedup();
    Ok(marks)
}

pub fn parse_musescore_navigation(
    staff: Node<'_, '_>,
    measure_count: usize,
) -> Result<Vec<NavigationMark>, String> {
    use NavigationKind::*;
    let mut marks = Vec::new();
    let mut targets: BTreeMap<String, NavigationKind> = BTreeMap::new();
    let measures: Vec<_> = staff
        .children()
        .filter(|n| {
            n.has_tag_name("Measure") && !n.children().any(|n| n.has_tag_name("multiMeasureRest"))
        })
        .collect();
    let field = |node: Node<'_, '_>, name: &str| {
        node.children()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    for (bar, measure) in measures.iter().enumerate() {
        if bar >= measure_count {
            return Err("Navigation extends outside the score.".into());
        }
        for jump in measure.descendants().filter(|n| n.has_tag_name("Jump")) {
            let to = field(jump, "jumpTo");
            let until = field(jump, "playUntil");
            let continue_at = field(jump, "continueAt");
            if to.is_empty() {
                return Err("MuseScore jump is missing its target.".into());
            }
            let ds = to != "start";
            if ds {
                targets.insert(to, Segno);
            }
            let ending = if !continue_at.is_empty() && continue_at != "end" {
                targets.insert(until, ToCoda);
                targets.insert(continue_at, Coda);
                2
            } else if until != "end" && !until.is_empty() {
                targets.insert(until, Fine);
                1
            } else {
                0
            };
            let kind = match (ds, ending) {
                (false, 1) => DaCapoAlFine,
                (false, 2) => DaCapoAlCoda,
                (false, _) => DaCapo,
                (true, 1) => DalSegnoAlFine,
                (true, 2) => DalSegnoAlCoda,
                (true, _) => DalSegno,
            };
            let play = field(jump, "playRepeats");
            if !play.is_empty() && !matches!(play.as_str(), "0" | "1") {
                return Err("Invalid MuseScore play-repeats setting.".into());
            }
            marks.push(NavigationMark {
                measure: bar,
                kind,
                play_repeats_after_jump: play == "1",
            });
        }
    }
    for (bar, measure) in measures.iter().enumerate() {
        for marker in measure.descendants().filter(|n| n.has_tag_name("Marker")) {
            let label = field(marker, "label");
            let kind = targets
                .get(&label)
                .copied()
                .or(match label.as_str() {
                    "segno" => Some(Segno),
                    "codab" => Some(Coda),
                    "coda" => Some(ToCoda),
                    "fine" => Some(Fine),
                    _ => None,
                })
                .ok_or("Unsupported custom MuseScore navigation marker.")?;
            marks.push(NavigationMark {
                measure: bar,
                kind,
                play_repeats_after_jump: false,
            });
        }
    }
    marks.sort_by_key(|mark| (mark.measure, mark.kind));
    marks.dedup();
    Ok(marks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repeats::{Ending, RepeatRegion, playback_order};
    fn score(marks: Vec<(usize, NavigationKind)>) -> Score {
        let mut score = Score::blank();
        for part in &mut score.parts {
            part.measures.resize(6, Default::default());
        }
        score.notation.navigation = marks
            .into_iter()
            .map(|(measure, kind)| NavigationMark {
                measure,
                kind,
                play_repeats_after_jump: false,
            })
            .collect();
        score
    }
    #[test]
    fn dc_ds_fine_and_coda_routes_are_bounded_and_one_shot() {
        use NavigationKind::*;
        assert_eq!(
            playback_order(&score(vec![(3, DaCapo)])).unwrap(),
            [0, 1, 2, 3, 0, 1, 2, 3, 4, 5]
        );
        assert_eq!(
            playback_order(&score(vec![(1, Fine), (3, DaCapoAlFine)])).unwrap(),
            [0, 1, 2, 3, 0, 1]
        );
        assert_eq!(
            playback_order(&score(vec![
                (1, Segno),
                (2, ToCoda),
                (3, DalSegnoAlCoda),
                (4, Coda)
            ]))
            .unwrap(),
            [0, 1, 2, 3, 1, 2, 4, 5]
        );
    }
    #[test]
    fn return_route_skips_repeats_and_uses_final_ending_unless_enabled() {
        use NavigationKind::*;
        let mut score = score(vec![(4, DaCapo)]);
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
        assert_eq!(
            playback_order(&score).unwrap(),
            [0, 1, 2, 0, 1, 3, 4, 0, 1, 3, 4, 5]
        );
        score.notation.navigation[0].play_repeats_after_jump = true;
        assert_eq!(
            playback_order(&score).unwrap(),
            [0, 1, 2, 0, 1, 3, 4, 0, 1, 2, 0, 1, 3, 4, 5]
        );
    }
    #[test]
    fn ambiguous_missing_and_unreachable_targets_fail() {
        use NavigationKind::*;
        for marks in [
            vec![(3, DalSegno)],
            vec![(3, DaCapoAlFine)],
            vec![(0, Segno), (1, Segno), (3, DalSegno)],
            vec![(2, ToCoda), (3, DaCapoAlCoda), (1, Coda)],
        ] {
            assert!(playback_order(&score(marks)).is_err());
        }
    }
    #[test]
    fn navigation_musicxml_round_trip_keeps_purpose_and_targets() {
        use NavigationKind::*;
        let score = score(vec![
            (1, Segno),
            (2, ToCoda),
            (3, DalSegnoAlCoda),
            (4, Coda),
        ]);
        let mut xml = String::from("<part>");
        for bar in 0..6 {
            xml.push_str(&format!(
                "<measure>{}{}</measure>",
                musicxml_navigation_before_measure(&score, bar),
                musicxml_navigation_after_measure(&score, bar)
            ));
        }
        xml.push_str("</part>");
        let document = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(
            parse_musicxml_navigation(document.root_element(), 6).unwrap(),
            score.notation.navigation
        );
    }
    #[test]
    fn musescore_navigation_uses_explicit_jump_labels() {
        use NavigationKind::*;
        let xml = "<Staff><Measure/><Measure><Marker><label>S</label></Marker></Measure><Measure><Marker><label>T</label></Marker></Measure><Measure><Jump><jumpTo>S</jumpTo><playUntil>T</playUntil><continueAt>C</continueAt></Jump></Measure><Measure><Marker><label>C</label></Marker></Measure><Measure/></Staff>";
        let document = roxmltree::Document::parse(xml).unwrap();
        let marks = parse_musescore_navigation(document.root_element(), 6).unwrap();
        let mut imported = score(vec![]);
        imported.notation.navigation = marks;
        assert_eq!(
            playback_order(&imported).unwrap(),
            playback_order(&score(vec![
                (1, Segno),
                (2, ToCoda),
                (3, DalSegnoAlCoda),
                (4, Coda)
            ]))
            .unwrap()
        );
    }

    #[test]
    fn full_musicxml_round_trip_preserves_navigation_and_repeat_replay() {
        use NavigationKind::*;
        let mut source = score(vec![(1, Fine), (4, DaCapoAlFine)]);
        source.notation.navigation[1].play_repeats_after_jump = true;
        source.notation.repeats.push(RepeatRegion::simple(0, 0));
        let xml = crate::export::musicxml(&source);
        let path = std::env::temp_dir().join(format!(
            "encore-navigation-roundtrip-{}-{}.musicxml",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, xml).unwrap();
        let result = crate::import::load(&path);
        let _ = std::fs::remove_file(&path);
        let imported = result.unwrap().score;
        assert_eq!(imported.notation.navigation, source.notation.navigation);
        assert_eq!(imported.notation.repeats, source.notation.repeats);
        assert_eq!(
            playback_order(&imported).unwrap(),
            playback_order(&source).unwrap()
        );
    }
}
