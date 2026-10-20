# Staff groups

Encore supports editable visual staff groups with brackets, braces, straight lines,
square brackets, group names and abbreviations, and barlines drawn separately,
through the group, or only between staves (Mensurstriche). Group definitions may
overlap or nest. Grouping preserves the existing instruments, voices, and playback.

Open **Layout → Staff groups and barlines**. Automatic defaults retain the legacy presentation. Custom groups materializes editable definitions; Add group selects a source staff range. Edit names, symbols, barlines and offsets, then Save staff groups to validate and apply the complete draft. Cancel leaves the document unchanged. Removing the final custom definition deliberately saves an ungrouped layout. Move earlier/later changes the equal-span precedence order.

The full score and each linked part save their own groups in their page layout.
Changing a part's groups does not change the score or other parts. Saved staff
references always address the original source score. Part projection and standalone
extraction reindex them to the retained staff order. When that order separates a
group's members with an unrelated staff, Encore splits the bracket into separate
fragments and reports the change instead of enclosing the unrelated staff.

Older documents retain automatic brackets and piano braces until explicit groups
are saved. An explicitly empty group list means no groups; it does not restore
automatic brackets. Opening the editor or viewing a linked part does not change
the source document.

For conflicting barline definitions, the smallest containing staff span takes
precedence. The later definition wins when spans are equal. This rule applies
separately to staff bodies and connections between adjacent staves, including
explicit separate-barline overrides. Group definitions are ordered in the saved
layout. Positive horizontal adjustment moves a bracket left; positive top/bottom
adjustments move that endpoint downward. Measurements use 1/100 inch page units.

MusicXML exports visual groups as standard numbered `part-group` start/stop pairs,
with names, symbols, and `group-barline` values. A native metadata supplement
preserves stable IDs, hidden group names, singleton-bracket settings, independent
endpoint offsets, and an explicitly empty list when reopened in Encore. Instrument
parts remain separate. MusicXML import also retains an initial `part-symbol`
inside a multi-staff source instrument as a visual group after flattening its
staves. It does not preserve semantic instrument grouping for later re-export.
Foreign absolute bracket placement and later changes of part symbols are
re-engraved with a warning. Relative group placement follows the source scaling.

MuseScore's ordinary bracket, brace, square, and line staff-definition spans are
imported. Its bracket columns are re-engraved, and custom barline spans are reported
as unsupported. Unsupported bracket shapes are omitted with a warning.

Current bounds are 256 groups per layout, unique IDs of 1–64 ASCII letters,
digits, underscores, hyphens, or periods, printable single-line names of at most
1024 UTF-8 bytes, and finite bracket adjustments within ±200 page units. A group
covers an inclusive contiguous range of source staves. If projection creates more
than 256 fragments, the excess decorations are omitted with an explicit warning;
the source groups and music remain intact.

Measure-specific group changes, group-aware empty-staff hiding, multimeasure rests,
arbitrary custom bracket shapes, per-group barline styles, semantic multi-staff
MusicXML export, and direct Finale group import remain pending parity work.

References: [Finale Group Attributes](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/GROUPDLG.htm),
[Finale linked-part groups](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Linked_Parts.htm),
[MusicXML part groups](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/part-group/),
[MusicXML part symbols](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/part-symbol/).
