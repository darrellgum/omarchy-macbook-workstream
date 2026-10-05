# Validation and scope

Recorded October 5, 2026. The hardware evidence is from one MacBookPro14,3;
see [hardware.md](hardware.md) for the exact OS/package baseline and open issues.
Development and review were assisted by AI coding agents. The tests below and
the human checks on the laptop are the evidence for the claims in this package.

## Source and build

- Base: T1Bridge v0.1.12, `81cbdf81026a16e02f0bea74735c6b029a8ffae2`.
- The exported patch applied cleanly to a fresh fetch of that exact commit.
- `scripts/build.sh` built all four release binaries with the checked-in Cargo
  lockfile, using Rust/Cargo 1.98.1 on the documented Arch/Omarchy machine.
- The patched source passed upstream `make quality`: **1,004 Rust tests passed,
  3 upstream tests ignored**, plus its formatting, lint, dependency, C,
  sanitizer, synthetic PAM, kernel, and packaging checks. These synthetic
  checks do not authenticate against or reconfigure the real system.
- A second check in the clean, fetched-and-patched source passed
  `cargo test --locked -p t1-touchbar --all-features --all-targets`:
  **144 passed, none failed or ignored**. The two test totals overlap.

The patch includes protocol/state tests, stale-completion and concurrency
checks, malformed/oversized hook payload handling, fixed labels, sender
timeouts, preserved stock colors, and Fn behavior. No model call or real
fingerprint is needed for these automated tests.

## Installer and examples

All shell files passed `bash -n`. `tests/installer.sh` passed **13 isolated
scenarios** using temporary homes, fake binaries, and mocked systemctl:

- Restore files, modes, symlinks, and original absence across repeat installs
  and upgrades; retain binaries referenced by an older selection.
- Roll back initial install, upgrade, and uninstall when restart fails;
  reject a restart that returns success while the service remains inactive.
- Uninstall successfully from an initially inactive service.
- Handle TERM immediately after each receipt commit without mismatching
  receipts and selections; release the installer lock after SIGKILL.
- Preserve user-edited selections and reject checksum/preflight failures.

The optional provider documentation's install/rollback recipe was exercised in
isolated directories for absent files, files, symlinks, dangling symlinks, and
refusal over later edits. Hook examples are valid JSON and preserve normal
application review. The installer does not enable them.

## Hardware checks and remaining limits

The user confirmed animated Workstream, real Codex desktop and Claude Code
events, the trackpad fix after reboot, Touch ID, and 1Password. Media controls
were checked against a silent test player. Password fallback was tested for
each fingerprint-enabled authentication consumer while fprintd was unavailable.

The publication build adds a regression fix preserving the stock renderer's
original monochrome palette. This build was compiled and tested without
replacing the already-working live installation during packaging. Installer
tests are isolated; they are not a second physical installation on another Mac.

The renderer schedules animation at most once per 34 ms. This is a rendering
limit, not a measured guarantee of OLED scanout rate. Idle/hidden/blanked states
stop animation requests; reduced motion is available.

We have not established reliable system suspend/resume, post-enrollment cold
boot persistence for Touch ID, camera/microphone capture, Bluetooth pairing,
external displays, or GPU power behavior. No T2 or Apple Silicon compatibility
is claimed. Software versions and upstream hooks can change; recheck current
upstream guidance before adapting these notes.

## Reproduce the local checks

From the repository root, as your normal user:

```bash
for script in scripts/*.sh tests/*.sh integrations/t1bridge-omarchy-provider; do
  bash -n "$script" || exit
done
./tests/installer.sh
./scripts/build.sh
source_dir=$(cat .build/source-path)
CARGO_TARGET_DIR="$PWD/.build/target" cargo test \
  --manifest-path "$source_dir/Cargo.toml" --locked \
  -p t1-touchbar --all-features --all-targets
```

For the wider upstream quality suite, enter that patched source directory and
follow its contributor/build requirements before running `make quality`.
It requires more development packages than the four-binary build. Raw local
logs are not included because build paths and machine diagnostics can contain
personal data.
