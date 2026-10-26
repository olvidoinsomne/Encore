//! Standard MusicXML harmony/frame interchange. No private Finale data is used.
use super::*;
use roxmltree::Node;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn child<'a, 'i>(node: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| n.has_tag_name(tag))
}
fn text(node: Node<'_, '_>, tag: &str) -> Option<String> {
    child(node, tag).map(|n| n.text().unwrap_or("").to_owned())
}
fn number<T: std::str::FromStr>(node: Node<'_, '_>, tag: &str) -> Result<Option<T>, String> {
    text(node, tag)
        .map(|value| {
            value
                .parse()
                .map_err(|_| format!("Invalid MusicXML chord {tag}."))
        })
        .transpose()
}
fn pitch(node: Node<'_, '_>, prefix: &str) -> Result<PitchClass, String> {
    let value = text(node, &format!("{prefix}-step"))
        .ok_or("A MusicXML chord pitch is missing its step.")?;
    let step = match value.as_str() {
        "C" => Step::C,
        "D" => Step::D,
        "E" => Step::E,
        "F" => Step::F,
        "G" => Step::G,
        "A" => Step::A,
        "B" => Step::B,
        _ => return Err("Invalid MusicXML chord pitch step.".into()),
    };
    let alter = number::<f32>(node, &format!("{prefix}-alter"))?.unwrap_or(0.);
    if !alter.is_finite() || alter.fract() != 0. || !(-2.0..=2.0).contains(&alter) {
        return Err(
            "Microtonal chord roots cannot be represented by Encore's semitone chord model.".into(),
        );
    }
    Ok(PitchClass {
        step,
        alter: alter as i8,
    })
}

pub fn musicxml(score: &Score, part: usize, measure: usize) -> String {
    let mut out = String::new();
    let mut symbols = score
        .notation
        .chords
        .iter()
        .filter(|s| s.anchor.part == part && s.anchor.measure == measure)
        .collect::<Vec<_>>();
    symbols.sort_by_key(|s| (s.anchor.tick, s.slot));
    for symbol in symbols {
        out.push_str(&format!("      <harmony print-object=\"{}\" print-frame=\"{}\" relative-x=\"{}\" relative-y=\"{}\">\n",if symbol.visible{"yes"}else{"no"},if symbol.show_fretboard{"yes"}else{"no"},symbol.offset[0],-symbol.offset[1]));
        let root = symbol.chord.root.unwrap_or(PitchClass {
            step: Step::C,
            alter: 0,
        });
        out.push_str(&format!(
            "        <root><root-step{}>{}</root-step>{}</root>\n",
            if symbol.chord.kind == ChordKind::None {
                " text=\"\""
            } else {
                ""
            },
            root.step.name(),
            if root.alter != 0 {
                format!("<root-alter>{}</root-alter>", root.alter)
            } else {
                String::new()
            }
        ));
        out.push_str(&format!(
            "        <kind text=\"{}\">{}</kind>\n",
            escape(&symbol.chord.suffix()),
            symbol.chord.kind.xml()
        ));
        if let Some(bass) = symbol.chord.bass {
            out.push_str(&format!(
                "        <bass><bass-step>{}</bass-step>{}</bass>\n",
                bass.step.name(),
                if bass.alter != 0 {
                    format!("<bass-alter>{}</bass-alter>", bass.alter)
                } else {
                    String::new()
                }
            ));
        }
        for degree in &symbol.chord.degrees {
            out.push_str(&format!("        <degree><degree-value>{}</degree-value><degree-alter>{}</degree-alter><degree-type>{}</degree-type></degree>\n",degree.value,degree.alter,match degree.operation{DegreeOperation::Add=>"add",DegreeOperation::Alter=>"alter",DegreeOperation::Subtract=>"subtract"}));
        }
        if let Some(frame) = &symbol.fretboard {
            out.push_str(&frame_xml(frame));
        }
        out.push_str(&format!(
            "        <offset>{}</offset>\n      </harmony>\n",
            symbol.anchor.tick
        ));
    }
    out
}
fn frame_xml(frame: &Fretboard) -> String {
    let mut out = format!(
        "        <frame><frame-strings>{}</frame-strings><frame-frets>{}</frame-frets><first-fret>{}</first-fret>",
        frame.frets.len(),
        frame.displayed_frets,
        frame.first_fret
    );
    for (index, fret) in frame.frets.iter().enumerate() {
        let Some(fret) = fret else { continue };
        out.push_str(&format!(
            "<frame-note><string>{}</string><fret>{fret}</fret>",
            frame.frets.len() - index
        ));
        if let Some(finger) = frame.fingers.get(index).copied().flatten() {
            out.push_str(&format!("<fingering>{finger}</fingering>"));
        }
        for barre in &frame.barres {
            if barre.from == index {
                out.push_str("<barre type=\"start\"/>");
            }
            if barre.to == index {
                out.push_str("<barre type=\"stop\"/>");
            }
        }
        out.push_str("</frame-note>");
    }
    out.push_str("</frame>\n");
    out
}

