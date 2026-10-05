# Reuse and upstream notices

The original Workstream contributions, downstream scripts, configuration examples,
and documentation in this repository are dedicated to the public domain under
[CC0 1.0 Universal](LICENSE), to the extent possible. No personal attribution is
requested for those contributions. Use, adapt, and incorporate them freely.

This dedication does not replace anyone else's rights or licenses:

- The source patch contains and modifies code from
  [T1Bridge](https://github.com/standardagents/t1bridge), pinned to v0.1.12,
  `81cbdf81026a16e02f0bea74735c6b029a8ffae2`. Its existing code remains under the
  [upstream MIT license](licenses/T1Bridge-MIT.txt).
- The rendered preview includes T1Bridge's existing icon and font artwork.
  Cupertino and Myna UI icons use MIT; Inter uses SIL OFL 1.1. Their notices and
  license text are preserved in [third-party notices](licenses/T1Bridge-THIRD_PARTY_NOTICES.md).
- Builds fetch the complete upstream tree and retain its license files. The
  fetched kernel components have their own GPL terms; this companion package
  does not build or install those components. Cargo dependencies retain their
  respective licenses in the fetched source/package metadata.

The preserved third-party notice file describes the wider upstream project;
this companion does not bundle its recovery tools or Apple firmware. No binaries,
firmware, calibration, keybags, or biometric data are distributed here.

This is an independent community experiment, not an official Omarchy or T1Bridge
release. Omarchy, Apple, Codex, and Claude names identify compatibility; they do
not imply endorsement.
