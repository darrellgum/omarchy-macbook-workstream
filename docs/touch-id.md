# Touch ID: verified locally, configured deliberately

On this MacBookPro14,3, Touch ID worked for **sudo, graphical administrator prompts, Omarchy screen unlock, and 1Password**. Enrollment alone was not enough: we verified each authentication consumer and its password fallback independently.

## Hardware and package prerequisites

Use the [T1Bridge installation guide](https://github.com/standardagents/t1bridge#install-official-packages) for drivers, service setup, and signed packages. This repository does not replace that hardware stack.

Our working fingerprint pair was:

| Package | Tested version |
| --- | --- |
| `libfprint-t1bridge` | 1.94.100-20 |
| `fprintd-t1bridge` | 1.94.5-17 |
| `libgusb` | 0.4.9-2 |

The two fingerprint packages replace the distribution's corresponding packages and must remain compatible. An unrelated desktop enrollment wizard can replace them; check what it intends to install first.

The local T1Bridge importer successfully loaded **this sensor's own Apple data**. No such data is included here. Preserve your machine's EFI data and keep an external backup; another laptop's files cannot substitute for it.

This laptop also needed an inbound IPv6 TCP 61500 firewall allowance for the private T1 link. Our rule was restricted to the dynamically identified T1 interface and validated protocol peer. A broad LAN/Wi-Fi port opening is not equivalent. Follow the [upstream firewall procedure](https://github.com/standardagents/t1bridge/blob/main/docs/setup.md#firewall-recovery-and-removal), because interface names and local policy differ.

## Enroll and verify interactively

Use a terminal in your normal graphical user session, with a working Polkit agent:

```bash
fprintd-list "$(id -un)"
fprintd-enroll -f right-index-finger
fprintd-verify -f right-index-finger
```

If the finger is already enrolled, verify it instead of enrolling again. Touch the sensor without pressing the power button. Lift and retouch the same finger at slightly different angles during enrollment. Require both `enroll-completed` and a later `verify-match` before changing authentication settings.

On our first attempt, enrollment returned `enroll-unknown-error` before scanning. Local diagnosis identified a device-keybag preparation/bootstrap failure. One diagnostic broker restart and retry then reached `enroll-completed`; the diagnostics were removed afterward. This is a narrowly observed recovery, not a general instruction to restart services for every enrollment error.

One verification scan did not match; repositioning the same finger produced a match. Require a real match rather than treating successful enrollment alone as proof.

## Authentication policy used here

We followed the [upstream PAM setup guidance](https://github.com/standardagents/t1bridge/blob/main/docs/setup.md#enable-fingerprint-sign-in-safely), with backups and an open root recovery session until all checks passed:

- Added a bounded fingerprint attempt to the existing **sudo** and **Polkit** authentication stacks, retaining their password and account/session checks. A closed-lid check bypassed the inaccessible fingerprint reader. The fingerprint attempt used three tries and a ten-second timeout.
- Configured Omarchy's separate `/etc/pam.d/omarchy-lock-fingerprint` service for its Quickshell lock screen. Kept `/etc/pam.d/omarchy-lock-password` and `/etc/pam.d/system-auth` unchanged.
- Tested an actual successful fingerprint for each consumer, then tested password access for each with `fprintd.service` temporarily unavailable. Restored the service before finishing.

The lock screen uses separate fingerprint and password conversations. Its fingerprint-only PAM file is **not** a replacement for a sudo, Polkit, or login stack. We do not ship a script that overwrites another system's PAM policy.

For rollback, restore only the consumer files you changed from their pre-change backups, or remove newly created overrides that had no predecessor. Do not overwrite subsequent administrator changes. Fingerprint enrollment and PAM enablement are separate; reverting PAM need not delete a saved fingerprint.

## 1Password

After Polkit fingerprint authentication worked, the user enabled **Settings → Security → Unlock using system authentication** in the Linux 1Password desktop app and confirmed it unlocked with Touch ID. 1Password still requires its account password in circumstances described by its [system-authentication documentation](https://support.1password.com/system-authentication-linux/).

Reboot persistence was not retested after enrollment. Other models, desktops, multi-user enrollment, and passwordless initial login are outside these local test results.
