//! Independent voice engraving regressions over the shared temporal map.
use super::*;
use crate::{
    document::Anchor,
    entry_style::{EntryStyle, StemDirection},
};

fn note(start: u32, layer: u8, voice: u8) -> Note {
    Note {
        start,
        layer,
        voice,
        voice_launch: (voice == 1).then_some(0),
        duration: Duration::Quarter,
        dotted: false,
        tuplet: None,
        pitches: vec![Pitch::from_midi(67)],
        velocity: 0,
        staccato: false,
        articulations: vec![],
        tied: false,
    }
}
fn fixture(notes: Vec<Note>) -> Score {
    let mut s = Score::blank();
    s.parts.truncate(1);
    s.parts[0].measures.truncate(1);
    s.parts[0].measures[0].notes = notes;
    s
}
fn anchor(n: &Note) -> Anchor {
    Anchor {
        part: 0,
        measure: 0,
        tick: n.start,
        layer: n.layer,
        voice: n.voice,
    }
}
fn voice_page(score: &Score) -> Page {
    let pitches = display::Index::new(score);
    let styles = score
        .notation
        .entries
        .iter()
        .map(|s| (s.anchor, s))
        .collect();
    let ornaments = ornaments::Index::new(score);
    let offsets = collision_offsets(score, &[0], &pitches, &styles, &ornaments);
    let context = RenderContext {
        spacing: vec![temporal_spacing(score, &[0], 0, &offsets, &ornaments)],
        ornaments,
        styles,
        pitches,
        offsets,
    };
    let region = StaffRegion {
        lines: 5,
        part: 0,
        measure: 0,
        x: 20.0,
        y: 100.0,
        width: 650.0,
        spacing: 10.0,
        content_x: 40.0,
        content_width: 610.0,
        temporal_anchors: context.spacing[0].clone(),
        anchor_offsets: context.offsets.get(&(0, 0)).cloned().unwrap_or_default(),
    };
    let mut page = Page::new();
    page.staves.push(region.clone());
    draw_measure_layer(
        &mut page,
        score,
        &region,
        &EngravingOptions::default(),
        4,
        true,
        &context,
    );
    page
}
#[test]
fn eight_independent_lanes_have_distinct_hits_and_voice_local_styles() {
    let mut notes = Vec::new();
    for layer in 0..4 {
        for voice in 0..2 {
            notes.push(note(0, layer, voice));
        }
    }
    let mut s = fixture(notes);
    s.validate().unwrap();
    let pages = engrave(&s, &Default::default());
    let p = &pages[0];
    assert_eq!(p.hits.len(), 8);
    let mut xs = p.hits.iter().map(|h| h.x).collect::<Vec<_>>();
    xs.sort_by(f32::total_cmp);
    assert!(xs.windows(2).all(|p| p[1] - p[0] >= 12.0));
    for hit in &p.hits {
        let n = &s.parts[0].measures[0].notes[hit.selection.note];
        assert!((p.staves[0].x_at_anchor(&anchor(n)) - hit.x).abs() < 0.001);
        let stem_x = hit.x + if n.voice == 0 { 5.0 } else { -5.0 };
        assert!(p.items.iter().any(|i|matches!(i,Primitive::Line{x1,x2,y1,y2,width,..}if (*x1-stem_x).abs()<0.001&&(*x2-stem_x).abs()<0.001&&(*width-1.25).abs()<0.001&&if n.voice==0{y2<y1}else{y2>y1})),"Paired voices must have opposed stems");
    }
    let mut style = EntryStyle::new(anchor(&s.parts[0].measures[0].notes[1]));
    style.visible = false;
    s.notation.entries.push(style);
    let pages = engrave(&s, &Default::default());
    assert_eq!(pages[0].hits.len(), 7);
    assert!(
        pages[0]
            .hits
            .iter()
            .any(|h| s.parts[0].measures[0].notes[h.selection.note].lane() == 0)
    );
    s.notation.layout.staff_scale = 0.7;
    let pages = engrave(&s, &Default::default());
    for h in &pages[0].hits {
        let n = &s.parts[0].measures[0].notes[h.selection.note];
        assert!((pages[0].staves[0].x_at_anchor(&anchor(n)) - h.x).abs() < 0.001);
    }
}
#[test]
fn secondary_branch_does_not_synthesize_outer_rests() {
    let mut secondary = note(480, 0, 1);
    secondary.voice_launch = Some(0);
    let mut s = fixture(vec![note(0, 0, 0), secondary]);
    let mut style = EntryStyle::new(anchor(&s.parts[0].measures[0].notes[1]));
    style.stem = StemDirection::None;
    s.notation.entries.push(style);
    let p = voice_page(&s);
    assert_eq!(p.hits.len(), 1);
    assert!(
        p.items
            .iter()
            .filter(|i| matches!(i, Primitive::Path { .. }))
            .count()
            == 1,
        "Only a stemless head should print; no leading or trailing quarter rests"
    );
    s.parts[0].measures[0].notes[1].pitches.clear();
    let p = voice_page(&s);
    assert!(
        p.items.iter().any(|i| matches!(i, Primitive::Path { .. })),
        "An explicit secondary rest must print"
    );
}
#[test]
fn independent_secondary_launches_cannot_beam_together() {
    let mut notes = vec![
        note(0, 0, 0),
        note(240, 0, 0),
        note(0, 0, 1),
        note(240, 0, 1),
    ];
    for n in &mut notes {
        n.duration = Duration::Eighth;
    }
    notes[3].voice_launch = Some(240);
    let mut s = fixture(notes);
    s.validate().unwrap();
    let beams = |p: &Page| {
        p.items.iter().filter(|i|matches!(i,Primitive::Path{points,closed:true,..}if points.len()==4&&(points[1].0-points[0].0).abs()>18.0)).count()
    };
    assert_eq!(beams(&voice_page(&s)), 0);
    s.parts[0].measures[0].notes[3].voice_launch = Some(0);
    assert_eq!(beams(&voice_page(&s)), 1);
}
#[test]
fn paired_voice_lyrics_receive_separate_underlay_rows() {
    let mut s = fixture(vec![note(0, 0, 0), note(0, 0, 1)]);
    for (i, text) in ["Primary", "Secondary"].iter().enumerate() {
        s.notation.lyrics.push(crate::lyrics::Lyric::new(
            anchor(&s.parts[0].measures[0].notes[i]),
            *text,
        ));
    }
    s.validate().unwrap();
    let pages = engrave(&s, &Default::default());
    let ys = pages[0]
        .items
        .iter()
        .filter_map(|p| {
            if let Primitive::Text { text, y, .. } = p {
                ["Primary", "Secondary"]
                    .contains(&text.as_str())
                    .then_some(*y)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(ys.len(), 2);
    assert!((ys[0] - ys[1]).abs() >= 18.0);
}
#[test]
fn secondary_graces_and_slur_endpoints_follow_the_displaced_voice() {
    let mut s = fixture(vec![
        note(0, 0, 0),
        note(480, 0, 0),
        note(0, 0, 1),
        note(480, 0, 1),
    ]);
    let start = anchor(&s.parts[0].measures[0].notes[2]);
    let end = anchor(&s.parts[0].measures[0].notes[3]);
    s.notation.slurs.push(crate::slurs::Slur::new(start, end));
    s.notation.grace.push(crate::grace::GraceGroup::new(
        start,
        vec![crate::grace::GraceNote::new(
            Pitch::from_midi(65),
            Duration::Sixteenth,
        )],
    ));
    s.validate().unwrap();
    let pages = engrave(&s, &Default::default());
    let p = &pages[0];
    let g = &p.grace_hits[0];
    assert_eq!(g.selection.anchor.voice, 1);
    let stem_x = g.x - 3.0;
    assert!(p.items.iter().any(|i|matches!(i,Primitive::Line{x1,x2,y1,y2,width,..}if(*x1-stem_x).abs()<0.001&&(*x2-stem_x).abs()<0.001&&(*width-0.75).abs()<0.001&&y2>y1)));
    let x = p.staves[0].x_at_anchor(&start);
    let y = p.hits.iter().find(|h| h.selection.note == 2).unwrap().y;
    assert!(p.items.iter().any(|i|matches!(i,Primitive::Path{points,..}if points.len()==82&&(points[0].0-x).abs()<0.001&&points[0].1>y)),"The secondary slur must attach to its displaced head and curve below");
}
