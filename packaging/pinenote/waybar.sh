#!/bin/sh
set -eu

# Retain the PineNote distribution's environment for its existing modules.
envfile=$(/usr/share/pinenote-config/sway/scripts/env_util.sh get_env)
. "$envfile"
exec waybar -c /etc/pinepaper/waybar/config \
    -s /etc/pinepaper/waybar/style.css "$@"
