# Finale 27.4.1 parity audit

Audit date: 20 October 2026. The comparison target is **Finale 27.4.1**, the
December 2023 release identified by MakeMusic's
[version history](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Finale-Versions.htm).
This is a functional compatibility checklist, not a claim that Encore has
already reached parity. The modern Encore interface remains the intended UI.

The starting point is Encore 0.5, commit `55f2cc0`; the tables also incorporate
the Encore 0.6–0.11 implementation passes recorded below. Source evidence was inspected
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
| Grace notes, slashed/unslashed groups and playback | Partial | Editable before/after groups retain pitches/chords, values, dots, beam choices, size, slash, principal connector and bounded borrowed-time playback across native, linked parts and MusicXML. MuseScore imports before/after groups and pickups. Ordinary-entry conversion/rebar shortcuts, grace-specific lyric/articulation/tuplet/tie structures and inserted global make-time remain open. |
| Four independent layers, each with two voices | Complete | Eight independent rhythmic lanes now retain source identity across entry/navigation, voice-local annotations, vector engraving, native persistence, linked parts, MusicXML, MIDI and both audio engines. Secondary launch points separate beam groups. This row covers basic polyphony; advanced layer policies and cross-voice/cross-staff notation remain tracked separately. |
| Per-pitch chord spelling and independent ties | Partial | Active chord tones now have exact tie subsets, enharmonic respelling and editable tie direction, preserved by native/event-region clipboard, linked transposition, MusicXML and performed playback. Arbitrary continuation controls, tie curvature and complete cross-voice ties remain open. |
| Clefs and clef changes | Partial | Three fixed staff clefs. Needs other C/F/G clefs, percussion, octave variants and mid-measure changes. |
| Keys and key changes | Partial | One constant circle-of-fifths signature. Needs regional/independent signatures, cancellation and custom/nonlinear keys. |
| Meter and meter changes | Partial | One constant meter, now with 1–32 beats and power-of-two denominators through 128. Changes, pickup measures, additive/composite, independent and display-only meters remain open. |
| Accidental display policies | Partial | Per-pitch automatic, forced, hidden, courtesy and parenthesized displays are editable, engraved and retained through native/MusicXML and linked transposition. Regional accidental policies and complete cross-layer/cross-bar rules remain open. |
| Stem, beam, flag and rest placement | Partial | Anchored stem direction/hiding, beam break/join/flat overrides, head shapes and cue sizing accompany automatic engraving. Manual lengths, arbitrary beam angles/hooks and rest offsets remain open. |
| Cross-staff notation and beaming | Missing | No source/destination staff relationship for notes or beams. |
| Dynamics and articulations | Partial | Velocity-derived dynamics, legacy staccato, accent, marcato, tenuto and staccatissimo now have editable marks, vector glyphs and playback. Custom marking definitions, fermata timing, ornaments and complete placement/performance rules remain open. |
| Slurs and phrase curves | Partial | Entry-anchored phrases now cross voices/layers on one staff, with solid/dashed/dotted styling, dash/gap lengths, editable endpoint offsets and asymmetric normalized controls; vector continuations and source handles span systems/pages. Native and Encore MusicXML round trips retain manual curves, standard line styles interchange, and MuseScore voice endpoints/styles import. Reset restores automatic shaping without changing style. Cross-staff/beat-attached curves, wavy lines, independent fragment controls and complete automatic collision profiles remain open. |
| Hairpins, octave lines, pedal, trills, glissandi and custom lines | Partial | Editable native/vector/MusicXML spans now cover crescendo/diminuendo, ±1–3 octave display, damper/sostenuto, explicit-interval trills and consecutive-note chromatic glissandi; supported basic MuseScore spanners import. Held-note CC11 swells, pedal holds and discrete ornaments share audio/MIDI scheduling. Niente/custom line styles, partial/soft pedal, automatic trill neighbors, continuous slides and harp/diatonic glissandi remain open. |
| Tremolos, measured repeats and ornaments | Partial | Explicit-interval trills, key-aware or explicit turns/inverted turns, lower/upper mordents and up/down chord rolls share native notation, vector marks, interchange and all three performed backends. Non-arpeggiate is supported. A recognized string tremolo Technique selects GM tremolo strings. Measured tremolo notation/repeats, fermata timing, custom long/accelerating ornaments and full ornament rules remain open. |
| Repeat barlines, endings, D.C./D.S., coda and Fine | Partial | Inclusive repeat regions support nested noncrossing repeats, 2–16 passes, contiguous numbered endings, vector barlines/voltas and bounded performed audio/MIDI. D.C./D.S., al Fine/al Coda, unique segno/coda targets and optional repeat replay use the same traversal, MusicXML import/export and MuseScore import. Custom labels with multiple targets, per-pass/custom conditional jumps, within-measure navigation, manual bracket placement and direct MUSX repeat import remain open. |
| Lyrics | Partial | Numbered verses/choruses/sections, atomic syllable typing/assignment, hyphens, melismas and elisions, adjustable baselines, lyric-aware spacing and vector continuations; native and MusicXML preservation and MuseScore import. Rich typography, above-staff underlay, verse-number alignment and Finale’s full Lyrics window/metatools remain open. |
| Chord symbols and fretboards | Partial | Free-beat letter-name chords support standard triads, sevenths/extensions, altered/add/omit degrees, alternate basses, N.C., eight stack slots and per-symbol printing/playback. Original vector fretboards have bounded guitar suggestions, 4–12 custom strings, open/muted stops, fingering, barres and known/unknown tunings. Native/MusicXML harmony/frame interchange, linked transposition and regional editing retain the supported semantics. Finale suffix/fretboard libraries, MIDI analysis/learned chords, numeric/Roman naming and arbitrary suffix typography remain open. |
| Guitar tablature, bends, slides and tuning | Partial | Editable chord diagrams retain explicit string tunings and frets; tablature staves, automatic standard/tab conversion, bends and continuous slides remain open. |
| Unpitched percussion layouts and drum maps | Missing | GM melodic pitched percussion is available, but does not supply percussion notation or drum-kit routing. |
| Staff styles, cutaway, slash/rhythmic notation and repeats | Missing | Fixed five-line staff presentation; no regional style definitions. |
| Cue notes, ossia, note sizes and custom noteheads | Partial | Per-entry 40–200% head sizing, six original vector notehead shapes and hidden entries are editable and persistent, with native/MusicXML preservation. These are visual controls; automatic cue extraction, ossia staffs and arbitrary glyph selection remain open. |
| Rehearsal marks and expressive/technique text | Partial | Beat-anchored dynamics, expressive text, technique text and boxed rehearsal marks have staff/system scope, offsets, native persistence, MusicXML preservation and basic MuseScore import. Text typography, automatic rehearsal sequencing, category/library editors, marking metatools and automatic instrument-technique switching remain open. |
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
| Simple Entry standard and laptop profiles | Partial | Laptop duration, pitch/chord, interval, rest, tie, accidental, stem/beam/display and common marking controls follow contextual mappings. Numeric keypad distinctions, standard profile, full tool behavior and metatools remain open. |
| Speedy Entry | Partial | Separate duration-triggered keyboard profile, pitch position and common editing commands. MIDI, Caps Lock pitch rows, insertion and complete frame behavior remain open. |
| MIDI keyboard step entry | Missing | No MIDI input device discovery, event stream or held-chord handling. |
| Real-time transcription | Missing | No recording, metronome/count-in, quantization, split-point or multi-voice transcription. |
| Selection and navigation | Partial | Whole-measure rectangles, measure stacks, visible linked-part source mapping, Select All and platform-specific range extension accompany single-event/cursor selection. Partial-measure/disjoint object selection and the full entry/chord navigation set remain open. |
| Cut, copy, paste and insert | Partial | Atomic event and whole-measure region clipboard workflows preserve supported attached annotations; region Replace, Empty Only and inserted measure stacks are available, with boundary-loss warnings. Filters, repeated paste, clip files and partial-measure/rhythmic shifting remain open. |
| Measure/staff structural editing | Partial | Insert/delete full-score measure stacks remap rhythmic anchors, repeats/endings, navigation, tempo, full-score layouts and each linked-part layout, with atomic dependency checks. Add/remove staves remain supported; staff reordering and general move/independent rhythmic ranges remain open. |
| Transposition, respelling and repitch | Partial | Regional chromatic, spelled fixed intervals and key-aware diatonic scale shifts preserve entry-specific tie/accidental mappings and reject range failures atomically. Regional key changes, advanced respelling policies and repitch-only mode remain open. |
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
| Full ensemble system layout | Partial | Automatic grouping follows saved physical page formats, margins, scaled staff size, lyrical/expression clearance and forced boundaries. Comprehensive collision avoidance for dense/custom engraving remains open. |
| Page size, margins, scaling and orientation | Partial | Full scores and linked parts save Letter/A4/Legal/Tabloid formats, portrait/landscape, editable margins and uniform 50–150% staff scale. Custom paper, mirrored margins, mixed per-staff sizes and complete page-format options remain open. |
| System breaks, locks and measure widths | Partial | Explicit system/page boundaries, visible/selected-system locks and per-measure width weights are editable and persistent. Locked membership is honored; internal beat-chart adjustments, drag layout tools, proportional absolute widths and complete collision resolution remain open. |
| Staff groups, brackets, braces and labels | Partial | Basic automatic staff presentation. Needs editable groups, bracket placement, barline spans and independent abbreviations. |
| Hide empty staves, multimeasure rests and staff spacing | Partial | Linked parts store extra staff spacing. Empty-staff hiding, multimeasure-rest ranges and per-system staff management remain pending. |
| Linked part definitions and voicing | Partial | Named stable definitions support individual/custom/multi-staff parts, piano grand-staff generation, source-linked editing and spelled written-pitch transpositions. Per-staff layer/voice masks, highest/lowest/ranked chord-tone selection, conditional multi-layer policies and self-contained extraction are implemented with exact source maps and documented metadata filtering. Per-part clefs and imported linked definitions remain pending. |
| Independent part formatting | Partial | Each part stores paper, orientation, margins, staff scale/gap, system membership, measure width weights and publication text. Per-item unlink/relink, visibility/position overrides and layout-copy workflows remain open. |
| Print and batch score/parts | Partial | Current part PDF/SVG and numbered batch linked-part PDFs use each part's written pitches and page format. Needs native print dialog, copies/ranges, print selection and detached editable extraction. |
| Publication metadata | Partial | Title/composer, linked-part names, copyright, left/center/right headers/footers and page numbering with title/composer/page tokens. Title pages, styled multiline text blocks, graphics and complete rich formatting remain open. |
| PDF naming and part titles | Partial | Linked-part pages and PDF/SVG names include the part name; batch files receive distinct numbered names. Needs configurable publication filename templates and saved-title conventions from 27.4.1. |
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
| Transport and mixer | Partial | Play/pause/stop, rendered-audio seeking, cursor-start playback, performed-bar loop ranges and selection-to-range controls accompany the playhead and mixer. Scrub, count-in, metronome and complete region/cue transport remain open. |
| Ties, dynamics and articulation playback | Partial | Independent pitch ties, explicit dynamics, sustained CC11 hairpins, slur overlap, grace timing, damper/sostenuto holds, trill/gliss note sequences and a small set of real GM technique changes share audio/MIDI scheduling. Per-pitch articulation, dynamic-layer crossfades and full instrument-specific interpretation remain open. |
| Tempo map and expressive timing | Partial | Beat-anchored tempo marks and linear-BPM accelerando/ritardando ramps share an analytic performed-tick/seconds map across repeats, audio, duration and playhead. MIDI emits bounded sampled tempo events; MusicXML preserves marks and dashes-delimited ramps, and MuseScore/MIDI tempo steps import. Curved ramps, MuseScore gradual-tempo spanners, swing, fermata timing and independent performance offsets remain open. |
| Repeat traversal | Partial | Shared bounded traversal performs nested bar repeats, alternate endings and conventional one-shot D.C./D.S./coda/Fine navigation in audio, MIDI, duration and playhead. Custom conditional pass programs, multiple named targets and complete Finale repeat assignments remain open. |
| Human Playback equivalent | Partial | A deterministic shared planner handles supported marks and exact pizzicato/arco/tremolo-string/muted-trumpet labels. Finale style interpretation, its full dictionary, instrument-specific behavior and custom library keyswitch rules remain open. |
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
| Direct Finale `.musx` reader | Partial | Basic EnigmaXML with four layer frames and a real 27.4.1 fixture. Documented controller/v2 branches now have bounded import with independent timing. Tuplets, grace structures and complete semantics/layout remain open. |
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
  up to eight source voices on their staff; direct MUSX reads its four layer
  frames. Engraving separates stems/beams/rests by layer; MIDI and sampled
  playback separate layers to avoid tie or repeated-pitch interference.
  Sixteen rhythm/layer regressions accompany this work. This is partial tuplet
  support: the 480-PPQ score grid rejects fractional ticks rather than rounding
  them. Ordinary triplets and representable 5:4 ratios work, while a 7:4 eighth
  does not. Nested, mixed-value and cross-bar grouping remain open. Four-frame MUSX coverage uses synthetic fixtures;
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

