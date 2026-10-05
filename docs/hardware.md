# A 2017 MacBook Pro on Omarchy

These are results from one **15-inch MacBookPro14,3 with Apple T1**, tested on October 4–5, 2026. They are a reproducible starting point, not a support promise for every Intel Mac. T2 and Apple Silicon machines need different hardware support.

The hardware foundation is [T1Bridge](https://github.com/standardagents/t1bridge), from Standard Agents. The custom Touch Bar renderer, activity integrations, and local compatibility fixes build on that project's work. Omarchy, libinput, fprintd, and the Linux drivers supply the rest of the desktop and input stack.

## Tested baseline

| Component | Version |
| --- | --- |
| Omarchy | 4.0.4-1 |
| Hyprland | 0.56.2-2 |
| Kernel | 7.2.5-3-omarchy |
| libinput | 1.31.3-1 |
| T1Bridge / DKMS | 0.1.12-1 |
| T1Bridge Omarchy integration | 0.2.2-1 |

These versions document the test environment. Use maintained, compatible packages and the upstream installation instructions rather than downgrading a current system to this snapshot.

## What worked

| Area | Evidence |
| --- | --- |
| Touch Bar | Panel restored, renderer active after reboot, custom animated activity display confirmed by the user. |
| AI activity | Real Codex desktop and Claude Code lifecycle signals appeared on the Touch Bar. |
| Trackpad while typing | Local libinput classification fix validated; user confirmed the problem was solved after reboot. [Details](trackpad.md). |
| Touch ID | Enrollment and matching; fingerprint-only sudo, Polkit, and screen unlock all passed. Each consumer also passed password fallback with the fingerprint service unavailable. [Details](touch-id.md). |
| 1Password | User confirmed Linux system-authentication unlock worked with Touch ID. |
| Keyboard backlight integration | Fixed a status query that unintentionally lowered brightness. Read-only reporting preserved the brightness value. [Details](desktop-integration.md). |
| Media backend | Volume, mute, play/pause, next, and previous passed controlled provider tests with a silent test player. This does not establish every player's behavior. |
| Wi-Fi and display | Wi-Fi connected; main display active. Intel and AMD graphics drivers loaded. |

## Known limits and unfinished checks

- **System sleep is unresolved.** T1Bridge lists suspend/resume as unsupported on its tested machine. We did not establish reliable lid-close sleep or wake here. Display blanking is a separate operation. [Upstream function support](https://github.com/standardagents/t1bridge#t1-function-support).
- **GPU power needs investigation.** A service named `omarchy-nvme-suspend-fix.service` wrote `d3cold_allowed=0` to a PCI device that was actually the Radeon GPU on this laptop. The GPU was runtime-active. We did not remove the rule, prove it caused the power behavior, or validate an alternative. Never assume a hard-coded PCI address identifies the same hardware on another machine.
- **Touch ID after reboot was not retested after enrollment.** Earlier reboots validated the Touch Bar and trackpad change. Cold power-on persistence also remains untested.
- Bluetooth was detected and unblocked, but pairing and audio were not tested. Startup firmware/baud-rate warnings remain observations rather than a proven functional failure.
- Camera and microphone devices were registered; capture was not tested. External displays and GPU switching were not tested.
- An older, inactive SPI DKMS package remained installed. It was not the active keyboard driver; cleanup was deferred.

The machine's Apple firmware, calibration, keybags, fingerprints, recovery backups, and raw diagnostic logs are deliberately absent from this repository. A second Mac must use its own data. Our local recovery was attended and specific to this machine; this project is not a firmware recovery bundle.
