# Carol of the Bells — sampled instruments

Encore 0.4 renders the same **Winter Ascendant** score with two audio engines:

- [Sampled orchestra](Carol%20of%20the%20Bells%20-%20Sampled.wav) — 83 seconds, including a three-second release tail.
- [Original procedural orchestra](Carol%20of%20the%20Bells%20-%20Procedural.wav) — 81.8 seconds, including its original release tail.
- Matching 16-second openings: [sampled](Carol%20of%20the%20Bells%20-%20Opening%20-%20Sampled.wav) and [procedural](Carol%20of%20the%20Bells%20-%20Opening%20-%20Procedural.wav).

The underlying music is unchanged: 36 bars at quarter note = 108, lasting 80 seconds. Four solo low-timpani statements precede the bowed double bass entrance at approximately 8.89 seconds. Its E1 whole note is tied into the following measure as the strings enter. The later orchestral build, quiet center and final climax retain their original notes, velocities, mixer settings and timing.

Every part in the sampled version now uses GeneralUser GS sample playback, including woodwinds and harp. Encore 0.4 exposes all 128 melodic General MIDI presets in its searchable instrument library. These include acoustic instruments, electronic sounds and sound effects; GS variation banks and drum kits are not exposed. The bundled bank has measured instrument-level trims to keep the original orchestral palette balanced. Both engines use Encore's shared room processing and soft limiter.

Both files are 44.1 kHz, 16-bit stereo WAV. The sampled peak is 0.91113; the procedural peak is 0.94360. Neither contains clipped PCM samples. The procedural file is byte-for-byte identical to the previous render. `Carol of the Bells - Epic.wav` and `Carol of the Bells - Opening.wav` now contain the sampled versions; the editable project and printed score are unchanged.

To compare within Encore, open **Instrument library → Playback sounds** and switch between **Sampled library** and **Procedural**. This selection also controls keyboard auditions and WAV exports.

Reproduce both renders from the project root:

```sh
./run-encore --compare-carol examples
```

See [the sound-bank attribution](../assets/soundfonts/NOTICE.md) for the exact source, checksum, license and sampled-engine source bundle.
