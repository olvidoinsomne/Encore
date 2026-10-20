# Platforms and score compatibility

Encore 0.11 targets Linux, macOS and Windows. Linux builds and packaging are
verified on the development machine. Native macOS and Windows validation is
still required; their build commands, packaging scripts and manual GitLab CI
jobs are in [BUILDING.md](BUILDING.md). A Rust target alone does not supply the
Apple SDK or the Windows MSVC toolchain. The macOS package is an unsigned `.app`;
Windows packaging produces a portable ZIP.

## Opening scores

Use **File → Open**, pass a score path to the executable, or run:

```sh
./run-encore --import source.mscz converted.encore
./run-encore --export source.mxl score.pdf
./run-encore --import source.musx converted.encore
```

| Input | Current support |
| --- | --- |
| Encore `.encore`, native JSON | Native opening and saving, including earlier Encore projects. |
| MusicXML `.musicxml`, `.xml`, compressed `.mxl` | Partwise MusicXML. Staves retain up to eight independent voices as four layers with primary/secondary voices on the original staff. Encore voice IDs 1–4 map primary layers and 5–8 map secondary layers; other labels are assigned deterministically without flattening. |
| MuseScore `.mscx`, `.mscz` | Native XML scores from MuseScore 2, 3 and 4, including their compressed containers. |
| Finale `.musx` | Initial direct reader for the modern EnigmaXML format, tested against a real Finale 27.4.1.146 document. The supported subset is described below. |
| MIDI `.mid`, `.midi` | Standard MIDI File type 0/1 with PPQ timing. Exact notation is inferred from performance events; unsupported timing is rejected in exact mode. |
| Legacy Finale `.mus` | Not read directly. Export MusicXML from Finale or another compatible converter, then open it in Encore. |
| Finale `.lib` | Not imported. These store reusable notation/document settings, rather than score or sample-bank contents. |

Imported scores open as new Encore documents. **Save** asks for an `.encore`
destination and does not overwrite the original file. Import notes identify
features that cannot be retained and can be reopened with **Edit → Import
notes…**. Unsupported timing or pitch structures produce an error instead of
being rounded into a misleading score.

MusicXML and MuseScore readers retain the supported pitches, rhythms, chords,
rests, ties, staccato, accent, marcato, tenuto, staccatissimo, instruments, initial tempo/key/meter and available mixer
information. They convert transposing instruments to concert pitch. Playback
uses retained tempo marks and supported ramps. Key changes use the
initial signature while preserving explicit note pitches. Lyrics and same-layer
slurs now retain the subset described below. Page layout, pedal/other expression lines
and advanced articulations are not fully retained. Breve through 128th values and exact representable tuplets are supported. Nested tuplets, fractional-grid ratios, percussion maps and changing time signatures still need simplification in the source program first.
When no explicit tempo is stored, import uses 120 quarter notes per minute and
reports that choice in the import notes.

### Lyrics and slurs

Native Encore scores retain numbered **Verse, Chorus and Section** lyric lines,
syllable hyphens, word-extension endpoints, undertie elisions and baseline
adjustments. Type/assign text with spaces between words, hyphens between
syllables and underscores for additional melody notes. Assigning more text than
there are notes fails atomically. Baselines and shared time spacing reserve
room for common vocal underlay; systems/pages receive continuation fragments.
Very long unbroken syllables and extremely dense multiple-line layouts still
need manual simplification; rich typography and Finale's full lyric window
remain open.

MusicXML imports/exports the supported line names/numbers, syllabic values,
elisions, baseline offsets and melisma endpoints. MuseScore imports numbered
verses and tick/fraction extensions. Pickup-spanning MuseScore location slurs
use raw source positions followed by each measure's pickup shift; unresolved
endpoints are rejected rather than moved silently. Hidden or above-staff lyrics, font markup
and per-repeat visibility produce loss notes. Non-numeric MusicXML lyric line
labels currently use line 1 with a warning; rename them numerically in the
source application when different lines would collide.

Slurs join later pitched entries on the same staff, including different layers
or voices. Native projects preserve automatic/above/below placement, arch height,
endpoint offsets, solid/dashed/dotted styling, dash spacing and asymmetric
normalized control insets/heights. Manual controls freeze automatic collision
lifting; Reset curve restores automatic shaping while retaining the line style.
Every system/page receives its own vector fragment and editable source handles.

