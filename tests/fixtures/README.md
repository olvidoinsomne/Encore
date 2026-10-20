# Finale 27 interoperability fixture

`finale27-basic.musx` is the small `tests/data/slur.musx` test document from
[openmusx/musxdom](https://github.com/openmusx/musxdom), an independent open-source
project by Robert G. Patterson. The upstream repository is MIT licensed with no
separate fixture license or exception. Its original license is retained in
`musxdom-LICENSE.txt`.

- Pinned upstream commit: `ecaa6297826ec393e0c5c987a47996e05515733d`
- [Original file at that commit](https://github.com/openmusx/musxdom/blob/ecaa6297826ec393e0c5c987a47996e05515733d/tests/data/slur.musx)
- SHA-256: `c65472e4691337387ad910cf7d86047ef22f3145cb83cd69a87a6ad652e34f99`
- Size: 13,247 bytes; copied without modification.
- Embedded creator: `Finale 27.4.1.146 for Macintosh`.
- Two staves, one measure each, 4/4, C major, quarter note = 96.
- Staff 1: D5 quarter, E5 quarter, C5 quarter, quarter rest.
- Staff 2: quarter rest, C5 half, quarter rest.

The archive contains only `mimetype`, `META-INF/container.xml`,
`NotationMetadata.xml`, and `score.dat`; it includes no plug-ins, fonts, sample
libraries, or Finale program code. Encore's test verifies real MUSX container
decoding and the above note pitches/timing. Slur shapes deliberately exercise the
import warning for unsupported engraving details.

The other MUSX tests construct their own minimal EnigmaXML and archive it in
memory. They cover linked nonsequential entry IDs, accidental/register mapping,
rests, ties, metadata, unsupported rhythms, malformed links, and bounded input.
