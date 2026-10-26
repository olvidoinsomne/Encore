//! Written repeat regions and a bounded performed-measure traversal.
//!
//! Endpoints are inclusive and zero based. A region with alternate endings
//! extends through its last ending, including the measures outside the first
//! backward barline. This keeps playback independent of drawing conventions.
use crate::model::{Note, Part, Score};
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_PERFORMED_MEASURES: usize = 16_384;
pub const MAX_PERFORMED_EVENTS: usize = 2_000_000;
const MAX_REGIONS: usize = 128;
const MAX_DEPTH: usize = 16;

mod navigation;
pub use navigation::{
    NavigationKind, NavigationMark, musicxml_navigation_after_measure,
    musicxml_navigation_before_measure, parse_musescore_navigation, parse_musicxml_navigation,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatRegion {
    pub start: usize,
    pub end: usize,
    /// Total number of passes, including the initial pass.
    pub times: u8,
    #[serde(default)]
    pub endings: Vec<Ending>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ending {
    pub start: usize,
    pub end: usize,
    pub passes: Vec<u8>,
}

impl RepeatRegion {
    pub fn simple(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            times: 2,
            endings: Vec::new(),
        }
    }
    pub fn body_end(&self) -> usize {
        self.endings
            .first()
            .map_or(self.end + 1, |ending| ending.start)
    }
    pub fn backward_at(&self, measure: usize) -> bool {
        if self.endings.is_empty() {
            self.end == measure
        } else {
            self.endings.iter().any(|ending| {
                ending.end == measure && ending.passes.iter().any(|pass| *pass < self.times)
            })
        }
    }
}

fn validate_regions(regions: &[RepeatRegion], measures: usize) -> Result<(), String> {
    if regions.len() > MAX_REGIONS {
        return Err(format!(
            "A score supports at most {MAX_REGIONS} repeat regions."
        ));
    }
    for region in regions {
        if region.start > region.end || region.end >= measures {
            return Err("A repeat region extends outside the written score.".into());
        }
        if !(2..=16).contains(&region.times) {
            return Err("Repeat counts must be 2–16 total passes.".into());
        }
        if !region.endings.is_empty() {
            let mut next = region.endings[0].start;
            let mut passes = BTreeSet::new();
            for ending in &region.endings {
                if ending.start != next
                    || ending.start < region.start
                    || ending.start > ending.end
                    || ending.end > region.end
                {
                    return Err("Alternate endings must be ordered, contiguous, and inside their repeat region.".into());
                }
                if ending.passes.is_empty() || ending.passes.len() > 16 {
                    return Err("Each ending needs at least one pass number.".into());
                }
                for &pass in &ending.passes {
                    if pass == 0 || pass > region.times || !passes.insert(pass) {
                        return Err(
                            "Every repeat pass must select exactly one alternate ending.".into(),
                        );
                    }
                }
                next = ending.end + 1;
            }
            if next != region.end + 1 || passes.len() != usize::from(region.times) {
                return Err(
                    "Alternate endings must finish the region and cover every repeat pass.".into(),
                );
            }
            if !region
                .endings
                .last()
                .unwrap()
                .passes
                .contains(&region.times)
            {
                return Err("The final repeat pass must use the last written ending.".into());
            }
        }
    }
    for (index, first) in regions.iter().enumerate() {
        for second in &regions[index + 1..] {
            if first.end < second.start || second.end < first.start {
                continue;
            }
            let (outer, inner) = if first.start <= second.start && first.end >= second.end {
                (first, second)
            } else if second.start <= first.start && second.end >= first.end {
                (second, first)
            } else {
                return Err("Repeat regions may nest but cannot cross.".into());
            };
            if outer.start == inner.start && outer.end == inner.end {
                return Err("Duplicate repeat regions are ambiguous.".into());
            }
            // An inner repeat cannot straddle an ending boundary: the skipped
            // portion would otherwise make its state ambiguous on later passes.
            if inner.end >= outer.body_end()
                && !outer
                    .endings
                    .iter()
                    .any(|ending| inner.start >= ending.start && inner.end <= ending.end)
            {
                return Err("A nested repeat cannot cross an alternate-ending boundary.".into());
            }
        }
    }
    Ok(())
}

/// Does not call Score::validate, so native validation can safely delegate here.
pub fn validate(score: &Score) -> Result<(), String> {
    playback_order(score).map(|_| ())
}

pub fn playback_order(score: &Score) -> Result<Vec<usize>, String> {
    let measures = score.measure_count();
    let regions = &score.notation.repeats;
    validate_regions(regions, measures)?;
    navigation::validate(score)?;
    let mut regions: Vec<_> = regions.iter().collect();
    regions.sort_by_key(|region| (region.start, std::cmp::Reverse(region.end)));
    let mut output = Vec::new();
    emit_range(0, measures, &regions, &mut output, 0, false)?;
    if !score.notation.navigation.is_empty() {
        let mut final_pass = Vec::new();
        emit_range(0, measures, &regions, &mut final_pass, 0, true)?;
        output = navigation::perform(score, &output, &final_pass)?;
    }
    let event_counts: Vec<usize> = (0..measures)
        .map(|bar| {
            score
                .parts
                .iter()
                .map(|part| part.measures.get(bar).map_or(0, |m| m.notes.len()))
                .sum()
        })
        .collect();
    let mut events = 0usize;
    for &bar in &output {
        events = events
            .checked_add(event_counts[bar])
            .ok_or("Repeat event count overflow.")?;
        if events > MAX_PERFORMED_EVENTS {
            return Err(format!(
                "Repeats exceed the {MAX_PERFORMED_EVENTS} performed-event limit."
            ));
        }
    }
    Ok(output)
}

fn emit_range(
    start: usize,
    end: usize,
    regions: &[&RepeatRegion],
    output: &mut Vec<usize>,
    depth: usize,
    final_pass: bool,
) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("Repeats exceed the 16-level nesting limit.".into());
    }
    let mut bar = start;
    while bar < end {
        if let Some(region) = regions
            .iter()
            .copied()
            .find(|region| region.start == bar && region.end < end)
        {
            let children: Vec<_> = regions
                .iter()
                .copied()
                .filter(|child| {
                    !std::ptr::eq(*child, region)
                        && child.start >= region.start
                        && child.end <= region.end
                })
                .collect();
            for pass in (if final_pass { region.times } else { 1 })..=region.times {
                emit_range(
                    region.start,
                    region.body_end(),
                    &children,
                    output,
                    depth + 1,
                    final_pass,
                )?;
                if let Some(ending) = region
                    .endings
                    .iter()
                    .find(|ending| ending.passes.contains(&pass))
                {
                    emit_range(
                        ending.start,
                        ending.end + 1,
                        &children,
                        output,
                        depth + 1,
                        final_pass,
                    )?;
                }
            }
            bar = region.end + 1;
        } else {
            if output.len() >= MAX_PERFORMED_MEASURES {
                return Err(format!(
                    "Repeats exceed the {MAX_PERFORMED_MEASURES} performed-measure limit."
                ));
            }
            output.push(bar);
            bar += 1;
        }
    }
    Ok(())
}

