//! Finale 27 entry shortcuts. Simple Entry uses its documented Laptop set;
//! Speedy Entry has a separate map because the same keys have different jobs.
use eframe::egui::{Key, Modifiers};
use encore::model::{Duration, Step};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntryProfile {
    #[default]
    SimpleLaptop,
    Speedy,
}

impl EntryProfile {
    pub fn key(self) -> &'static str {
        match self {
            Self::SimpleLaptop => "simple-laptop",
            Self::Speedy => "speedy",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::SimpleLaptop => "Simple Entry · Laptop",
            Self::Speedy => "Speedy Entry",
        }
    }
    pub fn from_key(key: Option<&str>) -> Self {
        match key {
            Some("speedy") => Self::Speedy,
            _ => Self::SimpleLaptop,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryCommand {
    Style(StyleCommand),
    EditExpressions,
    EditArticulations,
    Duration(Duration, bool), // bool: alter selected entry rather than tool
    SpeedyDuration(Duration),
    SpeedyRest(Duration),
    Pitch(Step, bool), // bool: add chord tone
    /// Add the caret pitch to the selected entry.
    Chord,
    /// A signed diatonic interval number: +2 is a second above; -9 a ninth below.
    Interval(i8),
    Enter,
    EnterRest,
    ToggleRest,
    ToggleRestTool,
    Dot(bool),      // bool: tool only
    Tie(bool),      // bool: previous entry
    Accidental(i8), // chromatic increment; 0 sets natural
    SetAccidental(i8),
    ToolAccidental(i8),
    Diatonic(i32),
    Octave(i8),
    Navigate(i8),
    Measure(i8),
    /// First entry (-1) or just after the last entry (+1) in this measure.
    Boundary(i8),
    Staff(i8),
    Layer(i8),
    ToggleVoice,
    Delete,
    RemoveTone,
    ClearSelection,
    Triplet,
    ExitEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleCommand {
    FlipStem,
    DefaultStem,
    CycleBeam,
    DefaultBeam,
    FlatBeam,
    ToggleHidden,
    Respell,
    ToggleAccidental,
    ToggleCourtesy,
    ToggleParentheses,
    DefaultAccidental,
    FlipTie,
    DefaultTie,
}

pub fn duration_digit(duration: Duration) -> &'static str {
    match duration {
        Duration::OneTwentyEighth => "0",
        Duration::SixtyFourth => "1",
        Duration::ThirtySecond => "2",
        Duration::Sixteenth => "3",
        Duration::Eighth => "4",
        Duration::Quarter => "5",
        Duration::Half => "6",
        Duration::Whole => "7",
        Duration::DoubleWhole => "8",
    }
}

fn digit(key: Key) -> Option<u8> {
    Some(match key {
        Key::Num0 => 0,
        Key::Num1 => 1,
        Key::Num2 => 2,
        Key::Num3 => 3,
        Key::Num4 => 4,
        Key::Num5 => 5,
        Key::Num6 => 6,
        Key::Num7 => 7,
        Key::Num8 => 8,
        _ => return None,
    })
}

fn duration(number: u8) -> Duration {
    [
        Duration::OneTwentyEighth,
        Duration::SixtyFourth,
        Duration::ThirtySecond,
        Duration::Sixteenth,
        Duration::Eighth,
        Duration::Quarter,
        Duration::Half,
        Duration::Whole,
        Duration::DoubleWhole,
    ][number as usize]
}

/// Document-wide layer selection uses distinct platform modifiers.
pub fn layer_command(key: Key, modifiers: Modifiers, mac: bool) -> Option<u8> {
    let matches = if mac {
        modifiers.mac_cmd && modifiers.alt && !modifiers.ctrl && !modifiers.shift
    } else {
        modifiers.alt && modifiers.shift && !modifiers.ctrl && !modifiers.mac_cmd
    };
    matches
        .then(|| digit(key))
        .flatten()
        .filter(|number| (1..=4).contains(number))
        .map(|number| number - 1)
}

pub fn entry_command(
    profile: EntryProfile,
    key: Key,
    modifiers: Modifiers,
    mac: bool,
) -> Option<EntryCommand> {
    use EntryCommand::*;
    let primary = if mac {
        modifiers.mac_cmd
    } else {
        modifiers.ctrl
    };
    let alternate = modifiers.alt;
    let shift = modifiers.shift;
    if profile == EntryProfile::Speedy
        && primary
        && !alternate
        && (!mac || !modifiers.ctrl)
        && key == Key::F
    {
        return Some(Style(if shift {
            StyleCommand::DefaultTie
        } else {
            StyleCommand::FlipTie
        }));
    }
    if profile == EntryProfile::SimpleLaptop {
        if primary {
            if alternate || (mac && modifiers.ctrl) {
                return None;
            }
            return match (key, shift) {
                (Key::F, false) => Some(Style(StyleCommand::FlipTie)),
                (Key::F, true) => Some(Style(StyleCommand::DefaultTie)),
                (Key::Minus, true) => Some(Style(StyleCommand::ToggleCourtesy)),
                (Key::Enter, false) => Some(Chord),
                (Key::ArrowLeft, false) => Some(Measure(-1)),
                (Key::ArrowRight, false) => Some(Measure(1)),
                (Key::F9, true) => Some(Interval(9)),
                _ => None,
            };
        }
        // Physical Control is distinct from Command on macOS. Do not let an
        // unsupported Control/AltGr chord fall through into plain note input.
        if mac && modifiers.ctrl {
            return None;
        }
        if alternate {
            if shift {
                return None;
            }
            if let Some(number) = digit(key) {
                return Some(Duration(duration(number), true));
            }
            return match key {
                Key::Slash => Some(Style(StyleCommand::FlatBeam)),
                Key::E => Some(Style(StyleCommand::Respell)),
                Key::X => Some(EditExpressions),
                Key::Minus => Some(ToolAccidental(-1)),
                Key::Equals => Some(ToolAccidental(1)),
                Key::N => Some(ToolAccidental(0)),
                Key::R => Some(ToggleRestTool),
                Key::Enter => Some(EnterRest),
                _ => None,
            };
        }
        if let Some(interval) = function_interval(key) {
            return match (interval, shift) {
                (2..=8, false) => Some(Interval(interval)),
                (2..=9, true) => Some(Interval(-interval)),
                _ => None,
            };
        }
        if let Some(step) = match key {
            Key::A => Some(Step::A),
            Key::B => Some(Step::B),
            Key::C => Some(Step::C),
            Key::D => Some(Step::D),
            Key::E => Some(Step::E),
            Key::F => Some(Step::F),
            Key::G => Some(Step::G),
            _ => None,
        } {
            return Some(Pitch(step, shift));
        }
        if let Some(number) = digit(key) {
            return (!shift).then(|| Duration(duration(number), false));
        }
        return match key {
            Key::L => Some(Style(if shift {
                StyleCommand::DefaultStem
            } else {
                StyleCommand::FlipStem
            })),
            Key::H if !shift => Some(Style(StyleCommand::ToggleHidden)),
            Key::P if !shift => Some(Style(StyleCommand::ToggleParentheses)),
            Key::Slash => Some(Style(if shift {
                StyleCommand::DefaultBeam
            } else {
                StyleCommand::CycleBeam
            })),
            Key::Backslash if !shift => Some(Style(StyleCommand::Respell)),
            Key::X if !shift => Some(EditExpressions),
            Key::Backtick if !shift => Some(EditArticulations),
            Key::Enter if shift => Some(EnterRest),
            Key::Enter => Some(Enter),
            Key::Tab if !shift => Some(EnterRest),
            Key::R if !shift => Some(ToggleRest),
            Key::Period => Some(Dot(shift)),
            Key::T => Some(Tie(shift)),
            Key::Equals | Key::Plus if !shift => Some(Accidental(1)),
            Key::Minus if !shift => Some(Accidental(-1)),
            Key::N if !shift => Some(Accidental(0)),
            Key::ArrowUp if shift => Some(Octave(1)),
            Key::ArrowDown if shift => Some(Octave(-1)),
            Key::ArrowUp => Some(Diatonic(1)),
            Key::ArrowDown => Some(Diatonic(-1)),
            Key::ArrowLeft if !shift => Some(Navigate(-1)),
            Key::ArrowRight if !shift => Some(Navigate(1)),
            Key::Backspace if !mac && !shift => Some(ClearSelection),
            Key::Backspace if !mac && shift => Some(Delete),
            Key::Delete | Key::Backspace if mac && shift => Some(ClearSelection),
            Key::Backspace if mac => Some(Delete),
            Key::Delete if !shift => Some(Delete),
            Key::Num9 if !shift => Some(Triplet),
            _ => None,
        };
    }

    // Speedy has its own modifier vocabulary: on macOS Option is used for
    // tuplets and measure boundaries, while Command remains an app modifier.
    let rest_modifiers = if mac {
        !primary && modifiers.ctrl && alternate && shift
    } else {
        primary && !alternate && shift
    };
    if rest_modifiers {
        return digit(key)
            .filter(|number| (1..=7).contains(number))
            .map(|number| SpeedyRest(duration(number)));
    }
    let edit_modifier = if mac {
        alternate && !primary && !modifiers.ctrl
    } else {
        primary && !alternate
    };
    if edit_modifier && !shift {
        return match key {
            Key::L => Some(Style(StyleCommand::DefaultStem)),
            Key::F if !mac => Some(Style(StyleCommand::FlipTie)),
            Key::Num0 => Some(SpeedyDuration(encore::model::Duration::OneTwentyEighth)),
            Key::Num3 => Some(Triplet),
            Key::ArrowLeft => Some(Boundary(-1)),
            Key::ArrowRight => Some(Boundary(1)),
            Key::Equals => Some(Tie(true)),
            _ => None,
        };
    }
    if primary || alternate || (mac && modifiers.ctrl) {
        return None;
    }
    if shift {
        return match key {
            Key::A => Some(Style(StyleCommand::ToggleCourtesy)),
            Key::B => Some(Style(StyleCommand::DefaultBeam)),
            Key::M => Some(Style(StyleCommand::FlatBeam)),
            Key::ArrowLeft => Some(Measure(-1)),
            Key::ArrowRight => Some(Measure(1)),
            Key::ArrowUp if !mac => Some(Staff(-1)),
            Key::ArrowDown if !mac => Some(Staff(1)),
            Key::Enter if mac => Some(Staff(-1)),
            Key::ArrowUp if mac => Some(Layer(1)),
            Key::ArrowDown if mac => Some(Layer(-1)),
            Key::Quote if !mac => Some(Layer(1)),
            Key::Backspace | Key::Delete if mac => Some(RemoveTone),
            Key::T => Some(Tie(true)),
            Key::S | Key::Plus | Key::Equals => Some(Accidental(1)),
            Key::F => Some(Accidental(-1)),
            // Shift+duration is an insertion workflow and must never replace a note.
            _ => None,
        };
    }
    if let Some(number) = digit(key).filter(|n| *n > 0) {
        return Some(SpeedyDuration(duration(number)));
    }
    match key {
        Key::Quote => Some(ToggleVoice),
        Key::L => Some(Style(StyleCommand::FlipStem)),
        Key::Slash | Key::B => Some(Style(StyleCommand::CycleBeam)),
        Key::Backslash => Some(Style(StyleCommand::FlatBeam)),
        Key::H | Key::O => Some(Style(StyleCommand::ToggleHidden)),
        Key::Num9 => Some(Style(StyleCommand::Respell)),
        Key::P => Some(Style(StyleCommand::ToggleParentheses)),
        Key::Num0 => Some(ExitEntry),
        Key::Enter if mac => Some(Staff(1)),
        Key::Enter => Some(Chord),
        Key::Backspace if !mac => Some(RemoveTone),
        Key::Backspace if mac => Some(Delete),
        Key::Delete => Some(Delete),
        Key::R => Some(ToggleRest),
        Key::Period => Some(Dot(false)),
        Key::T | Key::Equals => Some(Tie(false)),
        Key::Plus => Some(Accidental(1)),
        Key::Minus => Some(Accidental(-1)),
        Key::N => Some(SetAccidental(0)),
        Key::S => Some(SetAccidental(1)),
        Key::F => Some(SetAccidental(-1)),
        Key::X => Some(SetAccidental(2)),
        Key::V => Some(SetAccidental(-2)),
        Key::ArrowUp => Some(Diatonic(1)),
        Key::ArrowDown => Some(Diatonic(-1)),
        Key::ArrowLeft => Some(Navigate(-1)),
        Key::ArrowRight => Some(Navigate(1)),
        Key::OpenBracket => Some(Measure(-1)),
        Key::CloseBracket => Some(Measure(1)),
        _ => None,
    }
}

fn function_interval(key: Key) -> Option<i8> {
    Some(match key {
        Key::F2 => 2,
        Key::F3 => 3,
        Key::F4 => 4,
        Key::F5 => 5,
        Key::F6 => 6,
        Key::F7 => 7,
        Key::F8 => 8,
        Key::F9 => 9,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simple_laptop_has_finale_duration_order_and_alt_edits_selection() {
        let plain = Modifiers::NONE;
        for (key, d) in [
            (Key::Num0, Duration::OneTwentyEighth),
            (Key::Num1, Duration::SixtyFourth),
            (Key::Num2, Duration::ThirtySecond),
            (Key::Num3, Duration::Sixteenth),
            (Key::Num4, Duration::Eighth),
            (Key::Num5, Duration::Quarter),
            (Key::Num6, Duration::Half),
            (Key::Num7, Duration::Whole),
            (Key::Num8, Duration::DoubleWhole),
        ] {
            assert_eq!(
                entry_command(EntryProfile::SimpleLaptop, key, plain, false),
                Some(EntryCommand::Duration(d, false))
            );
            assert_eq!(
                entry_command(
                    EntryProfile::SimpleLaptop,
                    key,
                    Modifiers { alt: true, ..plain },
                    false
                ),
                Some(EntryCommand::Duration(d, true))
            );
        }
    }
    #[test]
    fn profiles_do_not_blend_conflicting_zero_enter_and_backspace() {
        let plain = Modifiers::NONE;
        assert_eq!(
            entry_command(EntryProfile::Speedy, Key::Num0, plain, false),
            Some(EntryCommand::ExitEntry)
        );
        assert_eq!(
            entry_command(EntryProfile::Speedy, Key::Backspace, plain, false),
            Some(EntryCommand::RemoveTone)
        );
        assert_eq!(
            entry_command(EntryProfile::SimpleLaptop, Key::Backspace, plain, false),
            Some(EntryCommand::ClearSelection)
        );
        assert_eq!(
            entry_command(EntryProfile::Speedy, Key::Enter, plain, true),
            Some(EntryCommand::Staff(1))
        );
        assert_eq!(
            entry_command(EntryProfile::SimpleLaptop, Key::Enter, plain, true),
            Some(EntryCommand::Enter)
        );
    }
    #[test]
    fn application_commands_do_not_leak_into_note_entry() {
        for key in [
            Key::C,
            Key::N,
            Key::O,
            Key::S,
            Key::V,
            Key::X,
            Key::Y,
            Key::Z,
        ] {
            assert_eq!(
                entry_command(
                    EntryProfile::SimpleLaptop,
                    key,
                    Modifiers {
                        ctrl: true,
                        command: true,
                        ..Modifiers::NONE
                    },
                    false
                ),
                None
            );
            assert_eq!(
                entry_command(
                    EntryProfile::SimpleLaptop,
                    key,
                    Modifiers {
                        mac_cmd: true,
                        command: true,
                        ..Modifiers::NONE
                    },
                    true
                ),
                None
            );
        }
    }

    #[test]
    fn laptop_chord_and_interval_commands_keep_their_modifiers() {
        for mac in [false, true] {
            let primary = Modifiers {
                ctrl: !mac,
                mac_cmd: mac,
                command: true,
                ..Modifiers::NONE
            };
            assert_eq!(
                entry_command(EntryProfile::SimpleLaptop, Key::Enter, primary, mac),
                Some(EntryCommand::Chord)
            );
            assert_eq!(
                entry_command(
                    EntryProfile::SimpleLaptop,
                    Key::F9,
                    Modifiers {
                        shift: true,
                        ..primary
                    },
                    mac,
                ),
                Some(EntryCommand::Interval(9))
            );
            for (key, interval) in [(Key::F2, 2), (Key::F8, 8)] {
                assert_eq!(
                    entry_command(EntryProfile::SimpleLaptop, key, Modifiers::NONE, mac),
                    Some(EntryCommand::Interval(interval))
                );
                assert_eq!(
                    entry_command(
                        EntryProfile::SimpleLaptop,
                        key,
                        Modifiers {
                            shift: true,
                            ..Modifiers::NONE
                        },
                        mac,
                    ),
                    Some(EntryCommand::Interval(-interval))
                );
            }
            assert_eq!(
                entry_command(EntryProfile::SimpleLaptop, Key::F9, Modifiers::NONE, mac),
                None
            );
        }
    }

    #[test]
    fn speedy_measure_boundaries_and_measure_jumps_are_distinct() {
        for mac in [false, true] {
            let edit = Modifiers {
                ctrl: !mac,
                command: !mac,
                alt: mac,
                ..Modifiers::NONE
            };
            for (key, direction) in [(Key::ArrowLeft, -1), (Key::ArrowRight, 1)] {
                assert_eq!(
                    entry_command(EntryProfile::Speedy, key, edit, mac),
                    Some(EntryCommand::Boundary(direction))
                );
                assert_eq!(
                    entry_command(
                        EntryProfile::Speedy,
                        key,
                        Modifiers {
                            shift: true,
                            ..Modifiers::NONE
                        },
                        mac,
                    ),
                    Some(EntryCommand::Measure(direction))
                );
            }
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::OpenBracket, Modifiers::NONE, mac),
                Some(EntryCommand::Measure(-1))
            );
        }
    }

    #[test]
    fn speedy_rest_chords_and_pending_insert_never_replace_a_note() {
        for mac in [false, true] {
            let rest = Modifiers {
                ctrl: true,
                command: !mac,
                alt: mac,
                shift: true,
                ..Modifiers::NONE
            };
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::Num5, rest, mac),
                Some(EntryCommand::SpeedyRest(Duration::Quarter))
            );
            assert_eq!(
                entry_command(
                    EntryProfile::Speedy,
                    Key::Num5,
                    Modifiers {
                        shift: true,
                        ..Modifiers::NONE
                    },
                    mac,
                ),
                None
            );
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::Num8, rest, mac),
                None
            );
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::S, Modifiers::NONE, mac),
                Some(EntryCommand::SetAccidental(1))
            );
            assert_eq!(
                entry_command(
                    EntryProfile::Speedy,
                    Key::S,
                    Modifiers {
                        shift: true,
                        ..Modifiers::NONE
                    },
                    mac,
                ),
                Some(EntryCommand::Accidental(1))
            );
        }
    }

    #[test]
    fn mac_delete_aliases_and_control_do_not_apply_windows_editing() {
        for key in [Key::Backspace, Key::Delete] {
            assert_eq!(
                entry_command(EntryProfile::SimpleLaptop, key, Modifiers::NONE, true),
                Some(EntryCommand::Delete)
            );
            assert_eq!(
                entry_command(
                    EntryProfile::Speedy,
                    key,
                    Modifiers {
                        shift: true,
                        ..Modifiers::NONE
                    },
                    true,
                ),
                Some(EntryCommand::RemoveTone)
            );
        }
        for profile in [EntryProfile::SimpleLaptop, EntryProfile::Speedy] {
            assert_eq!(
                entry_command(
                    profile,
                    Key::C,
                    Modifiers {
                        ctrl: true,
                        ..Modifiers::NONE
                    },
                    true,
                ),
                None
            );
        }
    }
}

