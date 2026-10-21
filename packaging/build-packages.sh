#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PACKAGE_BINARY=${1:-target/release/pinepaper}
arch=${2:-$(uname -m)}
case "$arch" in x86_64) export PACKAGE_ARCH=amd64;; aarch64) export PACKAGE_ARCH=arm64;; *) printf 'Unsupported architecture: %s\n' "$arch" >&2; exit 1;; esac
machine=$(readelf -h "$PACKAGE_BINARY" | sed -n 's/^ *Machine: *//p')
case "$arch:$machine" in
  x86_64:*X86-64*|aarch64:AArch64) ;;
  *) printf 'Binary architecture does not match requested %s: %s\n' "$arch" "$machine" >&2; exit 1;;
esac
export PACKAGE_VERSION
PACKAGE_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
export PACKAGE_LIBC_MIN
PACKAGE_LIBC_MIN=$(readelf --version-info "$PACKAGE_BINARY" | sed -n 's/.*Name: GLIBC_\([0-9.]*\) .*/\1/p' | sort -V | tail -1)
python3 - "$stage/nfpm.yaml" <<'PY'
import datetime, os, pathlib, sys, time
text = pathlib.Path('packaging/nfpm.yaml').read_text()
# Whole-second timestamps avoid tar/mtree rounding differences in pacman -Qkk.
epoch = int(os.environ.get('SOURCE_DATE_EPOCH', time.time()))
mtime = datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
text = text.replace('${PACKAGE_MTIME}', mtime)
for key in ('PACKAGE_BINARY', 'PACKAGE_ARCH', 'PACKAGE_VERSION', 'PACKAGE_LIBC_MIN'):
    text = text.replace('${' + key + '}', os.environ[key])
pathlib.Path(sys.argv[1]).write_text(text)
PY
for format in deb rpm archlinux; do nfpm package --config "$stage/nfpm.yaml" --packager "$format" --target dist/; done
DESTDIR="$stage" PREFIX=/usr bash packaging/install.sh "$PACKAGE_BINARY"
tar -C "$stage" -czf "dist/pinepaper-${PACKAGE_VERSION}-linux-${arch}.tar.gz" usr
cp packaging/PKGBUILD dist/PKGBUILD
printf 'Built DEB, RPM, Arch and tar packages for %s\n' "$arch"
