# Linked parts

Encore stores a linked part as a named definition over staves in the full score.
Notes are stored once, in concert pitch. Opening a part creates a view of those
notes; unfiltered parts support source-linked editing. Viewing a part does not
modify the document.

Use **Notation → Manage linked parts…** to generate parts from the instrument list,
name parts, and combine staves. A treble/bass piano pair is generated as a single
part. Custom parts can contain several staves, and the same staff can appear in
more than one part. Deleting a definition does not delete score notation.

Each definition stores its own paper size (Letter, A4, Legal, or Tabloid),
orientation, margins, staff scaling, measures per system, breaks, locks, relative
measure widths, page text, and extra staff spacing. Dense notation may use fewer
measures per system to preserve readable spacing. A part's format does not
change the full score or other part definitions.

Written parts use a spelled interval from concert pitch. For example, a B-flat
clarinet uses **+2 semitones, +1 diatonic step**, and an F horn uses **+7 semitones,
+4 diatonic steps**. Instrument defaults also include octave-transposing piccolo,
double bass, guitar, celesta and glockenspiel, and standard saxophone intervals.
An explicit **Concert pitch** setting is available. Note names, accidentals, and
key signatures transpose together. An interval that would require a pitch
outside MIDI 0–127, an accidental beyond double-sharp/double-flat, or a key beyond
seven sharps/flats is reported as unavailable. Playback uses the source score's
sounding pitches.

## Voiced parts

Expand **Staff voicing · layers, voices and chord positions** in Manage linked
parts. Enable custom voicing for an included staff and select any of its eight
layer/voice combinations. The layer buttons select both voices in that layer.
Choose all chord notes, the highest or lowest note, or selected positions counted
from the highest or lowest pitch. Positions 1–16 refer to sounding pitch order,
independently of the order notes were entered into the chord.

**Include single-note passages** keeps monophonic passages in a part defined by
chord positions, including positions that do not exist in a one-note entry.
Turning it off converts those entries to rests. A requested position absent from
a larger chord also produces a rest of the original duration. Explicit rests in
selected lanes remain; entries in unselected lanes are omitted.

The optional **Multiple-layer fallback** follows Finale's conditional voicing
workflow: bars containing more than one source layer display whole chords from
the chosen fallback layer's selected voices. Bars containing one layer use the
chord selector. Explicit notes and rests count when deciding whether a bar uses
multiple layers. Primary and secondary voices in the same layer do not by
themselves trigger this fallback.

A staff can have a different rule in each part. For example, create Flute 1 and
Flute 2 over the same source staff, select highest notes for the first and lowest
notes for the second, and include single-note passages in both for unison music.
The rules regenerate from current source notation whenever the score changes.
Encore does not infer player assignments from text such as “a2.”

Voiced parts are read-only musical views. **Edit source score** returns to the
source entry so changes also update every linked part. This prevents an edit to
a visible chord tone or region from changing hidden voices. Part formatting,
saving, navigation, playback, and export remain available. An all-notes rule
without an active filter keeps the ordinary linked editing behavior.

Lyrics, slurs, grace groups, and ornaments follow retained pitched anchors.
Accidental overrides and ties retain their selected pitches. Missing slur
endpoints remove the slur; a word extension shortens to its last retained note.
Grace chords use the same chord selector. Ties lacking a retained continuation
and ornaments incompatible with the resulting chord are removed. The inspector
and part manager report these adjustments. Layer directions follow selected
layers; staff chord symbols and pedals, global tempo, repeats, and navigation
remain. Secondary voice launches hidden by the rule become independent branches
with preserved beam breaks in the derived part.

## Printing and independent extraction

Current-part PDF/SVG export uses the part's page format and written pitches.
**Export all PDFs** creates a numbered PDF for every definition; numbering keeps
identically named players from overwriting one another.

To extract an independent document from the command line:

```text
encore --extract-part score.encore flute-1 flute-1.encore
encore --extract-part score.encore 2 flute-2.pdf
```

The part selector is its stable identity or its one-based position, with identity
matching taking precedence. PDF/SVG extraction uses written pitches and the
part's page format. Native Encore, MusicXML, MIDI, and WAV extraction uses concert
pitches, so the independent project's playback remains correct. A native extract
opens as an ordinary editable score and can generate its own written part views.
The GUI also provides independent native extraction. An extract retains selected
notation and its layout; subsequent edits are independent from the original.

Part definitions and voicing rules are saved inside the `.encore` document and
participate in undo/redo. MusicXML carries the extracted notation; it does not
carry Encore's linked definitions. Imported Finale and MuseScore linked-part
voicing definitions are not reconstructed. Per-region voicing overrides,
automatic multimeasure rests, per-part clef changes, and per-item unlink/relink
of engraving positions or visibility remain open.

The behavior and terminology were checked against Finale's public manuals:
[linked parts](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Linked_Parts.htm),
[Manage Parts](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/IDD_MANAGE_PARTS.htm),
[staff voicing](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/IDD_PART_VOICING.htm),
and [part voicing and source editing](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/Part_voicing.htm).
