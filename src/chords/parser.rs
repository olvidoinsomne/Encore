use super::*;

/// Parse common letter-name chord syntax without copying a proprietary suffix
/// library. The original suffix is retained independently of its semantics.
pub fn parse(input: &str) -> Result<ChordSpec, String> {
    if input.len() > 160 {
        return Err("A chord symbol may contain at most 160 bytes.".into());
    }
    let input = input.trim();
    if ["NC", "N.C.", "N.C", "N/C"].contains(&input.to_ascii_uppercase().as_str()) {
        return Ok(ChordSpec {
            root: None,
            kind: ChordKind::None,
            bass: None,
            degrees: vec![],
            suffix_text: None,
        });
    }
    let (root, used) = pitch_prefix(input)?;
    let mut suffix = input[used..].trim();
    let mut bass = None;
    for (index, character) in suffix.char_indices().rev() {
        if matches!(character, '/' | '_' | '|') {
            let tail = suffix[index + character.len_utf8()..].trim();
            if let Ok((pitch, length)) = pitch_prefix(tail)
                && length == tail.len()
            {
                bass = Some(pitch);
                suffix = suffix[..index].trim();
                break;
            }
        }
    }
    let original = suffix.to_owned();
    let mut depth = 0i8;
    for character in suffix.chars() {
        match character {
            '(' => {
                depth += 1;
                if depth > 2 {
                    return Err("Chord suffix parentheses are nested too deeply.".into());
                }
            }
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err("Unmatched closing chord parenthesis.".into());
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("Close every chord suffix parenthesis before applying it.".into());
    }
    let normalized = suffix
        .replace('♭', "b")
        .replace('♯', "#")
        .replace(['−', '–'], "-")
        .replace(['Δ', '△'], "maj")
        .replace(['°', 'º'], "dim")
        .replace('ø', "m7b5")
        .replace(' ', "");
    let aliases: &[(&str, ChordKind)] = &[
        ("m(maj7)", ChordKind::MajorMinor),
        ("minMaj7", ChordKind::MajorMinor),
        ("mMaj7", ChordKind::MajorMinor),
        ("mM7", ChordKind::MajorMinor),
        ("maj13", ChordKind::Major13th),
        ("maj11", ChordKind::Major11th),
        ("maj9", ChordKind::MajorNinth),
        ("maj7", ChordKind::MajorSeventh),
        ("min13", ChordKind::Minor13th),
        ("min11", ChordKind::Minor11th),
        ("min9", ChordKind::MinorNinth),
        ("min7", ChordKind::MinorSeventh),
        ("min6", ChordKind::MinorSixth),
        ("m7b5", ChordKind::HalfDiminished),
        ("dim7", ChordKind::DiminishedSeventh),
        ("aug7", ChordKind::AugmentedSeventh),
        ("sus2", ChordKind::SuspendedSecond),
        ("sus4", ChordKind::SuspendedFourth),
        ("sus", ChordKind::SuspendedFourth),
        ("m13", ChordKind::Minor13th),
        ("m11", ChordKind::Minor11th),
        ("m9", ChordKind::MinorNinth),
        ("m7", ChordKind::MinorSeventh),
        ("m6", ChordKind::MinorSixth),
        ("M13", ChordKind::Major13th),
        ("M11", ChordKind::Major11th),
        ("M9", ChordKind::MajorNinth),
        ("M7", ChordKind::MajorSeventh),
        ("13", ChordKind::Dominant13th),
        ("11", ChordKind::Dominant11th),
        ("9", ChordKind::DominantNinth),
        ("7", ChordKind::Dominant),
        ("6", ChordKind::MajorSixth),
        ("-13", ChordKind::Minor13th),
        ("-11", ChordKind::Minor11th),
        ("-9", ChordKind::MinorNinth),
        ("-7", ChordKind::MinorSeventh),
        ("-6", ChordKind::MinorSixth),
        ("+7", ChordKind::AugmentedSeventh),
        ("o7", ChordKind::DiminishedSeventh),
        ("dim", ChordKind::Diminished),
        ("aug", ChordKind::Augmented),
        ("major", ChordKind::Major),
        ("minor", ChordKind::Minor),
        ("maj", ChordKind::Major),
        ("min", ChordKind::Minor),
        ("m", ChordKind::Minor),
        ("-", ChordKind::Minor),
        ("+", ChordKind::Augmented),
        ("o", ChordKind::Diminished),
        ("5", ChordKind::Power),
        ("pedal", ChordKind::Pedal),
        ("M", ChordKind::Major),
        ("It6", ChordKind::Italian),
        ("Fr6", ChordKind::French),
        ("Ger6", ChordKind::German),
        ("", ChordKind::Major),
    ];
    let Some((prefix, kind)) = aliases
        .iter()
        .find(|(prefix, _)| normalized.starts_with(prefix))
    else {
        return Err(
            "Unknown chord suffix. Use a standard suffix such as m7, maj7, sus4, 7(b9), or add9."
                .into(),
        );
    };
    let mut remaining = &normalized[prefix.len()..];
    let mut degrees = Vec::new();
    if remaining.starts_with("/9") {
        if !matches!(kind, ChordKind::MajorSixth | ChordKind::MinorSixth) {
            return Err("Use 6/9 or m6/9 for a sixth/ninth chord.".into());
        }
        degrees.push(ChordDegree {
            value: 9,
            alter: 0,
            operation: DegreeOperation::Add,
        });
        remaining = &remaining[2..];
    }
    while !remaining.is_empty() {
        remaining = remaining.trim_start_matches(['(', ',', ')']);
        if remaining.is_empty() {
            break;
        }
        if let Some(tail) = remaining.strip_prefix("sus2") {
            degrees.push(ChordDegree {
                value: 3,
                alter: 0,
                operation: DegreeOperation::Subtract,
            });
            degrees.push(ChordDegree {
                value: 2,
                alter: 0,
                operation: DegreeOperation::Add,
            });
            remaining = tail;
            continue;
        }
        if let Some(tail) = remaining
            .strip_prefix("sus4")
            .or_else(|| remaining.strip_prefix("sus"))
        {
            degrees.push(ChordDegree {
                value: 3,
                alter: 0,
                operation: DegreeOperation::Subtract,
            });
            degrees.push(ChordDegree {
                value: 4,
                alter: 0,
                operation: DegreeOperation::Add,
            });
            remaining = tail;
            continue;
        }
        let (operation, tail) = if let Some(tail) = remaining.strip_prefix("add") {
            (Some(DegreeOperation::Add), tail)
        } else if let Some(tail) = remaining
            .strip_prefix("omit")
            .or_else(|| remaining.strip_prefix("no"))
        {
            (Some(DegreeOperation::Subtract), tail)
        } else {
            (None, remaining)
        };
        let (alter, tail) = if let Some(t) = tail.strip_prefix("bb") {
            (-2, t)
        } else if let Some(t) = tail.strip_prefix("##") {
            (2, t)
        } else if let Some(t) = tail.strip_prefix('b') {
            (-1, t)
        } else if let Some(t) = tail.strip_prefix('#') {
            (1, t)
        } else {
            (0, tail)
        };
        let length = tail.bytes().take_while(u8::is_ascii_digit).count();
        if length == 0 {
            return Err(format!(
                "Unknown chord suffix near '{remaining}'. Use add9, no3, b5, or #11."
            ));
        }
        let value = tail[..length]
            .parse::<u8>()
            .map_err(|_| "Invalid chord degree.")?;
        if !(1..=13).contains(&value) {
            return Err("Chord degrees must be between 1 and 13.".into());
        }
        let operation = operation.unwrap_or_else(|| {
            if kind.tones().iter().any(|(degree, _)| *degree == value) {
                DegreeOperation::Alter
            } else {
                DegreeOperation::Add
            }
        });
        degrees.push(ChordDegree {
            value,
            alter,
            operation,
        });
        remaining = &tail[length..];
    }
    let chord = ChordSpec {
        root: Some(root),
        kind: *kind,
        bass,
        degrees,
        suffix_text: Some(original),
    };
    chord.validate()?;
    Ok(chord)
}

pub(super) fn pitch_prefix(input: &str) -> Result<(PitchClass, usize), String> {
    let Some(first) = input.chars().next() else {
        return Err("Enter a chord such as C, Am7, or Bb7/D.".into());
    };
    let step = match first.to_ascii_uppercase() {
        'C' => Step::C,
        'D' => Step::D,
        'E' => Step::E,
        'F' => Step::F,
        'G' => Step::G,
        'A' => Step::A,
        'B' => Step::B,
        _ => return Err("A chord must start with A–G or N.C.".into()),
    };
    let mut used = first.len_utf8();
    let mut alter = 0;
    for character in input[used..].chars() {
        let change = match character {
            'b' | '♭' => -1,
            '#' | '♯' => 1,
            '𝄫' => -2,
            '𝄪' | 'x' => 2,
            _ => break,
        };
        alter += change;
        used += character.len_utf8();
    }
    if !(-2..=2).contains(&alter) {
        return Err("Chord roots support at most double accidentals.".into());
    }
    Ok((PitchClass { step, alter }, used))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_chords_and_slash_ninth_are_distinct() {
        for (text, kind) in [
            ("C", ChordKind::Major),
            ("Am", ChordKind::Minor),
            ("F#m7b5", ChordKind::HalfDiminished),
            ("DΔ7", ChordKind::MajorSeventh),
            ("Eb13", ChordKind::Dominant13th),
            ("G7sus4", ChordKind::Dominant),
            ("C6/9/E", ChordKind::MajorSixth),
        ] {
            let chord = parse(text).unwrap();
            assert_eq!(chord.kind, kind, "{text}");
            assert_eq!(chord.label(), text);
        }
        assert!(parse("C6/9").unwrap().bass.is_none());
        assert_eq!(parse("C6/9/E").unwrap().bass.unwrap().step, Step::E);
    }
    #[test]
    fn altered_degrees_retain_semantics() {
        let chord = parse("C7(b9,#11,no5)").unwrap();
        assert_eq!(chord.degrees[0].operation, DegreeOperation::Add);
        assert_eq!(chord.degrees[2].operation, DegreeOperation::Subtract);
        assert_eq!(pitches(&chord), vec![0, 1, 4, 6, 10]);
        let chord = parse("C13b9").unwrap();
        assert_eq!(chord.degrees[0].operation, DegreeOperation::Alter);
    }
    #[test]
    fn invalid_suffixes_are_rejected_instead_of_silently_changed() {
        for input in [
            "", "Qm", "Cbananas", "Cadd0", "Cbbbbb", "C7add999", "C7b9b9",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }
}
