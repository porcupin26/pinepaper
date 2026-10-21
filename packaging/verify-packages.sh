#!/usr/bin/env bash
# Verify the actual installed files from every native package format.
set -euo pipefail
cd "$(dirname "$0")/.."
binary=${1:-target/release/pinepaper}
arch=${2:-$(uname -m)}
case "$arch" in x86_64) debarch=amd64;; aarch64) debarch=arm64;; *) exit 1;; esac
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/arch" "$stage/rpm" "$stage/tar" "$stage/rpmdb"
deb="dist/pinepaper_${version}-1_${debarch}.deb"
rpm="dist/pinepaper-${version}-1.${arch}.rpm"
dpkg-deb --info "$deb"
rpm --dbpath "$stage/rpmdb" -qpi "$rpm"
dpkg-deb -x "$deb" "$stage/deb"
tar --zstd -xf "dist/pinepaper-${version}-1-${arch}.pkg.tar.zst" -C "$stage/arch"
rpm2cpio "$rpm" | (cd "$stage/rpm" && cpio -idm --quiet --no-absolute-filenames)
tar -xf "dist/pinepaper-${version}-linux-${arch}.tar.gz" -C "$stage/tar"
for format in deb rpm arch tar; do
  root="$stage/$format/usr"
  cmp "$binary" "$root/bin/pinepaper"
  test -x "$root/bin/pinepaper"
  test -s "$root/share/doc/pinepaper/README.md"
  test -s "$root/share/doc/pinepaper/docs/remarkable-reference.md"
  test -s "$root/share/doc/pinepaper/docs/images/library-landscape.png"
  test -s "$root/share/doc/pinepaper/docs/images/library-portrait.png"
  test -s "$root/share/doc/pinepaper/docs/images/library-menu-portrait.png"
  test -s "$root/share/licenses/pinepaper/FONT-LICENSE.txt"
  test -s "$root/share/icons/hicolor/scalable/apps/eu.pinepaper.PinePaper.Rust.svg"
  desktop-file-validate "$root/share/applications/eu.pinepaper.PinePaper.Rust.desktop"
done
printf 'All four package formats contain the expected binary, desktop files and documentation.\n'