- **Vocal underlay and phrase curves:** lyrics use persistent `(staff, measure,
  onset, layer)` anchors with 1–512 numbered lines per Verse/Chorus/Section.
  Atomic text assignment recognizes syllable hyphens and underscore melismas,
  rejects exhausted melodies without partial edits, and leaves other lyric
  lines unchanged. MusicXML retains line kinds, syllabic values, undertie
  elisions, baseline offsets and explicit extension endpoints. MuseScore 2–4
  readers retain supported lyric verses and legacy/modern same-layer slurs.
  Phrase curves store direction, arch height and endpoint offsets; printed
  vector fragments continue across systems and pages. Shared spacing reserves
  room for ordinary lyric syllables and lower-staff baselines. Tests cover
  native persistence, atomic invalid edits, XML round trips, MuseScore forms,
  unsupported-feature warnings and continuation geometry. Rich lyric font
  styling, above-staff lyrics, arbitrary slur Bézier control points,
  cross-staff/cross-voice curves and full Finale lyric-editor behavior remain
  explicitly partial. See the W3C MusicXML references for
  [lyrics](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/lyric/),
  [word extensions](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/extend/)
  and [slurs](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/slur/).


### Encore 0.7 integration qualification

The Cantabile study combines multiple numbered lyric lines, a cross-system
melisma, phrase slurs, first/second endings, piano grand-staff parts, B-flat
clarinet and F-horn written transpositions. Native reopen and MusicXML round
trips are exercised separately from PDF/SVG engraving and performed audio/MIDI.
Lyrics and slurs have contextual entry/edit controls; repeat/navigation edits
preflight the full route; linked views map all hit tests and written entry back
to shared concert pitches. Platform-specific lyric/slur/part shortcuts are
listed in [SHORTCUTS.md](SHORTCUTS.md). Single-event clipboard preserves lyric
syllables, with region-spanning underlay/curves still pending. Regression checks
cover focus, undo/redo, exhausted lyric input, dangling note/staff anchors,
written part entry, hidden staff selection and protected navigation targets.
The default remains monochromatic dark, with the persisted light option.

