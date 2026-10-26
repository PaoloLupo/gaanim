"""Colors: CSS strings, Color constructors, named constants, theme roles and the Tailwind palette.

Every swatch is labelled with the code that makes its color. A light bar
behind the right half of a swatch shows through translucent colors, and the
numbers beside it are the color's 8-bit channels, read back with ``.r``,
``.g``, ``.b`` and ``.a``. The last segments lay out the named constants, the
semantic roles of the default theme and the bundled Tailwind CSS palette.
"""

import os

from gaanim import (
    BLACK,
    BLUE,
    CORAL,
    CYAN,
    GOLD,
    GRAY,
    GREEN,
    NAVY,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    WHITE,
    YELLOW,
    Color,
    Scene,
    Zones,
    colors,
    stagger,
)

GROW = 0.5
SPREAD = 0.5
HOLD = 1.0

scene = Scene()
foreground = scene.colors.foreground
muted = scene.colors.muted


def label(code):
    """A variant label: the code, in the code role, which reads literally."""
    return scene.text(code, role="code")


def swatch(zone, paint, outline=None):
    """A rounded swatch filling ``zone``; ``outline`` strokes it, for colors close to the background."""
    shape = scene.geometry.rounded_rect(zone.width, zone.height, 0.06).fill(paint).no_stroke()
    if outline is not None:
        shape.stroke(outline, 0.02, align="center")
    return shape.place(zone)


def enter(shapes):
    """Grow ``shapes`` in one after another, then hold them."""
    scene.play(stagger(*[shape.animate.grow_from_center().duration(GROW) for shape in shapes], total=SPREAD))
    scene.wait(HOLD)


def channels(variants):
    """One row per (label, color): the label, a swatch over a light bar, and the color's channels."""
    grid = scene.layout.zones(
        Zones.grid(
            rows=len(variants) + 1, columns=[7.7, "1fr", 3.4], column_gap=0.3, row_gap="16px",
            padding="page_padding",
        )
    )
    label(".r  .g  .b  .a").fill(muted).place(grid[2], anchor="right")
    shapes = []
    for index, (code, color) in enumerate(variants, start=1):
        label(code).place(grid[3 * index], anchor="right")
        stage = grid[3 * index + 1]
        # A light bar behind the swatch's right half shows through translucent colors.
        inner = stage.inset((stage.height * 0.1, stage.width * 0.04))
        scene.geometry.rect(inner.width * 0.45, inner.height * 0.85).fill(foreground).place(inner, anchor="right")
        shapes.append(swatch(stage, color))
        value = Color(color)
        readout = f"{value.r:>3} {value.g:>3} {value.b:>3} {value.a:>3}"
        label(readout).fill(muted).place(grid[3 * index + 2], anchor="right")
    enter(shapes)


# A string is any CSS Color 4 color: hex with an optional alpha digit pair,
# a named color, or an rgb(), hsl(), hwb(), lab(), lch(), oklab() or oklch()
# function. Color(...) parses it once; fill() and stroke() take the string too.
scene.segment("css")
channels([
    ('Color("#2563eb")', Color("#2563eb")),
    ('Color("#2563eb80")', Color("#2563eb80")),
    ('Color("tomato")', Color("tomato")),
    ('Color("rgb(37 99 235 / 65%)")', Color("rgb(37 99 235 / 65%)")),
    ('Color("hsl(160 84% 39%)")', Color("hsl(160 84% 39%)")),
    ('Color("hwb(200 10% 20%)")', Color("hwb(200 10% 20%)")),
    ('Color("lch(60% 50 30)")', Color("lch(60% 50 30)")),
    ('Color("oklch(70% 0.19 48)")', Color("oklch(70% 0.19 48)")),
])

