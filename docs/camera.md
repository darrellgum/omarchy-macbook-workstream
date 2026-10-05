# On-demand FaceTime camera for browser calls

On the tested **2017 15-inch MacBookPro14,3 with Apple T1**, Linux detected the
FaceTime camera but Google Meet reported “Camera Not Found.” The physical UVC
device exposed H.264 video, while Chromium's direct V4L2 capture backend did
not accept that format. Decoding the camera locally worked.

This optional adapter exposes **FaceTime HD (Omarchy)** as a 1280×720 YUV420
camera. The user confirmed its Google Meet preview worked. It is separate from
Workstream: neither feature requires the other.

## How it works

The unprivileged [relay](../camera/relay.c) keeps a v4l2loopback output available
for camera discovery. When a capture client starts streaming, the driver sends
a usage event and the relay starts FFmpeg to decode the physical H.264 camera.
When the client stops, the relay stops and reaps FFmpeg and clears retained
frames. Synthetic black frames refresh the idle virtual camera every 250 ms
without opening the physical camera. Startup may show black briefly while the
physical camera begins delivering frames.

The relay does not capture audio, write image files, or send video over a
network. The application using the camera can still record or transmit video
with its usual permissions. Browser permission and camera selection remain
necessary. Turning off the relay stops its physical capture.

The wrapper locates the physical camera by Apple T1 USB identity and H.264
support, and the virtual output by its label. Video device numbers can change;
no particular `/dev/videoN` number is required. It refuses ambiguous matches.

## Tested scope

- One MacBookPro14,3 with the [documented Omarchy/T1Bridge baseline](hardware.md).
- v4l2loopback **0.15.4-2**, FFmpeg **2:9.0.1-4**, 720p at nominal 30 fps.
- Local live decoding, repeated capture, release after a killed capture client,
  service stop during capture, and user-confirmed Google Meet preview.