These are implemented workflows with explicitly partial Finale coverage. The
advanced limitations in their table rows remain open; full Finale parity has
not been achieved.


Local Linux release verification: 202 library and 83 editor/CLI tests pass;
format and all-target Clippy checks pass, with warnings denied. Cantabile's
MusicXML round trip retains all 65 syllables, both phrase curves, alternate
endings and all note pitches. Its sampled render is 27.5 seconds plus a
three-second release, with no clipped PCM samples. Full-score and three part
PDFs were inspected; the piano uses an A4 MediaBox. Both UI themes were rendered
with actual egui meshes. Carol's sampled regression SHA-256 remains
`fda033e7c61f0f5a0c15cc66e1e9046d852595c45123eb9f5b841262f1e8a82f`.
Native macOS/Windows qualification and the remaining partial rows stay open.

### Encore 0.8 integration qualification

- **Production layout foundation:** full scores and each linked part now save
  validated paper formats, margins, staff scale and spacing, explicit system
  and page breaks, locked system membership, relative measure width weights,
  publication headers/footers, copyright and starting page number. Staff scale
  changes printed symbols and editor hit maps without stretching the physical
  PDF sheet; page text remains legible at its independent text size. The layout
  dialog edits the current score/part format, with lock/unlock commands over
  selected visible systems. Structural measure edits remap formatting indices.
  Tests cover legacy format defaults, conflicts and bounds, native persistence,
  page/system membership, aligned weighted widths, scaling/hit-map inversion,
  publication text and independent written-part metadata. Custom paper,
  mirrored margins, beat-chart editing, drag layout tools, mixed staff sizes,
  title-page composition and comprehensive dense-score collision handling
  remain open. This is a production layout foundation, not full Page Layout
  tool parity.

