"""The box model: padding, decoration and size rules of a layout box.

A box keeps its padding between its edge and its content, draws its own
background, border, rounded corners and shadow, and takes its size from its
content, a length, a share of its parent or the free space. Each segment
covers one part of the model, and each label is the code that differs
between the boxes.
"""

import os

from gaanim import BoxStyle, Scene, Zones, stagger

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
header = scene.canvas.color("header")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
dark = scene.canvas.color("background")
seeks = []


def columns(count, split):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1]) for index in range(count)]


def example_width(split):
    """The width of the example column of a table with ``split``, in scene units."""
    return columns(1, split)[0][1].width


def table(variants, split):
    """One row per (label, box): the code on the left, the box it builds on the right."""
    entrances = []
    for (label_zone, zone), (label, box) in zip(columns(len(variants), split), variants):
        code = scene.text(label, role="code").place(label_zone, anchor="right")
        box.place(zone, anchor="left")
        entrances += [code.animate.fade_in().duration(0.4), box.animate.fade_in().duration(0.4)]
    start = scene.cursor
    scene.play(stagger(*entrances, each=0.04))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


# Padding is the space between a box's edge and its content, written like
# CSS: one value, (vertical, horizontal), (top, horizontal, bottom) or all
# four sides. Lengths are scene units, "Npx" or theme tokens.
scene.segment("padding")
SPLIT = ("3fr", "2fr")
frame = BoxStyle(background=header, border=rule, width=example_width(SPLIT))
content = BoxStyle(width="fill", padding=("16px", 0), align="center", background=accent, color=dark,
                   font_size="72px")


def padded(padding):
    """A content block that fills a framed box with ``padding``."""
    return L.box(L.box("content", style=content), style=frame, padding=padding)


table(
    [
        ("padding=0", padded(0)),
        ('padding="24px"', padded("24px")),
        ('padding="space_lg"', padded("space_lg")),
    ],
    SPLIT,
)

scene.segment("padding_sides")
table(
    [
        ('padding=("12px", "96px")', padded(("12px", "96px"))),
        ('padding=("8px", "32px", "64px")', padded(("8px", "32px", "64px"))),
        ('padding=("8px", "32px", "64px", "128px")', padded(("8px", "32px", "64px", "128px"))),
    ],
    SPLIT,
)

# A box draws its own background, border and rounded corners.
scene.segment("background")
SPLIT = ("8fr", "7fr")
tile = BoxStyle(width=example_width(SPLIT), height="120px", align="center", justify="center", font_size="56px")
table(
    [
        ("background=header", L.box("Box", style=tile, background=header)),
        ("border=rule", L.box("Box", style=tile, border=rule)),
        ('border=accent, border_width="6px"', L.box("Box", style=tile, border=accent, border_width="6px")),
        ('background=header, radius="24px"', L.box("Box", style=tile, background=header, radius="24px")),
        ('background=header, radius="full"', L.box("Box", style=tile, background=header, radius="full")),
    ],
    SPLIT,
)

# A shadow is a color blurred and offset under the box (shadow=True takes
# the theme's shadow color); clip=True cuts the content at the box's rounded
# edge.
scene.segment("shadow_and_clip")
SPLIT = ("3fr", "2fr")
tile = tile.but(width=example_width(SPLIT), height="160px")
table(
    [
        ("shadow=True", L.box("Box", style=tile, background=header, shadow=True)),
        ('shadow={"color": accent, "blur": "32px"}',
         L.box("Box", style=tile, background=header, shadow={"color": accent, "blur": "32px"})),
        ('shadow={"color": rule, "blur": 0}', L.box("Box", style=tile, background=header,
                                                     shadow={"color": rule, "blur": 0})),
        ("clip=True", L.stack(scene.geometry.circle(1.1).item(anchor="top"), style=tile, background=header,
                              radius="24px", clip=True)),
    ],
    SPLIT,
)

# Sizes: "hug" the content (the default), a length, a share of the parent
# or "fill" the free space.
scene.segment("width")
SPLIT = ("5fr", "11fr")
lane = BoxStyle(width=example_width(SPLIT), padding="8px", background=panel, border=rule)
chip = BoxStyle(padding=("8px", "24px"), background=accent, color=dark, font_size="56px")
table(
    [
        ('width="hug"', L.row(L.box("hug", style=chip, width="hug"), style=lane)),
        ('width="360px"', L.row(L.box("360px", style=chip, width="360px"), style=lane)),
        ('width="50%"', L.row(L.box("50%", style=chip, width="50%"), style=lane)),
        ('width="fill"', L.row(L.box("fill", style=chip, width="fill"), style=lane)),
    ],
    SPLIT,
)

# min and max bounds, a fixed height and an aspect ratio.
scene.segment("bounds")
SPLIT = ("1fr", "1fr")
lane = lane.but(width=example_width(SPLIT))
table(
    [
        ('min_width="560px"', L.row(L.box("min", style=chip, min_width="560px"), style=lane)),
        ('max_width="480px"', L.row(L.box("max_width wraps a long line", style=chip, max_width="480px"),
                                    style=lane)),
        ('height="160px"', L.row(L.box("160px", style=chip, height="160px"), style=lane)),
        ('width="480px", aspect_ratio=4', L.row(L.box("4:1", style=chip, width="480px", aspect_ratio=4),
                                                style=lane)),
    ],
    SPLIT,
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
