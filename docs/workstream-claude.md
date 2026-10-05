# Optional Claude Code lifecycle signals

`t1-workstream-claude` connects local Claude Code to the Workstream renderer.
It requires Claude Code 2.1.196 or later: the official
[common hook fields](https://code.claude.com/docs/en/hooks#common-input-fields)
include `prompt_id` from that version. Events without a valid prompt identifier
are ignored, avoiding an unsafe session-only fallback.

| Hook | Touch Bar label | Meaning |
| --- | --- | --- |
| `UserPromptSubmit` | CLAUDE WORKING | A prompt was submitted. |
| `Stop` | RESPONSE READY | Claude produced a response, not a guarantee that the work succeeded. |
| `StopFailure` | FAILED - CLAUDE ERROR | Claude's turn ended with an API error. |

Claude's [Stop hook](https://code.claude.com/docs/en/hooks#stop) does not fire
on user interruption. There is no Interrupt mapping. The working signal expires
after 10 seconds; terminal notices last 6 seconds. This is a brief activity
signal, not continuous monitoring of model execution. A cancelled or long turn
therefore becomes quiet. Explicit command wrappers can keep actual build or
test activity refreshed throughout a command.

## Build and install

Use the companion's [top-level build/install scripts](../README.md#build-and-try-workstream).
They install the renderer and both adapters in a versioned directory, preserve
previous selections, and provide rollback. Do not copy individual binaries
over the installer-managed command links.

The adapter and `t1-workstream` must share a directory. The adapter finds its
sender beside its resolved executable, without searching `PATH` or invoking a
shell. The Workstream renderer must already be active to display signals.
See [hook setup](hooks.md) after installation.

## Configure and verify

Merge the following entries into `~/.claude/settings.json`, preserving other
settings and hooks. These are ordinary command hooks using the official
[settings format](https://code.claude.com/docs/en/hooks#configuration).
Building the adapter does not activate hooks or change Claude's settings.

```json
{
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "\"${HOME}/.local/bin/t1-workstream-claude\"",
        "timeout": 1,
        "async": false
      }]
    }],
    "Stop": [{
      "hooks": [{
        "type": "command",
        "command": "\"${HOME}/.local/bin/t1-workstream-claude\"",
        "timeout": 1,
        "async": false
      }]
    }],
    "StopFailure": [{
      "hooks": [{
        "type": "command",
        "command": "\"${HOME}/.local/bin/t1-workstream-claude\"",
        "timeout": 1,
        "async": false
      }]
    }]
  }
}
```

Keep delivery synchronous so hooks do not race after being put in the
background. Review the configured hooks with Claude's `/hooks` interface and
honor any normal trust prompts. Test a real prompt and response in local Claude
Code before calling the integration live. A synthetic hook test proves the
adapter works, but cannot establish that Claude invoked it. Remove only these
entries to disable the integration; there is no need to disable unrelated hooks.

## Failure, privacy, and concurrency

The adapter accepts at most 64 KiB plus one overflow byte from standard input.
It uses only the event name, session and prompt identifiers, and the presence of
`agent_id` to ignore subagents. It never opens a transcript or forwards, stores,
or logs prompt text, assistant output, error details, or tool arguments.
Both identifiers must be nonempty strings of at most 512 bytes. They are hashed
into a Claude-prefixed correlation key; hashing is not anonymization.

Each prompt in each session has a distinct key. The latest started operation
wins, including across Claude, Codex, and explicit command wrappers. An older
prompt's completion cannot finish a newer operation. Subagent events cannot
finish the main prompt, and repeated starts cannot revive its finished notice.
Other Stop hooks can still ask Claude to continue; RESPONSE READY describes
the response event and does not certify that an entire session has ended.

Only fixed display labels and the opaque key reach the sibling sender. The
sender has a 200 ms deadline; the one-second hook timeout bounds stalled input.
Missing executables, invalid input, and unavailable runtime storage are quiet
failures. The adapter exits successfully without stdout or stderr and makes no
approval decision, continuation request, or context injection.

Run this focused developer check inside the patched upstream source directory
recorded in `.build/source-path` (not the companion repository root):

```sh
cargo test --locked -p t1-touchbar --features workstream-claude \
  --bin t1-workstream-claude --test workstream_claude
```

The tests include repeated prompts in one session, stale completions across
prompts and sessions, subagent isolation, fixed labels, malformed input,
unavailable storage, and sender timeout cleanup. They use synthetic data and
private temporary runtime directories; no Claude model call is needed.
