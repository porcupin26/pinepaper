#!/usr/bin/env python3
"""Opt-in desktop integration for hrdl's PineNote Arch Sway/Waybar layout."""
import json
import os
from pathlib import Path
import shutil


def main():
    if os.geteuid() != 0:
        raise SystemExit("Run with sudo after installing PinePaper.")
    source = Path(__file__).resolve().parent
    upstream = Path("/usr/share/pinenote-config/waybar/config")
    config = upstream.read_text()
    # Preserve the upstream JSONC, both bars, includes, and existing controls.
    anchor = '"modules-right": ['
    window_controls = [
        "custom/mvws_prev", "custom/gows_prev", "custom/gows_next",
        "custom/mvws_next", "custom/splitv", "custom/splith",
    ]
    if (anchor not in config or '"tray",' not in config
            or any(json.dumps(name) + "," not in config for name in window_controls)):
        raise SystemExit("Unsupported PineNote Waybar layout; no files changed.")
    if not shutil.which("pinepaper"):
        raise SystemExit("Install PinePaper before setting up the desktop.")
    modules = {
        "custom/pinepaper": {
            "format": " ",
            "tooltip-format": "Open PinePaper",
            # Sway owns the app process, so restarting Waybar cannot stop it.
            "on-click": "swaymsg exec pinepaper",
        },
        "custom/window-controls": {
            "format": "⋯",
            "tooltip-format": "Window controls",
        },
        "group/window-controls": {
            "orientation": "inherit",
            "drawer": {"click-to-reveal": True, "transition-duration": 0},
            "modules": ["custom/window-controls", *window_controls],
        },
    }
    for index, name in enumerate(window_controls):
        config = config.replace(
            json.dumps(name) + ",",
            '"group/window-controls",' if index == 0 else "",
            1,
        )
    config = config.replace(
        anchor,
        json.dumps(modules, indent=4, ensure_ascii=False)[1:-1] + ",\n        " + anchor,
        1,
    ).replace('"tray",', '"custom/pinepaper",\n            "tray",', 1)
    destination = Path("/etc/pinepaper/waybar")
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "config").write_text(config)
    shutil.copyfile(source / "waybar.css", destination / "style.css")
    shutil.copyfile(
        source.parent.parent / "assets/eu.pinepaper.PinePaper.Rust.svg",
        destination / "pinepaper.svg",
    )
    wrapper = Path("/usr/local/libexec/pinepaper-waybar")
    wrapper.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source / "waybar.sh", wrapper)
    wrapper.chmod(0o755)
    dropin = Path("/etc/sway/config.d/96-pinepaper-desktop.conf")
    dropin.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source / "sway.conf", dropin)
    print("Installed PinePaper startup and status-bar launcher.")
    print("As the desktop user: swaymsg reload && swaymsg exec pinepaper")


if __name__ == "__main__":
    main()
