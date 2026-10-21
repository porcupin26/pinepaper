#!/usr/bin/env bash
# Build on Ubuntu 22.04 for the glibc 2.35 baseline. Bundle Poppler and its libraries.
set -euo pipefail
cd "$(dirname "$0")/.."
binary=${1:-target/release/pinepaper}
arch=${2:-$(uname -m)}
if [ "$arch" != "$(uname -m)" ]; then
  printf 'AppImages must be built on the target architecture to bundle matching libraries.\n' >&2
  exit 1
fi
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
appdir="$stage/PinePaper.AppDir"
bundled="$stage/bundled-paths"
touch "$bundled"
mkdir -p "$appdir/usr/lib" dist
DESTDIR="$appdir" PREFIX=/usr bash packaging/install.sh "$binary"
for command in pdfinfo pdftoppm wl-copy wl-paste; do
  file=$(command -v "$command")
  cp "$file" "$appdir/usr/bin/"
  printf '%s\n' "$file" >> "$bundled"
done
# The Rust client implements the Wayland wire protocol itself; XKB is loaded dynamically.
xkb=$(ldconfig -p | awk '/libxkbcommon.so.0 / && !found {print $NF; found=1}')
cp -L "$xkb" "$appdir/usr/lib/"
printf '%s\n' "$xkb" >> "$bundled"
for file in "$appdir"/usr/bin/* "$xkb"; do
  while read -r dep; do
    case "$(basename "$dep")" in libc.so.*|libm.so.*|libpthread.so.*|libdl.so.*|librt.so.*|ld-linux*) continue;; esac
    if [ ! -e "$appdir/usr/lib/$(basename "$dep")" ]; then cp -L "$dep" "$appdir/usr/lib/"; fi
    printf '%s\n' "$dep" >> "$bundled"
  done < <(ldd "$file" | awk '/=> \// {print $3}')
done
cp -a /usr/share/poppler "$appdir/usr/share/"
cp -a /usr/share/X11/xkb "$appdir/usr/share/"
# Keep redistribution notices and an exact source-package inventory with the bundle.
python3 packaging/appimage-notices.py "$appdir" "$bundled"
cp packaging/eu.pinepaper.PinePaper.Rust.desktop "$appdir/"
cp assets/eu.pinepaper.PinePaper.Rust.svg "$appdir/"
ln -s eu.pinepaper.PinePaper.Rust.svg "$appdir/.DirIcon"
cat > "$appdir/AppRun" <<'RUN'
#!/usr/bin/env sh
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export PATH="$HERE/usr/bin:$PATH"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export XKB_CONFIG_ROOT="$HERE/usr/share/xkb"
export POPPLER_DATADIR="$HERE/usr/share/poppler"
exec "$HERE/usr/bin/pinepaper" "$@"
RUN
chmod 755 "$appdir/AppRun"
ARCH="$arch" APPIMAGE_EXTRACT_AND_RUN=1 appimagetool "$appdir" "dist/PinePaper-${version}-${arch}.AppImage"
