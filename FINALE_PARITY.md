# Finale 27.4.1 parity audit

Audit date: 20 October 2026. The comparison target is **Finale 27.4.1**, the
December 2023 release identified by MakeMusic's
[version history](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Finale-Versions.htm).
This is a functional compatibility checklist, not a claim that Encore has
already reached parity. The modern Encore interface remains the intended UI.

The starting point is Encore 0.5, commit `55f2cc0`; the tables also incorporate
the Encore 0.6 implementation pass recorded below. Source evidence was inspected
in `src/model.rs`, `src/app.rs`, `src/engraving.rs`, `src/export.rs`,
`src/audio.rs`, `src/sampled.rs`, `src/synth.rs`, and the three format readers.
Changes made after that commit must pass the acceptance checks below before a
row becomes complete. Importing, displaying, editing, saving and playing a
feature are separate obligations: storing a field alone does not complete it.

**Complete** means the stated narrow capability exists end to end; **Partial**
means only a subset works; **Missing** means the workflow is absent;
**Unverified** means it has not been exercised on the necessary platform.
Most rows remain open. There is deliberately no percentage based on counting
rows: a VST host and a keyboard command are very different amounts of work.

## Notation and engraving

The reference includes [independent layers and voices](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/sub-select-layer.htm),
[tuplets](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Tuplets.htm),
[expressions](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Expressions.htm),
[Smart Shapes](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/menu-smart-shape.htm),
and [Special Tools](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Special_Tools_Palette.htm).
The statuses describe the current implementation, with remaining constraints
stated explicitly. Native platform qualification remains separate from local
Linux verification.

| Capability | Status | Evidence and work remaining |
| --- | --- | --- |
| Pitched notes, simultaneous chord tones and explicit rests | Complete | Native events, editor, vector engraving and all exporters. Subject to documented size limits. |
| Full duration range, augmentation and tied overflow | Partial | Breve through 128th now reaches engraving/interchange/audio. One dot is supported where exact on the tick grid. Repeated dots and automatic splitting/rebar remain open. |
| Tuplets, nested tuplets and custom ratios | Partial | Exact actual:normal ratios and basic grouping, triplet entry and inspector ratios. Fractional-grid, nested, mixed-value and cross-bar groups remain open. |
| Grace notes, slashed/unslashed groups and playback | Missing | Foreign grace material is omitted or rejected. |
| Four independent layers, each with two voices | Partial | Four editable rhythms now share a staff, with independent stems, beams, exports and playback. Two separate voices within each layer remain open. |
| Per-pitch chord spelling and independent ties | Partial | Spelled pitches exist; the tie flag applies to an event. Needs per-note tie editing, tie direction and continuation control. |
| Clefs and clef changes | Partial | Three fixed staff clefs. Needs other C/F/G clefs, percussion, octave variants and mid-measure changes. |
| Keys and key changes | Partial | One constant circle-of-fifths signature. Needs regional/independent signatures, cancellation and custom/nonlinear keys. |
| Meter and meter changes | Partial | One constant meter, now with 1–32 beats and power-of-two denominators through 128. Changes, pickup measures, additive/composite, independent and display-only meters remain open. |
| Accidental display policies | Partial | Explicit pitch alters and ordinary automatic accidentals. Needs courtesy, parentheses, forced/hidden and regional policies. |
| Stem, beam, flag and rest placement | Partial | Automatic basic engraving. Needs manual direction, angles, joins, hooks, stem lengths and rest offsets. |
| Cross-staff notation and beaming | Missing | No source/destination staff relationship for notes or beams. |
| Dynamics and articulations | Partial | Velocity-derived dynamics, legacy staccato, accent, marcato, tenuto and staccatissimo now have editable marks, vector glyphs and playback. Custom marking definitions, fermata timing, ornaments and complete placement/performance rules remain open. |
| Slurs and phrase curves | Missing | Ties do not replace slurs; needs anchored, editable curves across systems. |
| Hairpins, octave lines, pedal, trills, glissandi and custom lines | Missing | No anchored spanner model or editor. |
| Tremolos, measured repeats and ornaments | Missing | No notation/performance representation. |
| Repeat barlines, endings, D.C./D.S., coda and Fine | Missing | Needs consistent engraved navigation and bounded playback traversal. |
| Lyrics | Missing | Needs verses/choruses/sections, syllables, hyphens, melismas, elisions, baselines, text assignment and editing. |
| Chord symbols and fretboards | Missing | Chord tones are available; chord-name/suffix objects and diagrams are absent. |
| Guitar tablature, bends, slides and tuning | Missing | No strings/frets, alternate tunings or standard/tab conversion. |
| Unpitched percussion layouts and drum maps | Missing | GM melodic pitched percussion is available, but does not supply percussion notation or drum-kit routing. |
| Staff styles, cutaway, slash/rhythmic notation and repeats | Missing | Fixed five-line staff presentation; no regional style definitions. |
| Cue notes, ossia, note sizes and custom noteheads | Missing | No independent visual note scaling or substitute notehead layer. |
| Rehearsal marks and expressive/technique text | Missing | Title/composer exist; no anchored text-expression categories. |
| Music fonts and symbol selection | Missing | Original vector symbols; no user-selectable SMuFL music font, metadata or arbitrary symbol browser. |
| Custom shapes, rich text and embedded graphics | Missing | No general shape designer, page/measure text blocks or graphic placement. |

