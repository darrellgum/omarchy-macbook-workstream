# t1-dash on T2 MacBooks

On T2 Macs the Touch Bar is a USB device (`05ac:8302`). In its default
configuration it only offers a HID keyboard interface, and the bar shows the
firmware's function keys. In configuration 2 it also exposes a display, driven
by the kernel's `appletbdrm` (a DRM card with a 60 x 2008 portrait mode), and a
multitouch digitizer, "Apple Inc. Touch Bar Display Touchpad". tiny-dfr uses
the same switch.

`t1-dash-t2d` (in `touchbar-renderer/t2d/`) is a small root daemon that owns
that display, the digitizer and a uinput keyboard, and serves the T1Bridge
renderer protocol on `/run/t1-dash-t2/touchbar.sock`. t1-dash itself runs
unchanged as your user and connects there when `T1_DASH_SOCKET` is set. The
daemon rotates frames into the portrait scanout, turns touches into renderer
input, reads the Fn key from the keyboard, and types Esc and F1 to F12 through
uinput. It only accepts connections from the user ID given at install time.

## Install

Build t1-dash and the bridge, then run the root half once and the user half
once:

```sh
cd touchbar-renderer
./build.sh
(cd t2d && cargo build --release --locked)
sudo t2/install-root.sh "$USER"   # bridge, udev rules, switches the bar to configuration 2
t2/install-user.sh                # t1-dash user service
```

The udev rule switches the bar to configuration 2 at boot and moves the Touch
Bar display and digitizer to a separate seat so the compositor ignores them.
tiny-dfr, if present, is disabled because only one program can drive the bar.

Rollback:

```sh
t2/rollback-user.sh
sudo t2/rollback-root.sh          # back to the firmware function-key bar
```

## Tested scope

One MacBookPro16,2 (13-inch, 2020) on Omarchy with the linux-t2 7.2.9 kernel.
The bridge protocol was tested on a build box against the real t1-dash with a
simulated display. See the pull request for what was verified on the laptop.
