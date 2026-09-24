#!/usr/bin/env python3
"""Generate Gaanim's brand assets from their construction geometry.

Usage:
    python tools/generate_brand.py

Everything is drawn on one unit grid. The symbol is pixel art: three frames of
one tween (square -> squircle -> circle) stepping 2 units to the right, each
8x8 units, stacked back to front. The wordmark is vector geometry built from
the same unit: x-height 8 (the circle's diameter), stroke 2 (the frame step),
ascender and descender 4, so the lockup is exactly 16 units tall, like the icon.

Outputs are checked in under docs/assets/brand/ (see README.md there), so the
docs build does not need Pillow. The application icon is also written as a
Windows resource (gaanim-app.res, linked by the gaanim_editor and
gaanim_launcher build scripts) and as a Rust pixel table for the window icon
(crates/gaanim_editor/src/app_icon_pixels.rs). Rerun this script after changing
geometry or colors. Requires Pillow for the PNG rasters.
"""

from __future__ import annotations

import io
import math
import pathlib
import struct

from PIL import Image, ImageChops, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / "docs" / "assets" / "brand"
APP_ICON_RUST = ROOT / "crates" / "gaanim_editor" / "src" / "app_icon_pixels.rs"

# ---- Palette ------------------------------------------------------------------------------

INK = "#14112E"  # text on light, icon tile
PAPER = "#F7F6FF"  # text on dark
VIOLET = "#7C6CFF"  # brand color, middle frame
HAZE_LIGHT = "#CBC6FF"  # oldest frame over light backgrounds
HAZE_DARK = "#3F37A8"  # oldest frame over dark backgrounds
GOLD = "#FFC933"  # current frame and i-dot over dark backgrounds
GOLD_DEEP = "#F0A800"  # i-dot over light backgrounds
GRAPHITE = "#1A1A1A"  # neutral dark surface (docs hero, social card)

THEMES = {
    "light": {"frames": (HAZE_LIGHT, VIOLET, INK), "text": INK, "dot": GOLD_DEEP},
    "dark": {"frames": (HAZE_DARK, VIOLET, GOLD), "text": PAPER, "dot": GOLD},
}

# ---- Symbol: pixel frames -----------------------------------------------------------------

# Row widths of each 8x8 frame, horizontally centred. Back to front.
FRAMES = (
    (8, 8, 8, 8, 8, 8, 8, 8),  # square
    (6, 8, 8, 8, 8, 8, 8, 6),  # squircle
    (4, 6, 8, 8, 8, 8, 6, 4),  # circle
)
FRAME_SIZE = 8
FRAME_STEP = 2
SYMBOL_W = FRAME_SIZE + FRAME_STEP * (len(FRAMES) - 1)  # 12
TILE = (12, 14) + (16,) * 12 + (14, 12)  # 16x16 icon tile with stepped corners
TILE_SIZE = 16


