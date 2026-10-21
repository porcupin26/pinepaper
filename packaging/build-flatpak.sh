#!/usr/bin/env bash
# Requires Flatpak/Builder, org.gnome.Sdk 49 and its rust-stable extension.
set -euo pipefail
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/source" dist
# Explicit allowlist: never package local notebooks, keys, screenshots or target/.
cp -a Cargo.toml Cargo.lock src assets packaging LICENSE README.md "$stage/source/"
mkdir -p "$stage/source/.cargo"
cargo vendor --locked "$stage/source/vendor" | sed 's|^directory = .*|directory = "vendor"|' > "$stage/source/.cargo/config.toml"
cp packaging/eu.pinepaper.PinePaper.Rust.json "$stage/manifest.json"
flatpak-builder --force-clean --bundle-sources --state-dir="$stage/state" --user --install-deps-from=flathub --repo="$stage/repo" "$stage/build" "$stage/manifest.json"
flatpak build-bundle "$stage/repo" "dist/PinePaper-${version}-${arch}.flatpak" eu.pinepaper.PinePaper.Rust --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo
flatpak build-bundle --runtime "$stage/repo" "dist/PinePaper-${version}-${arch}.Sources.flatpak" eu.pinepaper.PinePaper.Rust.Sources