# Channels as 0-255 integers, HSL with 0-1 saturation and lightness, or
# perceptual OKLCH, whose lightness steps look even. Wherever a color is
# expected, an (r, g, b) or (r, g, b, a) tuple works as well.
scene.segment("constructors")
channels([
    ("Color(128, 51, 204)", Color(128, 51, 204)),
    ("Color(128, 51, 204, 128)", Color(128, 51, 204, 128)),
    ("(20, 184, 166)", (20, 184, 166)),
    ('Color.from_hex("#ff8800")', Color.from_hex("#ff8800")),
    ("Color.from_rgb(20, 184, 166)", Color.from_rgb(20, 184, 166)),
    ("Color.from_rgba(255, 255, 255, 64)", Color.from_rgba(255, 255, 255, 64)),
    ("Color.from_hsl(215, 0.9, 0.55, 0.8)", Color.from_hsl(215, 0.9, 0.55, 0.8)),
    ("Color.from_oklch(0.68, 0.17, 240)", Color.from_oklch(0.68, 0.17, 240)),
])

# The fifteen named constants. BLACK and NAVY nearly vanish on the dark
# theme, so every swatch here carries a thin muted outline.
scene.segment("constants")
CONSTANTS = [
    ("WHITE", WHITE), ("GRAY", GRAY), ("BLACK", BLACK), ("NAVY", NAVY), ("BLUE", BLUE),
    ("TEAL", TEAL), ("CYAN", CYAN), ("GREEN", GREEN), ("YELLOW", YELLOW), ("GOLD", GOLD),
    ("ORANGE", ORANGE), ("CORAL", CORAL), ("RED", RED), ("PINK", PINK), ("PURPLE", PURPLE),
]  # fmt: skip
grid = scene.layout.zones(Zones.grid(rows=3, columns=5, column_gap="48px", row_gap="36px", padding="page_padding"))
shapes = []
for cell, (name, color) in zip(grid, CONSTANTS):
    art, caption = cell.split(Zones.rows(["1fr", "48px"], gap="16px"))
    label(name).place(caption)
    shapes.append(swatch(art, color, outline=muted))
enter(shapes)

# Theme roles: scene.colors.accent returns the active theme's color, so a
# scene that switches themes keeps its meaning; scene.colors[role] reads a
# role named in a variable. primary and body are aliases of foreground,
# secondary of muted and border of rule. Each label is shortened from
# scene.colors.
scene.segment("theme_roles")
ROLES = [
    "background", "foreground", "primary", "body", "muted", "secondary", "title", "accent",
    "chart", "panel", "header", "rule", "border", "success", "warning", "danger",
]  # fmt: skip
grid = scene.layout.zones(
    Zones.grid(
        rows=8, columns=["5fr", "3fr", "5fr", "3fr"], column_gap="48px", row_gap="12px", padding="page_padding"
    )
)
shapes = []
for index, role in enumerate(ROLES):
    row, column = index % 8, index // 8
    label(f"colors.{role}").place(grid[4 * row + 2 * column], anchor="right")
    shapes.append(swatch(grid[4 * row + 2 * column + 1], scene.colors[role], outline=muted))
enter(shapes)


def tailwind(families):
    """One row of swatches per Tailwind family, from shade 50 to 950, under a header of shades."""
    shades = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950]
    grid = scene.layout.zones(
        Zones.grid(
            rows=len(families) + 1, columns=["4fr"] + ["1fr"] * len(shades), column_gap="8px", row_gap="6px",
            padding="page_padding",
        )
    )
    for index, shade in enumerate(shades, start=1):
        label(str(shade)).fill(muted).place(grid[index])
    shapes = []
    for row, family in enumerate(families, start=1):
        first = row * (len(shades) + 1)
        label(f"tailwind.{family}").place(grid[first].inset((0, 0.25, 0, 0)), anchor="right")
        for index, shade in enumerate(shades, start=1):
            shapes.append(swatch(grid[first + index], colors.tailwind[family][shade]))
    enter(shapes)


# colors.tailwind holds the 26 families of Tailwind CSS, each a scale from 50
# to 950: colors.tailwind.blue[500] or colors.tailwind["blue"][500].
FAMILIES = colors.tailwind.families()
scene.segment("tailwind_hues")
tailwind(FAMILIES[:13])
scene.segment("tailwind_neutrals")
tailwind(FAMILIES[13:])

STEP = GROW + SPREAD + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Growing in and settled, in each of the six segments.
    scene.snapshots(snapshots, [STEP * index + t for index in range(6) for t in (0.6, 1.6)])
else:
    scene.render()