/// Source references retained alongside performed ticks for ties and slurs.
#[derive(Clone, Copy)]
pub(crate) struct PlaybackEvent<'a> {
    pub tick: u64,
    pub measure: usize,
    pub note: &'a Note,
}

pub(crate) fn events<'a>(
    part: &'a Part,
    measure_ticks: u32,
    order: &[usize],
) -> Vec<PlaybackEvent<'a>> {
    let mut events = Vec::new();
    for (performed, &source) in order.iter().enumerate() {
        if let Some(measure) = part.measures.get(source) {
            for note in &measure.notes {
                events.push(PlaybackEvent {
                    tick: performed as u64 * u64::from(measure_ticks) + u64::from(note.start),
                    measure: source,
                    note,
                });
            }
        }
    }
    events.sort_by_key(|event| event.tick);
    events
}

/// Ties and phrase overlap must not leak across a repeat jump or skipped ending.
pub(crate) fn written_adjacent(
    first: &PlaybackEvent<'_>,
    next: &PlaybackEvent<'_>,
    measure_ticks: u32,
) -> bool {
    let source_end = first.measure as u64 * u64::from(measure_ticks)
        + u64::from(first.note.start)
        + u64::from(first.note.ticks());
    let source_start = next.measure as u64 * u64::from(measure_ticks) + u64::from(next.note.start);
    first.note.lane() == next.note.lane()
        && first.tick + u64::from(first.note.ticks()) == next.tick
        && source_end == source_start
}