MusicXML retains standard line style, dash/gap lengths and relative endpoint
offsets. Encore adds a bounded `encore-slur-v1:` other-notation record for exact
normalized manual shape round trips; other applications may discard that record
and re-engrave the horizontal contour. Standard vertical control distances are
retained when both endpoint controls are available. Absolute horizontal and
per-system continuation controls are re-engraved with an import note. Slur
numbering uses serialized voice order, so cross-voice stops appearing before
starts pair correctly. Ambiguous reused levels fail explicitly. MuseScore
retains legacy numbered and modern location-based endpoints across voices on
the same staff, plus solid/dotted/narrow or wide dashed styles; custom
per-system control points produce an import note.

Cross-staff slurs are explicitly rejected in native editing and omitted with
an import note. Grouped multi-staff MusicXML export is required before that
workflow can interchange reliably. Curves attach to entries rather than
individual chord tones; beat-attached curves, wavy styles, per-fragment shape
overrides and complete automatic obstacle avoidance remain open. At most 16
slur levels may overlap in MusicXML document order. Cross-voice curves currently
affect notation only; same-lane phrases retain bounded legato playback.

### Grace groups

**Notation → Grace notes** edits groups attached before or after a pitched
principal entry. Groups have separate pitches/chords, note values and one dot,
beam breaks/joins/flat beams, 40–100% size, slash and an optional principal-note
connector. They do not consume or alter written measure time. Click a small
grace note to reopen its group. Native saves, source-aware regional editing and
linked written parts retain groups; removing their principal removes them.

MusicXML uses standard [`grace`](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/grace/)
notes without duration fields, chord membership, visual values, slash and
steal-time percentages. Valid hidden `other-notation` metadata retains Encore's
exact before/after side, size, beam and velocity choices. Other applications can
use the standard notes and timing. Principal connector slurs use a free standard
slur level; if all 16 levels are occupied, the native connector remains in metadata.
Foreign grace runs normally attach before the next pitched principal; explicit
Encore after markers remove that otherwise ambiguous association. MuseScore's
before/after grace tags are associated with their following serialized principal,
including pickup shifts and the stored reverse order of after groups.

All three playback paths share borrowed-time scheduling: before groups borrow
the previous contiguous performed note or delay the principal; after groups
borrow its tail. Borrowing is limited to 0–75% of a donor, with the principal
used when no eligible previous note exists, including after repeat jumps.
Displaced tie endpoints rearticulate. The original written notes remain exact.
MusicXML `make-time`, grace tuplets/ties/lyrics/articulations, arbitrary
grace-to-grace spanners, full grace-specific accidental policies and ordinary-
entry-to-grace conversion/rebar commands remain unsupported. MuseScore grace
spanners produce import notes. Direct Finale MUSX grace import remains pending.
Bounds are 32 notes/chords per group, 16 pitches per chord, 10,000 groups and
50,000 grace notes per document. Dense locked systems may still require manual
layout adjustment.

### Expressions, hairpins and tempo

**Notation → Expressions and tempo** edits beat-attached dynamic, text,
technique and rehearsal marks, crescendo/diminuendo spans and global tempo
marks/linear BPM ramps. Marks may sit on rests; removing a note does not remove
its beat expression. Staff/system scope and bounded placement offsets persist.
Explicit velocity expressions change note attacks without rewriting written
rhythms. Hairpins now also send continuous MIDI expression (CC11), so held notes
swell in sampled and procedural audio. A fixed lane attack reference provides
headroom for this controller envelope; it approximates GM loudness and does not
crossfade recorded dynamic layers. A few explicit Technique labels select actual
GM presets: `pizz.`/`pizzicato` selects pizzicato strings (acoustic bass for double
bass), `trem.`/`tremolo` selects tremolo strings, and `con sord.`/`muted` selects
muted trumpet. `arco`, `ord.` and `senza sord.` restore the staff's original
program. Other words and unsupported instrument/technique pairs remain descriptive.
Sampled playback and MIDI select those actual GM programs; the explicitly chosen
procedural fallback uses plucked, tremolo and muted-filter approximations.
These switches are not Garritan keyswitch compatibility.

