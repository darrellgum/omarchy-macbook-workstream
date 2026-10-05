# Omarchy MacBook Workstream

A little more life for a strip of OLED.

Workstream gives a T1 MacBook's Touch Bar a Tokyo Night activity ribbon: flowing
light while a command runs, a small completion sweep, and optional signals from
Codex and Claude Code. Escape, brightness, keyboard backlight, media, and volume
stay in fixed positions. Hold Fn for the full function-key row.

![A captured Workstream renderer frame with the MEASURING label and fixed controls](assets/workstream.png)

*An actual renderer frame from the test laptop. This still image does not show
the animation.*

This repository also includes a camera adapter and records the trackpad and
Touch ID work that made one MacBookPro14,3 pleasant to use with Omarchy. It is an early community handoff,
tested on one 2017 15-inch T1 MacBook Pro. Other models and software combinations
need testing. The hardware foundation is [T1Bridge](https://github.com/standardagents/t1bridge).

## Start here

| What you need | Where to look |
| --- | --- |
| A dark or nonfunctional Touch Bar | [Hardware setup and tested scope](docs/hardware.md) — get upstream T1Bridge working first |
| Accidental pointer movement while typing | [Apple SPI trackpad fix](docs/trackpad.md) |
| Fingerprint enrollment, login prompts, or 1Password | [Touch ID field notes](docs/touch-id.md) |
| Google Meet reports “Camera Not Found” | [On-demand FaceTime camera adapter](docs/camera.md) — tested with a Google Meet preview |
| Keyboard brightness changes when only its status should be shown | [Optional Omarchy provider fix](docs/desktop-integration.md) |
| Animations and AI activity | Build Workstream below, then [enable optional hooks](docs/hooks.md) |

## Build and try Workstream

**Prerequisite:** the packaged T1Bridge Touch Bar must already work, with
`systemctl --user is-active t1-touchbar.service` reporting `active`.
The installer is for that existing setup; it does not provision the T1 chip.

On Arch/Omarchy, have Git, a C17-capable compiler and archiver (normally from
`base-devel`), Rust/Cargo 1.88 or later, coreutils, and util-linux available.
Building downloads the exact upstream source and Cargo dependencies.

```sh
git clone https://github.com/darrellgum/omarchy-macbook-workstream.git
cd omarchy-macbook-workstream
./scripts/build.sh
./scripts/install.sh
```

Run these as your desktop user. The build applies the reviewable
[source patch](workstream/patches/0001-workstream.patch) to T1Bridge v0.1.12 at
`81cbdf81026a16e02f0bea74735c6b029a8ffae2`, using Cargo's lockfile.
The installer checks binary hashes, saves previous renderer and command
selections, installs all four tools together under `~/.local/libexec`, then
restarts and checks the user Touch Bar service. Failure restores the previous
selection. Nothing is installed into `/usr`.

Try a real command (ensure `~/.local/bin` is on your PATH):

```sh
t1-workstream run CHECKING -- git status --short
t1-workstream run BUILDING -- cargo build --locked
```

The second example belongs in a Rust project. Only the label appears on the
Touch Bar; the wrapper runs the exact arguments, inherits normal input/output,
and preserves the command's exit status. See [the renderer/protocol guide](docs/workstream.md)
for explicit integrations and reduced motion.

## Codex and Claude Code

[Hook setup](docs/hooks.md) is a separate, optional step. The examples preserve
application trust review and must be merged with existing settings.

- Codex: `WORKING`, `RESPONSE READY`, and `INTERRUPTED`.
- Claude Code: `CLAUDE WORKING`, `RESPONSE READY`, and `FAILED - CLAUDE ERROR`.

These are brief lifecycle signals: working notices expire after 10 seconds,
and response/error notices after 6 seconds. They do not continuously track model
execution, reveal reasoning, or expose Claude's changing spinner word. The
command wrapper refreshes activity for as long as its command actually runs.
The most recently started operation owns the ribbon; it is not a queue.

No prompt, response, transcript, or tool-output text is displayed or logged by
the adapters. Only fixed labels and opaque correlation keys reach Workstream.

## Restore the previous setup

Remove only the Workstream hook entries you added, then run:

```sh
./scripts/uninstall.sh
```

The uninstaller restores the original renderer and CLI paths, including files,
symlinks, and paths that did not exist. It refuses to overwrite selections you
changed after installation. Versioned binaries and archived recovery receipts
are retained. Installation requires a working user Touch Bar service. Uninstallation can
restore a failed or stopped service, then checks that the restored renderer
starts. Private recovery receipts live under
`${XDG_STATE_HOME:-$HOME/.local/state}/t1-workstream`.

The optional keyboard-provider, trackpad, and camera changes have separate
rollback instructions in their guides. The Workstream installer does not change
firmware, kernels, authentication, firewall rules, or application hook settings.
Camera setup is a separate opt-in procedure with explicit administrator steps
to install and load the v4l2loopback module.

## Review, reuse, and contribute

[Validation](docs/validation.md) records software versions, checks, and limits.
The Workstream patch is separated from hardware field notes so maintainers can
reuse small pieces independently. Useful next contributions include testing
another T1 model, improving upstream Apple SPI keyboard classification, and
integrating the keyboard-brightness status fix upstream.

Original contributions are dedicated to the public domain with **CC0**; no
personal credit is requested. Existing T1Bridge and artwork licenses remain in
place. See [NOTICE.md](NOTICE.md) for the boundaries and preserved notices.