- Synthetic C tests cover event handling, partial frames, startup timing, and
  child-process cleanup; see [validation](validation.md#camera-adapter).

The client-usage event is a v4l2loopback-specific ABI, verified against 0.15.4.
Other driver versions need checking. Only one independent V4L2 capture stream
can own this virtual device at a time; a browser or PipeWire may share that
stream internally. Close another application's preview if the camera is busy.

Cold reboot and suspend/resume have not been verified for this adapter. A
working preview does not establish a complete call, microphone support, or
compatibility with other Mac models. T2 and Apple Silicon are outside this
tested scope.

## Install explicitly

These steps are for an existing Omarchy installation whose T1 camera already
appears in `v4l2-ctl --list-devices` and offers H.264. Follow upstream T1Bridge
hardware setup first if it does not. This adapter cannot repair missing Apple
firmware or an absent physical device.

Use a terminal as your ordinary desktop user. Have a C17 compiler, Make, and
headers matching the running kernel available; DKMS needs those headers to
build its module. Install the separately maintained runtime packages:

```sh
omarchy pkg add v4l2loopback-dkms v4l2loopback-utils v4l-utils ffmpeg
```

From this repository's root, build and run the synthetic checks:

```sh
make -C camera all check
```

The following fresh-setup recipe assumes no existing v4l2loopback configuration
or loaded instance. Check first:

```sh
lsmod | grep '^v4l2loopback'
modprobe --showconfig | grep '^options v4l2loopback '
```

No matching output is expected on a fresh setup. If either command shows an
existing setup, reconcile its devices and module options before continuing.
Do not replace another virtual camera's settings or unload a module in use.
The example intentionally lets the driver select an available video number.

Install this adapter's two boot configuration files, refusing existing paths.
These are the explicit administrator steps:

```bash
(
  set -eu
  for path in /etc/modprobe.d/t1-camera.conf /etc/modules-load.d/t1-camera.conf; do
    if [[ -e "$path" || -L "$path" ]]; then
      printf 'Preserving existing file: %s\n' "$path" >&2
      exit 1
    fi
  done
  sudo install -o root -g root -m 644 camera/t1-camera.modprobe.conf /etc/modprobe.d/t1-camera.conf
  sudo install -o root -g root -m 644 camera/t1-camera.modules-load.conf /etc/modules-load.d/t1-camera.conf
  sudo modprobe v4l2loopback
)
```

Install the four user files, again refusing to overwrite existing files or
symlinks. This uses Omarchy's default user configuration directory:

```bash
(
  set -eu
  [[ $(id -u) -ne 0 ]] || { echo 'Run as the desktop user, without sudo.' >&2; exit 1; }
  for path in "$HOME/.local/libexec/t1-camera-relay" \
              "$HOME/.local/libexec/t1-camera-start" \
              "$HOME/.local/bin/t1-camera" \
              "$HOME/.config/systemd/user/t1-camera.service"; do
    if [[ -e "$path" || -L "$path" ]]; then
      printf 'Preserving existing file: %s\n' "$path" >&2
      exit 1
    fi
  done
  install -Dm 755 camera/.build/t1-camera-relay "$HOME/.local/libexec/t1-camera-relay"
  install -Dm 755 camera/t1-camera-start "$HOME/.local/libexec/t1-camera-start"
  install -Dm 755 camera/t1-camera "$HOME/.local/bin/t1-camera"
  install -Dm 644 camera/t1-camera.service "$HOME/.config/systemd/user/t1-camera.service"
  "$HOME/.local/libexec/t1-camera-start" --check
  systemctl --user daemon-reload
  systemctl --user enable --now t1-camera.service
)
```

If a setup step fails, stop and inspect its error. Files from earlier successful
steps remain available for inspection or removal using the instructions below.
No rollback or replacement of unrelated camera configuration is automatic.

## Use and troubleshoot

Refresh Meet, then choose **FaceTime HD (Omarchy)** in **Settings → Video**.
If the browser cached the old device list, restart the browser. Allow its normal
site-camera permission. Close the preview or turn the camera off after testing;
the physical camera light should go out when the application releases capture.

With `~/.local/bin` on your PATH:

```sh
t1-camera status
t1-camera off
t1-camera on
```

`off` stops the current session's service; it remains enabled for the next
graphical login. For diagnosis, use:

```sh
~/.local/libexec/t1-camera-start --check
journalctl --user -u t1-camera.service -n 30 --no-pager
```

The relay retries a failed decoder at most three times per capture session.
Close and reopen the application's camera to try again after resolving a
device conflict. Do not reset the T1 USB device or change its USB configuration
as a camera troubleshooting shortcut: the Touch Bar and Touch ID share it.

## Disable or remove

First close any applications using the camera. Disable automatic startup:

```sh
systemctl --user disable --now t1-camera.service
```

To remove the user files, first confirm the four paths below are still the
adapter files you installed. Preserve any later local changes you want to keep:

```sh
rm -- ~/.local/libexec/t1-camera-relay ~/.local/libexec/t1-camera-start \
  ~/.local/bin/t1-camera ~/.config/systemd/user/t1-camera.service
systemctl --user daemon-reload
```

If the two boot configuration files still contain this adapter's settings,
remove just those files:

```sh
sudo rm -- /etc/modprobe.d/t1-camera.conf /etc/modules-load.d/t1-camera.conf
```

The module can remain loaded until the next reboot. Do not force it to unload
while any application uses a virtual camera. Its packages can remain installed;
remove them separately only if no other software needs them. Removing this
adapter does not require changing the physical T1 camera driver, Touch Bar,
fingerprint, or Workstream configuration.

## Source references

- [T1Bridge camera application notes](https://github.com/standardagents/t1bridge/blob/main/docs/camera-apps.md)
- [Chromium's direct V4L2 capture backend](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/media/capture/video/linux/v4l2_capture_delegate.cc)
- [v4l2loopback 0.15.4 implementation and event ABI](https://github.com/v4l2loopback/v4l2loopback/blob/v0.15.4/v4l2loopback.c)

The relay is original CC0 userspace code. FFmpeg and v4l2loopback are separate
dependencies with their own licenses; see [notices](../NOTICE.md).