pub(crate) fn legato_extension(
    score: &Score,
    part: usize,
    event: &PlaybackEvent<'_>,
    next: Option<&PlaybackEvent<'_>>,
) -> u32 {
    let Some(next) = next else {
        return 0;
    };
    if event.note.playback_shortening() != 0.0
        || event.note.lane() != next.note.lane()
        || !written_adjacent(event, next, score.measure_ticks())
        || next.note.pitches.is_empty()
        || event.note.pitches.iter().any(|pitch| {
            next.note
                .pitches
                .iter()
                .any(|other| pitch.midi() == other.midi())
        })
    {
        return 0;
    }
    let anchor = crate::document::Anchor {
        part,
        measure: event.measure,
        tick: event.note.start,
        layer: event.note.layer,
        voice: event.note.voice,
    };
    if crate::slurs::continues(score, &anchor) {
        (event.note.ticks() / 16).clamp(1, 15)
    } else {
        0
    }
}

pub fn musicxml_before_measure(score: &Score, measure: usize) -> String {
    let mut out = String::new();
    for region in &score.notation.repeats {
        let ending = region.endings.iter().find(|ending| ending.start == measure);
        if region.start == measure || ending.is_some() {
            out.push_str("      <barline location=\"left\">");
            if let Some(ending) = ending {
                out.push_str(&format!(
                    "<ending number=\"{}\" type=\"start\"/>",
                    pass_text(&ending.passes)
                ));
            }
            if region.start == measure {
                out.push_str("<repeat direction=\"forward\"/>");
            }
            out.push_str("</barline>\n");
        }
    }
    out
}

pub fn musicxml_after_measure(score: &Score, measure: usize) -> String {
    let mut out = String::new();
    for region in score.notation.repeats.iter().rev() {
        let ending = region.endings.iter().find(|ending| ending.end == measure);
        let backward = region.backward_at(measure);
        if backward || ending.is_some() {
            out.push_str("      <barline location=\"right\">");
            if let Some(ending) = ending {
                out.push_str(&format!(
                    "<ending number=\"{}\" type=\"{}\"/>",
                    pass_text(&ending.passes),
                    if backward { "stop" } else { "discontinue" }
                ));
            }
            if backward {
                out.push_str(&format!(
                    "<repeat direction=\"backward\" times=\"{}\"{}/>",
                    region.times,
                    if score
                        .notation
                        .navigation
                        .iter()
                        .any(|mark| mark.kind.is_jump() && mark.play_repeats_after_jump)
                    {
                        " after-jump=\"yes\""
                    } else {
                        ""
                    }
                ));
            }
            out.push_str("</barline>\n");
        }
    }
    out
}

pub fn pass_text(passes: &[u8]) -> String {
    passes
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn parse_passes(text: &str) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    for component in text.split(',') {
        let component = component.trim().trim_end_matches('.');
        let bounds: Vec<_> = component.split(['-', '–']).collect();
        let start: u8 = bounds[0]
            .trim()
            .parse()
            .map_err(|_| "Invalid repeat ending pass number.")?;
        let end = if bounds.len() == 1 {
            start
        } else if bounds.len() == 2 {
            bounds[1]
                .trim()
                .parse()
                .map_err(|_| "Invalid ending range.")?
        } else {
            return Err("Invalid ending range.".into());
        };
        if start == 0 || start > end || end > 16 {
            return Err("Ending pass numbers must be 1–16.".into());
        }
        for pass in start..=end {
            if output.contains(&pass) {
                return Err("Duplicate ending pass number.".into());
            }
            output.push(pass);
        }
    }
    output.sort_unstable();
    Ok(output)
}

