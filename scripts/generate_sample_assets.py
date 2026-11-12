#!/usr/bin/env python3
"""Regenerate the PNG artwork used by the sample documents.

The samples ship small, deliberately simple images so that the repository stays
lightweight and the artwork stays reproducible: running this script rewrites
`samples/scenes/art/*.png` byte-for-byte. It has no third-party dependencies and
emits 8-bit RGB PNGs with a single zlib stream.

Usage:
    python3 scripts/generate_sample_assets.py
"""

from __future__ import annotations

import math

import pathlib
import struct
import zlib

ART_DIR = pathlib.Path(__file__).resolve().parent.parent / "samples" / "scenes" / "art"


def write_png(path: pathlib.Path, width: int, height: int, rows: list[bytearray]) -> None:
    """Writes an 8-bit RGB PNG built from one bytearray per row."""
    raw = bytearray()
    for row in rows:
        raw.append(0)  # filter type: none
        raw.extend(row)

    def chunk(kind: bytes, payload: bytes) -> bytes:
        body = kind + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    data = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(data)


def backdrop(width: int = 960, height: int = 540) -> list[bytearray]:
    """Night sky with a soft horizon glow, matching the visual-novel sample."""
    rows: list[bytearray] = []
    for y in range(height):
        row = bytearray()
        for x in range(width):
            horizon = y / height
            sky = 12.0 + 50.0 * horizon**2
            glow = max(0.0, 1.0 - abs(horizon - 0.72) * 5.0)
            vignette = 1.0 - 0.45 * (
                ((x - width / 2) / (width / 2)) ** 2 + ((y - height / 2) / (height / 2)) ** 2
            )
            r = sky * 0.55 + 90.0 * glow * 0.5
            g = sky * 0.70 + 40.0 * glow * 0.35
            b = sky * 1.00 + 120.0 * glow * 0.20
            row.extend(
                (
                    max(0, min(255, int(r * vignette))),
                    max(0, min(255, int(g * vignette))),
                    max(0, min(255, int(b * vignette))),
                )
            )
        rows.append(row)
    return rows


def character(width: int = 420, height: int = 520) -> list[bytearray]:
    """Simple silhouette so the sample has a transparent-friendly foreground."""
    rows: list[bytearray] = []
    centre = width / 2
    for y in range(height):
        row = bytearray()
        torso = 90 + (y / height) * 150
        head_radius = 62
        head_centre = (height * 0.22, centre)
        for x in range(width):
            inside = False
            # Head.
            head_distance = math.hypot(y - head_centre[0], x - head_centre[1])
            if head_distance <= head_radius:
                inside = True
            # Body: a widening trapezoid below the head.
            if y > head_centre[0] + head_radius * 0.6 and abs(x - centre) <= torso * 0.5:
                inside = True
            if inside:
                shade = 0.75 + 0.25 * math.sin((y / height) * math.pi)
                row.extend((int(70 * shade), int(96 * shade), int(140 * shade)))
            else:
                row.extend((0, 0, 0))
        rows.append(row)
    return rows


def main() -> None:
    ART_DIR.mkdir(parents=True, exist_ok=True)
    write_png(ART_DIR / "backdrop.png", 960, 540, backdrop())
    write_png(ART_DIR / "character.png", 420, 520, character())
    print(f"wrote {ART_DIR / 'backdrop.png'}")
    print(f"wrote {ART_DIR / 'character.png'}")


if __name__ == "__main__":
    main()
