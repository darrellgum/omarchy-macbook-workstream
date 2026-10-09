#!/usr/bin/env bash
# User half of the T2 install (no sudo): installs t1-dash and a user service that connects to t1-dash-t2d.
set -euo pipefail
cd "$(dirname "$0")/.."
BIN=target/release/t1-dash
[[ -x $BIN ]] || { echo "build first: ./build.sh" >&2; exit 1; }
CFG=${XDG_CONFIG_HOME:-$HOME/.config}
SHA=$(sha256sum "$BIN" | cut -c1-16)
DEST=$HOME/.local/libexec/t1-dash/$SHA
install -Dm755 "$BIN" "$DEST/t1-dash"
mkdir -p "$CFG/t1-dash" "$CFG/touchbar" "$CFG/systemd/user" "$HOME/.local/state/touchbar/bots"
ln -sfn "$DEST/t1-dash" "$CFG/t1-dash/renderer"
[[ -e $CFG/touchbar/config.toml ]] || install -m644 config/config.toml "$CFG/touchbar/config.toml"
install -m644 t2/t1-dash.user.service "$CFG/systemd/user/t1-dash.service"
systemctl --user daemon-reload
systemctl --user enable t1-dash.service
systemctl --user restart t1-dash.service
echo "renderer -> $(readlink "$CFG/t1-dash/renderer")"