def rows_to_pixels(widths, x0=0, y0=0):
    size = max(widths)
    return {
        (x0 + (size - w) // 2 + i, y0 + j)
        for j, w in enumerate(widths)
        for i in range(w)
    }


def frame_pixels(index, x0=0, y0=0):
    return rows_to_pixels(FRAMES[index], x0 + FRAME_STEP * index, y0)


def visible_frames(x0=0, y0=0):
    """Pixels each frame shows once the later frames are drawn over it."""
    shown, covered = [], set()
    for index in reversed(range(len(FRAMES))):
        pixels = frame_pixels(index, x0, y0) - covered
        covered |= pixels
        shown.append(pixels)
    return shown[::-1]


def trace(pixels):
    """Outline a pixel set as clockwise loops (holes come out counter-clockwise)."""
    edges = {}
    for x, y in pixels:
        for a, b in (((x, y), (x + 1, y)), ((x + 1, y), (x + 1, y + 1)),
                     ((x + 1, y + 1), (x, y + 1)), ((x, y + 1), (x, y))):
            if (b, a) in edges:
                del edges[(b, a)]
            else:
                edges[(a, b)] = True
    starts = {}
    for a, b in edges:
        starts.setdefault(a, []).append(b)
    loops = []
    while starts:
        first = next(iter(starts))
        loop, point = [first], first
        while True:
            nxt = starts[point].pop()
            if not starts[point]:
                del starts[point]
            if nxt == first:
                break
            loop.append(nxt)
            point = nxt
        # drop collinear vertices
        simple = [
            p for i, p in enumerate(loop)
            if (loop[i - 1][0] - p[0]) * (loop[(i + 1) % len(loop)][1] - p[1])
            != (loop[i - 1][1] - p[1]) * (loop[(i + 1) % len(loop)][0] - p[0])
        ]
        loops.append(simple)
    return loops


def pixel_path(pixels):
    return "".join(
        "M" + "L".join(f"{fmt(x)} {fmt(y)}" for x, y in loop) + "Z"
        for loop in sorted(trace(pixels))
    )


# ---- Wordmark: vector geometry ------------------------------------------------------------

X_HEIGHT = 8.0
STROKE = 2.0
OVERSHOOT = 0.12  # round tops/bottoms pass the x-height and baseline slightly
SPACE_STRAIGHT = 2.0  # stem to stem
SPACE_ROUND = 1.5  # stem to bowl
SYMBOL_GAP = 4.0  # symbol to wordmark in the lockup
ARC_STEPS = 96


def fmt(value):
    text = f"{value:.3f}".rstrip("0").rstrip(".")
    return "0" if text == "-0" else text


def arc_points(cx, cy, rx, ry, start, end, steps=ARC_STEPS):
    return [
        (cx + rx * math.cos(start + (end - start) * i / steps),
         cy + ry * math.sin(start + (end - start) * i / steps))
        for i in range(steps + 1)
    ]


class Rect:
    def __init__(self, x0, y0, x1, y1, role="text"):
        self.box, self.role = (x0, y0, x1, y1), role

    def moved(self, dx):
        x0, y0, x1, y1 = self.box
        return Rect(x0 + dx, y0, x1 + dx, y1, self.role)

    def svg(self):
        x0, y0, x1, y1 = map(fmt, self.box)
        return f"M{x0} {y0}H{x1}V{y1}H{x0}Z"

    def polygons(self):
        x0, y0, x1, y1 = self.box
        return [((x0, y0), (x1, y0), (x1, y1), (x0, y1))], []


class Bowl:
    """Full elliptical ring: overshoots vertically only, so stems stay flush."""

    role = "text"

    def __init__(self, cx, cy, rx, ry):
        self.c = (cx, cy, rx, ry)

    def moved(self, dx):
        cx, cy, rx, ry = self.c
        return Bowl(cx + dx, cy, rx, ry)

    def svg(self):
        cx, cy, rx, ry = self.c
        ix, iy = rx - STROKE, ry - STROKE
        f = fmt
        return (
            f"M{f(cx - rx)} {f(cy)}A{f(rx)} {f(ry)} 0 0 1 {f(cx + rx)} {f(cy)}"
            f"A{f(rx)} {f(ry)} 0 0 1 {f(cx - rx)} {f(cy)}Z"
            f"M{f(cx - ix)} {f(cy)}A{f(ix)} {f(iy)} 0 0 0 {f(cx + ix)} {f(cy)}"
            f"A{f(ix)} {f(iy)} 0 0 0 {f(cx - ix)} {f(cy)}Z"
        )

    def polygons(self):
        cx, cy, rx, ry = self.c
        outer = arc_points(cx, cy, rx, ry, 0, 2 * math.pi)
        inner = arc_points(cx, cy, rx - STROKE, ry - STROKE, 0, 2 * math.pi)
        return [outer], [inner]


class Arch:
    """Upper half of an elliptical ring (n and m shoulders)."""

    role = "text"

    def __init__(self, cx, cy, rx, ry):
        self.c = (cx, cy, rx, ry)

    def moved(self, dx):
        cx, cy, rx, ry = self.c
        return Arch(cx + dx, cy, rx, ry)

    def svg(self):
        cx, cy, rx, ry = self.c
        ix, iy = rx - STROKE, ry - STROKE
        f = fmt
        return (
            f"M{f(cx - rx)} {f(cy)}A{f(rx)} {f(ry)} 0 0 1 {f(cx + rx)} {f(cy)}"
            f"L{f(cx + ix)} {f(cy)}A{f(ix)} {f(iy)} 0 0 0 {f(cx - ix)} {f(cy)}Z"
        )

    def polygons(self):
        cx, cy, rx, ry = self.c
        outer = arc_points(cx, cy, rx, ry, math.pi, 2 * math.pi)
        inner = arc_points(cx, cy, rx - STROKE, ry - STROKE, 2 * math.pi, math.pi)
        return [outer + inner], []


class Hook:
    """Lower half of a circular ring, cut vertically at x = cut (the g tail)."""

    role = "text"

    def __init__(self, cx, cy, r, cut):
        assert abs(cut - cx) > r - STROKE, "the cut must clear the inner circle"
        self.c = (cx, cy, r, cut)

    def moved(self, dx):
        cx, cy, r, cut = self.c
        return Hook(cx + dx, cy, r, cut + dx)

    def _end(self):
        cx, cy, r, cut = self.c
        return math.acos((cut - cx) / r)

    def svg(self):
        cx, cy, r, cut = self.c
        ir, end = r - STROKE, self._end()
        f = fmt
        return (
            f"M{f(cx + r)} {f(cy)}A{f(r)} {f(r)} 0 0 1 {f(cx + r * math.cos(end))} {f(cy + r * math.sin(end))}"
            f"L{f(cut)} {f(cy)}L{f(cx - ir)} {f(cy)}A{f(ir)} {f(ir)} 0 0 0 {f(cx + ir)} {f(cy)}Z"
        )

    def polygons(self):
        cx, cy, r, cut = self.c
        outer = arc_points(cx, cy, r, r, 0, self._end())
        inner = arc_points(cx, cy, r - STROKE, r - STROKE, math.pi, 0)
        return [outer + [(cut, cy)] + inner], []


def glyphs():
    """Each glyph: (parts, advance, left side, right side); side 's' stem, 'r' round."""
    h, w, ov = X_HEIGHT, STROKE, OVERSHOOT
    r = h / 2
    bowl = Bowl(r, r, r, r + ov)
    m_arch = 7.0  # width of each m shoulder; its counters come out 3 wide (n: 4)
    mr = m_arch / 2
    return {
        "g": ([bowl, Rect(h - w, 0, h, 8.5), Hook(4.5, 8.5, 3.5, 1.5)], h, "r", "s"),
        "a": ([bowl, Rect(h - w, 0, h, h)], h, "r", "s"),
        "n": ([Rect(0, 0, w, h), Arch(r, r, r, r + ov), Rect(h - w, r, h, h)], h, "s", "s"),
        "i": ([Rect(0, 0, w, h), Rect(0, -2 * w, w, -w, role="dot")], w, "s", "s"),
        "m": ([Rect(0, 0, w, h), Arch(mr, mr, mr, mr + ov), Rect(m_arch - w, mr, m_arch, h),
               Arch(m_arch - w + mr, mr, mr, mr + ov),
               Rect(2 * m_arch - 2 * w, mr, 2 * m_arch - w, h)], 2 * m_arch - w, "s", "s"),
    }


def wordmark(x0=0.0):
    """Place 'gaanim'; returns (parts, width)."""
    table = glyphs()
    parts, x, prev = [], x0, None
    for ch in "gaanim":
        shapes, advance, left, right = table[ch]
        if prev is not None:
            x += SPACE_STRAIGHT if (prev, left) == ("s", "s") else SPACE_ROUND
        parts += [shape.moved(x) for shape in shapes]
        x += advance
        prev = right
    return parts, x - x0


# ---- SVG writers ----------------------------------------------------------------------------

def svg_doc(width, height, body, title, attrs=""):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {fmt(width)} {fmt(height)}"'
        f' width="{fmt(width * 2)}" height="{fmt(height * 2)}"{attrs} role="img"'
        f' aria-label="{title}">\n<title>{title}</title>\n{body}</svg>\n'
    )