Additional references: [lyrics](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Lyrics.htm),
[chord entry](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/menu-chord.htm),
[tablature](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Tablature.htm),
[staff styles](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/ht-staff-styles.htm),
[keys](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Key_signatures.htm),
[meters](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Time_signatures.htm),
and [Finale 27 font changes](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/What_s_new.htm).

## Editing, entry and keyboard behavior

Finale has distinct [Simple Entry](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Simple_Entry.htm),
[Speedy Entry](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Speedy_Entry.htm),
and [HyperScribe recording](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Recording_with_HyperScribe.htm)
workflows. A single blended shortcut map cannot reproduce their contextual
behavior. The official shortcut references are the
[Windows table](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Keyboard_Shortcuts_and_Special_Mouse_Clicks.htm)
and [macOS table](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/Keyboard_Shortcuts_and_Special_Mouse_Clicks.htm).
Linux should use the Windows convention by default.

| Capability | Status | Evidence and work remaining |
| --- | --- | --- |
| Mouse note/rest entry and chord addition | Partial | Click/Shift-click entry exists. Needs complete rhythm tools, insertion, repitch and drag constraints. |
| Simple Entry standard and laptop profiles | Partial | A–G input exists, but durations, arrows, rests and deletion differ. Numeric keypad and number row need distinct treatment in the standard profile. |
| Speedy Entry | Partial | Separate duration-triggered keyboard profile, pitch position and common editing commands. MIDI, Caps Lock pitch rows, insertion and complete frame behavior remain open. |
| MIDI keyboard step entry | Missing | No MIDI input device discovery, event stream or held-chord handling. |
| Real-time transcription | Missing | No recording, metronome/count-in, quantization, split-point or multi-voice transcription. |
| Selection and navigation | Partial | Single event/staff/cursor only. Needs regions, measure stacks, disjoint objects, selection extension and entry/chord/staff navigation. |
| Cut, copy, paste and insert | Partial | One event can be copied/cut/pasted with validation. Regions, filters, repeated paste, clip files and shifting insertion remain open. |
| Measure/staff structural editing | Partial | Add measures/staves and remove a staff. Needs insert/delete/move ranges and consistent remapping of anchors. |
| Transposition, respelling and repitch | Partial | Semitone/octave edits exist. Needs regional diatonic/chromatic intervals, spelled intervals and repitch-only mode. |
| Undo/redo and change list | Partial | 100 snapshots, history dialog and platform-specific commands. Detailed operation labels and complete Finale history behavior remain open. |
| Metatools and customizable commands | Missing | No marking assignment/programming or user shortcut editor. |
| Utilities and arranging | Missing | Explode/implode, voice/layer transforms, rhythmic augmentation/diminution, rebar and SmartFind/Paint. |
| Scripting and notation plug-ins | Missing | No independent automation API, FinaleScript equivalent, JW/TGTools equivalents or Finale plug-in ABI. |
| Multiple documents/windows and templates | Missing | One editor document. No simultaneous score/part windows, setup wizard, reusable document styles or templates. |