/// MusicXML repeat/ending syntax follows the public MusicXML 4.0 barline spec.
pub fn parse_musicxml(
    part: Node<'_, '_>,
    measure_count: usize,
) -> Result<Vec<RepeatRegion>, String> {
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    let mut endings = Vec::new();
    let mut active: Option<(usize, Vec<u8>)> = None;
    for (index, measure) in part
        .children()
        .filter(|n| n.has_tag_name("measure"))
        .enumerate()
    {
        for barline in measure.children().filter(|n| n.has_tag_name("barline")) {
            for element in barline.children().filter(|n| n.is_element()) {
                if element.has_tag_name("ending") {
                    let passes = parse_passes(element.attribute("number").unwrap_or(""))?;
                    match element.attribute("type") {
                        Some("start") if active.is_none() => active = Some((index, passes)),
                        Some("stop" | "discontinue") => {
                            let (start, expected) = active
                                .take()
                                .ok_or("An ending closes without a matching start.")?;
                            if passes != expected {
                                return Err("Ending start and stop pass lists disagree.".into());
                            }
                            endings.push(Ending {
                                start,
                                end: index,
                                passes,
                            });
                        }
                        _ => {
                            return Err("Overlapping or unsupported MusicXML ending markup.".into());
                        }
                    }
                } else if element.has_tag_name("repeat") {
                    match element.attribute("direction") {
                        Some("forward") => stack.push(index),
                        Some("backward") => {
                            let times = element
                                .attribute("times")
                                .unwrap_or("2")
                                .parse::<u8>()
                                .map_err(|_| "Invalid repeat count.")?;
                            let continuation =
                                ending_continuation(&regions, &endings, index, times);
                            let start = continuation.unwrap_or_else(|| stack.pop().unwrap_or(0));
                            regions.push(RepeatRegion {
                                start,
                                end: index,
                                times,
                                endings: Vec::new(),
                            });
                        }
                        _ => return Err("Invalid MusicXML repeat direction.".into()),
                    }
                }
            }
        }
    }
    if !stack.is_empty() || active.is_some() {
        return Err("An unfinished repeat or alternate ending needs a closing barline.".into());
    }
    attach_endings(&mut regions, endings)?;
    merge_ending_backwards(&mut regions);
    validate_regions(&regions, measure_count)?;
    Ok(regions)
}

fn ending_continuation(
    regions: &[RepeatRegion],
    endings: &[Ending],
    end: usize,
    _times: u8,
) -> Option<usize> {
    let current = endings.iter().find(|ending| ending.end == end)?;
    if current.passes.contains(&1) {
        return None;
    }
    regions
        .iter()
        .rev()
        .find(|region| {
            region.end + 1 == current.start && endings.iter().any(|ending| ending.end == region.end)
        })
        .map(|region| region.start)
}

fn attach_endings(regions: &mut [RepeatRegion], mut endings: Vec<Ending>) -> Result<(), String> {
    endings.sort_by_key(|ending| ending.start);
    // Innermost repeats claim their ending suffix before enclosing repeats.
    regions.sort_by_key(|region| region.end - region.start);
    for region in regions.iter_mut() {
        if let Some(first) = endings
            .iter()
            .position(|ending| ending.end == region.end && ending.start >= region.start)
        {
            let first = endings.remove(first);
            region.endings.push(first);
            while let Some(next) = endings
                .iter()
                .position(|ending| ending.start == region.end + 1)
            {
                let next = endings.remove(next);
                region.end = next.end;
                region.endings.push(next);
                // MusicXML's times attribute is not authoritative for a repeat
                // with numbered endings; infer larger counts from the labels.
                let passes: BTreeSet<_> = region
                    .endings
                    .iter()
                    .flat_map(|ending| ending.passes.iter().copied())
                    .collect();
                let last_pass = passes.last().copied().unwrap_or(region.times);
                region.times = region.times.max(last_pass);
                if passes.len() == usize::from(region.times)
                    && region
                        .endings
                        .last()
                        .unwrap()
                        .passes
                        .contains(&region.times)
                {
                    break;
                }
            }
        }
    }
    if !endings.is_empty() {
        return Err("An alternate ending has no matching repeat region.".into());
    }
    regions.sort_by_key(|region| (region.start, std::cmp::Reverse(region.end)));
    Ok(())
}

// Several written endings may have a backward barline (for example 1,3 and
// 2,4). They belong to the same traversal rather than independent nested loops.
fn merge_ending_backwards(regions: &mut Vec<RepeatRegion>) {
    let redundant: BTreeSet<_> = regions
        .iter()
        .enumerate()
        .filter_map(|(index, region)| {
            if !region.endings.is_empty() {
                return None;
            }
            regions
                .iter()
                .enumerate()
                .any(|(other, parent)| {
                    other != index
                        && !parent.endings.is_empty()
                        && parent.start == region.start
                        && parent.times >= region.times
                        && parent.endings.iter().any(|ending| ending.end == region.end)
                })
                .then_some(index)
        })
        .collect();
    let mut index = 0;
    regions.retain(|_| {
        let keep = !redundant.contains(&index);
        index += 1;
        keep
    });
}