def symbol_svg(colors, x0=0, y0=0, opacities=None):
    """Frames back to front. Opaque themes stack whole frames so no seams appear;
    the one-colour version draws only visible pixels so opacities do not add up."""
    out = []
    if opacities is None:
        for index, color in enumerate(colors):
            out.append(f'<path fill="{color}" d="{pixel_path(frame_pixels(index, x0, y0))}"/>')
    else:
        for pixels, opacity in zip(visible_frames(x0, y0), opacities):
            op = "" if opacity == 1 else f' fill-opacity="{opacity}"'
            out.append(f'<path fill="{colors[0]}"{op} d="{pixel_path(pixels)}"/>')
    return '<g shape-rendering="crispEdges">' + "".join(out) + "</g>\n"


def text_svg(parts, text_color, dot_color, dy):
    text = "".join(p.svg() for p in parts if p.role == "text")
    dot = "".join(p.svg() for p in parts if p.role == "dot")
    return (
        f'<g transform="translate(0 {fmt(dy)})">'
        f'<path fill="{text_color}" d="{text}"/>'
        f'<path fill="{dot_color}" shape-rendering="crispEdges" d="{dot}"/></g>\n'
    )


def lockup_svg(theme):
    parts, text_w = wordmark(SYMBOL_W + SYMBOL_GAP)
    width = SYMBOL_W + SYMBOL_GAP + text_w
    dy = (TILE_SIZE - X_HEIGHT) / 2  # x-height band centred on the 16-unit canvas
    if theme == "mono":
        body = symbol_svg(("currentColor",), 0, dy, opacities=(0.3, 0.6, 1)) + text_svg(
            parts, "currentColor", "currentColor", dy)
    else:
        th = THEMES[theme]
        body = symbol_svg(th["frames"], 0, dy) + text_svg(parts, th["text"], th["dot"], dy)
    return svg_doc(width, TILE_SIZE, body, "Gaanim")


