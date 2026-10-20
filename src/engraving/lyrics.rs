use super::*;
#[cfg(test)]
use crate::document::Anchor;
use crate::lyrics::{Lyric, LyricKind, Syllabic, position};

fn lines(score: &Score, part: usize) -> Vec<(LyricKind, u16, u8)> {
    score
        .notation
        .lyrics
        .iter()
        .filter(|l| l.anchor.part == part)
        .map(|l| (l.kind, l.verse, l.anchor.lane()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn base(score: &Score, regions: &[&StaffRegion]) -> f32 {
    let first = regions[0];
    let mut baseline = first.y + 85.0;
    for region in regions {
        if let Some(m) = score.parts[region.part].measures.get(region.measure) {
            let multiple = m
                .notes
                .iter()
                .any(|n| m.notes.first().is_some_and(|p| p.lane() != n.lane()));
            baseline = baseline.max(
                staff_dynamic_baseline(&m.notes, score.parts[region.part].clef, region.y, multiple)
                    + 25.0,
            );
        }
    }
    let first_measure = regions.iter().map(|r| r.measure).min().unwrap_or(0);
    let last_measure = regions.iter().map(|r| r.measure).max().unwrap_or(0);
    let below_slur = score
        .notation
        .slurs
        .iter()
        .filter(|s| {
            s.start.part == first.part
                && s.start.measure <= last_measure
                && s.end.measure >= first_measure
                && (s.direction == crate::slurs::SlurDirection::Below
                    || (s.direction == crate::slurs::SlurDirection::Auto
                        && !automatic_slur_above(score, s.start)))
        })
        .map(|s| s.height + 15.0 + s.start_offset[1].abs().max(s.end_offset[1].abs()))
        .fold(0.0, f32::max);
    baseline + below_slur
}

pub(super) fn width(text: &str, size: f32) -> f32 {
    text_width(text, size, true)
}

pub(super) fn horizontal_requirement(score: &Score, parts: &[usize], measure: usize) -> f32 {
    let mut lines: BTreeMap<(usize, LyricKind, u16, u8), f32> = BTreeMap::new();
    for lyric in score
        .notation
        .lyrics
        .iter()
        .filter(|l| l.anchor.measure == measure && parts.contains(&l.anchor.part))
    {
        *lines
            .entry((
                lyric.anchor.part,
                lyric.kind,
                lyric.verse,
                lyric.anchor.lane(),
            ))
            .or_default() += width(&lyric.text, 13.0) + 8.0;
    }
    lines.into_values().fold(0.0, f32::max)
}
pub(super) fn extra_below(score: &Score, part: usize, measure: usize, bars: usize) -> f32 {
    let max_baseline = score
        .notation
        .lyrics
        .iter()
        .filter(|l| {
            l.anchor.part == part
                && l.anchor.measure < measure + bars
                && l.extend_to.as_ref().unwrap_or(&l.anchor).measure >= measure
        })
        .map(|l| l.baseline.max(0.0))
        .fold(0.0, f32::max);
    if lines(score, part).is_empty() {
        0.0
    } else {
        24.0 + lines(score, part).len() as f32 * 18.0 + max_baseline
    }
}
/// Draw text and its joining graphics after all staff regions are known. Each
/// system receives its own continuation fragment, including across page breaks.
pub(super) fn draw(page: &mut Page, score: &Score) {
    let mut next_syllables = BTreeMap::new();
    let mut syllable_lines: BTreeMap<(usize, u8, LyricKind, u16), Vec<&Lyric>> = BTreeMap::new();
    for lyric in &score.notation.lyrics {
        syllable_lines
            .entry((
                lyric.anchor.part,
                lyric.anchor.lane(),
                lyric.kind,
                lyric.verse,
            ))
            .or_default()
            .push(lyric);
    }
    for lyrics in syllable_lines.values_mut() {
        lyrics.sort_by_key(|l| position(&l.anchor));
        for pair in lyrics.windows(2) {
            next_syllables.insert((pair[0].anchor, pair[0].kind, pair[0].verse), pair[1]);
        }
    }
    let mut systems: BTreeMap<(usize, u32), Vec<StaffRegion>> = BTreeMap::new();
    for r in &page.staves {
        systems
            .entry((r.part, r.y.to_bits()))
            .or_default()
            .push(r.clone());
    }
    for ((part, _), mut regions) in systems {
        regions.sort_by_key(|r| r.measure);
        let refs = regions.iter().collect::<Vec<_>>();
        let line_rows = lines(score, part)
            .into_iter()
            .enumerate()
            .map(|(i, key)| (key, i))
            .collect::<BTreeMap<_, _>>();
        let base = base(score, &refs);
        let first = &regions[0];
        let last = regions.last().unwrap();
        let start_pos = (first.measure, 0);
        let end_pos = (last.measure, score.measure_ticks());
        for lyric in score
            .notation
            .lyrics
            .iter()
            .filter(|l| l.anchor.part == part)
        {
            let row = line_rows
                .get(&(lyric.kind, lyric.verse, lyric.anchor.lane()))
                .copied()
                .unwrap_or(0);
            let y = base + row as f32 * 18.0 + lyric.baseline;
            if let Some(region) = regions.iter().find(|r| r.measure == lyric.anchor.measure) {
                centered_text(
                    page,
                    region.x_at_tick(lyric.anchor.tick as f32),
                    y,
                    &lyric.text,
                    13.0,
                    true,
                );
            }
            if let Some(end) = &lyric.extend_to
                && position(end) >= start_pos
                && position(&lyric.anchor) < end_pos
            {
                let left = regions
                    .iter()
                    .find(|r| r.measure == lyric.anchor.measure)
                    .map(|r| {
                        r.x_at_tick(lyric.anchor.tick as f32) + width(&lyric.text, 13.0) * 0.5 + 3.0
                    })
                    .unwrap_or(first.content_x);
                let right = regions
                    .iter()
                    .find(|r| r.measure == end.measure)
                    .map(|r| r.x_at_tick(end.tick as f32) + 7.0)
                    .unwrap_or(last.x + last.width - 9.0);
                if right > left + 3.0 {
                    page.line(left, y + 3.0, right, y + 3.0, 0.8, INK);
                }
            }
            if matches!(lyric.syllabic, Syllabic::Begin | Syllabic::Middle) {
                let next = next_syllables
                    .get(&(lyric.anchor, lyric.kind, lyric.verse))
                    .copied();
                if let Some(next) = next
                    && position(&next.anchor) >= start_pos
                    && position(&lyric.anchor) < end_pos
                {
                    let left = regions
                        .iter()
                        .find(|r| r.measure == lyric.anchor.measure)
                        .map(|r| {
                            r.x_at_tick(lyric.anchor.tick as f32)
                                + width(&lyric.text, 13.0) * 0.5
                                + 4.0
                        })
                        .unwrap_or(first.content_x);
                    let right = regions
                        .iter()
                        .find(|r| r.measure == next.anchor.measure)
                        .map(|r| {
                            r.x_at_tick(next.anchor.tick as f32)
                                - width(&next.text, 13.0) * 0.5
                                - 4.0
                        })
                        .unwrap_or(last.x + last.width - 9.0);
                    if right > left + 4.0 {
                        let count = ((right - left) / 95.0).ceil().max(1.0) as usize;
                        for i in 0..count {
                            let x = left + (right - left) * (i as f32 + 0.5) / count as f32;
                            page.line(x - 2.5, y - 4.0, x + 2.5, y - 4.0, 0.9, INK);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{self, Lyric};
    #[test]
    fn lyric_underlay_reserves_space_and_extends_over_systems() {
        let mut score = Score::demo();
        let a = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        let end = Anchor {
            voice: 0,
            part: 0,
            measure: 5,
            tick: 0,
            layer: 0,
        };
        lyrics::assign(
            &mut score,
            Lyric {
                extend_to: Some(end),
                ..Lyric::new(a, "Shine")
            },
        )
        .unwrap();
        let pages = engrave(&score, &EngravingOptions::default());
        assert!(
            pages
                .iter()
                .flat_map(|p| &p.items)
                .any(|p| matches!(p,Primitive::Text{text,..}if text=="Shine"))
        );
        assert!(
            pages
                .iter()
                .flat_map(|p| &p.items)
                .filter(|p| matches!(p,Primitive::Line{width,..}if *width==0.8))
                .count()
                >= 2
        );
    }
    #[test]
    fn long_ordinary_syllables_receive_separate_horizontal_space() {
        let mut score = Score::blank();
        score.parts.truncate(1);
        for tick in [0, 480, 960, 1440] {
            score.parts[0].measures[0].notes.push(Note {
                voice: 0,
                voice_launch: None,
                start: tick,
                layer: 0,
                duration: Duration::Quarter,
                dotted: false,
                tuplet: None,
                pitches: vec![Pitch::from_midi(60)],
                velocity: 80,
                staccato: false,
                articulations: vec![],
                tied: false,
            });
        }
        let start = Anchor {
            voice: 0,
            part: 0,
            measure: 0,
            tick: 0,
            layer: 0,
        };
        lyrics::assign_text(
            &mut score,
            start,
            LyricKind::Verse,
            1,
            "extraordinary wonderful choral articulation",
        )
        .unwrap();
        let words = ["extraordinary", "wonderful", "choral", "articulation"];
        let pages = engrave(&score, &EngravingOptions::default());
        let text = pages
            .iter()
            .flat_map(|p| &p.items)
            .filter_map(|item| {
                if let Primitive::Text { x, text, .. } = item {
                    words.contains(&text.as_str()).then_some((*x, text))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(text.len(), 4);
        for pair in text.windows(2) {
            assert!(
                pair[0].0 + width(pair[0].1, 13.0) + 3.0 < pair[1].0,
                "Neighboring syllables overlap"
            );
        }
    }
}