MusicXML preserves these marks, supported hairpin endpoints and dashes-delimited
tempo ramps; hidden standard extension elements retain Encore-specific expression
categories, playback endpoints and offsets during round trips. Other applications
can use the standard visible direction, dynamic, wedge and metronome data.
MuseScore imports tempo steps, dynamics, staff/system/rehearsal text and basic
hairpin spans; gradual-tempo spanners are reported as unsupported. MIDI import
retains later tempo steps. Imported per-note velocities remain authoritative
unless a direction explicitly supplies a playback velocity.

The tempo map integrates linear changes in BPM analytically and resets to written
tempo state on repeat/navigation jumps. The same map drives sampled/procedural
note timing, duration and playhead position. MIDI approximates ramps with tempo
samples normally every eighth of a quarter-note; very long files use a coarser
bounded grid. The four-beat 120→60 BPM regression differs from the analytic map
by under one millisecond. Limits are 20,000 expressions, 10,000 nonoverlapping
staff/layer hairpins, 4,096 tempo changes and 200,000 performed timing segments.
Curved/swing timing, fermatas, arbitrary controller lanes and full Finale Human
Playback interpretation remain open.

### Performance lines and shared scheduling

**Notation → Performance lines** edits octave, damper/sostenuto pedal, trill and
chromatic glissando spans with vector continuations across systems and pages.
Native saves and MusicXML retain their options; standard MusicXML directions and
note ornaments remain usable elsewhere. MuseScore imports the corresponding
basic spanners with supported relative locations or duration ticks.

Octave lines are **display instructions over concert pitches**: positive 12
prints 8va notes an octave lower while sound stays unchanged. This follows the
public [MusicXML octave-shift semantics](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/octave-shift/).
The editor's note entry and written linked parts apply that display relationship.
Choose a separate transposition to change the sound. One, two and three octave
lines in either direction are supported.

Damper holds notes released during its span across all eight notation voices;
sostenuto captures notes sounding at its start. MIDI exports CC64/66, and both
audio engines share the same held-note plan (including an explicit sostenuto
hold for the sampler). Release state resets at repeat/navigation jumps.
Overlapping pedal types and reattacking a captured sostenuto pitch in the same
layer before release are currently rejected: the sampler has no native CC66
voice capture. Separate rhythmic voices can sustain independent equal pitches. Partial
pedaling, soft pedal and arbitrary controller curves remain pending. The public
[MusicXML pedal format](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/pedal/)
provides the portable notation and sound attributes.

Trills alternate principal/upper pitches at an explicit 1–12 semitone interval
and 30–480 tick step; imported ordinary trills currently default to two semitones
and 60 ticks. Automatic key/accidental upper-neighbor inference, turns and
mordents remain pending. Chromatic glissandi require consecutive single-pitch
entries in one layer and perform distinct intervening pitches; continuous
portamento/slide and diatonic/harp-white-key modes remain pending. This distinction
matches [MusicXML glissando](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/glissando/).

The shared performed timeline decorates source-aware repeat traversal with
borrowed-time grace notes, per-pitch ties, ornaments and pedal holds. All three
backends use that plan. Sampled/procedural timing preserves fractional ticks;
MIDI rounds to its 480 PPQ grid. Optional chord accompaniment uses the staff's
instrument at velocity 80 in independent stack-slot lanes and ends at the next
chord or current bar end. N.C. stops that slot. Accompaniment lanes currently
keep independent pedal/expression state from the eight notation voices. Source
notes are unchanged.
Bounds are 10,000 lines, two million expanded pitch notes/grace events and one
million controller events; audio remains limited to ten performed minutes.

This is a bounded deterministic interpreter. Finale's broader
[Human Playback](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Human_Playback.htm)
styles, dictionary, instrument-specific interpretation and user keyswitch rules
remain open.

### Repeats and navigation

Native Encore, MusicXML and MuseScore support ordinary repeat barlines and
numbered alternate endings. Repeat regions may nest without crossing; endings
form a contiguous suffix and assign every pass to exactly one ending. Total
passes are 2–16. Barline dots and volta brackets appear in scores and linked
parts, with bracket continuations across systems/pages.

