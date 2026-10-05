# Built-in speakers and microphone on MacBookPro14,3

The tested 2017 15-inch T1 MacBook Pro exposed a **Cirrus Logic CS8409** audio
device, but its speakers and microphone did not work. PipeWire had the correct
speaker and internal-microphone routes selected and neither was muted. The
in-tree codec driver detected the chip without Apple's model-specific
amplifier and microphone initialization.

[davidjo/snd_hda_macbookpro](https://github.com/davidjo/snd_hda_macbookpro) supplies
that Apple-specific driver. After building it against the matching Omarchy
audio headers, the user heard both speaker test tones and confirmed Google
Meet's microphone indicator responded to their voice. This is upstream driver
work; this repository contributes reproducible preparation, compatibility
checks, a restricted DKMS example, and the field notes below.

## Exact tested scope

| Component | Pinned version |
| --- | --- |
| Hardware | MacBookPro14,3, Apple CS8409 subsystem `106b:3900` |
| Running kernel | `7.2.5-3-omarchy`, x86_64 |
| Installed headers | `linux-omarchy-headers` 7.2.5-3 |
| Compiler used locally | GCC 16.2.1 |
| Apple audio driver | [`89b22ff90b86468b186706861dd18663562defa7`](https://github.com/davidjo/snd_hda_macbookpro/tree/89b22ff90b86468b186706861dd18663562defa7) |
| Linux stable v7.2.5 | [`a300e35c0a4b4a38fb53742ea6e2a203c98ee523`](https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/commit/?id=a300e35c0a4b4a38fb53742ea6e2a203c98ee523) |
| Omarchy kernel package | [`7b11c97603dd9d751d803746560ee51640709725`](https://github.com/omacom/omarchy-pkgs/tree/7b11c97603dd9d751d803746560ee51640709725/pkgbuilds/linux-omarchy) |

This recipe deliberately targets that exact kernel. It is not a general
installer for newer kernels, other Intel Mac models, T2, or Apple Silicon.
Do not downgrade a maintained system just to match this snapshot. For another
kernel, use its matching source and distribution patches and repeat the checks.

## Why matching headers matters

Omarchy's `0510-sound-updates.patch` changes private HDA declarations, including
the `snd_hda_get_pin_label` signature. The installed kernel headers provide the
public headers, but the driver build also needs matching private headers from
the sound source tree. The preparation helper includes the relevant Omarchy
header changes before compiling the Apple driver.

Upstream has reports of builds that compiled and then faulted because private
HDA structure layouts did not match the running kernel. See
[issue 193](https://github.com/davidjo/snd_hda_macbookpro/issues/193) and
[issue 205](https://github.com/davidjo/snd_hda_macbookpro/issues/205).
We compared 19 HDA structures against the running kernel's BTF metadata,
including sizes, member offsets, and bitfields. This is an additional check of
the pinned build, not a substitute for matching source or a guarantee of all
runtime behavior.

## Prepare and check without administrator access

Use your normal desktop user. Required tools are Python 3, Make, a compatible
GCC toolchain, patch, kmod, pahole, and the matching installed kernel headers.
DKMS is needed only for the later installation step. The running kernel must
expose its BTF metadata for the compatibility check.

From this repository's root:

```sh
python3 audio/prepare.py
make -C .build/audio/source KERNELRELEASE=7.2.5-3-omarchy
python3 audio/verify-abi.py .build/audio/source/build/hda/codecs/cirrus/snd-hda-codec-cs8409.ko
```

Preparation downloads commit-pinned source files and patches, checks their
SHA-256 hashes, applies the changes, and verifies the prepared source hashes.
It does not substitute another kernel release when a download fails. Building
the prepared source then requires no network access. The helpers do not
install a module, restart audio, change firmware, or activate speakers or mic.

Stop if preparation, compilation, or compatibility verification fails. Do not
ignore a missing structure, change the expected hashes to accept an unexplained
download, or load a module merely because the compiler produced a `.ko` file.

The generated source retains upstream license notices. The helper scripts are
CC0; the driver and kernel source retain their GPL terms. See [notices](../NOTICE.md).

## Explicit installation for the tested kernel

These are administrator steps, separate from the Workstream installer. Review
the prepared source and [DKMS example](../audio/dkms.conf.example) first. Have a
normal reboot available and close applications using audio. A different or
already customized driver setup needs its own review before replacement.

After the build and compatibility check pass, clean generated build artifacts
so the DKMS source directory contains source rather than a previously built
module:

```sh
make -C .build/audio/source KERNELRELEASE=7.2.5-3-omarchy clean
```

The following recipe refuses an existing source destination or a custom codec
module. It installs a root-owned source copy; it does not give a privileged
future build a symlink to a mutable checkout in your home directory.

```bash
(
  set -eu
  [[ $(id -u) -ne 0 ]] || { echo 'Run from the desktop user account.' >&2; exit 1; }
  [[ $(uname -r) == 7.2.5-3-omarchy ]]
  [[ $(uname -m) == x86_64 ]]
  [[ $(cat /sys/class/dmi/id/product_name) == MacBookPro14,3 ]]
  [[ $(modinfo -F intree snd_hda_codec_cs8409) == Y ]]
  audio_name=macbookpro-cs8409
  audio_version=0.1.89b22ff.omarchy7253
  audio_dest=/usr/src/$audio_name-$audio_version
  [[ ! -e "$audio_dest" && ! -L "$audio_dest" ]]
  [[ -z $(dkms status -m "$audio_name" -v "$audio_version") ]]
  sudo install -d -m 755 -- "$audio_dest"
  sudo cp -R -- .build/audio/source/. "$audio_dest/"
  sudo install -o root -g root -m 644 audio/dkms.conf.example "$audio_dest/dkms.conf"
  sudo chown -R root:root -- "$audio_dest"
  sudo chmod -R u=rwX,go=rX -- "$audio_dest"
  sudo dkms add -m "$audio_name" -v "$audio_version"
  sudo dkms build -m "$audio_name" -v "$audio_version" -k 7.2.5-3-omarchy
  sudo dkms install -m "$audio_name" -v "$audio_version" -k 7.2.5-3-omarchy
)
```

If a step fails, inspect its error before continuing. This manual recipe leaves
completed steps in place; rollback is below. Check that `dkms status` reports
the module installed and `modinfo -F filename snd_hda_codec_cs8409` selects the
replacement under `updates/dkms`.

Before rebooting, check whether your boot image bundles the old CS8409 module.
The test laptop's image did not, so no image rebuild was needed. On a system
that does bundle it, rebuild through that system's normal initramfs/UKI process
after installing the replacement. Do not assume another installation has the
same boot-image layout.

Reboot normally to activate the replacement. Our attended local test instead
reloaded the audio stack without a reboot; that required stopping PipeWire and
its sockets, using ordinary module unloads, waiting for asynchronous codec
initialization, and restoring the services. That live-reload helper is not
distributed as an unattended installer. Cold-boot behavior remains an explicit
follow-up check for this recipe.

## Test speakers and microphone

Choose **Analog Stereo Duplex**, **Speakers**, and **Internal Microphone** in
the audio settings. Refresh a browser's meeting page after the device changes.
Start speaker playback quietly through PipeWire with normal volume control.
Upstream warns that direct `hw:`/`plughw:` playback bypasses volume control and
can be very loud; it is not the speaker test used here.

Confirm left and right speaker output, then speak and check the meeting application's
microphone meter. The driver exposes 44.1 kHz audio on this setup; PipeWire can
convert application formats. Upstream microphone support has limits, so actual
voice quality and headset behavior need their own tests.

Local evidence:

- Left and right built-in speaker output produced quiet, generated tones,
  confirmed by the user. This did not isolate each of the four tweeter/woofer drivers.
- Google Meet's microphone indicator responded to the user's voice.
- Microphone decoding to a null output succeeded; no recordings were saved or
  uploaded, and no audio samples are included in this repository.
- The replacement loaded with source version `9850C6A6DB576BCEA7BAC49`; the kernel
  log showed no Oops or panic during the checks.
- Existing Touch Bar, camera, and T1 hardware services remained active.

A complete remote call, subjective microphone quality, headset switching,
cold reboot, and suspend/resume were not validated. The earlier lid-close sleep
issue remains unresolved; this driver installation does not resolve it.

## Kernel updates and rollback

The DKMS example's `BUILD_EXCLUSIVE_KERNEL` matches only `7.2.5-3-omarchy`.
It will not build this private-header snapshot for a future kernel. Audio may
fall back to the generic driver after an upgrade until matching source and
compatibility checks are prepared. Do not simply widen that restriction.

To restore the stock driver, close audio applications, then run:

```sh
sudo dkms remove -m macbookpro-cs8409 -v 0.1.89b22ff.omarchy7253 --all
```

DKMS restores its archived original module and updates module dependencies.
If an initramfs/UKI was rebuilt with the replacement, rebuild it through the
same system workflow after removal. Reboot normally to activate the stock
driver; removal from disk does not replace a module already loaded in memory.
The root-owned source directory can remain for review. No T1, Touch Bar,
fingerprint, camera, or Workstream module needs to be removed.