def symbol_only_svg(theme):
    body = symbol_svg(THEMES[theme]["frames"])
    return svg_doc(SYMBOL_W, FRAME_SIZE, body, "Gaanim")


def icon_svg():
    x0 = (TILE_SIZE - SYMBOL_W) // 2
    y0 = (TILE_SIZE - FRAME_SIZE) // 2
    body = (
        f'<g shape-rendering="crispEdges"><path fill="{INK}" d="{pixel_path(rows_to_pixels(TILE))}"/></g>\n'
        + symbol_svg(THEMES["dark"]["frames"], x0, y0)
    )
    return svg_doc(TILE_SIZE, TILE_SIZE, body, "Gaanim")


# ---- Rasters --------------------------------------------------------------------------------

def rgb(hex_color):
    return tuple(int(hex_color[i:i + 2], 16) for i in (1, 3, 5))


def paint_pixels(image, pixels, color, scale, ox=0, oy=0):
    draw = ImageDraw.Draw(image)
    for x, y in pixels:
        draw.rectangle(
            [ox + x * scale, oy + y * scale, ox + (x + 1) * scale - 1, oy + (y + 1) * scale - 1],
            fill=rgb(color) + (255,),
        )


def icon_image(scale, tile=True):
    size = TILE_SIZE * scale
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    if tile:
        paint_pixels(image, rows_to_pixels(TILE), INK, scale)
    x0, y0 = (TILE_SIZE - SYMBOL_W) // 2, (TILE_SIZE - FRAME_SIZE) // 2
    for index, color in enumerate(THEMES["dark"]["frames"]):
        paint_pixels(image, frame_pixels(index, x0, y0), color, scale)
    return image


def icon_grid():
    """The 16x16 icon as palette indices (0 = transparent), back to front."""
    colors = (INK,) + THEMES["dark"]["frames"]
    grid = [[0] * TILE_SIZE for _ in range(TILE_SIZE)]
    x0, y0 = (TILE_SIZE - SYMBOL_W) // 2, (TILE_SIZE - FRAME_SIZE) // 2
    layers = [rows_to_pixels(TILE)] + [frame_pixels(i, x0, y0) for i in range(len(FRAMES))]
    for index, pixels in enumerate(layers, start=1):
        for x, y in pixels:
            grid[y][x] = index
    return colors, grid


def png_bytes(image):
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=True)
    return buffer.getvalue()


def ico_bytes(pngs):
    """ICO container with PNG-compressed entries (Windows Vista and later)."""
    header = struct.pack("<HHH", 0, 1, len(pngs))
    offset = len(header) + 16 * len(pngs)
    entries, data = b"", b""
    for size, png in pngs:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset)
        offset += len(png)
        data += png
    return header + entries + data


def res_bytes(pngs):
    """Win32 .res with one RT_GROUP_ICON (ordinal 1) over RT_ICON entries 1..n.

    MSVC link.exe and lld-link accept .res files as linker inputs directly.
    """

    def entry(kind, name, data, flags):
        header = struct.pack("<II", len(data), 32)
        header += struct.pack("<HHHH", 0xFFFF, kind, 0xFFFF, name)
        header += struct.pack("<IHHII", 0, flags, 0, 0, 0)
        return header + data + b"\0" * (-len(data) % 4)

    out = struct.pack("<II", 0, 32) + struct.pack("<HHHH", 0xFFFF, 0, 0xFFFF, 0) + bytes(16)
    group = struct.pack("<HHH", 0, 1, len(pngs))
    for number, (size, png) in enumerate(pngs, start=1):
        out += entry(3, number, png, 0x1010)  # RT_ICON, moveable | discardable
        group += struct.pack("<BBBBHHIH", size % 256, size % 256, 0, 0, 1, 32, len(png), number)
    out += entry(14, 1, group, 0x1030)  # RT_GROUP_ICON, moveable | pure | discardable
    return out


