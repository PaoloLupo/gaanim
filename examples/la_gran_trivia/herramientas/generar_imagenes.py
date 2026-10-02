"""Dibuja las imágenes de las preguntas en assets/imagenes (solo biblioteca estándar).

    python herramientas/generar_imagenes.py
"""

import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "assets" / "imagenes"


def png(path, width, height, pixels):
    rows = b"".join(b"\x00" + bytes(pixels[y * width * 3:(y + 1) * width * 3]) for y in range(height))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))
    path.write_bytes(data)


def hex_rgb(color):
    return [int(color[i:i + 2], 16) for i in (1, 3, 5)]


def saturno():
    """Un planeta con anillos sobre un cielo estrellado."""
    width, height = 960, 540
    sky, band_a, band_b = hex_rgb("#0b0620"), hex_rgb("#e8c58a"), hex_rgb("#c9a063")
    ring, cx, cy, r = hex_rgb("#d9c7a0"), width / 2, height / 2, 150
    tilt = 0.32  # el anillo visto de lado: elipse aplastada
    pixels = bytearray()

    def ring_cover(x, y):
        # Distancia "elíptica" al centro, girada un poco.
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
            if on_ring and v < 0:  # la mitad de atrás del anillo, tapada por el planeta
                color = ring if not planet else color
            if planet:
                band = math.sin((y - cy) / r * 9) > 0
                shade = 0.55 + 0.45 * max(0.0, 1 - d / r) ** 0.5
                color = [round(c * shade) for c in (band_a if band else band_b)]
            if on_ring and v >= 0:  # la mitad de adelante pasa sobre el planeta
                color = ring
            pixels += bytes(color)
    OUT.mkdir(parents=True, exist_ok=True)
    png(OUT / "saturno.png", width, height, pixels)


def bandera():
    """La bandera de Bangladés: un disco rojo algo corrido hacia el asta."""
    width, height = 900, 540
    green, red = hex_rgb("#006a4e"), hex_rgb("#f42a41")
    cx, cy, r = width * 0.45, height / 2, width * 0.2  # proporciones oficiales
    pixels = bytearray()
    for y in range(height):
        for x in range(width):
            # Borde suave: mezcla según la distancia al círculo.
            edge = min(max(r - math.hypot(x + 0.5 - cx, y + 0.5 - cy) + 0.5, 0), 1)
            pixels += bytes(round(g + (c - g) * edge) for g, c in zip(green, red))
    OUT.mkdir(parents=True, exist_ok=True)
    png(OUT / "bandera.png", width, height, pixels)


if __name__ == "__main__":
    saturno()
    bandera()
    print("listo:", OUT)
