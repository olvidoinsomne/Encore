# Platforms and score compatibility

Encore 0.6 targets Linux, macOS and Windows. Linux builds and packaging are
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
| MusicXML `.musicxml`, `.xml`, compressed `.mxl` | Partwise MusicXML. Staves retain up to four independent voices as layers on the original staff. |
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
uses the initial tempo; later tempo changes are reported. Key changes use the
initial signature while preserving explicit note pitches. Page layout, lyrics,
slurs, repeat playback, pedal/hairpin lines and advanced articulations are not
fully retained. Breve through 128th values and exact representable tuplets are supported. Nested tuplets, fractional-grid ratios, percussion maps and changing time signatures still need simplification in the source program first.
When no explicit tempo is stored, import uses 120 quarter notes per minute and
reports that choice in the import notes.

The direct Finale reader targets the **Finale 27 era** at the product's sunset.
It handles up to four layers per staff with supported ordinary rhythms, chords, rests,
complete chord ties, standard constant keys/meters, staff order, initial tempo
and available instrument identifiers. It recreates notation in Encore's layout.
It currently rejects multiple voices within one layer, tuplets, grace notes, partial chord ties, staff styles and custom/nonlinear keys. Its import notes also identify
losses of expressions, articulations, original mixer settings and plug-in setup.
MusicXML is the broader migration path for such Finale scores, subject to
Encore's own notation limits. This is not a promise that every Finale 27 score
can be converted without editing.

Finale officially recommends MusicXML for migration:
[MakeMusic's export instructions](https://makemusic.zendesk.com/hc/en-us/articles/25868952458519-Exporting-MusicXML-Files-from-Finale).
The direct reader uses independent public
[EnigmaXML documentation](https://github.com/Project-Attacca/enigmaxml-documentation/blob/main/docs/enigmaxml.md)
and the public MUSX container description. No Finale executable, proprietary
implementation, fonts or sample libraries are bundled.

MIDI import accepts format 0/1 files with PPQ divisions, running status, track/channel/program assignments, initial tempo/key/meter and available volume/pan information. It infers ordinary and exact conventional tuplet notation, separates overlapping independent rhythms when needed, and warns about ignored controllers or later tempo changes. SMPTE timing, unpitched drum maps and performances that cannot be represented exactly are rejected. File → Open offers **Exact timing** or an explicit rhythmic quantization grid. Exact mode rejects unnotatable performance timing. Choosing a grid changes note positions/durations and reports the affected-note count and maximum timing shift in the import notes. The original MIDI file is never changed. Detached playback gates can prevent exact reimport of MIDI exported from notation; MusicXML preserves the original written rhythm and articulation marks. This does not add real-time MIDI keyboard input.

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

## Appearance

Monochromatic dark mode is the default. The header's **Light mode / Dark mode** button switches the entire application
chrome, dialogs and library. The preference is remembered between launches;
score paper and printed exports remain white in both modes. Theme changes do
not alter score contents or undo history.