### Shortcut implementation contract

The first compatible profile should be explicitly named **Finale Simple
Entry — Laptop**. A standard/keypad profile and Speedy mode remain separate
requirements. Preserve text-field editing and modal focus; musical commands
must not also fire while typing names, lyrics or search terms.

Laptop duration selection is `0`=128th, `1`=64th, `2`=32nd, `3`=16th,
`4`=eighth, `5`=quarter, `6`=half, `7`=whole, `8`=breve. `Alt` on Windows
or `Option` on macOS plus the digit changes a selected duration. `A–G` enters;
`Shift+A–G` adds a chord tone; `Enter` uses caret pitch; `Tab` or
`Shift+Enter` enters a rest. Arrows move diatonically; shifted vertical arrows
move an octave. `R` converts the selected entry; `Alt/Option+R` changes the
entry tool. `T` and `Shift+T` tie forward/backward. `.` edits dots;
`Shift+.` selects the dot tool. These commands require distinct caret and
selection semantics, rather than changing a shared pitch field.

In Speedy mode the duration digits insert or modify an entry. `0` exits its
frame. `Enter` adds a chord pitch; deleting one chord pitch differs from
deleting the event. Platform-specific staff/layer navigation must follow its
own official table. Accidentals, ties, tuplets, stems and beams also have
mode-specific bindings.

On Windows redo is `Ctrl+Y` and `Ctrl+Shift+Z` opens history. On macOS redo is
`Command+Shift+Z` and `Command+Y` opens history. Global command translation
is not sufficient for all notation commands. Verify each profile against the
platform table, including keypad distinctions, modifier precedence, held keys,
non-US layouts, and all supported marking metatools. Unsupported commands
must be listed as pending instead of silently invoking unrelated operations.

## Page layout, parts and output

The reference includes [page layout](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Page_layout.htm),
[linked parts](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Linked_Parts.htm),
[part page formats](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/db-page-format-for-parts.htm),
and [graphics](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/menu-graphics.htm).

| Capability | Status | Evidence and work remaining |
| --- | --- | --- |
| Vector full score and single-staff view | Complete | Common vector layout feeds screen, multipage PDF and SVG. This narrow capability is not a linked-parts implementation. |
| Full ensemble system layout | Partial | Automatic grouping over fixed letter-size pages. Needs robust vertical spacing, overflow and collision avoidance for dense scores. |
| Page size, margins, scaling and orientation | Missing | Fixed page geometry; no stored document/part layout settings. |
| System breaks, locks and measure widths | Missing | No editable systems, manual spacing, beat charts or pagination controls. |
| Staff groups, brackets, braces and labels | Partial | Basic automatic staff presentation. Needs editable groups, bracket placement, barline spans and independent abbreviations. |
| Hide empty staves, multimeasure rests and staff spacing | Missing | No condensing/hiding, rest ranges or per-system staff management. |
| Linked part definitions and voicing | Partial | Staff-filtered view shares content. Needs multi-staff/custom/voiced parts, part-specific transpositions and part identity. |
| Independent part formatting | Missing | No part page settings, local marking overrides, unlink/relink or layout copying. |
| Print and batch score/parts | Partial | Export PDF and print externally. Needs native print dialog, part selection, copies, ranges and extracted documents. |
| Publication metadata | Partial | Title/composer. Needs page numbers, copyright, part names, headers/footers, text inserts, title pages and rich formatting. |
| PDF naming and part titles | Partial | File dialog supplies name. Needs saved-title defaults and automatic part-name additions from 27.4.1. |
| Graphics export and placement | Partial | SVG/PDF export. Needs selection export, raster formats and embedded/linked graphic objects. |

