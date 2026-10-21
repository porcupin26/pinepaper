# PinePaper

[![Build and packages](https://github.com/porcupin26/pinepaper/actions/workflows/build.yml/badge.svg)](https://github.com/porcupin26/pinepaper/actions/workflows/build.yml)

A native Rust notebook and reader for the PineNote and Linux Wayland desktops.
It builds on the direct Wayland pen experiment, with a monochrome interface,
pressure-sensitive ink, local storage and optional live sharing through your own relay.
No account is required.

## Features

- Library with a slim creation toolbar, compact folder rows, large page previews,
  grid/list views, global filename search, type filters, sorting, rename, move
  and recoverable trash. Sort and view choices persist across restarts.
- Quick sheets opens a fresh page in the same scratch notebook each time,
  preserving earlier pages, even if the notebook was renamed or moved.
- **Notebook** in the library toolbar opens a drawing page immediately with the pen selected.
  Tap the document title to rename it. Pen options and other context menus open
  in compact popovers over the page; tap outside to dismiss without leaving ink.
- Six pen nibs (ballpoint, fineliner, marker, pencil, mechanical, calligraphy),
  three weights and grayscale colours; highlighter, whole-stroke eraser, lasso
  selection/movement/deletion, and editable typed notes.
- Undo/redo, automatic saving after pen-up, page insertion/deletion, six paper
  templates, bookmarks, tags and page overview.
- PDF, text EPUB and UTF-8 text import. Originals are copied into the library.
  Reader controls can be hidden; page-edge taps, swipes and navigation buttons
  move between pages. Position and annotations persist.
- Annotated PDF export and transactionally consistent ZIP library backups.
- Built-in fullscreen keyboard with QWERTY/QWERTZ/AZERTY, Shift/Caps Lock,
  numbers, punctuation, accented letters, selection, clipboard and cursor keys.
  Tapped keys briefly reverse their colors for 500 ms, with updates limited to
  the affected keys. Physical keyboards use the compositor's XKB keymap.
- Responsive portrait/landscape layout, fullscreen button and F11, fit-page or
  fit-width view, finger scrolling, optional finger drawing and pen proximity
  suppression of palm gestures.
- Rounded controls with centered labels and a monochrome navigation sidebar.
- Monochrome vector toolbar and menu icons, embedded in the app and crisp at
  every display scale; no system icon theme is required.
- Battery percentage and charging indicator, Wi-Fi connection status and
  signal details. Tap the indicators for details; they update every 15 seconds
  and remain visible with the writing toolbar hidden. NetworkManager is read
  through D-Bus; no wireless scans or connection changes are made.
- PineNote front-light brightness/warmth, contrast, midtone darkness, interface
  size, pen pointer, grayscale/threshold/dithering hints, cleanup, driver speed,
  dither pattern, cleanup delay in **hardware frames**, periodic full refresh,
  immediate full refresh, and keyboard preferences. Every setting takes effect
  immediately and saves automatically, including presets and panel resets.
  On/off controls use switches; numeric controls use sliders that support
  tapping the track or dragging with a finger or pen. Light and refresh presets
  stay highlighted only while their related settings match the preset.
  Fullscreen switches the current window and remembers the preference.
  Rapid hardware adjustments keep the latest value; writes are checked and
  rolled back on failure. **Done** closes settings after pending writes finish.

## Interface reference and roadmap

The [feature and UI inventory](docs/remarkable-reference.md) links the official
reMarkable guides and illustrations, compares their workflows with PinePaper,
and separates implemented features from planned work. The first phase (0.5.0)
implements the library layout requested in the reference photograph.

![PinePaper library](docs/images/library-landscape.png)

Use **Menu** for file-type filters, import, sharing, settings, backups and trash.
Tap **Folder** to create a folder, **Notebook** to open a new drawing page, or
**Quick sheets** for a fresh page in your scratch notebook. The icon beside the
sort dropdown switches between grid and list. Scroll with a finger or mouse
wheel; open an item's three-dot menu to rename, move or trash it. Wi-Fi and
battery status remain visible at the bottom. Quick sheets creates a new scratch
notebook if its previous one is in the trash; it never restores deleted files
implicitly. No library migration is needed for this update.

## Install and start

PinePaper requires **Linux with a Wayland desktop**. On a PineNote running Arch,
use the **aarch64 Arch/pacman package** for access to the tablet's display controls.
On an Intel/AMD Linux computer, choose x86_64 instead.

Open [Actions → Test and package Linux](https://github.com/porcupin26/pinepaper/actions/workflows/build.yml),
select a successful run, and download `linux-aarch64-packages` or
`linux-x86_64-packages`. Extract the artifact ZIP into its own directory and run
`sha256sum -c SHA256SUMS`. Workflow artifacts require a GitHub login and remain
available for 30 days. Install one package matching your distribution:

```sh
# PineNote Arch / Arch Linux ARM, or desktop Arch
sudo pacman -U ./pinepaper-*.pkg.tar.zst
# Debian / Ubuntu
sudo apt install ./pinepaper_*.deb
# Fedora / RHEL-family
sudo dnf install ./pinepaper-*.rpm
# openSUSE
sudo zypper install ./pinepaper-*.rpm
```

Start **PinePaper** from the application launcher, or run `pinepaper` in a
terminal inside your Wayland session. Choose **Notebook** in the library toolbar to start
writing. Tap its title to rename it; **Menu → Import** adds an existing file.
Changes save automatically. **Menu → Settings** includes display controls, keyboard
preferences and the exchange server URL. The fullscreen button or F11 toggles
fullscreen; the preference is remembered.

Alternative formats:

- **AppImage:** from the same artifact, run `chmod +x PinePaper-*.AppImage`,
  then `./PinePaper-0.5.1-x86_64.AppImage` (substitute `aarch64` on ARM).
  If FUSE is unavailable, prefix the launch with `APPIMAGE_EXTRACT_AND_RUN=1`.
- **Flatpak:** download the matching `flatpak-x86_64` or `flatpak-aarch64`
  artifact, verify its checksums, then run the commands below. The separate
  `.Sources.flatpak` is source material; install the ordinary application bundle.
- **Tar archive:** it contains a `usr/` installation tree, not an installer.
  Extract to a private directory, install the runtime dependencies listed below,
  then run `./usr/bin/pinepaper` inside it. Native packages also register the
  launcher icon and file associations.

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user ./PinePaper-0.5.1-x86_64.flatpak
flatpak run eu.pinepaper.PinePaper.Rust
```

Flatpak uses a separate library under
`~/.var/app/eu.pinepaper.PinePaper.Rust/data/pinepaper/`. Copying an existing
library into it is explicit. The sandbox can restrict tablet hardware controls;
the native Arch package provides the fullest PineNote integration.

## Build and run

Requires Linux, a Wayland compositor, Rust 1.96 and a C compiler. No GTK or
Python runtime is used. SQLite and UI fonts are bundled into the executable.
Install `libxkbcommon`, Poppler utilities (`pdfinfo`, `pdftoppm`), and
`wl-clipboard`. NetworkManager supplies Wi-Fi status. Sway and the existing
`org.pinenote.PineNoteCtl` service supply PineNote refresh integration.

```sh
git clone https://github.com/porcupin26/pinepaper.git
cd pinepaper
cargo build --release --locked
./target/release/pinepaper
./target/release/pinepaper --data-dir /path/to/separate-library --windowed
./target/release/pinepaper /path/to/book.pdf
```

Install system dependencies before building:

```sh
# Debian / Ubuntu
sudo apt install build-essential pkg-config libxkbcommon0 poppler-utils wl-clipboard
# Arch / Arch Linux ARM
sudo pacman -S --needed base-devel rust libxkbcommon poppler wl-clipboard
# Fedora
sudo dnf install gcc gcc-c++ rust cargo libxkbcommon poppler-utils wl-clipboard
```

Use rustup to install Rust 1.96 if the distribution's toolchain is older.
NetworkManager is optional for Wi-Fi status; PineNote hardware services are
provided by the tablet's operating system. Neither is required for desktop drawing.

For cross-compiling from an x86 Linux host:

```sh
rustup target add aarch64-unknown-linux-gnu
# Install gcc-aarch64-linux-gnu, for example with apt.
cargo build --release --locked --target aarch64-unknown-linux-gnu
```

On a native ARM64 machine, use
`CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=cc cargo build --release --locked`.
The `.cargo/config.toml` cross-linker setting otherwise expects the cross GCC.

To install from the project root:

```sh
sudo PREFIX=/usr/local bash packaging/install.sh target/release/pinepaper
```

The binary is `pinepaper`; the desktop application ID is
`eu.pinepaper.PinePaper.Rust`. Launching it again focuses the existing window
and uses the startup fullscreen preference. Opening another file forwards the
import request to the running process. Run as the desktop user, not root.

### Optional PineNote desktop integration

The following helpers are in the source checkout's `packaging/pinenote/`
directory; clone this repository to use them after installing a binary package.
On the hrdl PineNote Arch desktop, optional Sway/Waybar integration starts
PinePaper at login and adds its notebook icon immediately beside the Wi-Fi
tray icon. Tapping it opens PinePaper or brings the existing window forward:

```sh
sudo python3 packaging/pinenote/install-desktop.py
# Run these as the desktop user, inside Sway:
swaymsg reload
swaymsg exec pinepaper
```

This setup helper requires Python only during installation. It keeps the
distribution's bar controls and styles, with overrides under `/etc/pinepaper`
and `/etc/sway/config.d/96-pinepaper-desktop.conf`. Window-management buttons
are grouped under **⋯** so the launcher stays visible in portrait. Closing
PinePaper is still allowed; the icon reopens it. To undo, remove that Sway
drop-in and reload Sway.
After a distribution update changes the Waybar layout, rerun the helper to
refresh the copied configuration.

To make a short power-button press refresh the e-ink panel instead of putting
the tablet to sleep, install the optional power-button configuration:

```sh
sudo install -Dm644 packaging/pinenote/power-refresh.conf /etc/sway/config.d/97-pinepaper-power-refresh.conf
sudo install -Dm644 packaging/pinenote/logind-power-refresh.conf /etc/systemd/logind.conf.d/90-pinenote-power-button.conf
sudo systemctl reload systemd-logind
swaymsg reload
```

The refresh shortcut also works in fullscreen and on the lock screen. Automatic
idle sleep remains configured by the desktop.

## Controls

Write with the pen in contact with the page. Hover does not draw. Tap the
active **Pen** tool to select nib, width or colour. **Lasso** surrounds ink or
text; drag within the selection to move it and tap **Del** to remove it.
**Text** places a note or edits an existing one. The fullscreen keyboard is
inside the app and opens for names, text, tags, search and file paths.

**Pages** opens thumbnails, bookmarks and tags. The page menu contains
templates, tags, fit-width/fit-page, export, and display settings. New pages
inherit the template. Clear page can be undone; deleting a page asks for
confirmation. Imported PDF/book pages cannot be deleted.

Shortcuts: F11 fullscreen, Ctrl+N notebook, Ctrl+O import, Ctrl+Z undo,
Ctrl+Shift+Z redo, Page Up/Down navigation, Escape tools, Delete selection,
Ctrl+Q quit. Escape dismisses an open dialog.

## Live sharing

Set **Settings → Sharing → Exchange server** to your server's `https://` URL
(or `http://` for a trusted local test). Every participant uses the same server.
Open a document, choose **Page menu → Share document → Share current document**,
and give the five-character code to others. They choose **Menu → Join shared
document** and enter the code. The sharing screen displays the code and participant
count; the page footer shows connection status. Use **Leave sharing** to disconnect.
Opening another document also leaves the current session.

The separate [PinePaper Relay Server repository](https://github.com/porcupin26/pinepaper-relayserver)
contains the `pinepaper-exchange` relay, packages and deployment instructions.
It routes WebSocket messages over HTTP(S), without a database, document
storage or history. A connected participant supplies the initial document;
every device saves its own local copy. PDFs include the original file (up to
64 MiB); books include their paginated text. Leaving a room never deletes local
copies. Rooms disappear when everyone disconnects or the server restarts.
Restarting the app requires sharing the saved document with a new code.

Concurrent strokes and text objects merge by unique IDs. Updates to the same
object or metadata field use deterministic last-writer ordering. Undo reverses
only your own changes and preserves later peer edits. Pages, templates, titles,
bookmarks and tags synchronize too; library folders and reading positions remain
local. Remote changes wait until active pen gestures or text editing finish.
Networking, compression and file transfer run on a background thread. After a
connection interruption, peers exchange complete state to reconcile edits.

A room code gives editing access to anyone who knows it; there are no accounts,
roles or end-to-end encryption. Use HTTPS on a public server. At least one
participant with the document must remain online for newcomers to receive it.
The server URL can be changed for your next session without affecting the current
one. An ended room can be restarted by sharing any participant's saved copy.

To test real client collaboration against a running relay without touching your
library:

```sh
PINEPAPER_EXCHANGE_TEST_URL=http://127.0.0.1:8787 \
  cargo test --locked live_three_clients -- --ignored --nocapture
```

## Data compatibility and recovery

The default library is `$XDG_DATA_HOME/pinepaper/`, usually
`~/.local/share/pinepaper/`. `--data-dir` or `PINEPAPER_DATA_DIR` selects another
directory. The SQLite tables and JSON page format remain compatible with
Python PinePaper 0.2.2, including unknown payload fields. No destructive schema
migration is performed. A self-contained `before-rust-*.zip` is made in
`exports/` before the first Rust session opens an existing library.

Quit the Python app before using the same library in Rust. Each version can
otherwise keep an old in-memory copy of a page. The Rust app locks its own
library against a second Rust process. Saves use WAL journaling and full
synchronous transactions on a worker thread. The UI shows saving failures and
retains failed payloads for retry; normal close/navigation flushes pending
writes and refuses to proceed on failure.

**Backup** produces a ZIP with a SQLite backup snapshot and all imported
originals. Restore into a fresh directory and inspect with `--data-dir` before
replacing your active library. Uninstalling the application does not remove
the library. Original Android/Rnote documents are not converted automatically.

For upgrades, close PinePaper and install the newer package using the same
package manager. To uninstall, use `pacman -R pinepaper`, `apt remove pinepaper`,
`dnf remove pinepaper`, `zypper remove pinepaper`, or
`flatpak uninstall eu.pinepaper.PinePaper.Rust`, as appropriate. Keep a backup
before replacing or moving a library. For source installs, remove the executable,
desktop file, icon, metainfo and documentation under the prefix passed to
`packaging/install.sh` (normally `/usr/local`).

## Troubleshooting

- **No window or Wayland connection failure:** launch as the logged-in desktop
  user inside a Wayland session. Check `echo "$WAYLAND_DISPLAY"`; SSH shells
  normally do not inherit desktop session variables. An X11-only session is unsupported.
- **PDF import/rendering fails:** check `pdfinfo -v` and `pdftoppm -v`. Native
  packages need Poppler installed; AppImage and Flatpak bundle it.
- **Display controls unavailable:** hardware settings need the PineNote driver,
  permissions and `org.pinenote.PineNoteCtl` service supplied by its OS. A normal
  desktop or sandbox may not expose these. Failed writes are reported and reverted.
- **Keyboard or small interface:** use the built-in keyboard and adjust interface
  size in Settings; it works while fullscreen. F11 can return to a normal window.
- **Sharing cannot connect:** all clients must use the same reachable server URL;
  `localhost` refers to the current device. Check the relay's `/healthz`, Wi-Fi,
  firewall and HTTPS certificate. A code only exists while someone remains connected.
- **Saving fails:** keep the app open, check free disk space and permissions for
  the selected data directory, and retry. The app retains failed saves for retry.

## Rendering and responsiveness

The client consumes every Wayland tablet frame, including pressure, without
waiting for a toolkit animation/frame clock. Ink is rasterised into a persistent
pixel buffer; only the affected region is copied into a free shared-memory
buffer. Buffers owned by the compositor are never modified. Pen-up queues a
save without rebuilding the page. History copies and serialization are separate
from motion events. Rendering is antialiased, with per-stroke coverage so
overlapping highlighter segments do not accumulate opacity within one stroke.
Raw and display-toned pixels remain separate.

Hardware status, PDF rendering, thumbnails, imports, exports and backups use
workers. Full page redraws happen for structural edits, navigation and resize.
Fixed document coordinates keep annotations attached through rotation.
Physical pen-to-panel latency still depends on compositor, driver and e-ink
waveforms; application timings cannot establish that latency.

## Packages and GitHub Actions

The workflow runs formatting, Clippy, tests and visual smoke checks, then builds:

| Format | x86_64 | ARM64 / PineNote |
| --- | --- | --- |
| Arch `.pkg.tar.zst` | Yes | **Yes — native package for PineNote Arch** |
| Debian/Ubuntu `.deb` | Yes | Yes |
| Fedora/openSUSE `.rpm` | Yes | Yes |
| `.AppImage` | Yes | Yes |
| Flatpak `.flatpak` | Yes | Yes |
| Portable installation tree `.tar.gz` | Yes | Yes |

Builds run on pushes to `main`, pull requests, `v*` tags and manual workflow dispatch.
Artifacts and SHA-256 manifests are attached to the workflow run; the workflow
does not publish releases or upload to distribution repositories. Common native
packages and AppImages use Ubuntu 22.04 builders (glibc 2.35 baseline). Flatpak
builds use the GNOME 49 runtime and a vendored Cargo.lock dependency set.
AppImage/Flatpak include Poppler utilities; native packages declare them as
dependencies. Packaging tools, Actions and external Flatpak sources are pinned.
Flatpak builds also produce a `.Sources.flatpak` bundle. AppImages include
third-party licenses and the exact Ubuntu source-package versions and links
under `usr/share/licenses/pinepaper/third-party/`.

Use the ARM64 Arch package on PineNote:

```sh
sudo pacman -U pinepaper-0.5.1-1-aarch64.pkg.tar.zst
```

The source `packaging/PKGBUILD` supports x86_64 and aarch64 after a matching
`v0.5.1` tag exists. Before that tag exists, use Cargo/nFPM directly. For local
package builds, install Python 3, binutils, curl, tar and zstd, then run:

```sh
bash packaging/install-build-tools.sh .tools
export PATH="$PWD/.tools:$PATH"
cargo build --release --locked
bash packaging/build-packages.sh target/release/pinepaper x86_64
# Or a cross-built PineNote binary:
bash packaging/build-packages.sh target/aarch64-unknown-linux-gnu/release/pinepaper aarch64
# Inspect native packages (requires dpkg-deb, rpm/rpm2cpio, cpio, desktop-file-utils):
bash packaging/verify-packages.sh target/release/pinepaper x86_64
```

AppImage and Flatpak scripts are in `packaging/` and run on the target CPU
architecture; the workflow uses native ARM64 runners. Flatpak has its own XDG data
directory; copying/importing an existing library is explicit. Its sandbox may
make direct front-light writes and Sway refresh hints unavailable. Use a native
Arch package for full PineNote hardware integration. Unsupported controls do
not silently modify hardware.

## Verification

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked -- --smoke artifacts/smoke
# Optional: real protocol/input checks in an isolated headless Sway session.
# Requires sway, grim and wtype; never touches the user's desktop or library.
cargo build --release --locked --bin pinepaper --example wayland_pointer
python3 tests/wayland_smoke.py
```

CI also runs the real three-client collaboration test against a pinned revision
of the relay, including reconnection and joining while another participant leaves.
Tests cover transactional persistence, folder cycles, page restrictions,
backup restoration, EPUB spine order, readable PDF export, hover/contact,
240 intermediate pen samples, undo/redo, Unicode editing, portrait/landscape
drafts and control bounds, sparse-stroke erasing, and pixel parity for all nibs
with overlapping highlights and contrast. The smoke command uses only a
temporary library and writes screenshots, an exported PDF and backup under
the specified output folder.

The headless compositor test checks real Wayland input, saved ink, physical
keyboard entry, rotation with an open draft, fullscreen and clean shutdown.
`examples/hardware_probe.rs` is a manual PineNote diagnostic. Without arguments
it reads status. Its explicit `--exercise` option temporarily changes the
lights and display settings, verifies readback, then restores their values.

EPUB layout is text-oriented: publisher CSS, embedded illustrations and DRM
are unsupported, as in the Python app. Exported PDF annotations are flattened.
The eraser removes whole strokes. This app targets Linux Wayland; Windows,
macOS and an X11-only session are not supported package targets.

## Repository hygiene

This directory is the standalone repository root. Build artifacts, packages,
test exports, databases, credentials and local configuration are ignored.
Personal tablet backups and installation SSH credentials live outside it.
Only source, locked dependencies, bundled font assets/licenses, tests,
packaging and project documentation belong in commits.

Code is MIT licensed. Bundled DejaVu font licensing is in
`assets/FONT-LICENSE.txt`.
