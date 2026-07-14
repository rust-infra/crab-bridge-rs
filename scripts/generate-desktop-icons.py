#!/usr/bin/env python3
"""Generate CrabBridge desktop icons (RGBA PNG + Windows ICO)."""

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


def ico(png_images: list[tuple[int, bytes]]) -> bytes:
    """Build a multi-size ICO container with embedded PNG payloads (Vista+).

    Each entry is ``(size, png_bytes)``. ICONDIRENTRY width/height must match the
    embedded PNG; a stored value of 0 means 256.
    """
    count = len(png_images)
    header = struct.pack("<HHH", 0, 1, count)
    # ICONDIRENTRY is 16 bytes; image data follows the directory.
    offset = 6 + 16 * count
    entries = bytearray()
    payload = bytearray()
    for size, data in png_images:
        if size < 1 or size > 256:
            raise ValueError(f"ICO size must be 1..=256, got {size}")
        # Width/height are one byte each; 0 means 256.
        dim = 0 if size == 256 else size
        entries.extend(
            struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
        )
        payload.extend(data)
        offset += len(data)
    return bytes(header + entries + payload)


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
    png_sizes = [
        ("icon.png", 512),
        ("128x128@2x.png", 256),
        ("128x128.png", 128),
        ("32x32.png", 32),
    ]
    for name, size in png_sizes:
        path = ROOT / name
        path.write_bytes(png(size, size, render(size)))
        print(f"wrote {path}")

    # Windows/macOS tauri-build decode ICONDIRENTRY sizes against PNG IHDR.
    ico_pngs = [
        (size, png(size, size, render(size))) for size in (256, 128, 64, 48, 32, 16)
    ]
    ico_path = ROOT / "icon.ico"
    ico_path.write_bytes(ico(ico_pngs))
    print(f"wrote {ico_path}")


if __name__ == "__main__":
    main()
