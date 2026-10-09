# Reuse and upstream notices

The original Workstream contributions, camera relay, downstream scripts, configuration examples,
and documentation in this repository are dedicated to the public domain under
[CC0 1.0 Universal](LICENSE), to the extent possible. No personal attribution is
requested for those contributions. Use, adapt, and incorporate them freely.

This dedication does not replace anyone else's rights or licenses:

- The alternative renderer in `touchbar-renderer/` vendors the T1Bridge
  renderer client from the upstream touchbar-doom example in
  `touchbar-renderer/src/t1bridge/`. That code remains under its
  [MIT license](touchbar-renderer/LICENSES/touchbar-client-MIT.txt). The bundled
  DejaVu Sans Bold font keeps its
  [license](touchbar-renderer/LICENSES/DejaVu-fonts-copyright.txt). Grok Bot
  avatar artwork is not included; the optional integration builds only from
  geometry generated locally from the user's own installed app.
- The source patch contains and modifies code from
  [T1Bridge](https://github.com/standardagents/t1bridge), pinned to v0.1.12,
  `81cbdf81026a16e02f0bea74735c6b029a8ffae2`. Its existing code remains under the
  [upstream MIT license](licenses/T1Bridge-MIT.txt).
- The rendered preview includes T1Bridge's existing icon and font artwork.
  Cupertino and Myna UI icons use MIT; Inter uses SIL OFL 1.1. Their notices and
  license text are preserved in [third-party notices](licenses/T1Bridge-THIRD_PARTY_NOTICES.md).
- Workstream builds fetch the complete T1Bridge tree and retain its license files.
  Its kernel components have their own GPL terms; the Workstream build and
  installer do not build or install those components. Cargo dependencies retain their
  respective licenses in the fetched source/package metadata.
- The camera relay is original userspace code that invokes separately installed
  FFmpeg and uses the Linux V4L2 interface and v4l2loopback's client-usage event
  ABI. The [v4l2loopback source](https://github.com/v4l2loopback/v4l2loopback/blob/v0.15.4/v4l2loopback.c)
  was consulted to implement that interface; no driver implementation is
  bundled. v4l2loopback and FFmpeg retain their own licenses. Camera setup
  explicitly installs the distribution's separately packaged virtual-camera
  module; the Workstream installer does not install it.
- The audio preparation helper fetches pinned source and patches from
  [Linux stable](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/),
  [Omarchy's kernel package](https://github.com/omacom/omarchy-pkgs), and
  [davidjo/snd_hda_macbookpro](https://github.com/davidjo/snd_hda_macbookpro).
  That kernel-driver code remains under its upstream GPL terms and retains
  its source notices and license file in the generated build tree. Original
  preparation/verification helpers and field notes here are CC0; that
  dedication does not relicense the fetched driver or patches. No compiled
  audio module is distributed by this repository.

The preserved third-party notice file describes the wider upstream project;
this companion does not bundle its recovery tools or Apple firmware. No binaries,
firmware, calibration, keybags, or biometric data are distributed here.

This is an independent community experiment, not an official Omarchy or T1Bridge
release. Omarchy, Apple, Codex, and Claude names identify compatibility; they do
not imply endorsement.
