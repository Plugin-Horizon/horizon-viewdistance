#!/usr/bin/env python3
"""Joins an offline-mode Horizon server as a bare protocol client and prints every distance packet.

  distance_probe.py <port> <seconds> [name]

Prints one line per ClientboundSetChunkCacheRadius (view distance) and ClientboundSetSimulationDistance
packet, with the seconds since the join. A server that never changes its distances prints nothing.
Needs the Conn class of suggest_probe.py from the Horizon repo's tooling; a copy is in this file.
"""
import hashlib, socket, struct, sys, time, uuid, zlib

SET_CHUNK_CACHE_RADIUS = 97
SET_SIMULATION_DISTANCE = 113
KEEP_ALIVE_IN, KEEP_ALIVE_OUT = 45, 28
PLAYER_LOADED = 44


def varint(n):
    n &= 0xFFFFFFFF
    out = b""
    while True:
        b, n = n & 0x7F, n >> 7
        out += bytes([b | 0x80]) if n else bytes([b])
        if not n:
            return out


def parse_varint(buf, i=0):
    n = shift = 0
    while True:
        b = buf[i]; i += 1
        n |= (b & 0x7F) << shift
        if not b & 0x80:
            return n, i
        shift += 7


def string(s):
    b = s.encode()
    return varint(len(b)) + b


def offline_uuid(name):
    h = bytearray(hashlib.md5(("OfflinePlayer:" + name).encode()).digest())
    h[6] = (h[6] & 0x0F) | 0x30
    h[8] = (h[8] & 0x3F) | 0x80
    return uuid.UUID(bytes=bytes(h))


class Conn:
    def __init__(self, port, name):
        self.s = socket.create_connection(("127.0.0.1", port), timeout=30)
        self.t = None
        self.buf = b""
        self.send(0, varint(777) + string("127.0.0.1") + struct.pack(">H", port) + varint(2))
        self.send(0, string(name) + offline_uuid(name).bytes)
        while True:
            pid, d = self.recv()
            if pid == 3:
                self.t = parse_varint(d)[0]
            elif pid == 2:
                break
            elif pid == 0:
                raise SystemExit(f"login refused: {d!r}")
        self.send(3)
        while True:
            pid, d = self.recv()
            if pid == 15:
                self.send(7, varint(0))
            elif pid == 4:
                self.send(4, d)
            elif pid == 5:
                self.send(5, d)
            elif pid == 3:
                self.send(3)
                break
            elif pid == 2:
                raise SystemExit(f"disconnected in configuration: {d!r}")
        self.s.settimeout(1.0)

    def exact(self, n):
        while len(self.buf) < n:
            c = self.s.recv(65536)
            if not c:
                raise EOFError
            self.buf += c
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def vi(self):
        n = shift = 0
        while True:
            b = self.exact(1)[0]
            n |= (b & 0x7F) << shift
            if not b & 0x80:
                return n
            shift += 7

    def send(self, pid, body=b""):
        data = varint(pid) + body
        if self.t is not None and self.t >= 0:
            data = varint(len(data)) + zlib.compress(data) if len(data) >= self.t else varint(0) + data
        self.s.sendall(varint(len(data)) + data)

    def recv(self):
        data = self.exact(self.vi())
        if self.t is not None and self.t >= 0:
            size, i = parse_varint(data)
            data = zlib.decompress(data[i:]) if size else data[i:]
        pid, i = parse_varint(data)
        return pid, data[i:]


if __name__ == "__main__":
    port, seconds = int(sys.argv[1]), float(sys.argv[2])
    c = Conn(port, sys.argv[3] if len(sys.argv) > 3 else "probe")
    c.send(PLAYER_LOADED)
    start = time.time()
    end = start + seconds
    while time.time() < end:
        try:
            pid, d = c.recv()
        except socket.timeout:
            continue
        except EOFError:
            print(f"{time.time() - start:7.1f}s  disconnected")
            break
        if pid == KEEP_ALIVE_IN:
            c.send(KEEP_ALIVE_OUT, d)
        elif pid == SET_CHUNK_CACHE_RADIUS:
            print(f"{time.time() - start:7.1f}s  view distance -> {parse_varint(d)[0]}", flush=True)
        elif pid == SET_SIMULATION_DISTANCE:
            print(f"{time.time() - start:7.1f}s  simulation distance -> {parse_varint(d)[0]}", flush=True)
