# Optional application hooks

Build and install Workstream first using the repository's top-level scripts.
They install both adapters and their sender in the same versioned directory;
the adapter always invokes its sibling sender directly.

The JSON files in `examples/` are fragments to **merge**, not replacements for
your existing settings. Back up the destination before editing, preserve
unrelated settings and hooks, and add each Workstream event only once.

## Codex

Merge [codex-hooks.json](../examples/codex-hooks.json) into the `hooks.json` next
to your active Codex user configuration (normally `~/.codex/hooks.json`, or
under your configured `CODEX_HOME`). Review and trust the definitions through
the app's hook review banner or Settings > Hooks; the CLI has `/hooks`.
New or changed hooks may be skipped until reviewed.

Send a real prompt, wait for a response, then separately test interruption.
The adapter needs both `session_id` and `turn_id`. See
[the detailed Codex guide](workstream-codex.md) for behavior and
[the official configuration reference](https://learn.chatgpt.com/docs/hooks).

## Claude Code

Requires Claude Code **2.1.196 or later** for `prompt_id`; this laptop was tested
with 2.1.289. Merge [claude-hooks.json](../examples/claude-hooks.json) into
`~/.claude/settings.json`. Review with `/hooks` and honor normal trust prompts.

Start a real prompt and wait for a response to verify delivery. Claude's Stop
event does not fire on interruption; the working notice then expires on its
own. See [the detailed Claude guide](workstream-claude.md) and
[Claude's hook reference](https://code.claude.com/docs/en/hooks).

## Disable or uninstall

Disable or remove just the entries whose command is `t1-workstream-codex` or
`t1-workstream-claude`. Do this before uninstalling the tools so the applications
do not keep calling missing executables. Keep any unrelated hook entries.

## What to expect

The working label lasts at most 10 seconds unless another explicit activity
updates the strip. Long turns can be quiet until their response arrives.
Response/error notices last 6 seconds. `RESPONSE READY` describes an application
event, not a promise that the task succeeded or that no other hook will continue
the conversation. Hooks are synchronous, bounded, and fail quietly. They do not
approve actions or alter conversations.