#[cfg(test)]
mod engraving_tests {
    use super::*;
    #[test]
    fn engraving_commands_and_layer_modifiers_follow_each_platform_profile() {
        use EntryCommand::*;
        use StyleCommand::*;
        for mac in [false, true] {
            let primary = Modifiers {
                ctrl: !mac,
                mac_cmd: mac,
                command: true,
                ..Modifiers::NONE
            };
            for profile in [EntryProfile::SimpleLaptop, EntryProfile::Speedy] {
                assert_eq!(
                    entry_command(profile, Key::L, Modifiers::NONE, mac),
                    Some(Style(FlipStem))
                );
                assert_eq!(
                    entry_command(profile, Key::F, primary, mac),
                    Some(Style(FlipTie))
                );
                assert_eq!(
                    entry_command(
                        profile,
                        Key::F,
                        Modifiers {
                            shift: true,
                            ..primary
                        },
                        mac
                    ),
                    Some(Style(DefaultTie))
                );
                assert_eq!(
                    entry_command(profile, Key::P, Modifiers::NONE, mac),
                    Some(Style(ToggleParentheses))
                );
            }
            assert_eq!(
                entry_command(
                    EntryProfile::SimpleLaptop,
                    Key::Backslash,
                    Modifiers::NONE,
                    mac
                ),
                Some(Style(Respell))
            );
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::Backslash, Modifiers::NONE, mac),
                Some(Style(FlatBeam))
            );
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::Num9, Modifiers::NONE, mac),
                Some(Style(Respell))
            );
            let layers = if mac {
                Modifiers {
                    mac_cmd: true,
                    command: true,
                    alt: true,
                    ..Modifiers::NONE
                }
            } else {
                Modifiers {
                    alt: true,
                    shift: true,
                    ..Modifiers::NONE
                }
            };
            for (key, index) in [
                (Key::Num1, 0),
                (Key::Num2, 1),
                (Key::Num3, 2),
                (Key::Num4, 3),
            ] {
                assert_eq!(layer_command(key, layers, mac), Some(index));
            }
            assert_eq!(layer_command(Key::Num5, layers, mac), None);
            assert_eq!(layer_command(Key::Num2, Modifiers::NONE, mac), None);
        }
        assert_eq!(
            entry_command(
                EntryProfile::Speedy,
                Key::ArrowUp,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
                true
            ),
            Some(Layer(1))
        );
        assert_eq!(
            entry_command(
                EntryProfile::Speedy,
                Key::ArrowUp,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
                false
            ),
            Some(Staff(-1))
        );
        assert_eq!(
            entry_command(
                EntryProfile::Speedy,
                Key::Quote,
                Modifiers {
                    shift: true,
                    ..Modifiers::NONE
                },
                false
            ),
            Some(Layer(1))
        );
    }
}

#[cfg(test)]
mod voice_tests {
    use super::*;
    #[test]
    fn apostrophe_switches_speedy_voice_without_stealing_layer_shortcut() {
        for mac in [false, true] {
            assert_eq!(
                entry_command(EntryProfile::Speedy, Key::Quote, Modifiers::NONE, mac),
                Some(EntryCommand::ToggleVoice)
            );
            assert_eq!(
                entry_command(EntryProfile::SimpleLaptop, Key::Quote, Modifiers::NONE, mac),
                None
            );
        }
        assert_eq!(
            entry_command(EntryProfile::Speedy, Key::Quote, Modifiers::SHIFT, false),
            Some(EntryCommand::Layer(1))
        );
    }
}
