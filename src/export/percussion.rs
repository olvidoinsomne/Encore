//! Portable unpitched MusicXML representation, separate from sound-ID tokens.
use crate::{entry_style::Notehead, model::Part, percussion::Sound};
use std::fmt::Write as _;
pub(crate) fn pitch(sound: &Sound) -> String {
    let p = sound.display_pitch();
    format!(
        "<unpitched><display-step>{}</display-step><display-octave>{}</display-octave></unpitched>",
        p.step.name(),
        p.octave
    )
}
pub(crate) fn instrument(part: usize, id: u8) -> String {
    format!("<instrument id=\"P{}-S{id}\"/>", part + 1)
}
pub(crate) fn notehead(head: Notehead, scale: f32) -> String {
    let name = match head {
        Notehead::Normal => "normal",
        Notehead::Cross => "x",
        Notehead::Diamond => "diamond",
        Notehead::Triangle => "triangle",
        Notehead::Slash => "slash",
        Notehead::Square => "square",
    };
    format!("<notehead font-size=\"{}\">{name}</notehead>", 20. * scale)
}
pub(super) fn device(part: usize) -> String {
    let port = part * 2 + 1;
    format!(
        "<midi-device{}>Encore staff {}</midi-device>",
        if port <= 16 {
            format!(" port=\"{port}\"")
        } else {
            String::new()
        },
        part + 1
    )
}
pub(super) fn part_list(part: usize, p: &Part, volume: f32) -> String {
    let staff = p.percussion.as_ref().unwrap();
    let id = part + 1;
    let mut out = format!(
        "<score-part id=\"P{id}\"><part-name>{}</part-name><part-abbreviation>Perc.</part-abbreviation>",
        super::xml(&p.name)
    );
    for sound in &staff.sounds {
        let _ = write!(
            out,
            "<score-instrument id=\"P{id}-S{}\"><instrument-name>{}</instrument-name></score-instrument>",
            sound.id,
            super::xml(&sound.name)
        );
    }
    out.push_str(&device(part));
    for sound in &staff.sounds {
        let _ = write!(
            out,
            "<midi-instrument id=\"P{id}-S{}\"><midi-channel>10</midi-channel><midi-program>{}</midi-program>{}<volume>{volume:.2}</volume><pan>{:.2}</pan></midi-instrument>",
            sound.id,
            u16::from(staff.kit) + 1,
            sound.midi.map_or(String::new(), |key| format!(
                "<midi-unpitched>{}</midi-unpitched>",
                u16::from(key) + 1
            )),
            p.pan * 90.
        );
    }
    out.push_str("</score-part>\n");
    out
}
pub(super) fn native(part: &Part) -> String {
    part.percussion.as_ref().map_or(String::new(),|staff|{let json=serde_json::to_string(staff).unwrap_or_default();format!("<direction><direction-type><other-direction print-object=\"no\">encore-percussion:{}</other-direction></direction-type></direction>",super::xml(&json))})
}