## Playback, MIDI and libraries

The reference includes [Human Playback](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Human_Playback.htm),
[MIDI editing](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/tool-midi.htm),
[percussion maps](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/db-percussion-midi-map-editor.htm),
[VST/Audio Units](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/VSTAudio_Units.htm),
and [audio files](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Audio_Files.htm).

| Capability | Status | Evidence and work remaining |
| --- | --- | --- |
| Sampled GM instrument catalog | Complete | 128 melodic bank-0 programs through SF2; auditions and WAV share the engine. A GM catalog is not Garritan library compatibility. |
| Transport and mixer | Partial | Play/pause/stop, playhead, volume/pan, mute/solo. Needs seek, loop/region playback, scrub, count-in and metronome. |
| Ties, dynamics and articulation playback | Partial | Ties, event velocities, staccato and four ordinary articulation effects. Explicit dynamic events, ornaments, hairpins, technique switches and per-pitch articulation remain open. |
| Tempo map and expressive timing | Missing | One BPM; no accelerando/ritardando, swing, fermata timing or independent performance offsets. |
| Repeat traversal | Missing | Playback timeline currently follows measures once in order. |
| Human Playback equivalent | Missing | No style interpreter or score-text/technique dictionary. Reverb and sample variation alone do not supply this. |
| MIDI performance editor | Missing | No controller lanes, pitch bends, patch/bank changes, fine note timing or captured data. |
| External MIDI I/O | Missing | No device routing, MIDI Thru, input/output maps or sync. |
| Percussion playback | Missing | Needs drum-kit bank/channel selection coupled to notation maps. |
| Multiple devices and instrument changes | Missing | One selected sample bank/backend; no score-time instrument/device routing. |
| VST2 Windows / Audio Units macOS host | Missing | No scanning, instantiation, editor windows, state, audio/MIDI routing or crash isolation. Native tests required. |
| Garritan/ARIA playback | Missing | Requires the user's licensed installed player and compatible host; proprietary assets are not bundled. |
| SFZ library definitions | Missing | A separate sampler format; support alone would not reproduce encrypted Garritan content. |
| Finale notation `.lib` libraries | Missing | No import of expressions, articulations, chord suffixes or document styles. These differ from audio libraries. |
| Audio track recording/import and synchronization | Missing | No recorded backing track, input capture or sync timeline. |
| Audio file output | Partial | 44.1 kHz stereo 16-bit WAV. Needs appropriate additional formats, region/part output and configurable rendering. |

