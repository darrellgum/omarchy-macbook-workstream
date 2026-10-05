# Workstream renderer and protocol

Workstream adds a fixed activity ribbon to the unprivileged Touch Bar renderer.
The first design uses Tokyo Night colors, a quiet idle label, a rotating blue
indicator, bright flowing lines and a traveling light during explicitly reported
work, and a brief green sweep after a successful
command. Failed commands show a red line and a `FAILED` label. Touching a key
adds a short blue accent without delaying its normal action.

The row keeps Escape, display brightness, keyboard backlight, media controls,
mute, and volume. The controls have fixed positions; the ribbon is not a touch
target. Holding Fn reveals the original full-width Escape/F1–F12 layout in
Tokyo Night colors. The packaged default renderer retains its original
monochrome palette and layout. Desktop blanking and Touch ID overlays retain
priority.

## Build and select

Use the companion repository's [build/install instructions](../README.md#build-and-try-workstream).
Its scripts fetch and patch the pinned upstream source, build all four tools,
preserve existing selections, and provide a matching uninstaller. Run them
from this repository's root, as your desktop user:

```sh
./scripts/build.sh
./scripts/install.sh
```

The installer selects the Tokyo renderer through the existing
`$XDG_CONFIG_HOME/t1bridge/renderer` entry (normally
`$HOME/.config/t1bridge/renderer`) and restarts the user `t1-touchbar.service`.
The Tokyo binary directly runs the renderer, so selection cannot recurse
through the launcher. `scripts/uninstall.sh` restores the previous selection.
No firmware, kernel module, or privileged-service change is required.

## Real command activity

```sh
t1-workstream run BUILDING -- cargo build --locked
t1-workstream run CHECKING -- cargo test --locked
```

The wrapper runs the argument array directly and inherits normal input/output.
Only the explicit label is displayed; command arguments and output are not put
in the activity file. The command still runs when the display is unavailable,
and its exit status is preserved. A successful exit produces `DONE`; a failed
exit produces `FAILED`. No completion percentage is inferred.

The newest operation owns the single ribbon. Completion of an older operation
cannot clear newer work. A wrapper refreshes its status every second; an orphaned
running status expires after ten seconds. Completion stays for three seconds,
failure/attention for six. These are ephemeral hints, not an activity history.

Explicit integrations can use `begin ID LABEL`, `update ID LABEL`, and
`done|failed|attention ID [LABEL]`. Use a unique ID for every operation. Terminal
states cannot be revived by delayed updates. The optional
[Codex lifecycle adapter](workstream-codex.md) requires a separate trust review
and observed real hook invocation before it can be called connected.
The optional [Claude Code adapter](workstream-claude.md) offers prompt, response,
and API-error signals for Claude Code 2.1.196 or later. Both adapters use fixed
labels and require a real application turn to verify delivery.

## Motion and storage

Animation requests are capped at one per 34 ms (about 29 fps), coalesced while
the hardware owns the frame, and stopped when settled, hidden by Fn, or blanked.
Static key art is cached. A 240 ms press accent stays within the pressed key;
the success sweep lasts 700 ms. Physical delivery rate still depends on the
hardware service and should be measured on each machine.

Set `T1_WORKSTREAM_REDUCED_MOTION=1` in the renderer's user-service environment
to keep state labels while disabling animated effects.

Runtime records are bounded, private, atomic, and expire against Linux uptime.
They live only under `$XDG_RUNTIME_DIR/t1-workstream`. The reader ignores unsafe,
malformed, missing, and expired records. There is no network listener or root
activity process. A short-lived local abstract socket provides writer exclusion
without unsafe code or stale lock files.
