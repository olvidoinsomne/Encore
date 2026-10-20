# Encore implementation contract

Encore 0.7 is an original clean-room Rust music notation application. No Finale source, assets, file parsers or proprietary UI are used. The native interface has persistent light/dark themes around a white, printable score page. No Finale proprietary code or assets are used; foreign readers use public format documentation.

## Score and instrument catalog

`src/model.rs` owns the serialized types. Time uses 480 ticks per quarter. Pitches are written at concert pitch: MIDI = `(octave + 1) * 12 + step semitone + alter`. Empty pitch vectors are rests. Notes have an absolute start within their measure; events carry a zero-based layer (0–3), with one nonoverlapping voice in each layer. Independent layers may overlap. Optional tuplet ratios preserve exact integer ticks; unsupported fractional-grid rhythms are rejected. A piano grand staff uses two `Part`s. Serde JSON is the native `.encore` format; saves are validated and atomic.

`Instrument::ALL` contains each of the 128 melodic General MIDI programs exactly once, in zero-based bank order. `name()`, `family()`, `gm_program()` and `default_clef()` provide presentation and export metadata. `from_gm_program(u8)` returns the canonical variant for valid programs and `None` above 127. The original 18 named variants and their serialized names are retained; additional programs use `Instrument::GeneralMidi(u8)`. Explicit aliases for an original program are valid, and score validation rejects program values above 127. `Instrument::LEGACY` preserves the original 18 voices in their original order for procedural synthesis and regression checks.

Older `.encore` projects load without instrument conversion. Projects containing the additional `GeneralMidi` representation require Encore 0.4 or later and cannot be opened by older builds. MIDI and MusicXML export all 128 program identities; MusicXML converts the zero-based program to its one-based numbering.

Other core APIs:

- `Duration::ALL`, `ticks()` and `name()`; `Note::ticks()`.
- `Pitch::midi()`, `from_midi(u8)`, `staff_step()` (`C4 = 28`), `from_staff_step(i32, key_fifths)` and `name()`.
- `Score::demo()` (First Light), `Score::carol_demo()` (Winter Ascendant), `Score::blank()`, `measure_ticks()`, `measure_count()`, `validate()`, `add_measures()`, `add_part()`, `normalize()`, `load()` and `save()`.
- Export functions `musicxml(&Score)` and `midi(&Score)` honor key, time, gaps/rests, chords, ties, staccato, tempo, mixer state and program selection. Validation rejects malformed scores before export.

`src/instruments.rs` supplies character descriptions, concert-pitch or keyboard audition guides, and four-bar test phrases. The original orchestral ranges remain practical composing guides. Additional GM profiles describe useful keyboard registers rather than asserting a physical instrument's playing compass. All 128 phrases validate and remain inside their displayed guides.

## Engraving

`src/engraving.rs` turns a score and `EngravingOptions` into vector `Page`s. Options select a note, playhead, optional staff filter and independent `PartLayout` paper geometry. Each page contains line, ellipse, path and outlined text primitives, staff regions and note hit maps. SVG and PDF output use these same pages. Text and notation are portable vector outlines with no runtime font dependency for printing.

Coordinates grow downward. Staff diatonic references are treble bottom-line E4 = 30, bass G2 = 18 and alto F3 = 24; a pitch's vertical position is `bottom_y - (staff_step - bottom_step) * spacing / 2`. Layout aims for four measures per system and adapts staff groups and page count to the score. The playhead uses absolute quarter beats. Shared nonlinear measure maps align onsets across displayed staves and reserve space for short-note bursts; `StaffRegion::x_at_tick()` and `tick_at_x()` are also used by the editor playhead and mouse entry.

## Anchored lyrics and slurs

`src/document.rs` stores additive, empty-default `Notation` in the score.
`Anchor` identifies a rhythmic entry by source staff, measure, onset tick and
layer. Removing/reindexing staves or deleting notes updates or removes dependent
annotations; validation rejects dangling anchors. `src/lyrics.rs` owns numbered
verse/chorus/section syllables, syllabic state, optional extension endpoints and
baseline offsets. `assign_text` tokenizes and applies an entire text assignment
using one sorted melody traversal and a map of lyric keys, then validates before
committing metadata. `src/slurs.rs` stores same-layer phrase endpoints,
direction, arch height and endpoint offsets. Validation uses sorted endpoint
sets and a sweep of active phrases, with at most 16 overlaps per staff.

The engraving child modules add outlined lyric text, hyphens, word extensions
and original flattened vector phrase curves after staff regions are known.

