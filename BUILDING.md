# Building Encore

Encore uses Rust's native desktop libraries: eframe/winit for the window,
rodio/CPAL for audio, rfd for file dialogs, and the Rust OxiSynth SoundFont
renderer. The source is intended for Linux, Windows and macOS. **The current
release is verified on Linux; Windows and macOS builds need native validation.**
The packaging scripts and optional CI jobs make that validation repeatable.

Use Rust **1.90 or newer** for the documented CI toolchain (the crate's minimum
is 1.88), including `rustfmt` and `clippy`. Build from the project directory:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo build --release --locked
```

Tests render notation and sampled audio without an audio device or display
server. Running the desktop application needs an available graphics session and
audio output. Keep `assets/` with a portable release: the text/music fonts are
embedded at compile time, but the SoundFont is discovered at runtime.

## Linux

On Debian/Ubuntu install the native headers before building:

```sh
sudo apt-get install build-essential pkg-config libasound2-dev libudev-dev \
  libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev \
  libxrandr-dev libgl1-mesa-dev
cargo build --release --locked
sh scripts/package-linux.sh
```

The archive in `dist/` includes the executable, sampled bank, notices and matching
source. Extract it and run `./run-encore`. It uses the host's graphics/audio
libraries; it is not a fully static binary or an installer. Use a sufficiently
old Linux build environment when distributing to older systems. File dialogs
use the desktop's XDG portal.

## Windows

Install Rust with the **MSVC** toolchain, Visual Studio Build Tools' **Desktop
development with C++** workload, and a Windows SDK. Use a native Windows terminal
with that environment available. No Linux ALSA or Apple SDK is involved.

```powershell
cargo build --release --locked
pwsh -NoProfile -File scripts/package-windows.ps1
```

The script uses PowerShell 7 and creates a portable ZIP in `dist/`. Extract the
whole folder and run `Encore.exe`, keeping `assets` beside it. A consumer
installer, file associations and Authenticode signing are separate release
steps. The ZIP does not bundle a Rust toolchain or a C++ build environment.

## macOS

Install Xcode Command Line Tools (`xcode-select --install`) and Rust on the Mac.
The normal build targets the machine's architecture: Apple Silicon or Intel.

```sh
cargo build --release --locked
sh scripts/package-macos.sh
```

The ZIP in `dist/` contains **Encore.app**, with the sampled bank and notices
under `Contents/Resources`. This is an **unsigned, unnotarized** development app;
public distribution should add an application icon, Developer ID signing and
notarization. File associations and Finder's document-open event handling are
also separate integration work; use Encore's File → Open dialog for now. The
bundle declares macOS 11 or later. That declaration does not replace testing on
the supported OS versions.

To build both architectures on a Mac with the appropriate SDK/toolchain:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin
lipo -create target/aarch64-apple-darwin/release/encore \
  target/x86_64-apple-darwin/release/encore -output /tmp/encore-universal
sh scripts/package-macos.sh /tmp/encore-universal
```

The output filename uses the build host architecture; rename a universal ZIP
accordingly. Cross-compiling from Linux is not part of the verified workflow:
installing a Rust target alone does not supply Apple's SDK or a Windows linker.

## GitLab CI and releases

`.gitlab-ci.yml` verifies and packages Linux using a Docker-compatible GitLab
runner and Rust 1.90. Its manual Windows/macOS jobs require native **shell
runners**, with Rust and the native prerequisites installed. Register runners
with `encore-native` plus `windows` or `macos` tags before starting those jobs.
Their presence in CI does not mean those platforms have already been tested.

Package scripts accept an already-built binary path and an output directory.
They copy the matching Rust source, Cargo lockfile, bank, fonts, notices and the
exact OxiSynth source/rebuild materials. Keep those materials with distributed
binaries; see `assets/soundfonts/NOTICE.md`. Caches, Git credentials, repository
metadata, and large demo WAVs are excluded. These scripts do not sign, publish
or upload releases.