def app_icon_rust():
    colors, grid = icon_grid()
    palette = ",\n".join(
        "    [{}, {}, {}, {}]".format(*rgb(c), 255) for c in colors
    )
    rows = "\n".join('    *b"{}",'.format("".join(str(i) for i in row)) for row in grid)
    return f"""// Generated by tools/generate_brand.py; do not edit.

/// Side of the icon's pixel grid.
pub(crate) const ICON_GRID: usize = {TILE_SIZE};

/// RGBA colours for grid indices 1..; index 0 is transparent.
pub(crate) const ICON_PALETTE: [[u8; 4]; {len(colors)}] = [
{palette},
];

/// One ASCII digit per pixel, row by row: an index into the palette plus one.
pub(crate) const ICON_PIXELS: [[u8; {TILE_SIZE}]; {TILE_SIZE}] = [
{rows}
];
"""


def vector_mask(parts, size, scale, ox, oy, supersample=4):
    """Rasterise wordmark parts with the same geometry the SVG uses."""
    w, h = size[0] * supersample, size[1] * supersample
    s = scale * supersample
    total = Image.new("L", (w, h), 0)
    for part in parts:
        fills, holes = part.polygons()
        mask = Image.new("L", (w, h), 0)
        draw = ImageDraw.Draw(mask)
        for poly in fills:
            draw.polygon([((ox + x) * s, (oy + y) * s) for x, y in poly], fill=255)
        for poly in holes:
            draw.polygon([((ox + x) * s, (oy + y) * s) for x, y in poly], fill=0)
        total = ImageChops.lighter(total, mask)
    return total.resize(size, Image.LANCZOS)


def social_image():
    """1280x640 repository social preview: dark lockup on graphite."""
    width, height, scale = 1280, 640, 8
    image = Image.new("RGBA", (width, height), rgb(GRAPHITE) + (255,))
    parts, text_w = wordmark(SYMBOL_W + SYMBOL_GAP)
    lockup_w = SYMBOL_W + SYMBOL_GAP + text_w
    ox = (width - lockup_w * scale) // 2
    oy = (height - TILE_SIZE * scale) // 2 + (TILE_SIZE - FRAME_SIZE) // 2 * scale
    for index, color in enumerate(THEMES["dark"]["frames"]):
        paint_pixels(image, frame_pixels(index), color, scale, ox, oy)
    for role, color in (("text", PAPER), ("dot", GOLD)):
        mask = vector_mask([p for p in parts if p.role == role], (width, height), scale,
                           ox / scale, oy / scale)
        image.paste(Image.new("RGBA", (width, height), rgb(color) + (255,)), (0, 0), mask)
    return image.convert("RGB")


# ---- Main -----------------------------------------------------------------------------------

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    files = {
        "gaanim-logo.svg": lockup_svg("light"),
        "gaanim-logo-dark.svg": lockup_svg("dark"),
        "gaanim-logo-mono.svg": lockup_svg("mono"),
        "gaanim-symbol.svg": symbol_only_svg("light"),
        "gaanim-symbol-dark.svg": symbol_only_svg("dark"),
        "gaanim-icon.svg": icon_svg(),
    }
    for name, text in files.items():
        (OUT / name).write_text(text, encoding="utf-8", newline="\n")

    # Integer scales only: every pixel of the grid stays a whole number of device pixels.
    favicon = [(TILE_SIZE * s, png_bytes(icon_image(s))) for s in (1, 2, 3)]
    (OUT / "favicon.ico").write_bytes(ico_bytes(favicon))
    # Application icon: Explorer, taskbar and title bar sizes up to 256 px.
    app = [(TILE_SIZE * s, png_bytes(icon_image(s))) for s in (1, 2, 3, 4, 6, 8, 16)]
    (OUT / "gaanim-app.ico").write_bytes(ico_bytes(app))
    (OUT / "gaanim-app.res").write_bytes(res_bytes(app))
    APP_ICON_RUST.write_text(app_icon_rust(), encoding="utf-8", newline="\n")
    icon_image(32).save(OUT / "gaanim-icon-512.png")
    # Apple icons must be opaque; the system rounds the corners itself.
    apple = Image.new("RGBA", (180, 180), rgb(INK) + (255,))
    motif = icon_image(11, tile=False)
    apple.alpha_composite(motif, ((180 - motif.width) // 2, (180 - motif.height) // 2))
    apple.convert("RGB").save(OUT / "apple-touch-icon.png")
    social_image().save(OUT / "gaanim-social.png")

    for path in sorted(OUT.iterdir()):
        print(f"{path.relative_to(ROOT)}  {path.stat().st_size} B")


if __name__ == "__main__":
    main()
