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
| Stop / Selection tool | Escape | Escape |
| Selection tool | Ctrl+Shift+A | Command+Shift+A |
| Select layer 1–4 | Alt+Shift+1–4 | Command+Option+1–4 |
| Zoom in / out | Ctrl+= / Ctrl+− | Command+= / Command+− |

Clipboard operations handle a note/chord/rest or selected whole-measure region.
Paste retains supported attached notation and rejects invalid destinations;
partial-measure selection, filtered paste and repeated paste remain pending. **Space** is Encore's convenient transport key on every OS;
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
| Flip stem / automatic stem | L / Shift+L |
| Break or join previous beam / automatic | / / Shift+/ |
| Flatten beam | Alternate+/ |
| Respell active chord pitch | Backslash or Alternate+E |
| Hide/show entry | H |
| Parenthesize accidental | P |
| Show/hide courtesy accidental | Primary+Shift+− |
| Flip tie / automatic tie direction | Primary+F / Primary+Shift+F |
| Expression / articulation controls | X or Alternate+X / backtick |
| Select accidental tool: flat / natural / sharp | Alternate+− / Alternate+N / Alternate+= |

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
| Next layer | Shift+apostrophe | Shift+Up (previous: Shift+Down) |
| Toggle Voice 1 / Voice 2 within the layer | apostrophe | apostrophe |
| Flip / automatic stem | L / Ctrl+L | L / Option+L |
| Break/join / automatic beam | / or B / Shift+B | Same |
| Flatten beam | Backslash or Shift+M | Same |
| Respell / parenthesize accidental | 9 / P | Same |
| Hide/show / courtesy accidental | H or O / Shift+A | Same |
| Flip / automatic tie direction | Ctrl+F / Ctrl+Shift+F | Command+F / Command+Shift+F |
| Show/hide accidental | * (text-producing key) | Same |

Speedy currently operates without MIDI input. Caps Lock pitch rows,
insert mode, and MIDI held-chord input remain pending.
The modified asterisk command for restoring automatic accidentals requires
native keyboard validation; the Entry engraving window provides that control. macOS Finale distinguishes **Return**
(staff navigation) from keypad **Enter**, often Fn+Return (chord entry).
Encore cannot yet promise that distinction on all keyboards; mouse/Shift-click
chord entry remains available. A macOS backward Delete event may be reported
as Backspace; Encore accepts both logical Delete/Backspace for ordinary event
deletion and their shifted forms for tone removal. Native Clear-key behavior
still needs keyboard validation.

Four independent layers can share each staff, each with two rhythmic voices.
Select the layer and V1/V2 in the toolbar. In Speedy Entry, apostrophe toggles
voices. Enter Voice 1 first, then select its note or rest and switch to Voice 2
to launch a branch. Navigation, ties, lyrics and entry styling stay in the active
voice. Different launch points break secondary beams. Deleting a primary entry
with a secondary branch is refused until the branch is removed or relocated;
converting it to a rest retains the launch. An isolated secondary clipboard
event is detached from its launch with a warning. Layer dynamics and techniques
apply to both voices. Select a chord tone in the Inspector to target its pitch edits.
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

## Lyrics, phrase slurs and linked parts

These bindings apply in their tool context, following the official
[Windows](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Keyboard_Shortcuts_and_Special_Mouse_Clicks.htm)
and [macOS](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/Keyboard_Shortcuts_and_Special_Mouse_Clicks.htm)
tables. Text fields retain normal editing; live score typing resumes when the
score owns focus. Document save, undo and redo remain available.

| Context/action | Windows / Linux | macOS |
| --- | --- | --- |
| Lyrics: show lyric window | Ctrl+L | Command+L |
| Lyrics: next syllable / hyphenate | Space / hyphen | Same |
| Lyrics: word extension | Underscore | Same |
| Lyrics: previous / next lyric number | Up / Down | Same |
| Lyrics: previous / next note | Left / Right | Same |
| Lyrics: remove preceding character / syllable | Backspace / Delete | Logical Backspace / Delete |
| Slurs: flip active phrase curve | Ctrl+F | Command+F |
| Slurs: delete active curve | Delete | Logical Delete |
| Slurs: reset curve adjustments | Backspace | Backspace fallback; native Clear qualification pending |
| Next / previous linked part | Ctrl+Alt+. / Ctrl+Alt+, | Command+Option+. / Command+Option+, |
| Last viewed score/part | Ctrl+Alt+/ | Command+Option+/ |

Use **Slurs** then click its start/end notes, or drag between them; Finale's
second-click drag also works. The Notation window exposes endpoint, direction
and curve controls. Full shape handles, lyric justification/shift shortcuts,
click-assignment cursor, metatools, and native keypad/Clear distinctions remain
on the parity audit. Repeat regions, endings and navigation marks are edited in
**Notation → Repeats and endings**; no invented global repeat shortcut is assigned.

**Notation → Expressions and tempo** opens the dynamic, text, rehearsal, hairpin and tempo editors. Text fields retain ordinary typing behavior. Finale expression metatools and Smart Shape assignment shortcuts remain pending; no unrelated global shortcut is substituted.

**Notation → Performance lines** opens octave, pedal, trill and chromatic glissando editing. Finale Smart Shape metatools and drag-to-assign shortcuts remain pending. Existing text and modal focus protections apply.