- **Regional editing:** source-aware whole-measure selection supports atomic
  event/region cut, copy, replacement, empty-only and inserted-stack paste.
  Native annotations, pitch-specific entry styles and complete contained spans
  travel with the music; losses at copy boundaries are reported. Stack edits
  remap both score and linked-part formatting. Chromatic, spelled and key-aware
  diatonic regional transposition are available. Partial measures, filtered and
  repeated paste, arbitrary rhythmic insertion and clip files remain open.
- **Manual engraving and ties:** anchored appearance overrides support stem
  direction, break/join/flat beams, visibility, cue head scale, six notehead
  shapes and per-pitch accidental/tie choices. Native saves, linked written
  projections and clipboard preserve these fields. MusicXML exports standard
  visible elements plus valid hidden metadata for exact Encore round trips.
  Validation and engraving index overrides by source onset; large documents
  avoid a full override scan for every rendered note. General custom glyphs,
  rest offsets, arbitrary stem lengths, beam angles and cross-staff groups
  remain open.
- **Contextual shortcuts:** added Simple/Speedy engraving commands, global
  platform layer selection, Speedy layer navigation and selection/layout range
  commands. Windows Fit Measures uses Ctrl+M; macOS uses Command+Shift+M.
  Text and modal focus remain protected. Native physical keypad/Clear/asterisk
  validation, standard profile, programmable metatools and all unsupported
  workflows remain open.

