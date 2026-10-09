#!/usr/bin/env bash
set -uo pipefail
CFG=${XDG_CONFIG_HOME:-$HOME/.config}
systemctl --user disable --now t1-dash.service
rm -f "$CFG/systemd/user/t1-dash.service" "$CFG/t1-dash/renderer"
systemctl --user daemon-reload
echo "t1-dash user service removed (config kept in $CFG/touchbar)"
