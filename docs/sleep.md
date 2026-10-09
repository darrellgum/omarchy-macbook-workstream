# MacBookPro14,3 S3 sleep: Touch Bar, Touch ID and Wi-Fi after wake

On the tested 15-inch MacBookPro14,3, deep (S3) sleep works on a locally
patched Omarchy kernel. After resume three things broke: the Touch Bar stayed
dark, Touch ID stopped responding, and Wi-Fi often failed to reconnect. The
files in [sleep/](../sleep) fix all three. They are optional and separate from
both Touch Bar renderers.

Getting this working is what made the [t1-dash renderer](touchbar-renderer.md)
worth building: once the Touch Bar and Touch ID survived sleep, the bar was
something I could rely on every day.

This guide does not cover getting S3 itself to work; that needs a kernel where
suspend and resume already succeed. The helper does nothing unless the running
kernel matches `KERNEL_GLOB` (default `*-amdfix`, the tested local build), so
it is inert on a stock kernel.

## Touch Bar and Touch ID: tell the T1 the host is awake

On sleep, firmware `_PTS` sends the T1 a "host sleeping" message through the
EC. On wake, macOS (and Boot Camp's `AppleSOC.sys`, through `ASOC.SOCW(1)`)
sends "host powering on" about 0.4 s before the T1 reappears on USB. Linux
never sends it, so the T1 stays parked. USB-side fixes (forcing
re-enumeration, replaying macOS's post-wake HID reports, restarting the
renderer) did not help.

The fix after resume:

1. `echo '\_SB.PCI0.XHC1.RHUB.ASOC.SOCW 1' > /proc/acpi/call` (needs the
   `acpi_call` module). It returns `0x0` in about 90 ms and the Touch Bar and
   Touch ID come back, dim.
2. Write feature report 3, `03 02 f4 01`, to T1 USB interface 6
   ([t1-display-on.py](../sleep/t1-display-on.py)) to restore brightness.

`SOCW(1)`'s EC and T1 waits have timeouts (about 1 s and 4 s). An earlier
report said it hard-froze a 14,3; that did not happen here across several
wakes, but other firmware may differ. **Never call `ASOC.FRST`**: it is an EC
hard reset that leaves the T1 stuck in recovery (`05ac:1281`).
[t1-socw.sh](../sleep/t1-socw.sh) sends `SOCW(1)` once by hand for testing.

## SSD and Wi-Fi

- Before sleep the NVMe SSD's `d3cold_allowed` is set to 0.
- Wi-Fi (brcmfmac) is signed off, the radio turned off and the driver
  unloaded before sleep, so the slot can fully power down; after resume it is
  reloaded. About 8 s later the `wifichk` step checks for an IPv4 address. If
  the connection came back on its own, on any band, it is left alone. Only when
  there is no IPv4 address does it rescan and reconnect, then disconnect,
  rescan and connect once more if it is still down. Logs record the frequency
  only, never network names.
- A boot-time unit adds a runtime logind drop-in (in `/run`, gone on reboot)
  so closing the lid suspends.

## Install (administrator steps)

```sh
sudo pacman -S --needed acpi_call   # or acpi_call-dkms for a custom kernel
sudo install -Dm755 sleep/macbook143-s3 /usr/local/sbin/macbook143-s3
sudo install -Dm644 sleep/t1-display-on.py /usr/local/lib/macbook143-s3/t1-display-on.py
sudo install -Dm644 sleep/macbook143-s3-sleep.service sleep/macbook143-lid.service -t /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable macbook143-s3-sleep.service macbook143-lid.service
```

Optional overrides go in `/etc/default/macbook143-s3`: `KERNEL_GLOB`,
`NVME_PCI` (default `0000:02:00.0`), and `WIFI_IF` (default: the brcmfmac
interface). Check results with `journalctl -t macbook143-s3`.

Rollback:

```sh
sudo systemctl disable macbook143-s3-sleep.service macbook143-lid.service
sudo rm /etc/systemd/system/macbook143-s3-sleep.service /etc/systemd/system/macbook143-lid.service \
  /usr/local/sbin/macbook143-s3 /usr/local/lib/macbook143-s3/t1-display-on.py
sudo rm -f /run/systemd/logind.conf.d/90-macbook143-lid.conf
sudo systemctl daemon-reload
```

## Tested scope

One MacBookPro14,3 on Omarchy with a locally patched 7.2.5 kernel and
T1Bridge 0.1.12, across several suspend and resume cycles: the Touch Bar,
Touch ID (lock-screen unlock right after wake) and Wi-Fi came back. The
published helper is the tested one with personal details removed and a few
values made configurable (interface name, kernel match, display-on script
moved to its own file). That cleaned-up version passed `bash -n` and Python
compile checks but has not yet run through a real suspend cycle.

A clean upstream fix would live in the driver: `SOCW(0)` on suspend and
`SOCW(1)` on resume, then the display-on report.
