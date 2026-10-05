# Stop pointer movement while typing

On the tested MacBookPro14,3, disabling tap-to-click stopped accidental clicks, but palms still moved the pointer. Hyprland already reported `disable_while_typing = true`.

The built-in **Apple SPI Keyboard** reported vendor ID `0000`. The shipped libinput Apple SPI keyboard rule expected `05AC`, so the keyboard missed its internal-device classification. The internal touchpad had no eligible keyboard to pair with for typing suppression. This was a classification problem; we did not need to change palm pressure thresholds or disable another input device.

## Check before applying

This workaround was validated against libinput **1.31.3**. First inspect the relevant keyboard without capturing keystrokes:

```bash
for entry in /sys/class/input/event*; do
  [ "$(cat "$entry/device/name")" = 'Apple SPI Keyboard' ] || continue
  printf '/dev/input/%s\n' "${entry##*/}"
  cat "$entry/device/id/bustype" "$entry/device/id/vendor"
done
```

Our keyboard reported bus `001c` (SPI), vendor `0000`. Use the printed event path below; event numbers change across boots. The `libinput` command is supplied by `libinput-tools` on the tested Arch package split.

```bash
# Replace eventN with the path discovered above.
libinput quirks list /dev/input/eventN
```

If it already reports `AttrKeyboardIntegration=internal`, this missing classification is not your problem. Check the compositor setting and other causes instead.

## Local correction

Back up an existing `/etc/libinput/local-overrides.quirks` before editing it. Add this section once, preserving other sections:

```ini
[Apple SPI Keyboard internal pairing]
MatchName=Apple SPI Keyboard
MatchUdevType=keyboard
MatchBus=spi
AttrKeyboardIntegration=internal
```

Use the local override file, not a packaged file under `/usr/share/libinput`. Validate immediately:

```bash
libinput quirks validate
libinput quirks list /dev/input/eventN
```

Validation must succeed and the keyboard must now show `AttrKeyboardIntegration=internal`. Restore the previous file if parsing fails: a malformed quirk can disable the whole quirks database. [libinput's override documentation](https://wayland.freedesktop.org/libinput/doc/latest/device-quirks.html).

Confirm Hyprland has typing suppression enabled:

```bash
hyprctl getoption input:touchpad:disable_while_typing
```

The expected result is `bool: true`. On the tested Lua configuration, this setting lives under `input.touchpad` in the user's Hyprland input configuration. See [the Lua example](../configs/touchpad.lua) for the tested input settings. Preserve existing settings when changing it; reload Hyprland and check `hyprctl configerrors` after any Lua edit.

**Log out and start a new desktop session, or reboot.** A Hyprland configuration reload alone does not recreate its libinput context. Test ordinary typing with a palm resting on the pad, then lift your hand and confirm normal pointer movement resumes. Modifier-only keypresses and physical clicks are special cases; see [libinput's typing-suppression behavior](https://wayland.freedesktop.org/libinput/doc/latest/palm-detection.html#disable-while-typing).

The user confirmed the unwanted pointer movement was solved after reboot. Tap-to-click and tap-and-drag remained disabled by preference; reenabling them is a separate, untested choice for this setup.

## Rollback and maintenance

Remove only the `Apple SPI Keyboard internal pairing` section and its four properties. Delete the override file only if it contains nothing else that needs preserving. Run `libinput quirks validate`, then start another desktop session.

Recheck the need for this local rule after kernel/libinput updates. The long-term fix belongs in the driver or upstream libinput device classification; local quirks are not a stable public API.
