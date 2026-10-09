#!/usr/bin/env python3
"""Fake t1-touchbar-hw service (protocol v1 minor 0) for an end-to-end smoke test on the box.
Runs t1-dash against it, dumps the received frames, injects touches, checks TapKeys."""
import os, socket, struct, subprocess, sys, time, array, mmap, zlib

W, H = 2170, 60
SOCK = "/tmp/t1-dash-test.sock"
OUT = sys.argv[2] if len(sys.argv) > 2 else "previews"
BIN = sys.argv[1]
HDR = struct.Struct("<4sHHII")

def pkt(t, rid, payload=b""): return HDR.pack(b"T1HW", 1, t, len(payload), rid) + payload

def png(path, xrgb):
    rows = b"".join(b"\0" + bytes(sum(([xrgb[i+2], xrgb[i+1], xrgb[i]] for i in range(y*W*4, (y+1)*W*4, 4)), [])) for y in range(H))
    c = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + c(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0)) + c(b"IDAT", zlib.compress(rows)) + c(b"IEND", b""))

def input_frame(fn, contacts):
    p = struct.pack("<QBBH", time.monotonic_ns(), 1 if fn else 0, len(contacts), 0)
    for (i, x, y) in contacts: p += struct.pack("<BBBBII", i, 1, 1, 0, x, y)
    return pkt(0x9002, 0, p)

if os.path.exists(SOCK): os.unlink(SOCK)
srv = socket.socket(socket.AF_UNIX, socket.SOCK_SEQPACKET); srv.bind(SOCK); srv.listen(4)
env = dict(os.environ, T1_DASH_TEST_SOCKET=SOCK, T1_DASH_CONFIG="/nonexistent")
proc = subprocess.Popen([BIN], env=env)
srv.settimeout(5)
conn, _ = srv.accept(); conn.settimeout(0.1)
mem = None; frames = 0; keys = []; results = {}
script = [(1, "esc", lambda c: c.send(input_frame(False, [(1, 40, 30)]))),
          (2, "release", lambda c: c.send(input_frame(False, []))),
          (3, "fn-down", lambda c: c.send(input_frame(True, []))),
          (5, "fn+F5", lambda c: c.send(input_frame(True, [(2, 900, 30)]))),
          (6, "fn-up", lambda c: c.send(input_frame(False, [])))]
start = time.time(); step = 0
try:
    while time.time() - start < 6:
        try:
            msg, anc, _, _ = conn.recvmsg(65536, socket.CMSG_SPACE(4))
        except socket.timeout:
            msg = None
        if msg:
            magic, major, t, ln, rid = HDR.unpack_from(msg)
            if t == 0x0001:  # Hello
                conn.send(pkt(0x8001, rid, struct.pack("<HHIIII", 0, 0, W, H, 1, 3)))
            elif t == 0x0002:  # RegisterBuffer
                fds = array.array("i"); [fds.frombytes(d[:4]) for (_, _, d) in anc]
                mem = mmap.mmap(fds[0], W * H * 4, prot=mmap.PROT_READ, flags=mmap.MAP_SHARED)
                conn.send(pkt(0x8002, rid))
            elif t == 0x0003:  # SubmitFrame
                buf, fid = struct.unpack_from("<IQ", msg, 16)
                conn.send(pkt(0x8002, rid)); frames += 1
                if frames == 1: png(f"{OUT}/fake-hw-first-frame.png", mem[:])
                if step >= 3 and "fn" not in results: png(f"{OUT}/fake-hw-fn-frame.png", mem[:]); results["fn"] = True
                conn.send(pkt(0x9001, 0, struct.pack("<IQ", buf, fid)))
            elif t == 0x0004:  # TapKeys
                n = msg[16]; keys += [struct.unpack_from("<H", msg, 20 + 2 * i)[0] for i in range(n)]
                conn.send(pkt(0x8002, rid))
            else:
                conn.send(pkt(0x8002, rid))
        el = time.time() - start
        while frames and step < len(script) and el >= script[step][0] * 0.6:
            script[step][2](conn); step += 1
finally:
    proc.terminate(); proc.wait(5)
print(f"frames={frames} keys={keys}")
ok = frames >= 2 and keys[:2] == [1, 63]
print("PASS" if ok else "FAIL"); sys.exit(0 if ok else 1)
