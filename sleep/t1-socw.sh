#!/bin/bash
# Manual test: send the T1 "host awake" message (ACPI ASOC.SOCW(1)) once. Run as root after a wake
# where the Touch Bar and Touch ID stay dead. 0x0 means the T1 acknowledged. Never calls FRST.
set -u
P='\_SB.PCI0.XHC1.RHUB.ASOC.SOCW'
lsmod | grep -q '^acpi_call' || modprobe acpi_call || { echo "acpi_call missing"; exit 1; }
echo "$P 1" > /proc/acpi/call
echo "SOCW(1) returned: $(tr -d '\0' < /proc/acpi/call)"
