# Omarchy desktop-provider compatibility

Tested with **Omarchy 4.0.4-1** and **t1bridge-omarchy 0.2.2-1**.

The packaged Touch Bar desktop provider requested keyboard-brightness status through `omarchy-brightness-keyboard status`. On this installed Omarchy version, `status` was not a read-only operation: the command took the brightness-down path. Displaying the status could therefore dim the keyboard.

The local adapter handles only the provider request `v1 show-keyboard-backlight`. It requires exactly one `*kbd_backlight` device under `/sys/class/leds`, reads and validates its current and maximum brightness, calculates a percentage, and invokes the Omarchy keyboard OSD. It never writes the brightness files. Unsupported or ambiguous device layouts fail rather than choosing arbitrarily. All other provider operations delegate to the packaged provider.

The adapter also suppresses a renderer-fallback notification when no optional renderer was selected at all. An explicitly configured renderer that is missing, broken, or exits unexpectedly still produces a warning.

The adapter was selected through a user service override; the packaged provider was left intact. A before/after check showed that displaying keyboard brightness preserved its value. Media/volume backend tests used an isolated silent player, then restored the prior sound state.

These are narrow compatibility corrections, independent of the optional animated renderer. Before carrying them forward, check whether the current Omarchy and T1Bridge integration releases have already fixed the same behavior.

## Optional installation

This adapter is optional and is not installed by the Workstream renderer installer. It requires a working packaged `t1bridge-omarchy` provider and `t1-touchbar.service`. The following commands use the usual home-directory configuration paths, matching the supplied systemd drop-in. Run them from this repository's root in a Bash terminal as your normal user; no `sudo` is needed.

First inspect your existing user service configuration with `systemctl --user cat t1-touchbar.service`. A later drop-in that sets `T1BRIDGE_DESKTOP_PROVIDER` can override the supplied `50-workstream-provider.conf`; resolve conflicting provider selections deliberately rather than deleting unrelated drop-ins.

This recipe preserves an existing wrapper and same-named drop-in, including symlinks. Save the backup directory printed before installation:

```bash
(
  set -euo pipefail
  test -x /usr/lib/t1bridge-omarchy/desktop-provider
  test -f integrations/t1bridge-omarchy-provider
  test -f configs/50-workstream-provider.conf
  provider_path="$HOME/.local/libexec/t1bridge-omarchy-provider"
  dropin_path="$HOME/.config/systemd/user/t1-touchbar.service.d/50-workstream-provider.conf"
  for target in "$provider_path" "$dropin_path"; do
    if [[ -e $target && ! -f $target && ! -L $target ]]; then
      printf 'Refusing non-file destination: %s\n' "$target" >&2
      exit 1
    fi
  done
  backup_parent="${XDG_STATE_HOME:-$HOME/.local/state}/omarchy-macbook-workstream/provider-backups"
  install -d -m 700 "$backup_parent"
  provider_backup=$(mktemp -d "$backup_parent/install.XXXXXXXX")
  printf 'Provider backup: %s\n' "$provider_backup"
  if [[ -e $provider_path || -L $provider_path ]]; then
    cp -a -- "$provider_path" "$provider_backup/provider.before"
  else
    touch "$provider_backup/provider.absent"
  fi
  if [[ -e $dropin_path || -L $dropin_path ]]; then
    cp -a -- "$dropin_path" "$provider_backup/dropin.before"
  else
    touch "$provider_backup/dropin.absent"
  fi
  install -d -- "$(dirname "$provider_path")" "$(dirname "$dropin_path")"
  install -m 755 integrations/t1bridge-omarchy-provider "$provider_backup/provider.new"
  install -m 644 configs/50-workstream-provider.conf "$provider_backup/dropin.new"
  mv -T -- "$provider_backup/provider.new" "$provider_path"
  mv -T -- "$provider_backup/dropin.new" "$dropin_path"
  sha256sum "$provider_path" "$dropin_path" > "$provider_backup/installed.sha256"
  systemctl --user daemon-reload
  systemctl --user restart t1-touchbar.service
  systemctl --user is-active t1-touchbar.service
)
```

The drop-in selects `T1BRIDGE_DESKTOP_PROVIDER=%h/.local/libexec/t1bridge-omarchy-provider`; `%h` is expanded by systemd. Check the effective selection with `systemctl --user show t1-touchbar.service -p Environment`. Changing the provider briefly restarts the Touch Bar user service. It does not install drivers, change authentication, or replace system files.

## Rollback

Run this in Bash and enter the backup directory printed by installation. It checks that both installed files still match before restoring previous files, or removing files that did not previously exist. If either file was edited since installation, stop and reconcile those changes manually.

```bash
(
  set -euo pipefail
  read -r -p 'Provider backup directory: ' provider_backup
  test -d "$provider_backup"
  provider_path="$HOME/.local/libexec/t1bridge-omarchy-provider"
  dropin_path="$HOME/.config/systemd/user/t1-touchbar.service.d/50-workstream-provider.conf"
  test ! -L "$provider_path"
  test ! -L "$dropin_path"
  sha256sum --check "$provider_backup/installed.sha256"
  for name in provider dropin; do
    test -e "$provider_backup/$name.before" ||
      test -L "$provider_backup/$name.before" ||
      test -f "$provider_backup/$name.absent"
  done
  rm -- "$provider_path" "$dropin_path"
  if [[ ! -f $provider_backup/provider.absent ]]; then
    cp -a -- "$provider_backup/provider.before" "$provider_path"
  fi
  if [[ ! -f $provider_backup/dropin.absent ]]; then
    cp -a -- "$provider_backup/dropin.before" "$dropin_path"
  fi
  systemctl --user daemon-reload
  systemctl --user restart t1-touchbar.service
  systemctl --user is-active t1-touchbar.service
)
```

Keep the backup until the restored provider works. Other provider drop-ins and the selected renderer are left alone.

Hardware access, default controls, the provider protocol, and the renderer interface come from [T1Bridge](https://github.com/standardagents/t1bridge) and its optional Omarchy integration. See the upstream [desktop integration guidance](https://github.com/standardagents/t1bridge/blob/main/docs/setup.md#desktop-controls-and-customization). These notes claim only the local compatibility changes and tests described above.
