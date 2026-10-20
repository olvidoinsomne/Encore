# Sampled instruments: attribution and rebuilding

## GeneralUser GS 2.0.3

The unmodified `GeneralUser-GS.sf2` bank is by **S. Christian Collins**.

Encore 0.4 exposes all **128 melodic bank-0 General MIDI programs** from this
bundled bank, including woodwinds, harp, guitars, organs, choirs, electronic
sounds and effects. No additional bank assets were downloaded for this catalog
expansion. The upstream bank includes 261 instrument presets and 13 drum kits;
Encore does not yet expose GS variations or drum-kit selection. All 128 Encore
choices use SoundFont sample playback, though electronic and effect presets
include sound designs rather than recordings of acoustic instruments.

- Author: <https://www.schristiancollins.com/generaluser>
- Upstream: <https://github.com/mrbumpy409/GeneralUser-GS>
- Pinned source commit: `684543d5e5efaef08d02be50dcda8d552478fa60`
- Exact file: <https://raw.githubusercontent.com/mrbumpy409/GeneralUser-GS/684543d5e5efaef08d02be50dcda8d552478fa60/GeneralUser-GS.sf2>
- File size: **32,319,396 bytes**
- SHA-256: `9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe`

See `GeneralUser-GS-source.json`, the retained upstream
`GeneralUser-GS-LICENSE.txt` (license version 2.0), and
`GeneralUser-GS-README.md`. The author's terms permit private and commercial
music creation, software use, and bank modification. The author also explicitly
notes that some historical sample origins cannot be fully traced. This bank
therefore has an upstream provenance statement, rather than an independently
verified recording history for every sample. Keep the upstream license with it.

The official documentation identifies the release as **2.0.3, February 22, 2026**.
Its embedded `INAM` still reads `GeneralUser GS 2.0.3 BETA`, and `ICRD` reads
`Tuesday 15 October 2024, 09:47:28`. These are retained upstream metadata;
Encore has not relabeled or modified the file.

Encore selects bank 0's **program 43, Double Bass** (zero-based), whose sustained
sample zones are distinct from **program 32, Acoustic Bass**. The opening E1
uses `Double Bass-G1`, pitched down three semitones, with the instrument's
sustain loop. Timpani uses program 47's `Timpani Soft` / `Timpani Hard` samples.
All note pitches remain concert pitch.

## OxiSynth and the SoundFont parser

Sample playback uses **OxiSynth 0.1.0** by Bartłomiej Maryńczak (PolyMeilex),
with `oxisynth-chorus 0.1.0` and `oxisynth-reverb 0.1.0`. These libraries are
licensed **LGPL-2.1**; the full license is retained at
`../third-party/OxiSynth-LGPL-2.1.txt`. Upstream source:
<https://github.com/PolyMeilex/OxiSynth>.

The published crate source records these commits:

| Crate | Upstream source commit |
| --- | --- |
| `oxisynth 0.1.0` | `87c08c1a0165c7ea3577d86571f7688622387674` |
| `oxisynth-chorus 0.1.0`, `oxisynth-reverb 0.1.0` | `33ef995279a25a06fb3f969c5e372a534d097a3f` |
| `soundfont 0.1.0` parser | `74c579ff5a6c31547236c14bebeb7c4979f01b4f` |

The parser is **MIT**, copyright 2021 Bartłomiej Maryńczak; its full notice is
retained at `../third-party/SoundFont-parser-MIT.txt`. `Cargo.lock` pins the
published crate checksums. The exact, unmodified source archives for all four
crates are included in `../third-party/sources/`, together with `SHA256SUMS`,
registry download URLs, and local-path rebuild instructions. Encore does not
modify these upstream libraries.

OxiSynth implements the bank's SoundFont modulators, including its velocity,
filter, envelope, and controller programming. This is why Encore uses it for
this bank. SoundFont playback remains sample based and does not provide a
commercial orchestra library's recorded legato transitions or articulation set.

## Rebuilding or replacing the libraries

Encore's Rust source, `Cargo.toml`, and `Cargo.lock` are supplied in this project.
With the documented Rust toolchain and platform build dependencies installed,
run `cargo build --release --locked` to rebuild. Cargo retrieves the exact
published dependency sources, or uses the existing local Cargo cache.

To use modified OxiSynth libraries, extract the supplied `.crate` archives
(gzip-compressed tar files) from `assets/third-party/sources/`, modify those
sources, and use local `[patch.crates-io]` path overrides. The source bundle
README gives complete extraction and override commands. The first local-path
build updates the lockfile; later builds can use `--locked`. No signing key,
account, or proprietary build tool is needed. Encore places no restriction on
reverse engineering for debugging modifications to these libraries.

This workspace supplies the corresponding Encore Rust source, build files,
exact sampled-engine library sources, and license notices needed to rebuild
and relink with modified libraries under LGPL 2.1 section 6. Keep these
materials together with a redistributed matching binary, including any future
source modifications and updated build instructions. The source bundle does
not require downloading these four libraries from upstream.
