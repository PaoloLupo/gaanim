"""Draw the question images into assets/images (standard library only).

    python tools/make_images.py
"""

import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "assets" / "images"


def png(path, width, height, pixels):
    rows = b"".join(b"\x00" + bytes(pixels[y * width * 3:(y + 1) * width * 3]) for y in range(height))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))
    path.write_bytes(data)


def hex_rgb(color):
    return [int(color[i:i + 2], 16) for i in (1, 3, 5)]


def saturn():
    """A ringed planet over a starry sky."""
    width, height = 960, 540
    sky, band_a, band_b = hex_rgb("#0b0620"), hex_rgb("#e8c58a"), hex_rgb("#c9a063")
    ring, cx, cy, r = hex_rgb("#d9c7a0"), width / 2, height / 2, 150
    tilt = 0.32  # the ring seen edge-on: a flattened ellipse
    pixels = bytearray()

    def ring_cover(x, y):
        # "Elliptic" distance to the center, turned a little.
        a = -0.35
        u = (x - cx) * math.cos(a) - (y - cy) * math.sin(a)
        v = ((x - cx) * math.sin(a) + (y - cy) * math.cos(a)) / tilt
        d = math.hypot(u, v)
        return 1.0 if 190 < d < 300 and not 238 < d < 248 else 0.0, v

    for y in range(height):
        for x in range(width):
            color = sky
            star = ((x * 73856093) ^ (y * 19349663)) % 2003
            if star < 3:
                color = hex_rgb("#fff8ff")
            on_ring, v = ring_cover(x, y)
            d = math.hypot(x - cx, y - cy)
            planet = d < r
            if on_ring and v < 0:  # the back half of the ring, hidden by the planet
                color = ring if not planet else color
            if planet:
                band = math.sin((y - cy) / r * 9) > 0
                shade = 0.55 + 0.45 * max(0.0, 1 - d / r) ** 0.5
                color = [round(c * shade) for c in (band_a if band else band_b)]
            if on_ring and v >= 0:  # the front half passes over the planet
                color = ring
            pixels += bytes(color)
    OUT.mkdir(parents=True, exist_ok=True)
    png(OUT / "saturn.png", width, height, pixels)


def flag():
    """The flag of Bangladesh: a red disc shifted a little toward the hoist."""
    width, height = 900, 540
    green, red = hex_rgb("#006a4e"), hex_rgb("#f42a41")
    cx, cy, r = width * 0.45, height / 2, width * 0.2  # official proportions
    pixels = bytearray()
    for y in range(height):
        for x in range(width):
            # Soft edge: blend by the distance to the circle.
            edge = min(max(r - math.hypot(x + 0.5 - cx, y + 0.5 - cy) + 0.5, 0), 1)
            pixels += bytes(round(g + (c - g) * edge) for g, c in zip(green, red))
    OUT.mkdir(parents=True, exist_ok=True)
    png(OUT / "flag.png", width, height, pixels)


if __name__ == "__main__":
    saturn()
    flag()
    print("done:", OUT)
