#!/usr/bin/env bash
# Undo install-root.sh: remove the bridge and rules, put the Touch Bar back in its default key mode.
set -uo pipefail
systemctl disable --now t1-dash-t2d.service
rm -f /etc/systemd/system/t1-dash-t2d.service /usr/local/libexec/t1-dash-t2d /etc/udev/rules.d/90-t1-dash-t2.rules
systemctl daemon-reload; udevadm control --reload
for d in /sys/bus/usb/devices/*; do
  if [[ $(cat "$d/idVendor" 2>/dev/null) == 05ac && $(cat "$d/idProduct" 2>/dev/null) == 8302 ]]; then
    echo 0 > "$d/bConfigurationValue"; echo 1 > "$d/bConfigurationValue"; echo "Touch Bar $d -> configuration 1"
  fi
done
echo ROLLBACK_ROOT_DONE
