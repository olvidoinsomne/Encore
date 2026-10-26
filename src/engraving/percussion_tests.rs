use super::*;
use crate::{
    document::Anchor,
    entry_style::{EntryStyle, Notehead, StemDirection},
    grace::{GraceGroup, GraceNote},
    lyrics::Lyric,
    percussion::{Sound, Staff, token},
    slurs::Slur,
};
fn at(tick: u32, layer: u8, voice: u8) -> Anchor {
    Anchor {
        part: 0,
        measure: 0,
        tick,
        layer,
        voice,
    }
}
fn n(tick: u32, layer: u8, voice: u8, ids: &[u8]) -> Note {
    Note {
        start: tick,
        layer,
        voice,
        voice_launch: (voice == 1).then_some(0),
        duration: Duration::Quarter,
        dotted: false,
        tuplet: None,
        pitches: ids.iter().map(|id| token(*id)).collect(),
        velocity: 0,
        staccato: false,
        articulations: vec![],
        tied: false,
    }
}
fn fixture(lines: u8) -> Score {
    let mut s = Score::blank();
    s.parts.truncate(1);
    s.parts[0].measures.truncate(1);
    s.parts[0].name = "Drums".into();
    s.parts[0].percussion = Some(Staff {
        kit: 0,
        lines,
        sounds: vec![
            Sound {
                id: 35,
                name: "Bass drum".into(),
                midi: Some(36),
                position: 0,
                notehead: Notehead::Normal,
            },
            Sound {
                id: 38,
                name: "Snare".into(),
                midi: Some(38),
                position: 4,
                notehead: Notehead::Normal,
            },
            Sound {
                id: 42,
                name: "Closed hat".into(),
                midi: Some(42),
                position: 8,
                notehead: Notehead::Cross,
            },
            Sound {
                id: 46,
                name: "Open hat".into(),
                midi: Some(46),
                position: 8,
                notehead: Notehead::Diamond,
            },
        ],
    });
    s.parts[0].measures[0].notes = vec![n(0, 0, 0, &[42])];
    s
}
#[test]
fn sound_identity_and_playback_mapping_do_not_change_printed_position_or_accidentals() {
    let mut s = fixture(5);
    s.key_fifths = 6;
    s.validate().unwrap();
    let pages = engrave(&s, &EngravingOptions::default());
    let r = &pages[0].staves[0];
    let h = &pages[0].sound_hits[0];
    assert_eq!(h.sound_id, 42);
    assert!((h.y - (r.bottom_line_y() - 40.0)).abs() < 0.001);
    let original = svg(&pages[0]);
    s.key_fifths = 0;
    s.parts[0].percussion.as_mut().unwrap().sounds[2].midi = Some(76);
    assert_eq!(original, svg(&engrave(&s, &EngravingOptions::default())[0]));
    let index = display::Index::new(&s);
    assert_eq!(
        index.head(at(0, 0, 0), token(42), Notehead::Normal),
        Notehead::Cross
    );
    assert_eq!(
        index.head(at(0, 0, 0), token(42), Notehead::Square),
        Notehead::Square
    );
}
#[test]
fn single_line_staff_and_scaled_mouse_positions_share_the_same_geometry() {
    let mut s = fixture(1);
    s.parts[0].measures[0].notes[0].pitches = vec![token(35)];
    s.notation.layout.staff_scale = 0.7;
    let pages = engrave(&s, &EngravingOptions::default());
    let p = &pages[0];
    let r = &p.staves[0];
    assert_eq!(r.lines, 1);
    assert!((p.sound_hits[0].y - r.center_y()).abs() < 0.001);
    assert_eq!(r.position_at_y(r.center_y()), 0.0);
    assert!((r.position_at_y(r.center_y() - r.spacing) - 2.0).abs() < 0.001);
    let staff_lines=p.items.iter().filter(|item|matches!(item,Primitive::Line{x1,x2,y1,y2,color,..} if *color==STAFF && (*y1-r.center_y()).abs()<0.001 && (*y2-*y1).abs()<0.001 && (*x2-*x1)>r.width*0.9)).count();
    assert_eq!(staff_lines, 1);
    let bar=p.items.iter().any(|item|matches!(item,Primitive::Line{x1,x2,y1,y2,..}if (*x1-r.x).abs()<0.001 && (*x2-*x1).abs()<0.001 && ((*y2-*y1)-r.spacing).abs()<0.001));
    assert!(bar);
}
#[test]
fn explicit_whole_rest_hangs_from_the_single_staff_line() {
    let mut score = fixture(1);
    let note = &mut score.parts[0].measures[0].notes[0];
    note.pitches.clear();
    note.duration = Duration::Whole;
    let pages = engrave(&score, &EngravingOptions::default());
    let staff = &pages[0].staves[0];
    assert!(pages[0].items.iter().any(|p|matches!(p,Primitive::Path{points,closed:true,..} if points.len()==4 && (points[0].1-staff.center_y()).abs()<0.001 && (points[2].1-staff.center_y()-4.0).abs()<0.001)));
}
#[test]
fn paired_percussion_tie_curves_outside_the_lower_voice() {
    let mut s = fixture(5);
    let mut first = n(0, 0, 0, &[42]);
    first.duration = Duration::Half;
    first.tied = true;
    let mut last = n(960, 0, 0, &[42]);
    last.duration = Duration::Half;
    let mut low = n(0, 0, 1, &[38]);
    low.duration = Duration::Whole;
    s.parts[0].measures[0].notes = vec![first, low, last];
    s.validate().unwrap();
    let pages = engrave(&s, &EngravingOptions::default());
    let top = pages[0]
        .sound_hits
        .iter()
        .find(|h| h.anchor == at(0, 0, 0))
        .unwrap()
        .y;
    assert!(pages[0].items.iter().any(|item|matches!(item,Primitive::Path{points,closed:true,..} if points.len()==41 && (points[0].1-top+7.0).abs()<0.001 && points.iter().all(|p|p.1<=top-7.0+0.001))));
}
#[test]
fn all_lanes_and_equal_position_sound_heads_keep_distinct_source_hits() {
    let mut s = fixture(5);
    for sound in &mut s.parts[0].percussion.as_mut().unwrap().sounds {
        sound.position = 4;
    }
    s.parts[0].measures[0].notes = (0..8)
        .map(|lane| n(0, lane % 4, lane / 4, &[35, 38, 42]))
        .collect();
    s.parts[0].measures[0]
        .notes
        .sort_by_key(|n| (n.start, n.layer, n.voice));
    let mut style = EntryStyle::new(at(0, 0, 0));
    style.stem = StemDirection::Up;
    s.notation.entries.push(style);
    s.validate().unwrap();
    let pages = engrave(&s, &EngravingOptions::default());
    let hits = &pages[0].sound_hits;
    assert_eq!(hits.len(), 24);
    for lane in 0..8 {
        let found = hits
            .iter()
            .filter(|h| h.anchor.lane() == lane)
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 3);
        let x = found.iter().map(|h| h.x.to_bits()).collect::<BTreeSet<_>>();
        assert_eq!(x.len(), 3);
        assert!(found.iter().all(|h| h.anchor.selection(&s).is_some()));
    }
}
#[test]
fn percussion_graces_tuplets_slurs_lyrics_and_ties_use_mapped_positions() {
    let mut s = fixture(5);
    s.parts[0].measures[0].notes = (0..3)
        .map(|i| {
            let mut note = n(i * 160, 0, 0, &[42]);
            note.duration = Duration::Eighth;
            note.tuplet = Some(crate::model::Tuplet::TRIPLET);
            note
        })
        .collect();
    let mut style = EntryStyle::new(at(0, 0, 0));
    style.tie_pitches = Some(vec![42]);
    s.notation.entries.push(style);
    s.notation.slurs.push(Slur::new(at(0, 0, 0), at(320, 0, 0)));
    s.notation.lyrics.push(Lyric::new(at(0, 0, 0), "R"));
    s.notation.grace.push(GraceGroup::new(
        at(160, 0, 0),
        vec![GraceNote::new(token(38), Duration::Sixteenth)],
    ));
    s.validate().unwrap();
    let pages = engrave(&s, &EngravingOptions::default());
    let p = &pages[0];
    let r = &p.staves[0];
    assert!((p.grace_hits[0].y - (r.bottom_line_y() - 20.0)).abs() < 0.001);
    let hit = p
        .slur_hits
        .iter()
        .find(|h| h.handle == SlurHandle::Start)
        .unwrap();
    assert!((hit.y - (r.bottom_line_y() - 40.0 - 10.0)).abs() < 0.001);
    let original = svg(p);
    for sound in &mut s.parts[0].percussion.as_mut().unwrap().sounds {
        sound.midi = None;
    }
    assert_eq!(original, svg(&engrave(&s, &EngravingOptions::default())[0]));
    assert!(!original.contains("NaN"));
}
