# Contributor guidance

This repository packages optional user-level Workstream code and documented MacBook fixes.

- Do not commit firmware, EFI data, calibration, keybags, biometrics, credentials, transcripts, local logs, machine serials, device/network identities, or personal filesystem paths.
- Public project URLs and explicit author/upstream attribution belong in documentation.
- Hardware claims must distinguish tested behavior on MacBookPro14,3 from upstream support and untested features.
- Keep the T1Bridge dependency pinned. Preserve upstream license notices and its C/Rust runtime boundary. Shell scripts here are downstream build/install integration only.
- Installation must be explicit, user-level, reversible, and preserve previous renderer/CLI selections and unrelated hooks. No implicit firmware, kernel, PAM, firewall, or sleep changes.
- Keep hooks optional, document their limited duration, and never capture conversation text or bypass application trust prompts.
- Run shell syntax and installer tests for integration changes. Rust changes in the upstream patch require upstream make quality plus all-feature Touch Bar tests. Check patch application on the pinned revision before release.