Local Linux 0.8 qualification: **243 library and 100 editor/CLI tests** pass;
format, all-target Clippy with warnings denied and optimized release build pass.
The two new native studies have inspected PDF previews. Production Layout's
MusicXML round trip preserves 65 lyric records, two slurs, two expressions, a
hairpin, a tempo ramp and its repeat; Entry Engraving preserves all 13 entry
styles, including its partial chord tie. Both modal windows were rendered at
980×680 with actual egui meshes and fonts. The production sampled render lasts
24.887 seconds plus a three-second release and contains no clipped PCM samples.
Carol's 83-second sampled WAV remains byte-identical to the 0.7 regression hash.
The monochromatic dark default, light option and native-platform limitations
remain unchanged.

### Encore 0.9 integration qualification

Grace groups, letter-name harmony/fretboards and octave/pedal/trill/glissando
lines now have native models, editors, vector engraving and documented
MusicXML/MuseScore interchange. Shared performed notes/controllers drive both
audio engines and MIDI, including borrowed grace timing, fixed-interval
ornaments, restricted sostenuto, sustained CC11 envelopes, optional harmony
and selected GM technique changes. Independent accompaniment-controller and
sample-bank limitations remain explicit in COMPATIBILITY.md.

Source-linked chord and grace hits, ottava/written-pitch entry, note-attached
versus free-beat lifecycle repairs, event clipboard v4, whole-region metadata
and part-switching have controller regressions. Go to Measure, page scrolling,
page fit, rendered-audio seeking, cursor-start and performed-range loops extend
listening/navigation without modifying printed source rhythms. Tempo clocks are
cached across selection changes. Native physical keyboard/audio/platform
qualification remains separate from display-independent controller checks.

Local Linux qualification: **287 library and 118 editor/CLI tests** pass; format,
all-target Clippy with warnings denied and optimized release build pass.
Both new native studies export PDF/SVG/MusicXML/MIDI and sampled WAV. Their
printed pages and dark editor were inspected. Ornaments retains all eight
grace groups and supported chord/frame geometry; Performance Lines retains all
four lines, ten expressions, hairpin and tempo ramp through MusicXML. Standard
MusicXML frames do not retain tuning, and imported field order can differ.
Both sampled study renders contain no clipped PCM samples. Carol remains
byte-identical to the recorded 0.7/0.8 sampled SHA-256. The default remains
monochromatic dark with a remembered light option. Full Finale parity has not
been achieved; remaining table gaps and native-host checks remain open.


### Encore 0.10 integration qualification

Each of four layers now carries two independent rhythmic voices. Native primary
defaults remain omitted for old-score compatibility. Secondary branches retain
validated launch points; Speedy apostrophe and toolbar V1/V2 operate within the
current layer. Navigation, ties, lyrics, slurs, grace and entry styles retain
voice identity. Launch deletion is refused atomically while a dependent branch
remains; a primary rest can continue launching it. All note entry also validates
glissando endpoints, pedal capture and annotation invariants before accepting
the edit. Keyboard/undo/redo frames exercise the new workflow.