/// MuseScore 2–4 repeat markers; volta spans use their explicit measure count.
pub fn parse_musescore(
    staff: Node<'_, '_>,
    measure_count: usize,
    division: u32,
    measure_ticks: u32,
) -> Result<Vec<RepeatRegion>, String> {
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    let mut endings = Vec::new();
    for (index, measure) in staff
        .children()
        .filter(|n| {
            n.has_tag_name("Measure") && !n.children().any(|n| n.has_tag_name("multiMeasureRest"))
        })
        .enumerate()
    {
        for volta in measure
            .descendants()
            .filter(|node| node.has_tag_name("Volta"))
        {
            let field = |name: &str| {
                volta
                    .children()
                    .find(|n| n.has_tag_name(name))
                    .and_then(|n| n.text())
            };
            let passes = parse_passes(field("endings").unwrap_or(""))?;
            let bars = if let Some(measures) = field("measures") {
                measures
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Invalid MuseScore volta length.")?
            } else if let Some(ticks) = field("ticks") {
                let ticks = ticks
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| "Invalid MuseScore volta duration.")?;
                let scaled = ticks
                    .checked_mul(480)
                    .ok_or("MuseScore volta duration overflow.")?;
                let denominator = u64::from(division) * u64::from(measure_ticks);
                if denominator == 0 || scaled % denominator != 0 {
                    return Err("A MuseScore volta must span complete measures.".into());
                }
                (scaled / denominator) as usize
            } else {
                // Modern Spanner stores the duration as next/location/measures.
                volta
                    .parent()
                    .filter(|n| n.has_tag_name("Spanner"))
                    .and_then(|n| n.children().find(|n| n.has_tag_name("next")))
                    .and_then(|n| n.children().find(|n| n.has_tag_name("location")))
                    .and_then(|n| n.children().find(|n| n.has_tag_name("measures")))
                    .and_then(|n| n.text())
                    .ok_or("MuseScore volta is missing its measure span.")?
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Invalid MuseScore volta span.")?
            };
            if bars == 0
                || bars > measure_count
                || index
                    .checked_add(bars)
                    .is_none_or(|end| end > measure_count)
            {
                return Err("MuseScore volta extends outside the score.".into());
            }
            endings.push(Ending {
                start: index,
                end: index + bars - 1,
                passes,
            });
        }
        for node in measure
            .descendants()
            .filter(|n| n.has_tag_name("startRepeat") || n.has_tag_name("endRepeat"))
        {
            if node.has_tag_name("startRepeat") {
                stack.push(index);
            } else {
                let times = node
                    .text()
                    .unwrap_or("2")
                    .trim()
                    .parse::<u8>()
                    .map_err(|_| "Invalid MuseScore repeat count.")?;
                let continuation = ending_continuation(&regions, &endings, index, times);
                let start = continuation.unwrap_or_else(|| stack.pop().unwrap_or(0));
                regions.push(RepeatRegion {
                    start,
                    end: index,
                    times,
                    endings: Vec::new(),
                });
            }
        }
    }
    if !stack.is_empty() {
        return Err("An unfinished MuseScore repeat needs an end repeat.".into());
    }
    attach_endings(&mut regions, endings)?;
    merge_ending_backwards(&mut regions);
    validate_regions(&regions, measure_count)?;
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn score(measures: usize, repeats: Vec<RepeatRegion>) -> Score {
        let mut score = Score::blank();
        for part in &mut score.parts {
            part.measures.resize(measures, Default::default());
        }
        score.notation.repeats = repeats;
        score
    }
    #[test]
    fn simple_nested_and_adjacent_repeats_play_in_written_order() {
        assert_eq!(playback_order(&score(4, vec![])).unwrap(), [0, 1, 2, 3]);
        assert_eq!(
            playback_order(&score(4, vec![RepeatRegion::simple(1, 2)])).unwrap(),
            [0, 1, 2, 1, 2, 3]
        );
        assert_eq!(
            playback_order(&score(
                4,
                vec![RepeatRegion::simple(0, 3), RepeatRegion::simple(1, 2)]
            ))
            .unwrap(),
            [0, 1, 2, 1, 2, 3, 0, 1, 2, 1, 2, 3]
        );
    }
    #[test]
    fn first_second_endings_and_arbitrary_pass_lists() {
        let region = RepeatRegion {
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
        };
        assert_eq!(
            playback_order(&score(5, vec![region.clone()])).unwrap(),
            [0, 1, 2, 0, 1, 3, 4]
        );
        let mut region = region;
        region.times = 4;
        region.endings[0].passes = vec![1, 3];
        region.endings[1].passes = vec![2, 4];
        assert_eq!(
            playback_order(&score(4, vec![region])).unwrap(),
            [0, 1, 2, 0, 1, 3, 0, 1, 2, 0, 1, 3]
        );
    }
    #[test]
    fn rejects_crossings_duplicate_regions_bad_endings_and_unbounded_expansion() {
        for regions in [
            vec![RepeatRegion::simple(0, 2), RepeatRegion::simple(1, 3)],
            vec![RepeatRegion::simple(0, 2), RepeatRegion::simple(0, 2)],
        ] {
            assert!(playback_order(&score(4, regions)).is_err());
        }
        let mut region = RepeatRegion::simple(0, 3);
        region.endings = vec![Ending {
            start: 2,
            end: 3,
            passes: vec![1],
        }];
        assert!(playback_order(&score(4, vec![region])).is_err());
        let regions = (0..8)
            .map(|index| RepeatRegion {
                start: index,
                end: 15 - index,
                times: 16,
                endings: vec![],
            })
            .collect();
        assert!(
            playback_order(&score(16, regions))
                .unwrap_err()
                .contains("performed-measure")
        );
    }
    #[test]
    fn musicxml_repeat_round_trip_preserves_nested_regions_and_voltas() {
        let region = RepeatRegion {
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
        };
        let score = score(4, vec![region.clone()]);
        let mut xml = String::from("<part>");
        for bar in 0..4 {
            xml.push_str(&format!(
                "<measure>{}{}</measure>",
                musicxml_before_measure(&score, bar),
                musicxml_after_measure(&score, bar)
            ));
        }
        xml.push_str("</part>");
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(parse_musicxml(doc.root_element(), 4).unwrap(), [region]);
    }
    #[test]
    fn musescore_simple_and_volta_markers_are_retained() {
        let xml = "<Staff><Measure><startRepeat/></Measure><Measure><Volta><endings>1</endings><measures>1</measures></Volta><endRepeat>2</endRepeat></Measure><Measure><Volta><endings>2</endings><measures>1</measures></Volta></Measure></Staff>";
        let doc = roxmltree::Document::parse(xml).unwrap();
        let regions = parse_musescore(doc.root_element(), 3, 480, 1920).unwrap();
        assert_eq!(playback_order(&score(3, regions)).unwrap(), [0, 1, 0, 2]);
    }
    #[test]
    fn ending_ranges_have_unique_bounded_passes() {
        assert_eq!(parse_passes("1–3, 5.").unwrap(), [1, 2, 3, 5]);
        for text in ["", "0", "17", "2-1", "1,1", "1-17"] {
            assert!(parse_passes(text).is_err());
        }
    }

    #[test]
    fn musicxml_infers_three_pass_endings_and_round_trips_four_pass_lists() {
        let xml = "<part><measure><barline><repeat direction='forward'/></barline></measure><measure><barline><ending number='1,2' type='start'/></barline><barline><ending number='1,2' type='stop'/><repeat direction='backward'/></barline></measure><measure><barline><ending number='3' type='start'/></barline><barline><ending number='3' type='discontinue'/></barline></measure></part>";
        let document = roxmltree::Document::parse(xml).unwrap();
        let regions = parse_musicxml(document.root_element(), 3).unwrap();
        assert_eq!(regions[0].times, 3);
        assert_eq!(
            playback_order(&score(3, regions)).unwrap(),
            [0, 1, 0, 1, 0, 2]
        );
        let region = RepeatRegion {
            start: 1,
            end: 4,
            times: 4,
            endings: vec![
                Ending {
                    start: 3,
                    end: 3,
                    passes: vec![1, 3],
                },
                Ending {
                    start: 4,
                    end: 4,
                    passes: vec![2, 4],
                },
            ],
        };
        let score = score(5, vec![region.clone()]);
        let mut xml = String::from("<part>");
        for bar in 0..5 {
            xml.push_str(&format!(
                "<measure>{}{}</measure>",
                musicxml_before_measure(&score, bar),
                musicxml_after_measure(&score, bar)
            ));
        }
        xml.push_str("</part>");
        let document = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(
            parse_musicxml(document.root_element(), 5).unwrap(),
            [region]
        );
    }
}
