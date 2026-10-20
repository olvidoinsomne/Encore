# Exact sampled-engine source archives

These are the unmodified published crates.io source packages used by this Encore
checkout. A `.crate` file is a gzip-compressed tar archive; no installer is needed.
Their SHA-256 hashes match the corresponding registry checksums in the project's
`Cargo.lock`. `provenance.json` records exact download URLs, package versions,
file sizes, hashes, and source-file counts.

| Package | License | Published source package |
| --- | --- | --- |
| OxiSynth 0.1.0 | LGPL-2.1 | <https://static.crates.io/crates/oxisynth/oxisynth-0.1.0.crate> |
| OxiSynth Chorus 0.1.0 | LGPL-2.1 | <https://static.crates.io/crates/oxisynth-chorus/oxisynth-chorus-0.1.0.crate> |
| OxiSynth Reverb 0.1.0 | LGPL-2.1 | <https://static.crates.io/crates/oxisynth-reverb/oxisynth-reverb-0.1.0.crate> |
| SoundFont parser 0.1.0 | MIT | <https://static.crates.io/crates/soundfont/soundfont-0.1.0.crate> |

The LGPL text and the parser's MIT notice are copied verbatim in the parent
`assets/third-party` directory. No changes have been made to these archives.

To verify from this directory on Linux:

```sh
sha256sum -c SHA256SUMS
```

To extract all four packages, run from the Encore project root:

```sh
mkdir -p .third-party-src
for encore_crate in assets/third-party/sources/*.crate; do
    tar -xzf "$encore_crate" -C .third-party-src
done
```

To rebuild Encore using these copies, add the following **new section** to the
project's `Cargo.toml` (or add its entries to an existing section of that name):

```toml
[patch.crates-io]
oxisynth = { path = ".third-party-src/oxisynth-0.1.0" }
oxisynth-chorus = { path = ".third-party-src/oxisynth-chorus-0.1.0" }
oxisynth-reverb = { path = ".third-party-src/oxisynth-reverb-0.1.0" }
soundfont = { path = ".third-party-src/soundfont-0.1.0" }
```

Edit the extracted source as desired, then run `cargo build --release` using the
Rust toolchain and platform dependencies documented in the project's README.
The first build updates `Cargo.lock` to record the local path replacements, so
omit `--locked` on that build. Subsequent builds can use `--locked`. An already
populated Cargo cache permits `--offline`; other dependencies are still normal
Cargo dependencies and are not all vendored in this small source bundle.

The supplied Encore source and build files, these exact library sources, and the
license notices provide the source needed to rebuild/relink Encore with changes
to its sampled-engine libraries. Keep them together when redistributing the
matching binary. This bundle does not impose a restriction on debugging or
reverse engineering modifications to these libraries.
