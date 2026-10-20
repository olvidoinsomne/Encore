#!/bin/sh
# Package an already-built native Linux binary. This is not an installer.
set -eu
encore_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$encore_root/scripts/package-common.sh"
encore_binary=${1:-"$encore_root/target/release/encore"}
encore_output=${2:-"$encore_root/dist"}
if [ ! -f "$encore_binary" ]; then
    echo "Build first with cargo build --release --locked, or pass the binary path." >&2
    exit 1
fi
mkdir -p "$encore_output"
encore_output=$(CDPATH= cd -- "$encore_output" && pwd)
encore_name="Encore-$(encore_version)-linux-$(uname -m)"
encore_stage=$(mktemp -d "$encore_output/.encore-package.XXXXXX")
trap 'rm -rf "$encore_stage"' EXIT HUP INT TERM
mkdir -p "$encore_stage/$encore_name"
encore_copy_source "$encore_stage/$encore_name"
cp -R "$encore_root/assets" "$encore_stage/$encore_name/"
cp "$encore_binary" "$encore_stage/$encore_name/encore"
chmod +x "$encore_stage/$encore_name/encore"
cat > "$encore_stage/$encore_name/run-encore" <<'LAUNCHER'
#!/bin/sh
set -eu
encore_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$encore_directory/encore" "$@"
LAUNCHER
chmod +x "$encore_stage/$encore_name/run-encore"
tar -czf "$encore_output/$encore_name.tar.gz" -C "$encore_stage" "$encore_name"
echo "$encore_output/$encore_name.tar.gz"
