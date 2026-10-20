#!/bin/sh
# Package an already-built native macOS binary into an unsigned .app and ZIP.
set -eu
encore_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$encore_root/scripts/package-common.sh"
if [ "$(uname -s)" != Darwin ]; then
    echo "Run this script on macOS with a native macOS binary." >&2
    exit 1
fi
encore_binary=${1:-"$encore_root/target/release/encore"}
encore_output=${2:-"$encore_root/dist"}
if [ ! -f "$encore_binary" ]; then
    echo "Build first with cargo build --release --locked, or pass the binary path." >&2
    exit 1
fi
mkdir -p "$encore_output"
encore_output=$(CDPATH= cd -- "$encore_output" && pwd)
encore_version_string=$(encore_version)
encore_name="Encore-$encore_version_string-macos-$(uname -m)"
encore_stage=$(mktemp -d "$encore_output/.encore-package.XXXXXX")
trap 'rm -rf "$encore_stage"' EXIT HUP INT TERM
encore_bundle="$encore_stage/Encore.app"
mkdir -p "$encore_bundle/Contents/MacOS" "$encore_bundle/Contents/Resources"
cp "$encore_binary" "$encore_bundle/Contents/MacOS/encore"
chmod +x "$encore_bundle/Contents/MacOS/encore"
cp -R "$encore_root/assets" "$encore_bundle/Contents/Resources/"
encore_copy_source "$encore_bundle/Contents/Resources/source"
# The source can be rebuilt in place with the same assets used by the app.
ln -s ../assets "$encore_bundle/Contents/Resources/source/assets"
cat > "$encore_bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>encore</string>
  <key>CFBundleIdentifier</key><string>org.encore-score.Encore</string>
  <key>CFBundleName</key><string>Encore</string>
  <key>CFBundleDisplayName</key><string>Encore</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$encore_version_string</string>
  <key>CFBundleVersion</key><string>$encore_version_string</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
plutil -lint "$encore_bundle/Contents/Info.plist"
ditto -c -k --sequesterRsrc --keepParent "$encore_bundle" "$encore_output/$encore_name.zip"
echo "$encore_output/$encore_name.zip"
