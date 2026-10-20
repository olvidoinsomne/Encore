#!/bin/sh
# Shared packaging helpers; sourced by the native Unix packaging scripts.
set -eu

encore_version() {
    sed -n 's/^version = "\([^"]*\)"$/\1/p' "$encore_root/Cargo.toml" | head -n 1
}

encore_copy_source() {
    encore_destination=$1
    mkdir -p "$encore_destination"
    # Keep the exact source and the LGPL rebuild materials with the executable.
    # Exclude Git metadata, dependency caches, local exports and rendered WAVs.
    for encore_item in src scripts Cargo.toml Cargo.lock LICENSE README.md ARCHITECTURE.md BUILDING.md COMPATIBILITY.md FINALE_PARITY.md SHORTCUTS.md LINKED_PARTS.md .gitlab-ci.yml; do
        cp -R "$encore_root/$encore_item" "$encore_destination/"
    done
    if [ -d "$encore_root/tests" ]; then
        cp -R "$encore_root/tests" "$encore_destination/"
    fi
    mkdir -p "$encore_destination/examples"
    for encore_example in "$encore_root"/examples/*.encore "$encore_root"/examples/*.pdf "$encore_root"/examples/*.png; do
        if [ -f "$encore_example" ]; then
            cp "$encore_example" "$encore_destination/examples/"
        fi
    done
    if [ -d "$encore_root/examples/Cantabile Parts" ]; then
        cp -R "$encore_root/examples/Cantabile Parts" "$encore_destination/examples/"
    fi
}