Vector engraving separates simultaneous heads and hit maps, opposes paired
stems, and keeps secondary branch beams and tuplet groups separate. It does not
invent leading/trailing secondary rests. MusicXML IDs 1–8 and source labels
preserve eight lanes; implicit secondary gaps use `forward`, explicit rests
remain notes and valid hidden metadata retains launch points. Documented direct
MUSX controller/v2 branches now use their own cursor without advancing Voice 1;
orphan/nested/overlapping branches are rejected. Real direct-MUSX Voice 2 fixture
qualification and unsupported tuplets/grace/cross-staff structures remain open.

MIDI, sample playback and procedural playback keep equal-pitch lifetimes
independent. Dynamics, hairpins, technique changes and octave-display lines
remain layer-scoped; pedal covers all notation voices. Chord accompaniment uses
separate lanes 8–15. Event clipboard v5 and regional clipboard v2 prevent older
clipboard readers flattening secondary content; source-linked projections and
whole regions retain launch/style identity. Isolated event paste reports when
it detaches a secondary launch.

Local Linux qualification: **311 library and 126 editor/CLI tests** pass; format,
all-target Clippy with warnings denied and the optimized release build pass.
Independent Voices and Eight Voice Engraving export/reopen supported rhythms,
pitches, voices and launch points through MusicXML, and render PDF/SVG/MIDI and
sampled WAV. Both printed studies and both actual egui themes were inspected.
Neither sampled study clips; Carol's sampled regression remains byte-identical.
Dense multi-voice collision optimization, platform keyboard qualification and
all remaining partial/missing audit rows stay open.

### Advanced slur checkpoint (0.11)

Same-staff cross-voice/layer phrases, solid/dashed/dotted styles and asymmetric
manual controls are implemented end to end. Number levels follow MusicXML
document order, including stops serialized before starts in independent voices;
grace connectors use the same allocation. Native geometry and source endpoint
identity survive linked projections and bounded Encore MusicXML metadata.
Cross-staff curves remain explicit gaps because native staves currently export
as separate MusicXML parts. Beat-attached shapes, wavy strokes, per-system
control overrides and Finale's complete automatic collision/profile engine are
not claimed. The implementation follows the public [MusicXML slur reference](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/slur/)
and [number-level rules](https://www.w3.org/2021/06/musicxml40/musicxml-reference/data-types/number-level/),
with [Finale's documented slur editing](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Slurs.htm)
as the clean-room workflow reference.


### Encore 0.11 integration qualification

Voiced linked parts retain explicit source event/tone maps after filtering and
written transposition. The manager supports per-staff layer/voice masks and
chord ranks, including conditional behavior in multi-layer measures. Filtered
annotation losses are reported. Read-only part views provide Edit source score
and standalone extraction; entry, cut/copy/paste and regional editing cannot
silently alter hidden music. Layout, mixer controls and history remain usable.
CLI extraction selects a stable part ID or one-based number, using written
pitches for PDF/SVG and concert pitches for native/XML/MIDI/audio.

Turns, inverted turns, lower/upper mordents, upward/downward rolls and
non-arpeggiate signs have native models, editors, original vector symbols,
MusicXML/MuseScore interchange and shared performed scheduling. Key-derived
neighbors and explicit semitone overrides, bounded window/delay, grace/repeat
interactions and tied continuations are covered. Principal sustain remains
continuous after the ornament. Implicit global fermata time, measured tremolos,
cross-staff rolls and complete Human Playback interpretation remain open.
Performed same-key sostenuto reattacks fail visibly.

The advanced slur checkpoint above adds same-staff cross-voice/layer geometry,
styles and numeric control points. Small-window controls scroll within the
viewport, with a real-frame reset/style regression. Cached ornament indices
reserve head/accidental, grace and roll columns and vertical clearance. Event
clipboard v6 and region v3 preserve ornaments and advanced slur metadata while
continuing to accept older supported versions. Whole-number decimal MusicXML
alterations now parse consistently; fractional/microtonal pitches still fail
explicitly.

Local Linux qualification: **354 library and 133 editor/CLI tests** pass; format,
all-target Clippy with warnings denied and the optimized release build pass.
Three new native studies export/reopen supported slur/ornament semantics, and
three voiced parts extract/reopen with retained concert pitches. PDF/SVG, MIDI
and sampled WAV exports and bounded editor dialogs were inspected. Sampled
studies do not clip; Carol's sampled regression remains byte-identical. Full
Finale parity and native macOS/Windows qualification remain open.
