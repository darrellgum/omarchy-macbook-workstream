#!/usr/bin/env bash
# Select t1-dash as the T1Bridge renderer. Changes ONLY:
#   - copies the built binary to ~/.local/libexec/t1-dash/<sha>/t1-dash (new dir)
#   - backs up the current ~/.config/t1bridge/renderer link target, then repoints the link
#   - writes ~/.config/touchbar/config.toml only if it does not exist
# It does not restart anything; run `systemctl --user restart t1-touchbar.service` yourself.
set -euo pipefail
cd "$(dirname "$0")"
BIN=target/release/t1-dash
[[ -x $BIN ]] || { echo "build first: ./build.sh" >&2; exit 1; }
CFG_HOME=${XDG_CONFIG_HOME:-$HOME/.config}
LINK=$CFG_HOME/t1bridge/renderer
STATE=${XDG_STATE_HOME:-$HOME/.local/state}/t1-dash
SHA=$(sha256sum "$BIN" | cut -c1-16)
DEST=$HOME/.local/libexec/t1-dash/$SHA
mkdir -p "$DEST" "$STATE" "$CFG_HOME/t1bridge" "$CFG_HOME/touchbar" "$HOME/.local/state/touchbar/bots"
install -m 0755 "$BIN" "$DEST/t1-dash"

if [[ -L $LINK ]]; then
  CUR=$(readlink "$LINK")
  case $CUR in
    */t1-dash/*) echo "already selected ($CUR); keeping existing backup" ;;
    *) printf '%s\n' "$CUR" > "$STATE/previous-renderer"; echo "backed up previous target: $CUR" ;;
  esac
elif [[ -e $LINK ]]; then
  mv "$LINK" "$STATE/previous-renderer.file"
  echo "moved previous renderer file to $STATE/previous-renderer.file"
else
  : > "$STATE/previous-renderer"   # empty = there was no selection (built-in renderer)
fi
ln -sfn "$DEST/t1-dash" "$LINK"
[[ -e $CFG_HOME/touchbar/config.toml ]] || install -m 0644 config/config.toml "$CFG_HOME/touchbar/config.toml"
echo "renderer -> $(readlink "$LINK")"
echo "activate with: systemctl --user restart t1-touchbar.service    (undo: ./rollback.sh)"