Single-event copy/cut/paste carries attached lyric syllables and entry engraving
styles, including per-pitch ties, plus before/after grace groups. Whole-measure regional clipboard data also
preserves fully contained lyric/slur/hairpin/performance-line spans, chord symbols, fretboards, grace groups, expressions and engraving styles. Cross-boundary spans are omitted or shortened with an explicit warning.
Score-wide repeats, navigation and tempo travel only with full source staff
stacks. Every cut/paste validates before committing.

## Whole-measure regions

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| Select all visible measures/staves | Ctrl+A | Command+A |
| Copy / cut / paste selected region | Ctrl+C / X / V | Command+C / X / V |
| Insert copied measure stacks | Ctrl+I | Command+I |
| Extend by full measure | Ctrl+Shift+Left / Right | Option+Shift+Left / Right |
| Extend by visible staff | Shift+Up / Down | Same |
| Extend to first / last bar | Shift+Home / End | Same |
| Extend to top / bottom visible staff | Shift+Page Up / Down | Same |
| Clear region, retaining measure positions | Backspace | Clear / Fn+Shift+Delete |
| Delete full source measure stack | Delete | Delete |

A linked part containing only some source staves clears its selected music when
Delete is pressed; it cannot delete hidden players' measure stacks. Partial-note
selection with plain Shift+Left/Right, paste filters, repeated paste and clip
files remain pending. The region tool provides explicit Replace, Empty Only and
Insert paste modes and custom chromatic, spelled-interval and key-aware diatonic
transposition controls.

## Layout and entry engraving

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| Fit Measures controls (Selection tool) | Ctrl+M | Command+Shift+M |
| Lock selected systems | Ctrl+L or L in Selection | Command+L or L in Selection |
| Unlock selected systems | Ctrl+U / Ctrl+Shift+U or U in Selection | Command+Shift+U or U in Selection |

**Notation → Entry engraving** also offers explicit notehead, cue size,
visibility, accidental, stem/beam and per-pitch tie controls. Appearance changes
preserve sounding pitch and timing. Automatic layout is recalculated after
edits; manual beam slopes, lengths and rest offsets remain pending.

## Chord symbols

Open **Notation → Chord symbols and fretboards**, type a standard letter-name chord and apply it. **Type on score** enables contextual entry at staff beats. These navigation keys follow Finale's chord tool; the properties window retains ordinary text editing.

| Action in chord entry | Key |
| --- | --- |
| Accept current chord | Enter |
| Accept and move to next entry/beat | Space or Shift+Right |
| Accept and move to previous entry/beat | Shift+Space or Shift+Left |
| Next / previous measure | Tab / Shift+Tab |
| Higher / lower stacked chord slot | Up / Down |
| Edit draft / delete current chord | Backspace / Delete |
| Exit chord mode | Escape |

Type roots A–G with `b`/`#` accidentals, standard suffixes such as `m7`, `maj7`, `7(b9,#11)`, `sus4`, `add9`, `no3`, and an optional `/bass`. `6/9` is a sixth/ninth suffix; `C6/9/E` also specifies the bass. Linked parts accept written chord names. Finale suffix-selection numbers, MIDI analysis and learned chords remain pending.

## Page view and playback

| Action | Windows / Linux | macOS |
| --- | --- | --- |
| Go to measure/beat | Ctrl+Shift+G | Command+Shift+G |
| Fit page width | Ctrl+] | Command+Option+] |
| Fit page in window | Ctrl+[ | Command+Option+[ |
| Scroll one screen vertically | Page Up / Page Down | Same |
| Previous / next physical page | Ctrl+Page Up / Down | Command+Page Up / Down |
| Small vertical increment | Alt+Page Up / Down | Option+Page Up / Down |
| Scroll one screen horizontally | Home / End | Same |
| Beginning / end of score view | Ctrl+Home / End | Command+Home / End |
| Small horizontal increment | Alt+Home / End | Option+Home / End |

Selection-range extension retains its own shifted navigation keys. Page view
navigation and zoom do not change the score or create undo entries.

**Playback…** opens range/cursor-start controls and the audio seek slider. Loop
bounds are numbered performed bars, including repeat/navigation visits; the
selected-region shortcut includes its first through last visit. These session
options affect listening, while exports retain the complete performed score.


The Slurs tool supports same-staff cross-voice/layer phrases, solid/dashed/dotted
strokes and numeric asymmetric control points. Select a visible slur handle to
open its controls. Reset curve retains style; direct drag editing of per-system
controls remains pending. Voiced linked parts use source-score editing for
music changes; their view/navigation, formatting and extraction controls remain
available. Ornament assignment uses Notation → Ornaments and arpeggios; Finale
articulation trigger keys and programmable metatools remain pending.

Measured tremolos use **Notation → Measured tremolos**. The editor assigns a
single-note/chord roll or pairs the selected principal with its next same-voice
entry in the bar. Stroke count, playback and offsets are editable; there is no
new fixed keyboard shortcut or claim of programmable Finale metatool parity.

## Unpitched percussion context

On a percussion staff, Enter uses the selected named sound and duration; Shift-click in the sound palette enters it directly. Up/Down walks mapped sounds in staff-position order, including alternatives sharing a position. Simple Entry modifies the selected chord sound; Speedy moves its entry sound. Duration, rest, tie, chord, layer/voice and navigation commands retain their ordinary context. A–G, intervals, octave shifts, accidental/respelling and melodic transposition commands do not reinterpret drum IDs. Grace editing offers the same named sounds. MIDI keyboard entry and mouse-drag sound cycling remain pending. This follows the public [Finale percussion entry reference](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Percussion.htm) within Encore’s currently implemented entry profiles.