Measure-boundary **D.C., D.S., al Fine, al Coda, Segno, To Coda and Coda** marks
use the same performed timeline for sampled audio, procedural audio, MIDI,
duration and playhead tracking. Each D.C./D.S. jump executes once; Fine and
To Coda activate on their corresponding return route. Return routes normally
skip repeats and use final endings. The editor can enable repeat replay after
jumps; jumps within one document share that setting. Segno, Coda, Fine and
To Coda each have one unambiguous target. MusicXML uses its public
[`sound`](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/sound/)
and [`repeat`](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/repeat/)
attributes; MuseScore uses explicit marker/jump labels.

Validation rejects crossed/ambiguous repeats, missing or unreachable targets,
custom pass/offset navigation, and more than 16,384 performed measures or
2,000,000 performed note/rest events. Audio retains its ten-minute performed
length limit. Ties and phrase overlap stop at jumps or skipped endings. MIDI
unfolds the performance and includes UTF-8 lyric meta events at performed
syllable onsets. Multiple named targets, arbitrary conditional jump programs,
within-measure navigation and direct Finale MUSX repeat import remain pending.

The direct Finale reader targets the **Finale 27 era** at the product's sunset.
It handles up to four layers per staff, including documented primary controller/secondary v2 branches, with supported ordinary rhythms, chords, rests,
complete chord ties, standard constant keys/meters, staff order, initial tempo
and available instrument identifiers. It recreates notation in Encore's layout.
It rejects orphan/nested/overlapping secondary branches, tuplets, grace notes, partial chord ties, staff styles and custom/nonlinear keys. Its import notes also identify
losses of expressions, articulations, original mixer settings and plug-in setup.
Direct MUSX does not yet decode lyric underlay or slur/Smart Shape objects;
export MusicXML from Finale to migrate those supported Encore features.
MusicXML is the broader migration path for such Finale scores, subject to
Encore's own notation limits. This is not a promise that every Finale 27 score
can be converted without editing.