/// Parse one or more stacked harmony chords. Its caller resolves XML cursor,
/// staff mapping and written-to-concert transposition before document commit.
pub fn read_harmony(node: Node<'_, '_>, anchor: Anchor) -> Result<Vec<ChordSymbol>, String> {
    if node
        .children()
        .any(|n| n.has_tag_name("numeral") || n.has_tag_name("function"))
    {
        return Err(
            "Functional/Nashville harmony is not yet supported; its chord symbols were omitted."
                .into(),
        );
    }
    let mut symbols: Vec<ChordSymbol> = Vec::new();
    let mut current: Option<ChordSpec> = None;
    let mut inversion = None;
    let mut has_kind = false;
    for element in node.children().filter(|n| n.is_element()) {
        match element.tag_name().name() {
            "root" => {
                if let Some(chord) = current.take() {
                    if !has_kind {
                        return Err("MusicXML harmony is missing its required kind.".into());
                    }
                    finish_spec(chord, inversion.take(), anchor, &mut symbols)?;
                }
                has_kind = false;
                current = Some(ChordSpec {
                    root: Some(pitch(element, "root")?),
                    kind: ChordKind::Major,
                    bass: None,
                    degrees: vec![],
                    suffix_text: None,
                });
            }
            "kind" => {
                if has_kind {
                    return Err("MusicXML harmony has a repeated kind without a new root.".into());
                }
                has_kind = true;
                let chord = current
                    .as_mut()
                    .ok_or("MusicXML harmony kind has no root.")?;
                chord.kind = ChordKind::from_xml(element.text().unwrap_or(""))
                    .ok_or("Unknown MusicXML chord kind.")?;
                chord.suffix_text = element.attribute("text").map(str::to_owned);
                if chord.kind == ChordKind::None {
                    chord.root = None;
                }
            }
            "bass" => {
                current
                    .as_mut()
                    .ok_or("MusicXML harmony bass has no root.")?
                    .bass = Some(pitch(element, "bass")?);
            }
            "inversion" => {
                inversion = Some(
                    element
                        .text()
                        .unwrap_or("")
                        .parse::<usize>()
                        .map_err(|_| "Invalid chord inversion.")?,
                )
            }
            "degree" => {
                let chord = current
                    .as_mut()
                    .ok_or("MusicXML degree has no chord root.")?;
                let value = number::<u8>(element, "degree-value")?
                    .ok_or("A chord degree is missing its value.")?;
                let alter = number::<f32>(element, "degree-alter")?
                    .ok_or("A chord degree is missing its alteration.")?;
                if !alter.is_finite() || alter.fract() != 0. || !(-2.0..=2.0).contains(&alter) {
                    return Err("Microtonal chord degrees are not supported.".into());
                }
                let operation = match text(element, "degree-type").as_deref() {
                    Some("add") => DegreeOperation::Add,
                    Some("alter") => DegreeOperation::Alter,
                    Some("subtract") => DegreeOperation::Subtract,
                    _ => return Err("Invalid MusicXML degree operation.".into()),
                };
                chord.degrees.push(ChordDegree {
                    value,
                    alter: alter as i8,
                    operation,
                });
            }
            _ => {}
        }
    }
    if let Some(chord) = current {
        if !has_kind {
            return Err("MusicXML harmony is missing its required kind.".into());
        }
        finish_spec(chord, inversion, anchor, &mut symbols)?;
    }
    if symbols.is_empty() {
        return Err("MusicXML harmony contains no supported chord root.".into());
    }
    let offset = |name: &str| -> Result<f32, String> {
        node.attribute(name)
            .unwrap_or("0")
            .parse::<f32>()
            .map_err(|_| "Invalid chord display offset.".into())
    };
    for symbol in &mut symbols {
        symbol.visible = node.attribute("print-object") != Some("no");
        symbol.show_fretboard = node.attribute("print-frame") != Some("no");
        symbol.offset = [offset("relative-x")?, -offset("relative-y")?];
        symbol.validate()?;
    }
    if let Some(frame) = child(node, "frame") {
        symbols[0].fretboard = Some(read_frame(frame)?);
        symbols[0].validate()?;
    }
    Ok(symbols)
}
fn finish_spec(
    mut chord: ChordSpec,
    inversion: Option<usize>,
    anchor: Anchor,
    symbols: &mut Vec<ChordSymbol>,
) -> Result<(), String> {
    if symbols.len() >= 8 {
        return Err("More than eight stacked chords at one beat are unsupported.".into());
    }
    if chord.bass.is_none()
        && let Some(inversion) = inversion
        && inversion > 0
    {
        let tones = chord.tones();
        let (degree, semitones) = tones
            .get(inversion)
            .copied()
            .ok_or("Chord inversion exceeds its tone count.")?;
        let root = chord.root.ok_or("N.C. cannot have an inversion.")?;
        chord.bass = Some(root.transposed(Transposition {
            semitones,
            diatonic_steps: degree.saturating_sub(1) as i8,
        })?);
    }
    chord.validate()?;
    let mut symbol = ChordSymbol::new(anchor, chord);
    symbol.slot = symbols.len() as u8;
    symbols.push(symbol);
    Ok(())
}
fn read_frame(node: Node<'_, '_>) -> Result<Fretboard, String> {
    let count = number::<usize>(node, "frame-strings")?
        .ok_or("A fretboard is missing its string count.")?;
    if !(4..=12).contains(&count) {
        return Err("Fretboards outside 4–12 strings are unsupported.".into());
    }
    let mut frame = Fretboard {
        frets: vec![None; count],
        fingers: vec![None; count],
        tuning: None,
        first_fret: number(node, "first-fret")?.unwrap_or(1),
        displayed_frets: number(node, "frame-frets")?
            .ok_or("A fretboard is missing its fret count.")?,
        barres: vec![],
    };
    let mut starts = BTreeMap::new();
    for note in node.children().filter(|n| n.has_tag_name("frame-note")) {
        let string =
            number::<usize>(note, "string")?.ok_or("A fretboard note is missing its string.")?;
        if string == 0 || string > count {
            return Err("Fretboard string index is outside its diagram.".into());
        }
        let index = count - string;
        let fret = number::<u8>(note, "fret")?.ok_or("A fretboard note is missing its fret.")?;
        if frame.frets[index].replace(fret).is_some() {
            return Err("Multiple stops on one fretboard string are unsupported.".into());
        }
        frame.fingers[index] = text(note, "fingering").and_then(|f| f.parse().ok());
        for barre in note.children().filter(|n| n.has_tag_name("barre")) {
            match barre.attribute("type") {
                Some("start") => {
                    starts.insert(fret, index);
                }
                Some("stop") => {
                    let start = starts
                        .remove(&fret)
                        .ok_or("Fretboard barre has no start.")?;
                    frame.barres.push(Barre {
                        from: start.min(index),
                        to: start.max(index),
                        fret,
                    });
                }
                _ => return Err("Invalid fretboard barre type.".into()),
            }
        }
    }
    if !starts.is_empty() {
        return Err("Fretboard barre is missing its ending.".into());
    }
    frame.validate()?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_harmony_frame_round_trip_preserves_semantics() {
        let mut score = Score::blank();
        let mut symbol = ChordSymbol::new(
            Anchor {
                voice: 0,
                part: 0,
                measure: 0,
                tick: 240,
                layer: 0,
            },
            parse("C7(b9)/E").unwrap(),
        );
        symbol.fretboard = Some(Fretboard::guitar(vec![
            None,
            Some(3),
            Some(2),
            Some(3),
            Some(2),
            Some(0),
        ]));
        symbol.visible = false;
        score.notation.chords.push(symbol.clone());
        let xml = format!("<measure>{}</measure>", musicxml(&score, 0, 0));
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let result = read_harmony(
            doc.root_element().first_element_child().unwrap(),
            symbol.anchor,
        )
        .unwrap();
        assert_eq!(result[0].chord, symbol.chord);
        assert_eq!(
            result[0].fretboard.as_ref().unwrap().frets,
            symbol.fretboard.unwrap().frets
        );
        assert!(result[0].fretboard.as_ref().unwrap().tuning.is_none());
        assert!(!result[0].visible);
    }
    #[test]
    fn no_chord_exports_valid_required_root() {
        let mut score = Score::blank();
        score.notation.chords.push(ChordSymbol::new(
            Anchor {
                voice: 0,
                part: 0,
                measure: 0,
                tick: 0,
                layer: 0,
            },
            parse("N.C.").unwrap(),
        ));
        let xml = format!("<measure>{}</measure>", musicxml(&score, 0, 0));
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(
            read_harmony(
                doc.root_element().first_element_child().unwrap(),
                score.notation.chords[0].anchor
            )
            .unwrap()[0]
                .chord
                .kind,
            ChordKind::None
        );
    }
}
