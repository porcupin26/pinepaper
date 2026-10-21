#!/usr/bin/env bash
set -euo pipefail
binary=${1:-target/release/pinepaper}
prefix=${PREFIX:-/usr/local}
dest=${DESTDIR:-}
install -Dm755 "$binary" "$dest$prefix/bin/pinepaper"
install -Dm644 packaging/eu.pinepaper.PinePaper.Rust.desktop "$dest$prefix/share/applications/eu.pinepaper.PinePaper.Rust.desktop"
install -Dm644 assets/eu.pinepaper.PinePaper.Rust.svg "$dest$prefix/share/icons/hicolor/scalable/apps/eu.pinepaper.PinePaper.Rust.svg"
install -Dm644 packaging/eu.pinepaper.PinePaper.Rust.metainfo.xml "$dest$prefix/share/metainfo/eu.pinepaper.PinePaper.Rust.metainfo.xml"
install -Dm644 LICENSE "$dest$prefix/share/licenses/pinepaper/LICENSE"
install -Dm644 assets/FONT-LICENSE.txt "$dest$prefix/share/licenses/pinepaper/FONT-LICENSE.txt"
install -Dm644 README.md "$dest$prefix/share/doc/pinepaper/README.md"
