# Optional Codex lifecycle signals

`t1-workstream-codex` connects three documented Codex hooks to Workstream. It
reports brief lifecycle signals, not the model's thoughts or a continuous busy
indicator:

| Hook | Touch Bar label | Meaning |
| --- | --- | --- |
| `UserPromptSubmit` | WORKING | A user turn was submitted. |
| `Stop` | RESPONSE READY | Codex is yielding a response; this does not certify successful work. |
| `Interrupt` | INTERRUPTED | The user interrupted the active turn. |

The working signal expires after 10 seconds; response and interruption notices
expire after 6 seconds. A long turn can therefore leave the ribbon quiet until
its final event. Use the command wrapper for continuously refreshed status of
real builds and tests. This first adapter intentionally omits tool-stage and
approval hooks: a temporary approval pause needs different resumption semantics
from a terminal notice.

## Build and install

Use the companion's [top-level build/install scripts](../README.md#build-and-try-workstream).
They install the renderer and both adapters in a versioned directory, preserve
previous selections, and provide rollback. Do not copy individual binaries
over the installer-managed command links.

The adapter and `t1-workstream` must share a directory. The adapter finds its
sender beside its resolved executable, without searching `PATH` or invoking a
shell. The Workstream renderer must already be active to display signals.
See [hook setup](hooks.md) after installation.

## Configure and verify hooks

The [official Codex Hooks guide](https://learn.chatgpt.com/docs/hooks) documents
user-level `hooks.json`, inline configuration, supported events, and trust
review. Merge these entries into the chosen hook source, preserving existing
hooks. This file is a suggested configuration only; building the adapter does
not install or activate hooks.

```json
{
  "description": "Optional Touch Bar lifecycle signals",
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "\"${HOME}/.local/bin/t1-workstream-codex\"",
        "timeout": 1,
        "async": false
      }]
    }],
    "Stop": [{
      "hooks": [{
        "type": "command",
        "command": "\"${HOME}/.local/bin/t1-workstream-codex\"",
        "timeout": 1,
        "async": false
      }]
    }],
    "Interrupt": [{
      "hooks": [{
        "type": "command",
        "command": "\"${HOME}/.local/bin/t1-workstream-codex\"",
        "timeout": 1,
        "async": false
      }]
    }]
  }
}
```

In Codex desktop, inspect and trust the exact definitions in the chat's hook
review banner or **Settings > Hooks > From Config > User config**. The CLI also
provides `/hooks` for review. New or changed non-managed hooks are skipped until
reviewed.
Keep these small hooks synchronous so background delivery cannot reorder them.
Do not bypass hook trust for installation.

Verify a real new turn in the desktop app before describing the integration as
active. An enabled CLI feature or a synthetic payload test does not prove that
the desktop invoked a hook. Check submission, normal response, and interruption
individually. To disable the integration, disable these entries in the desktop
hook settings or CLI `/hooks`, or remove only these entries from the hook source.

## Failure and concurrency behavior

The adapter reads at most 64 KiB plus one overflow byte from standard input.
Malformed, oversized, unsupported, or incomplete events are ignored. It uses
only the event name and nonempty session/turn strings of at most 512 bytes;
missing or invalid identifiers produce no signal. Conversation text,
command arguments, outputs, and transcript paths are never forwarded or logged.
Identifiers are hashed into opaque correlation keys, not anonymized identifiers.

Each session/turn pair gets a distinct operation key. The sender uses one
latest-started operation for the ribbon: a completion for an older or different
operation cannot replace the current one. Repeated starts of the current
operation do not revive a finished notice. A matching response can finish the
current operation even after its working signal expired. These guarantees avoid
one conversation's response clearing another operation's activity; the strip is
not a queue or a complete view of concurrent conversations.

The sibling sender has a 200 ms deadline and receives only fixed labels and an
opaque operation key. Missing executables, unavailable runtime state, and sender
failures are ignored. The adapter emits harmless `{}` JSON and exits successfully;
it never grants or denies approval, injects context, or asks Codex to continue.
The one-second Codex hook timeout also bounds a stalled input stream.

Inside the patched upstream source directory recorded in `.build/source-path`,
run the adapter's focused checks with:

```sh
cargo test --locked -p t1-touchbar --features workstream-codex \
  --bin t1-workstream-codex
```
