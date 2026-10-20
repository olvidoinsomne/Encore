# Encore keyboard shortcuts

Encore uses Finale 27's **Simple Entry · Laptop** profile by default. Choose
**Speedy Entry** for duration-triggered entry. The profile is remembered
between launches. These are the implemented subsets; the complete Finale
command set remains tracked in [FINALE_PARITY.md](FINALE_PARITY.md).

`Primary` means **Ctrl** on Windows/Linux and **Command** on macOS.
`Alternate` means **Alt** on Windows/Linux and **Option** on macOS. Text
fields keep their normal typing and clipboard behavior.

## Document and playback commands

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| New score | Ctrl+N | Command+N |
| Open | Ctrl+O | Command+O |
| Save / Save as | Ctrl+S / Ctrl+Shift+S | Command+S / Command+Shift+S |
| Undo | Ctrl+Z | Command+Z |
| Redo | Ctrl+Y | Command+Shift+Z |
| Undo/redo history | Ctrl+Shift+Z | Command+Y |
| Copy / cut selected event | Ctrl+C / Ctrl+X | Command+C / Command+X |
| Paste event at cursor | Ctrl+V | Command+V |
| Play / pause | Space | Space |
| Stop | Escape | Escape |

Clipboard operations currently handle one note, chord or rest. Paste retains
its notation and performance fields and rejects collisions or barline
overflow. Region/measure-stack selection, filtered paste and repeated paste
remain pending. **Space** is Encore's convenient transport key on every OS;
Finale's Windows table also documents menu accelerators and Space-click.

## Simple Entry · Laptop

| Action | Shortcut |
| --- | --- |
| Select duration | 0=128th, 1=64th, 2=32nd, 3=16th, 4=eighth, 5=quarter, 6=half, 7=whole, 8=breve |
| Change selected duration | Alternate+duration digit |
| Enter pitch / add chord tone | A–G / Shift+A–G |
| Add caret pitch to selected entry | Primary+Enter |
| Add interval above / below | F2–F8 / Shift+F2–F9; Primary+Shift+F9 adds ninth above |
| Enter caret pitch | Enter |
| Enter rest | Tab, Shift+Enter or Alternate+Enter |
| Convert selected note/rest | R |
| Toggle rest entry tool | Alternate+R |
| Dot selected entry / dot entry tool | . / Shift+. |
| Tie forward / backward | T / Shift+T |
| Raise/lower accidental; natural | = / − / N |
| Move pitch by diatonic step / octave | Up or Down / Shift+Up or Down |
| Previous / next entry | Left / Right |
| Previous / next measure | Primary+Left / Primary+Right |
| Delete selected event | Delete |
| Clear selection | Backspace on Windows/Linux; Shift+Delete on macOS |

The Laptop profile deliberately avoids Finale's standard profile distinction
between top-row interval keys and keypad duration keys. The current egui
logical key events do not reliably distinguish these physical keys. A complete
standard/keypad profile, marking triggers and metatools
remain pending.

## Speedy Entry

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| Enter/change duration | 1–8: 64th through breve | Same |
| 128th duration | Ctrl+0 | Option+0 |
| Enter rest of duration 1–7 | Ctrl+Shift+digit | Control+Option+Shift+digit |
| Exit entry frame | 0 | 0 |
| Move pitch | Up / Down | Same |
| Previous / next entry | Left / Right | Same |
| First / after last entry in measure | Ctrl+Left / Ctrl+Right | Option+Left / Option+Right |
| Previous / next measure | [ / ] or Shift+Left / Shift+Right | Same |
| Move to staff above / below | Shift+Up / Shift+Down | Shift+Return / Return |
| Remove chord pitch / convert sole pitch to rest | Backspace | Shift+Delete |
| Delete entire event | Delete | Delete |
| Dot entry | . | . |
| Tie forward / backward | T / Shift+T | Same |
| Set sharp / flat / natural / double sharp / double flat | S / F / N / X / V | Same |
| Raise / lower accidental by half step | + or Shift+S / − or Shift+F | Same |

Speedy currently operates without MIDI input. Caps Lock pitch rows,
independent voices, insert mode, courtesy accidentals/stem/beam commands and
MIDI held-chord input remain pending. macOS Finale distinguishes **Return**
(staff navigation) from keypad **Enter**, often Fn+Return (chord entry).
Encore cannot yet promise that distinction on all keyboards; mouse/Shift-click
chord entry remains available. A macOS backward Delete event may be reported
as Backspace; Encore accepts both logical Delete/Backspace for ordinary event
deletion and their shifted forms for tone removal. Native Clear-key behavior
still needs keyboard validation.

Four independent layers can share each staff. Select the active layer in the
toolbar; each has independent note navigation and rhythm. A second voice
inside each layer and the complete platform-specific layer shortcut set remain
pending. Select a chord tone in the Inspector to target its pitch edits.
The Inspector also edits staccato, accent, marcato, tenuto and staccatissimo.
Finale's articulation trigger keys and programmable marking metatools remain
pending; these checkboxes are the current entry route.

## Tuplet behavior and present limits

Simple Entry `9` starts a triplet on the note/rest just entered and fills the
remaining two positions with rests. Subsequent notes replace those rests.
Speedy prepares a group **before** entry with `Ctrl+3` on Windows or `Option+3`
on macOS. Both stop triplet entry after the group is filled. The toolbar also
offers a persistent triplet tool; the Inspector edits representable ratios.
`Alt/Option+9` custom entry, mixed-value, nested and cross-bar grouping remain
pending. Ratios that do not fit the exact 480-PPQ tick grid are rejected.

References checked against MakeMusic's official
[Windows shortcut table](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Keyboard_Shortcuts_and_Special_Mouse_Clicks.htm),
[macOS shortcut table](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/Keyboard_Shortcuts_and_Special_Mouse_Clicks.htm),
[Simple Entry guide](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Simple_Entry.htm),
[Speedy Entry guide](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Speedy_Entry.htm),
and [tuplet guide](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Tuplets.htm).