`src/grace.rs` stores bounded `GraceGroup`s keyed by a principal `Anchor` and
before/after placement. Nested `GraceNote`s carry pitches and visual durations;
they never enter the written rhythmic event stream. The grace collector resolves
source staff/voice anchors after import and applies pickup shifts once. Standard
MusicXML grace fields are combined with valid hidden metadata for exact native
options; `MusicXmlIndex` caches export fragments and sweeps active phrase-slur
levels once instead of scanning the score for every exported entry.
`src/engraving/grace.rs` reserves shared temporal gaps and first-onset prefixes,
draws small vector heads/stems/beams/accidentals/slashes/connectors and records
separate `GraceHit`s. `GraceSelection` identifies the principal, side and local
note index, so ornament editing cannot be confused with principal note indices.
The performance pipeline decorates unfolded principal events with fractional
borrowed-time notes before final ties; written PPQ onsets stay unchanged.
Each system/page receives its own continuation fragment. Lyric widths inform
shared temporal spacing and system measure counts; lyric baselines and slur
clearance inform vertical staff spacing. Foreign format `annotations` collectors
retain source voice identities until each reader resolves them into native
staff/layer anchors, including pickup shifts. MusicXML emits matched numbered
slur starts/stops and explicit lyric-extension starts/stops. Unsupported rich
text, cross-staff phrases or custom shape details produce import notes.

## Repeats and navigation

`src/repeats.rs` validates inclusive repeat regions, nested bodies and contiguous
alternate endings. `src/repeats/navigation.rs` adds conventional one-shot
D.C./D.S., Fine and coda navigation over either the normal or final-ending
route. `Score::playback_order()` is shared by duration, playhead, MIDI and both
audio schedulers. Traversal is capped at 16,384 performed bars and two million
events; jumps break ties and phrase overlap across nonadjacent source bars.
Native editing and MusicXML/MuseScore readers reject ambiguous targets or
unsupported custom pass conditions. Removing a final bar is preflighted against
all navigation dependencies before the editor commits the structural change.

## Saved page production

`src/layout.rs` defines `DocumentLayout`, also re-exported as the backward-
compatible `parts::PartLayout` type name. The native full score stores
`Notation::layout`; linked parts retain independent instances. Format validation
checks printable paper area, finite margins/scaling, measure-index bounds,
nonoverlapping system locks, break/lock conflicts, unique width weights and
bounded publication text. `remap_measures()` updates formatting after structural
measure edits without treating page settings as musical clipboard contents.

Engraving works in scaled musical coordinates, then transforms every primitive,
staff region and hit point back onto the physical sheet. Nonlinear time-map
anchors remain normalized, preserving the mouse inverse under staff scaling.
Explicit boundaries limit automatic systems; locks fix membership; relative
weights allocate remaining width while reserving basic glyph room. Headers,
footers, copyright and pagination are drawn after music scaling so their text
size and physical margins remain stable. `src/app/layout.rs` edits the current
full-score or part format and exposes lock/unlock helpers for contextual keys.

## Linked parts

`src/parts.rs` stores `LinkedPart` definitions inside `Score::notation.parts`.
A definition has a stable ID, name, source staff indices, a spelled
`Transposition` and independent `PartLayout`. Empty definitions generate views
from the current instrument list without mutating the score. `project()` clones
only for presentation, remaps lyric/slur anchors and selections, and transforms
written notes and key signatures. `Projection::source_staves` maps all canvas
hits back to the source; inverse transposition converts written mouse entry to
concert pitch. Playback always consumes the source. Definitions participate in
native serialization and score undo/redo. `export_pdf_parts()` preflights all definitions before writing numbered PDFs
one at a time, bounding memory even when parts overlap.

## Regional editing

`src/editing.rs` owns atomic whole-measure editing. `Region` keeps ordered
source staff indices, including nonadjacent staves displayed in linked parts.
A bounded, versioned `RegionClip` carries measure rows and supported relative
notation anchors. Copying omits cross-boundary spans with explicit warnings;
score-wide repeat/navigation/tempo metadata travels only with full staff stacks.
Paste supports replacement, empty-only collision protection and inserted measure
stacks. Native source pitches, instrument assignments and linked definitions
remain separate from musical clipboard contents. Partial-measure selections,
clipboard filters and clip files are future work.

Insert/delete remaps anchored notation, tempo points, repeat endings and every
saved full-score/part layout. Missing navigation targets and removed alternate
endings or ramp endpoints fail without changing the source. Chromatic/spelled
transposition updates MIDI-keyed engraving overrides, chord roots/basses and attached grace pitches alongside notes. A known
metadata-field guard rejects unsupported future notation rather than silently
losing it. `src/app/regions.rs` supplies selection/action controls; clipboard
routing keeps previous single-event formats readable, and all changes enter the
shared score snapshot history.

## Audio

Application audio goes through `src/audio.rs` and `AudioEngine`, either `Procedural` or `Sampled(SampleBank)`. `load_default()` returns the selected engine and an optional visible fallback message. The bundled bank is the default. An explicit invalid `ENCORE_SOUNDFONT` never silently selects another bank. GUI playback, auditions and WAV exports clone the active engine; command-line audio operations load it once. GUI exports call `export_to_with_engine`. The original `synth::render` remains a deterministic procedural comparison.

`src/sampled.rs` loads a bounded, validated SF2 bank and requires all **128 melodic bank-0 programs**. A bank with missing programs is rejected instead of substituting a piano or procedural voice. All catalog entries, including woodwinds, harp, electronic presets and sound effects, use SoundFont sample regions. The bundled GeneralUser GS bank advertises 261 presets and 13 drum kits upstream; Encore currently exposes only its 128 melodic bank-0 programs. GS variations, bank selection and drum-kit mapping are outside this version's scope. Electronic and effect patches can use synthesized source material and SF2 processing; the catalog is not a collection of 128 acoustic recording sessions.

