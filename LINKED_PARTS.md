# Linked parts

Encore stores a linked part as a named definition over staves in the full score.
Notes are stored once, in concert pitch. Opening a part creates a view of those
same notes; changing notation in a part updates the score and every other part
that contains the staff. Viewing a part does not modify the document.

Use **Notation → Manage linked parts…** to generate parts from the instrument list, name
parts, and combine staves. A treble/bass piano pair is generated as a single
part. Custom parts can contain several staves, and the same staff can appear in
more than one part. Deleting a definition does not delete score notation.

Each definition stores its own paper size (Letter, A4, Legal, or Tabloid),
orientation, measures per system, and extra staff spacing. Dense notation may use
fewer measures per system to preserve readable spacing. Changes to a part's
format do not change the full score or other part definitions.

Written parts use a spelled interval from concert pitch. For example, a B-flat
clarinet uses **+2 semitones, +1 diatonic step**, and an F horn uses **+7 semitones,
+4 diatonic steps**. Instrument defaults also include octave-transposing piccolo,
double bass, guitar, celesta and glockenspiel, and standard saxophone intervals.
An explicit **Concert pitch** setting is available. Note names, accidentals, and
key signatures transpose together. An interval that would require a pitch
outside MIDI 0–127, an accidental beyond double-sharp/double-flat, or a key beyond
seven sharps/flats is reported as unavailable rather than silently changing the
spelling. Playback continues to use the source score's sounding pitches.

Lyrics, word extensions, slurs, and repeats remain linked to source rhythmic
positions. PDF export of a part uses that part's page format and written pitches.
**Export all PDFs** creates a separate numbered PDF for every definition;
numbering prevents identically named players from overwriting one another.

Part definitions are saved inside the `.encore` document and participate in undo
and redo. Exported PDFs are independent documents. Advanced Finale features
still awaiting implementation include voiced parts extracted from selected
layers/chord notes, automatic multimeasure rests, per-part clef changes, and
unlinking individual engraving positions or visibility.

The behavior and terminology were checked against Finale's public manuals:
[linked parts](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Linked_Parts.htm)
and [Manage Parts](https://usermanuals.finalemusic.com/FinaleMac/Content/Finale/IDD_MANAGE_PARTS.htm).
