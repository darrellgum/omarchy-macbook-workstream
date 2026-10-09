#!/usr/bin/env python3
"""Send the T1 Touch Bar "display on" feature report (report 3: 03 02 f4 01) on USB interface 6.

macOS sends this after wake. After ASOC.SOCW(1) the bar comes back dim/parked; this restores it.
Exits 1 if the T1 interface-6 hidraw node is not present yet. Pass --check to only print report 3.
"""
import fcntl, glob, os, sys

def ioc(n, size):  # _IOC(_IOC_READ|_IOC_WRITE, 'H', n, size)
    return (3 << 30) | (size << 16) | (ord('H') << 8) | n

def find():
    for h in glob.glob('/sys/class/hidraw/hidraw*'):
        if '/1-3:2.6/' in os.path.realpath(h + '/device') + '/':
            return '/dev/' + os.path.basename(h)
    return None

dev = find()
if not dev:
    sys.exit('T1 interface 6 hidraw not found')
fd = os.open(dev, os.O_RDWR)
if '--check' not in sys.argv:
    r = bytearray(15); r[0:4] = bytes([3, 2, 0xF4, 1])
    fcntl.ioctl(fd, ioc(6, 15), r)  # HIDIOCSFEATURE
b = bytearray([3] + [0] * 14)
fcntl.ioctl(fd, ioc(7, 15), b)       # HIDIOCGFEATURE
print(dev, 'report 3:', bytes(b).hex())