OxiSynth renders the samples and SF2 modulators. Each part receives a fresh synthesizer; a shared decoded font is loaned under a mutex and restored by RAII on success, error or unwinding. Loans end between parts. This avoids channel/program interference and duplicate sample decoding. Commands use rounded frame positions; OxiSynth's 64-frame DSP buffering can add up to 63 frames of latency. Ordinary articulation helpers preserve legacy behavior when their metadata is empty; accent, marcato, tenuto and staccatissimo adjust bounded attack velocity and release time. Ties merge matching chord pitches independently within each layer, and note-offs precede retriggers at the same frame. Mute/solo, staff volume and pan are honored. The bundled GeneralUser GS bank has measured orchestral mix trims; additional GM presets and other banks retain native relative levels.

`src/synth.rs` retains the original 18 procedural voices. New GM programs use related family approximations only when Procedural mode is explicitly selected or a visible fallback has occurred. These approximations do not replace presets in sampled mode. Both audio backends use the shared room/limiter pass in `synth::finish`, 44.1 kHz stereo output and the same WAV encoder. The original procedural Carol WAV files remain the comparison baseline.

`Playback` owns a persistent rodio output stream and sink, with play, stop, pause, resume, playing/paused state, position and volume controls. Score playback and auditions use worker jobs and discard stale results when an edit or source change cancels the request. Rendering is limited to ten minutes of music in memory. Retain bank attribution and the sampled engine's LGPL source/rebuild bundle with the application.

## Interface and commands

`src/app.rs` owns the native editor, note input, inspector, mixer, history and transport. The **Instrument library** opens from **Instruments**, the Instruments panel's **+**, or the keyboard's **Instrument lab** button. Search and family filters browse all 128 sounds. Selection exposes the actual preset, character and guide range, single-note and phrase auditions, velocity and hold controls, phrase WAV export, **Add staff** and **Use on active staff**. Changing the active staff's sound preserves its notes, changes its clef and participates in undo/redo. Playback sound controls select the sampled library, procedural mode or a replacement complete SF2 bank.

`src/main.rs` owns desktop startup and command-line operations. `--instruments` renders 128 audition WAV files. `--procedural` and `--soundfont PATH` select the CLI audio engine. `--compare-carol` renders the same score with both engines and writes matching opening excerpts. The launcher continues to use an available release binary or build one.

Automated tests cover native compatibility and validation, all program identities and MIDI/MusicXML exports, engraving, all sampled presets, the original procedural voices, audio scheduling and failure recovery, library filtering, instrument changes and undo, and keyboard/transport behavior. Display-independent tests render actual egui meshes and fonts for editor and library visual QA.

## Score import and preferences

`src/import.rs` exposes `load(&Path) -> Result<ImportResult, String>` with an editable score and visible warnings. Separate modules read partwise MusicXML/MXL, MuseScore XML/MSCZ, Standard MIDI File 0/1 and a bounded subset of modern Finale MUSX. Compressed inputs have entry, path and expanded-size limits; XML never resolves network resources. Unsupported rhythm/pitch structures fail explicitly. Importing does not retain a native save path, and `needs_native_save` protects unsaved imported documents. The original file is read only.

The default dark theme uses neutral monochromatic colors. `src/shortcuts.rs` maps separate Finale Simple Laptop and Speedy profiles; `src/app/entry.rs` applies validated edits, and `src/app/clipboard.rs` handles bounded single-event clipboard transfers. The entry profile is persisted separately from score data.

The app's `Theme` selects a shared palette for all chrome and configures egui visuals independently of font setup. The preference is stored through eframe persistence under `encore.theme`; changing it does not edit the score. macOS bank discovery includes `Contents/Resources/assets`, and native packaging/CI is documented in BUILDING.md.

## Harmonic notation

`src/chords.rs` separates spelled `PitchClass` roots/basses and semantic kinds/degrees from display suffixes. `ChordSymbol` uses a free staff-beat anchor normalized to layer zero and a bounded stack slot. It remains attached when a note is deleted. `src/chords/parser.rs` parses common letter-name suffixes; `frame.rs` validates original string geometry and performs a bounded standard-guitar search. Known tuning and unknown imported geometry are distinct states.

Linked projections transpose harmonic spelling and diagram stops alongside nested grace pitches. Regional copying/remapping treats these as staff-anchored musical content. Event clipboard version 4 carries principal-linked before/after grace groups; free-beat harmony/performance lines travel with whole regions. `src/chords/musicxml.rs` writes standard harmony/frame elements and reads semantic degrees rather than inferring sound from suffix text. Import collectors resolve staff/voice mappings, pickup shifts and cursor offsets before committing a validated score. `src/engraving/chords.rs` supplies vector diagrams, spacing/clearance and semantic hit rectangles. The shared performance pipeline realizes concert harmony in independent stack lanes.
