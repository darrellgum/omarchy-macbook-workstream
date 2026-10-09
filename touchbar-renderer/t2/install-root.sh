#!/usr/bin/env bash
# Root half of the T2 install. Run with sudo from touchbar-renderer/t2: sudo ./install-root.sh <desktop-user>
# Installs the bridge daemon, its unit and udev rules, then switches the Touch Bar to display mode.
set -euo pipefail
cd "$(dirname "$0")"
user=${1:?usage: sudo ./install-root.sh <desktop-user>}
uid=$(id -u "$user")
bin=../t2d/target/release/t1-dash-t2d
[[ -x $bin ]] || { echo "build first: (cd ../t2d && cargo build --release)" >&2; exit 1; }
systemctl disable --now tiny-dfr.service 2>/dev/null || true
install -Dm755 "$bin" /usr/local/libexec/t1-dash-t2d
sed "s/@UID@/$uid/" t1-dash-t2d.service > /etc/systemd/system/t1-dash-t2d.service
install -Dm644 90-t1-dash-t2.rules /etc/udev/rules.d/90-t1-dash-t2.rules
systemctl daemon-reload
udevadm control --reload
for d in /sys/bus/usb/devices/*; do
  if [[ $(cat "$d/idVendor" 2>/dev/null) == 05ac && $(cat "$d/idProduct" 2>/dev/null) == 8302 && $(cat "$d/bConfigurationValue") != 2 ]]; then
    echo 0 > "$d/bConfigurationValue"; echo 2 > "$d/bConfigurationValue"; echo "Touch Bar $d -> configuration 2"
  fi
done
modprobe appletbdrm || true
sleep 2
udevadm trigger --subsystem-match=drm --subsystem-match=input --action=change
systemctl enable --now t1-dash-t2d.service
sleep 2
systemctl --no-pager --lines=8 status t1-dash-t2d.service || true
echo INSTALL_ROOT_DONE
