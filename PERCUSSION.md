# Unpitched percussion

Encore stores percussion sound identities separately from their printed positions
and playback MIDI keys. Pitched timpani, mallets and other melodic percussion
continue to use ordinary pitched staves.

## Entry and maps

Click **+ Drum staff** in Instruments to add a five-line GM drum staff. The named
sound palette replaces its piano keyboard. Click a sound to select and audition
it, Shift-click to enter it at the caret, or press Enter using the selected
sound. Duration, rest, chord, layer, Voice 2, tuplet, articulation, tie and undo
controls use the ordinary entry workflow. Up/Down selects sounds in printed
position order, including multiple sounds at one position. Shift+Up/Down,
A–G, intervals, pitch transposition, respelling and accidentals do not reinterpret
drum identities as pitches. Mouse entry chooses the nearest mapped staff
position, preferring the selected sound when several sounds share that position.
Clicking a printed chord head selects that individual sound.

**Percussion map** opens an editable draft. Save applies the entire map only if
the resulting score is valid; Cancel leaves the document unchanged. A populated
pitched staff cannot be implicitly converted to drum IDs. An empty staff may
receive a map, or a new drum staff can be added alongside existing music.

Each sound has:

- A stable ID, 0–127, retained by entries, grace groups and per-sound ties.
- A unique name, printed position and one of six built-in vector noteheads.
- An optional zero-based playback MIDI key, independent of its ID and position.

Position 0 is the bottom line of a five-line staff or the sole line of a one-line
staff; positive steps move upward by half a space. Positions range from −16 to
24. Multiple sounds can share a position or playback key. Changing a map does
not rewrite saved notes. Removing a sound that is still used is rejected.
An unmapped sound remains valid notation; sampled playback and MIDI export
report the missing mapping when that sound is used.

An entry's Normal notehead inherits the sound map. A non-Normal entry override
applies to its heads. An explicit Normal override over a non-Normal map head,
duration-specific map heads and custom font glyphs remain pending. Grace groups
use named sound selection and the same map, making flams possible without
turning sound IDs into melodic neighbors.

## Sampled playback

The bundled GeneralUser GS bank contains 13 drum kits: Standard 1–3, Room,
Power, Electronic, 808/909, Dance, Jazz, Brush, Orchestral, SFX and CM-64/32L.
The kit picker lists the kits actually present in the active SoundFont. A numeric
program can select another kit when available. Non-GM kits, especially SFX, can
place their sounds on different keys; edit the playback map to match the bank.
A mapped key does not guarantee a sample exists in an arbitrary third-party kit.
Missing kits produce an explicit
error; Encore does not silently substitute another kit or a piano.

Percussion uses SoundFont bank 128. Each staff merges its eight notation voices
into one kit engine/channel, allowing a closed hi-hat to choke an open hi-hat
played by another voice. Simultaneous strikes of the same output key are
coalesced. Retriggered hits do not receive stale note-offs from older strikes.
Sound-ID ties and grace timing are resolved before playback-key mapping.
Dynamics and hairpins change strike velocities. Sustained per-layer CC11
expression is not applied to a shared drum channel. Procedural synthesis does
not invent drum sounds: choose Sampled library for active unpitched staves.

Pitch ornaments, octave shifts, trill/glissando/pedal lines and playable chord
realization on drum staves are rejected. Written chord symbols can remain as
silent annotations. Pitched instruments elsewhere in the score retain their
normal playback. One-line triangle and five-line kit notation alongside pitched
timpani are demonstrated in `examples/Percussion Maps Study.encore`.

## Interchange and parts

MusicXML uses percussion clefs, staff-line counts, unpitched display positions,
individual instrument IDs and one-based `midi-unpitched` values. Encore-native
metadata retains the exact map in its own round trips. Foreign files without
usable playback mappings retain unmapped sounds for explicit repair.
MuseScore percussion definitions and MIDI channel 10 are imported where their
supported data can be represented; import notes describe incomplete information.
Legacy Finale percussion libraries and device-specific maps are not imported.

SMF MIDI reserves two ports per source staff. Percussion merges into channel 10
of the first port; pitched notation and harmony voices use the other channels.
The 64-staff limit stays within MIDI port values 0–127. MusicXML numeric device
ports have a smaller range, so export omits a numeric port beyond its supported
range rather than writing an invalid value.

Linked score/part views and extraction preserve sound IDs, maps and notation.
Pitched transposition skips drum tokens. Percussion part voicing supports lane
selection; highest/lowest/ranked melodic chord filters are rejected. Event and
region clipboard payloads retain percussion context and reject pitched/unpitched
or incompatible used-sound mappings before changing the destination.

## Remaining Finale gaps

This is a percussion foundation, not complete Finale percussion parity. Open
work includes duration-specific/custom heads, arbitrary line and ledger profiles,
regional layout changes, reusable map libraries, separate external input maps,
MIDI keyboard entry, mid-score kit/device changes, licensed Garritan/ARIA hosts,
Finale `.lib`/device-map import and full instrument-specific performance rules.

The public reference separates notation layouts from playback maps and describes
Up/Down selection of sounds sharing a position:
[Finale percussion](https://usermanuals.finalemusic.com/FinaleWin/Content/Finale/Percussion.htm).
Interchange follows the
[MusicXML percussion tutorial](https://www.w3.org/2021/06/musicxml40/tutorial/percussion/)
and [MIDI unpitched definition](https://www.w3.org/2021/06/musicxml40/musicxml-reference/elements/midi-unpitched/).