An independent host or equivalent arranging tool can close a functional gap
without copying a proprietary implementation. Existing Finale notation
plug-ins cannot be assumed to load in a new application. See
[Finale plug-ins](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/def-plug-ins.htm),
[JW tools](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/sub-jw-tools.htm),
[ARIA SFZ behavior](https://usermanuals.garritan.com/ARIAPlayer/Content/sfz_libraries.htm),
and [Finale libraries](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/ht-libraries.htm).

## Interchange, reliability and platforms

| Capability | Status | Evidence and work remaining |
| --- | --- | --- |
| Native Encore persistence | Complete | Validated JSON, atomic saves, previous Encore files and dirty-document protection. |
| Direct Finale `.musx` reader | Partial | Basic EnigmaXML with four layer frames and a real 27.4.1 fixture. Voice-2 structures, tuplets and complete semantics/layout remain open. |
| Legacy Finale `.mus` and related templates | Missing | Actionable MusicXML conversion path only; no direct decoder. |
| Finale project writing | Missing | Native output is Encore JSON; no MUSX/MUS writer. |
| MusicXML import/export | Partial | Supported partwise subset and MXL input; extensive notation/layout fidelity still depends on missing model features. |
| MuseScore input | Partial | MSCX/MSCZ v2–4 subset. This is useful additional scope, not a substitute for Finale parity. |
| MIDI interchange | Partial | Type-1 output plus exact or explicitly grid-quantized MIDI import. Adaptive transcription, captured controllers and performance editing remain open. |
| Batch conversion and file migration | Partial | Single-file CLI commands and demos; needs folders, reproducible reports and mixed-score error handling. |
| Native Linux | Complete | Built/tested on development Linux; portable package verified. |
| Native Windows and macOS | Unverified | Build/package/CI recipes exist; platform host, audio, dialogs, shortcuts and packaging tests still required. |
| Installation and OS integration | Partial | Portable Linux/Windows and unsigned macOS packaging; no signed installers, notarization or tested document associations. |
| Keyboard and screen-reader accessibility | Partial | Expanded entry profiles exist. Canvas notation lacks a validated semantic accessibility tree and full keyboard editing. |
| Light/dark appearance | Complete | Remembered switch; monochromatic dark chrome and preview highlights are the default. Paper/export appearance remains suitable for printing. |
| Localization, alternate layouts and high DPI | Unverified | No locale catalog or comprehensive native tests. English, US shortcut assumptions require review. |
| Recovery and long-score behavior | Partial | Atomic explicit save, undo and input bounds. Needs crash recovery, autosave and documented performance at practical orchestral scale. |

File-format reference:
[Finale file types](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/def-finale-file-extensions.htm),
[MusicXML import](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/sub-import.htm),
and [MIDI interchange](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/MIDI_files.htm).
The precise present format limits are in [COMPATIBILITY.md](COMPATIBILITY.md).

## Acceptance gates and implementation order

1. **Command compatibility and rhythm foundation.** Explicit entry profiles;
   context-aware dispatch and focus rules; complete durations; exact tuplets;
   conventional notation, MIDI and sampled-audio timing tests.
2. **Polyphony and score structure.** Layers/voices, instrument/staff separation,
   stable object anchors, regional meter/key/clefs/tempo and safe editing.
3. **Expressive notation.** Per-note ties, articulations, text, slurs, hairpins,
   ornaments, grace notes and region-spanning graphics across page breaks.
4. **Vocal and fretted notation.** Multi-verse lyrics, chord suffixes/diagrams,
   tablature and all required entry/edit/spacing workflows.
5. **Form and instrumental writing.** Repeats/endings/coda, transpositions,
   percussion, cross-staff writing, cues, ossia and staff styles.
6. **Production engraving.** Editable layout, collision handling, fonts,
   graphics, linked/voiced parts, multimeasure rests and batch printing.
7. **Performance and recording.** MIDI input/import/output, tempo/controller
   maps, expressive playback, repeat traversal, recording and audio tracks.
8. **Library and extension compatibility.** Native VST2/AU hosting and ARIA
   integration using installed licensed assets; notation library migration;
   independent arranging/scripting functionality.
9. **Format fidelity.** A diverse, redistributable Finale/MuseScore/MusicXML
   corpus covering every completed feature; per-feature loss reports; no
   silent pitch/rhythm changes; native save/reopen and export reimport checks.
10. **Release qualification.** Native Windows/macOS/Linux builds; shortcut
    tests for each OS/profile; accessible editing; long orchestral scores;
    packaging, recovery and visual/aural reference comparisons.

A row is complete only after an editor operation can create and modify it,
undo/redo and native reopen preserve it, printed output is inspected,
interchange preserves it where the target format allows, and audible timing
is checked when relevant. Unsupported imported material must fail clearly or
produce a visible loss report. Full parity additionally requires reviewing the
[Finale topic index](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Encyclopedia.htm)
for commands within each completed workflow; this grouped audit is not an
enumeration of every submenu, plug-in parameter or glyph.

## Current implementation record

This section records changes made after the audited baseline. The owner of an
implementation must record its tests and remaining limitations before updating
the corresponding status above. **No full-parity milestone has been reached.**

- **Entry command maps:** added separate Simple Entry Laptop and Speedy maps,
  remembered profile selection, platform-specific redo/history bindings,
  compatible duration order, chord/interval entry, diatonic pitch movement,
  rests, ties, accidentals and navigation. Seven focused map tests pass,
  including platform Delete aliases and rejecting unsupported modified entry
  commands. The working application handlers pass the full Linux test suite; native macOS and Windows shortcut validation remains open. [SHORTCUTS.md](SHORTCUTS.md) lists the supported subset;
  standard/keypad entry, custom shortcuts, marking triggers, complete Speedy
  commands and native macOS keyboard validation remain open.
- **Rhythms and layers:** breve through 128th, optional `actual:normal` ratio
  counts from 1 through 32, and four independent rhythms per staff. Native
  saves keep backward-compatible layer 0 defaults. MusicXML/MuseScore preserve
  up to four source voices on their staff; direct MUSX reads its four layer
  frames. Engraving separates stems/beams/rests by layer; MIDI and sampled
  playback separate layers to avoid tie or repeated-pitch interference.
  Sixteen rhythm/layer regressions accompany this work. This is partial tuplet
  support: the 480-PPQ score grid rejects fractional ticks rather than rounding
  them. Ordinary triplets and representable 5:4 ratios work, while a 7:4 eighth
  does not. Nested, mixed-value and cross-bar grouping and a second voice inside
  each layer remain open. Four-frame MUSX coverage uses synthetic fixtures;
  the real Finale 27.4.1 fixture remains a basic single-layer score. Meter
  denominators now range from 1 through 128 in powers of two, with 1–32 beats.
- **Entry editing:** triplet conversion creates two placeholder rests that
  subsequent notes replace; prepared Speedy triplets stop after three entries.
  Duration/dot changes keep an advancing cursor at the new end and extend the
  last measure only when appropriate, respecting the measure limit. Individual
  chord-tone edits and backward ties preserve the editing location. Focused
  entry tests cover these behaviors, layers and bounded continuation.
- **MIDI import:** Standard MIDI File types 0 and 1 with PPQ timing, channels,
  ports, programs, initial tempo/meter/key and initial volume/pan. Exact note
  timing is rescaled to 480 PPQ; ordinary and exact conventional tuplet notation is inferred,
  including bar-crossing ties. Overlapping voices or unequal chord velocities
  become separate editable staves. Importer regressions cover supported and
  malformed input, including Encore's orchestral MIDI round trip. In exact mode, fractional-grid/unsupported rhythms,
  SMPTE timing, type 2 and channel-10 drums are rejected. MIDI import also exposes an explicit quantization grid, with adjusted-note count and maximum endpoint shift reported; the exact default never silently rounds timing. Controllers, pedal,
  bends, aftertouch, SysEx and later tempo/meter/key/mixer changes are not
  retained; visible warnings identify losses. See the
  [MIDI Association's SMF overview](https://midi.org/standard-midi-files).
- **Ordinary articulations:** accent, marcato, tenuto and staccatissimo have
  native fields, Inspector controls, original vector symbols, MusicXML output,
  MusicXML/MuseScore import and MIDI/procedural/SF2 performance. Twelve focused
  tests cover native compatibility/validation, glyphs, both readers and audible
  or scheduled playback effects. Legacy scores with no added markings retain
  their prior gate and velocity calculations. New marks use deterministic
  bounded performance rules: accent adds 12 velocity units, marcato adds 18
  with a 72% gate, tenuto adds 3 with a full gate, and staccatissimo uses a 30%
  gate. Combined marks use the strongest accent and shortest gate; zero
  velocity stays silent. These are ordinary performance defaults, not Human
  Playback or library keyswitch emulation. A tied continuation governs final
  release; imported split marcato values warn about that precedence. Fermata
  pauses and custom articulation definitions remain pending.

- **Engraving and integration qualification:** shared nonlinear time maps keep displayed staves aligned and provide an inverse for mouse entry. Short-note bursts receive more space; very dense full bars and manual production layout remain open. Staff dynamics are deduplicated across layers and reserve room beneath notes and articulations. The original sampled Carol rendering remains byte-identical after the new empty-default metadata and layer routing.
