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
events, the trackpad fix after reboot, Touch ID, 1Password, and a Google Meet
camera preview through the optional adapter. Subsequent audio work restored both
speaker channels and microphone response in Meet. Media controls
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
boot persistence for Touch ID, camera/audio cold-boot persistence, Bluetooth pairing,
external displays, or GPU power behavior. No T2 or Apple Silicon compatibility
is claimed. Software versions and upstream hooks can change; recheck current
upstream guidance before adapting these notes.

## Camera adapter

The optional [camera adapter](camera.md) was tested on the same MacBookPro14,3
with FFmpeg 2:9.0.1-4 and v4l2loopback 0.15.4-2. It depends on v4l2loopback's
0.15.4 client-usage event ABI; different releases need verification.

- Physical H.264 capture and decoding worked at 1280×720, nominal 30 fps.
- The virtual camera supplied YUV420 video. A four-second null-output check
  delivered 101 frames including startup; a later reopening delivered 60 frames.
  Numeric frame statistics confirmed live video beyond the initial black frames.
- Closing or forcibly killing the capture client released the physical camera.
  Stopping the service during capture also released it; restarting left it idle.
- User confirmation establishes a working Google Meet preview. A complete
  meeting, cold reboot, and suspend/resume were not tested here. Subsequent
  speaker/microphone checks are recorded in the audio section below.
- The C17 build passes warnings-as-errors. Synthetic ASan/UBSan tests cover
  empty event queues, consumer state changes, partial frames, black-buffer
  isolation, startup timing, child termination, reaping, and exit diagnostics.
  Those tests do not open a camera.

Camera images and raw diagnostic logs are not included in this repository.
Physical capture starts only for an active capture client; idle synthetic
black frames keep virtual-camera timestamps fresh. This behavior was checked
locally, not inferred solely from the service being active.

## Audio driver

The [audio guide](audio.md) records the exact Linux, Omarchy, and Apple-driver
revisions used for kernel `7.2.5-3-omarchy`, compiled with GCC 16.2.1. The
in-tree CS8409 module detected this Apple codec but lacked its model-specific
initialization. No additional changes to the upstream Apple driver code were
needed; the preparation includes Omarchy's matching private HDA header changes.

- A fresh run of the public preparation helper downloaded and checksum-verified
  all 23 pinned inputs, reproducing the 20 source/license files used by the
  working installation. The public build recipe then passed without network
  access, and the resulting module passed all 19 live layout checks.
- All **25 isolated audio-helper tests passed**, covering source integrity,
  repeat preparation, failure cleanup, wrong-kernel rejection, and malformed or
  mismatched ABI metadata. These tests do not install or activate audio.
- Nineteen HDA structures matched live kernel BTF: sizes, member offsets, and
  bitfields. This was checked before loading; successful compilation alone
  would not establish compatibility with the distribution's audio backports.
- DKMS built and signed the module, preserved the original, and selected the
  replacement. Its loaded source version matched the verified build.
- User confirmed quiet left/right built-in speaker output, then confirmed
  Google Meet's microphone indicator responded to their voice.
- Microphone decoding to a null output succeeded. No recordings were saved or
  uploaded. Test tones were generated audio; they are not bundled here.
- Audio services restarted without rebooting. The Touch Bar, camera adapter,
  and existing T1 hardware services remained active. The kernel log showed no
  Oops or panic from the driver load and audio checks.

Cold reboot, suspend/resume, headset switching, a complete remote call, and
subjective microphone quality remain untested. The DKMS example deliberately
accepts only this exact kernel. No compatibility with a future kernel or another
Mac is implied by these results. Build helpers do not install or activate the
driver; installation is a separate, explicit administrator operation.

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

For the separate audio helpers, run the isolated tests without audio activation:

```sh
python3 -m unittest discover -s audio/tests -p 'test_*.py'
```

Then use the pinned source build and live compatibility-check commands in the
[audio guide](audio.md#prepare-and-check-without-administrator-access). Those
commands do not install or load the candidate module.
