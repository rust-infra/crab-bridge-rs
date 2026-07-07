#!/usr/bin/env python3
"""Generate CrabBridge desktop icons (RGBA PNG)."""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "crabbridge-desktop" / "icons"


def png(w: int, h: int, pixels: bytes) -> bytes:
    rows = []
    stride = w * 4
    for y in range(h):
        rows.append(b"\x00" + pixels[y * stride : (y + 1) * stride])
    data = b"".join(rows)
    compressed = zlib.compress(data, 9)

    def chunk(tag: bytes, payload: bytes) -> bytes:
        crc = zlib.crc32(tag + payload) & 0xFFFFFFFF
        return struct.pack(">I", len(payload)) + tag + payload + struct.pack(">I", crc)

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", compressed) + chunk(b"IEND", b"")


def render(size: int) -> bytes:
    pixels = bytearray(size * size * 4)
    cx = cy = (size - 1) / 2
    radius = size * 0.42

    for y in range(size):
        for x in range(size):
            dx = x - cx
            dy = y - cy
            dist = math.hypot(dx, dy)
            idx = (y * size + x) * 4

            if dist > radius:
                pixels[idx : idx + 4] = bytes([0, 0, 0, 0])
                continue

            # Orange crab shell gradient
            t = dist / radius
            r = int(255 - 35 * t)
            g = int(120 - 40 * t)
            b = int(45 + 10 * t)
            a = 255

            # Bridge arch (white)
            arch_y = cy + size * 0.05
            arch_w = size * 0.34
            arch_h = size * 0.18
            nx = (x - cx) / arch_w
            ny = (y - arch_y) / arch_h
            if abs(nx) <= 1 and -1 <= ny <= 0 and (nx * nx + (ny + 0.35) ** 2) <= 1.05:
                r, g, b = 255, 255, 255

            # Eyes
            for ex in (-size * 0.12, size * 0.12):
                if math.hypot(x - (cx + ex), y - (cy - size * 0.12)) <= size * 0.045:
                    r, g, b = 30, 30, 30

            pixels[idx : idx + 4] = bytes([r, g, b, a])

    return bytes(pixels)


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    for name, size in [
        ("icon.png", 512),
        ("128x128@2x.png", 256),
        ("128x128.png", 128),
        ("32x32.png", 32),
    ]:
        path = ROOT / name
        path.write_bytes(png(size, size, render(size)))
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