Finale officially recommends MusicXML for migration:
[MakeMusic's export instructions](https://makemusic.zendesk.com/hc/en-us/articles/25868952458519-Exporting-MusicXML-Files-from-Finale).
The direct reader uses independent public
[EnigmaXML documentation](https://github.com/Project-Attacca/enigmaxml-documentation/blob/main/docs/enigmaxml.md)
and the public MUSX container description. No Finale executable, proprietary
implementation, fonts or sample libraries are bundled.

MIDI import accepts format 0/1 files with PPQ divisions, running status, track/channel/program assignments, initial tempo/key/meter and available volume/pan information. It infers ordinary and exact conventional tuplet notation, separates overlapping independent rhythms when needed, and retains later tempo steps and warns about ignored controllers. SMPTE timing, unpitched drum maps and performances that cannot be represented exactly are rejected. File → Open offers **Exact timing** or an explicit rhythmic quantization grid. Exact mode rejects unnotatable performance timing. Choosing a grid changes note positions/durations and reports the affected-note count and maximum timing shift in the import notes. The original MIDI file is never changed. Detached playback gates can prevent exact reimport of MIDI exported from notation; MusicXML preserves the original written rhythm and articulation marks. This does not add real-time MIDI keyboard input.

```sh
./run-encore --import-midi performance.mid converted.encore --quantize 16th
```

Available grids run from quarter through 128th, including eighth through 128th triplets. Ordinary `--import` remains exact.

## Sound and notation libraries

Encore currently loads **SoundFont 2 (`.sf2`) banks** containing all 128 melodic
bank-0 General MIDI presets. Every catalog instrument uses samples when the
sampled engine is selected. The included bank and licensing remain unchanged.

Finale 27's library workflow is broader than loading a sample file:

| Library or interface | Encore 0.6 |
| --- | --- |
| Complete GM SF2 bank | Supported. |
| SFZ definitions | Not supported yet. |
| Garritan / ARIA libraries | Not loaded directly; these need their licensed ARIA player and a compatible host. |
| VST/VST2, VST3, Audio Units plug-ins | No plug-in host is implemented yet. |
| Finale `.lib` notation/settings libraries | Not supported yet. |

Finale hosted [VST instruments on Windows and Audio Units on macOS](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/VSTAudio_Units.htm).
ARIA supports [SFZ instrument definitions](https://usermanuals.garritan.com/ARIAPlayer/Content/sfz_libraries.htm),
but adding SFZ parsing alone would not reproduce Garritan's proprietary sample
decoding and player behavior. Future plug-in hosting would use a user's own
licensed, installed player and libraries; it would not redistribute Finale's
Garritan assets. Finale's separate
[notation libraries](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/ht-libraries.htm)
contain expressions, articulations, chord suffixes and document settings.

## Linked parts and transposition

**Notation → Manage linked parts…** defines named parts over one or several source
staves. Piano treble/bass pairs generate as one part. Notes, lyrics, slurs and
repeats remain shared with the full score; viewing a part does not modify the
document. Definitions and independent paper size/orientation, measures per
system and staff spacing are stored in the `.encore` file and participate in
undo/redo. Current-part PDF/SVG and batch part PDFs use these settings.

The source score stays in concert pitch. Written parts use a spelled chromatic
and diatonic interval, with standard defaults for B-flat/E-flat/F instruments
and octave-transposing instruments. Note entry in a written part converts back
to the source pitch. Unrepresentable MIDI pitches, accidentals beyond double
sharps/flats and key signatures beyond seven sharps/flats produce visible
errors rather than clamping or changing the source. Playback uses source
concert pitches. Voiced layer/chord-note extraction is implemented with the filters and read-only source workflow in LINKED_PARTS.md. Automatic
multimeasure rests, per-part clefs and per-item unlink/relink are not yet
implemented. See [linked parts](LINKED_PARTS.md).

## Page production

**Document → Page and system layout** edits the current full-score or linked-part
format. Formats retain Letter/A4/Legal/Tabloid paper, landscape orientation,
four physical margins, staff size (50–150%), extra staff gap and the preferred
number of measures per system. Scaling preserves physical page size and keeps
mouse entry aligned with the displayed staff. PDF and SVG share the resulting
pages, including independent part formats.

At the current measure, create a system/page break, adjust its relative width,
or lock/unlock visible systems. Locks preserve their measure membership;
automatic systems still adapt to dense music. Selection-mode lock commands
apply to the selected visible systems. Width controls are relative weights
inside the available system width, not absolute engraving points. Explicit
page breaks begin a fresh page before that measure. In very dense locked
systems, the user may still need fewer measures or a smaller staff size.

Left/center/right headers and footers accept `{title}`, `{composer}`, `{page}`
and `{pages}` tokens. The first page retains the title/composer block; headers
begin on later pages. Copyright prints on the first page. Starting page numbers
are saved; `{pages}` is the final displayed page number. These settings remain
native Encore metadata: foreign page layout is still re-engraved. Custom paper,
mirrored margins, rich text/title pages, arbitrary beat-chart positions and full
collision editing remain future production work.

## Whole-measure editing

Selection mode supports measure/staff rectangles, including nonadjacent source
staves shown together in a linked part. **Edit → Measure range tools** exposes
clear, transpose, blank-measure insertion and full-stack deletion. Standard
copy/cut/paste works with either a selected event or a measure region. Region
paste can replace destination measures, require empty measures, or insert a
full-score measure stack first. Matching time signatures and representable
pitches are required; destination instrument assignments and publication
formats are preserved.

Regional clipboard data includes lyrics, phrase/hairpin/performance-line spans, entry engraving
styles, chord symbols/fretboards, attached grace groups and expressions that fit inside the selected bounds. A copied lyric
keeps its syllable when its word extension crosses the boundary; crossing
slurs/hairpins are omitted with a warning. Repeats, navigation and tempo travel
only with all source staves selected, and jumps whose targets fall outside the
copy are explicitly omitted. D.C. clips must be pasted at the beginning of the
document so their implicit target cannot silently change. Insertion/deletion shifts source anchors and saved
layouts consistently. Dependency failures leave the complete score unchanged. Region transposition
supports chromatic, spelled fixed intervals and the current key's diatonic scale
while preserving chromatic inflections.
Single-event clipboard versions 1–3 remain readable; version 4 additionally carries both before/after grace groups with their concert pitches and rebound principal anchors. Version 3 adds per-pitch engraving/tie overrides. Partial-measure selections, selective clipboard filters,
repeated paste and Finale clip files remain unimplemented.

## Appearance

Monochromatic dark mode is the default. The header's **Light mode / Dark mode** button switches the entire application
chrome, dialogs and library. The preference is remembered between launches;
score paper and printed exports remain white in both modes. Theme changes do
not alter score contents or undo history.

## Chord symbols and fretboard diagrams

**Notation → Chord symbols and fretboards** edits letter-name harmony at a staff beat independently of note/rest entry. Standard major/minor/augmented/diminished, suspended/power, sixth/seventh/ninth/11th/13th forms, added/omitted/altered degrees, slash bass and N.C. have explicit semantics. Unknown suffixes fail visibly. Eight simultaneous stack slots, alternate bass placement, display offsets, printing and harmony playback are saved natively. The source stores concert roots/basses; linked parts display and accept written names without changing source pitches. Regional transposition also transposes harmonic roots, basses and diagram stops.

Original guitar-shape suggestions use standard six-string tuning. Custom diagrams support 4–12 strings, muted/open/stopped positions, frets 1–36, up to eight displayed frets, fingerings and barres. Their tuning is explicit or unknown; imported MusicXML frame geometry does not establish a tuning and is never guessed. Diagram transposition shifts playable stops, including open strings, while keeping labels consistent; an unrepresentable custom shape rejects the operation atomically.

MusicXML exports/imports standard `harmony`, root/kind/bass/degree, stack positions, rhythmic offsets and `frame` geometry. Cursor-relative offsets and transposing instruments are converted into source positions/pitches. MuseScore imports readable standard Harmony suffixes and tonal pitch classes, reporting unsupported functional harmony and private suffix-library references. Standard MusicXML does not preserve Encore's per-symbol harmony-play switch, bass-layout preference or diagram tuning; native Encore files preserve all fields.

Finale `.lib` chord/fretboard libraries, arbitrary suffix glyph typography, learned/MIDI-derived chord analysis, Roman/Nashville names, fretboard library groups and tablature conversion remain unimplemented.


Eight-voice note identity includes staff, bar, tick, layer and voice. Legacy native
notes omit primary-voice defaults and retain their original sound. Secondary
branches optionally store a primary launch onset; imported independent MusicXML
voices can remain unlaunched rather than inventing a connection. Native files
and Encore MusicXML metadata retain launch points, while ordinary MusicXML
consumers can preserve independent rhythms without interpreting those points.
MIDI and sample playback route the eight notation voices independently to avoid
equal-pitch note-off collisions; optional chord stacks use eight additional lanes.
Dynamics, techniques, hairpins and octave-display lines remain layer-scoped.
Event clipboard v5 and region clipboard v2 carry secondary voice identity, accept earlier versions and
warn when pasting an isolated secondary entry detaches its launch. Complete
measure-region paste retains validated branch relationships.


## Turns, mordents and chord rolls

Seven voice-local kinds cover turn/inverted turn, lower/upper mordent, up/down
arpeggiation and non-arpeggiate. Turns/mordents require one principal pitch;
rolls/brackets require 2–16 chord pitches. Each entry accepts one mark, with a
1–960 tick window, 0–75% delay, print offsets and an independent playback switch.
Neighbors follow the key or explicit ±1–4 semitone overrides. Final principal
pitches sustain through the remaining gate; rolled tied members remain held.
Grace notes, repeat traversal, CC11 and pedal use the same performed plan.
Generated repeated keys under sostenuto produce an explicit unsupported-capture
error. Played trills/glissandi overlapping these principal ornaments are also
rejected rather than combined unpredictably.

Native projects preserve all controls. MusicXML writes standard ornaments,
accidental marks, arpeggiate/non-arpeggiate plus bounded native options; basic
MuseScore articulations/arpeggios import. Lower mordent uses MusicXML `mordent`
and a vertical stroke; upper mordent uses `inverted-mordent` without that stroke.
Cross-staff rolls, measured tremolos, fermata time, long/accelerating ornaments
and complete Human Playback remain open. Reference: [MusicXML ornaments](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/ornaments/).

Event clipboard v6 additionally retains ornament controls; region v3 also
preserves advanced slur styles/controls. Earlier supported versions remain
readable. Voiced-part copying is routed through standalone extraction or source
editing to avoid copying hidden voices unintentionally.
