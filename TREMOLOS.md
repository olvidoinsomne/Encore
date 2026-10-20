# Measured tremolos

**Notation → Measured tremolos** adds measured repeated attacks to a selected
note or chord, including mapped unpitched percussion. A single-entry tremolo
repeats that entry's pitches. **Alternate with the next entry** uses two
consecutive equal undotted quarter or half entries in the same staff, measure,
layer and voice. Both displayed values double; their actual combined duration
stays unchanged. Select either endpoint to reopen the same pair.

Choose 1–8 strokes, playback on/off and a two-dimensional placement adjustment.
Save validates the complete score before replacing a mark. Invalid pairings,
missing principals and incompatible attachments leave the previous score intact.
The editor lists existing marks for editing and removal. Voiced linked views
route musical edits through the source score; printed part projections retain
only tremolos whose required principals survive their voice/pitch filter.

For unflagged notes, one, two and three strokes give eighth, sixteenth and
thirty-second pulse divisions. Existing flags subdivide further. Dots lengthen
the repeated window; tuplets scale pulse timing. Playback needs a division of at
least one tick and is bounded to two million expanded pitch events. Audio rounds
to sample frames and MIDI to its tick grid. Finer visual strokes can be retained
with playback disabled. Written notes, measure durations and source anchors stay
unchanged.

The shared performed timeline applies pulse timing before normal articulation,
dynamics, technique, tempo and repeat scheduling. Single-entry marks can coexist
with supported borrowed grace timing. Alternating pairs permit grace groups
before the first or after the second principal, with internal grace windows
rejected. Paired principals cannot send or receive ties. New pulses always reattack; only the final pulse of a single-entry tremolo can
tie into an ordinary continuation.
Played ornaments, trills and glissandi cannot overlap a played tremolo window.
Same-key slurs do not extend an old gate into a newer attack. Drum rolls retain
sound-ID identity until output-key mapping and reuse the kit's choking and
retrigger rules. These are deterministic measured repeats, not Finale's complete
instrument-specific Human Playback interpretation.

MusicXML writes standard single/start/stop tremolo notations. An alternating
pair keeps actual durations while doubling note types and writing the required
2:1 time modification. Encore metadata retains exact source anchors, playback
choice and placement. Supported standard pairs and singles import; MuseScore's
basic single-chord r8/r16/r32/r64 tremolos import. Paired MuseScore and buzz-roll
semantics remain pending and are reported rather than guessed.

Native persistence, linked projections/extraction, structural edits and region
clipboard preserve tremolos. Region clipboard v5 retains complete alternating
pairs. Event clipboard v8 retains a single-entry mark; copying only one endpoint
reports that the alternating mark was omitted. Earlier supported clipboard
versions remain readable. Pasting validates metadata and source context before
changing the destination.

The group/tremolo study demonstrates nested piano/ensemble groups, alternating
quarter and half pairs, dotted and tuplet singles, and sampled drum rolls.
Unmeasured and buzz rolls, alternating complex/dotted/tuplet values, cross-bar
pairs, tied paired principals, instrument-specific interpretation and fermata
time holds remain open parity work. Measure-repeat symbols are a separate staff
presentation feature and are not implemented by tremolo playback.

References: [Finale tremolos](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Tremolos.htm),
[MusicXML tremolo](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/tremolo/).

Very large tremolo-heavy scores may render slowly: some annotation helpers still
rebuild display indexes rather than sharing the main cached context. Broader
performance profiling and complete dense-score collision handling remain open.
