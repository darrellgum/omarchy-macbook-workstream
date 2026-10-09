#!/usr/bin/env bash
# Restore the renderer selection saved by install.sh. Only touches ~/.config/t1bridge/renderer.
set -euo pipefail
CFG_HOME=${XDG_CONFIG_HOME:-$HOME/.config}
LINK=$CFG_HOME/t1bridge/renderer
STATE=${XDG_STATE_HOME:-$HOME/.local/state}/t1-dash
if [[ -e $STATE/previous-renderer.file ]]; then
  rm -f "$LINK"; mv "$STATE/previous-renderer.file" "$LINK"
elif [[ -f $STATE/previous-renderer ]]; then
  PREV=$(cat "$STATE/previous-renderer")
  if [[ -z $PREV ]]; then rm -f "$LINK"; echo "removed selection (built-in renderer)";
  else
    [[ -x $PREV ]] || echo "warning: previous target $PREV is not executable" >&2
    ln -sfn "$PREV" "$LINK"
  fi
else
  echo "no backup in $STATE; nothing to restore" >&2; exit 1
fi
echo "renderer -> $(readlink "$LINK" 2>/dev/null || echo '(built-in)')"
echo "activate with: systemctl --user restart t1-touchbar.service"
