"""Dimensions: linear dimensions with symbols and readouts, the side they sit on, their style and parts.

``scene.mechanics.dimension`` draws a fixed technical dimension;
``dimension_between`` follows two endpoints (points, drawables or anchor
points) and can read their distance, scaled and formatted, or any reactive
value. Every endpoint below moves, so the readouts change as they play.
Each label is the code that builds the variant.
"""

import os

from gaanim import Anchor, Easing, Scene, Zones, computed

MOVE = 2.0
HOLD = 0.6

scene = Scene()
M = scene.mechanics
fg = scene.colors.foreground
accent = scene.colors.accent
muted = scene.colors.muted
panel = scene.colors.panel


def label(zone, text, anchor="right"):
    """A code label in ``zone``; the lines of a long one align to the left."""
    scene.text(text, role="code", text_align="left").place(zone, anchor=anchor)


def table(entries, columns=("5fr", "3fr")):
    """One row per (label, build): ``build(a, b)`` dimensions a fixed point a and a point b that moves away."""
    grid = scene.layout.zones(Zones.grid(rows=len(entries), columns=list(columns), column_gap="64px", padding="page_padding"))
    reach = scene.viz.parameter(0.45)
    parts = []
    for index, (text, build) in enumerate(entries):
        label(grid[2 * index], text)
        stage = grid[2 * index + 1]
        y = stage.center[1] - 0.35
        a = (stage.left + 0.1, y)
        b = scene.geometry.dot(0.06).fill(muted).move_to(computed(lambda f, x=stage.left, w=stage.width: x + f * w - 0.1, inputs=[reach]), y)
        parts += [scene.geometry.dot(0.06).fill(muted).move_to(*a), b, build(a, b, stage)]
    scene.play([part.animate.fade_in() for part in parts], duration=0.4)
    scene.play([reach.animate.set(1.0).duration(MOVE).easing(Easing.SMOOTH)])
    scene.wait(HOLD)


def cells(rows, columns, label_height):
    """A padded grid of cells, each split into a label strip and a stage."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows([label_height, "1fr"], gap="16px")) for zone in grid]


# dimension() draws once between fixed coordinates; dimension_between follows
# its endpoints. label is a symbol, show_value reads the distance times
# scale, and value reads any number or Parameter instead.
scene.segment("linear")
length = scene.viz.parameter(120.0)
table([
    ("dimension(x1, y, x2, y, 0.5)", lambda a, b, stage: M.dimension(stage.left + 0.1, a[1], stage.right - 0.1, a[1], 0.5)),
    ("dimension_between(a, b, 0.5)", lambda a, b, stage: M.dimension_between(a, b, 0.5)),
    ('dimension_between(a, b, 0.5, label="$L$")', lambda a, b, stage: M.dimension_between(a, b, 0.5, label="$L$")),
    ('dimension_between(a, b, 0.5,\n    show_value=True, unit="m")', lambda a, b, stage: M.dimension_between(a, b, 0.5, show_value=True, unit="m")),
    (
        'dimension_between(a, b, 0.5,\n    label="$L$", value=length,\n    format=".0f", unit="mm")',
        lambda a, b, stage: M.dimension_between(a, b, 0.5, label="$L$", value=length, format=".0f", unit="mm"),
    ),
    (
        'dimension_between(a, b, 0.5,\n    show_value=True, scale=100,\n    format=".0f", unit="cm")',
        lambda a, b, stage: M.dimension_between(a, b, 0.5, show_value=True, scale=100, format=".0f", unit="cm"),
    ),
])

# The sign of offset puts the dimension to the left (+) or right (-) of the
# a -> b direction; side= fixes it in scene terms even if the ends swap.
# Each box grows, and its corners carry the dimensions with it.
CORNERS = {"tl": Anchor.TOP_LEFT, "tr": Anchor.TOP_RIGHT, "bl": Anchor.BOTTOM_LEFT, "br": Anchor.BOTTOM_RIGHT}


def boxes(variants):
    """Label each cell and dimension a box in it whose corners are tl, tr, bl and br."""
    parts, grows = [], []
    for (label_zone, stage), (text, build) in zip(cells(1, 3, 2.2), variants):
        label(label_zone, text, anchor="bottom")
        box = scene.geometry.rect(2.6, 1.4).fill(panel).stroke(muted, 0.03).move_to(stage.center[0] - 0.3, stage.center[1])
        parts += [box, build({name: box.anchor_point(anchor) for name, anchor in CORNERS.items()})]
        grows.append(box.animate.scale_to(1.3).duration(MOVE).easing(Easing.SMOOTH))
    scene.play([part.animate.fade_in() for part in parts], duration=0.4)
    scene.play(grows)
    scene.wait(HOLD)


scene.segment("placement")
boxes([
    ("dimension_between(\n  tl, tr, 0.4,\n  show_value=True)", lambda c: M.dimension_between(c["tl"], c["tr"], 0.4, show_value=True)),
    ("dimension_between(\n  tl, tr, -0.4,\n  show_value=True)", lambda c: M.dimension_between(c["tl"], c["tr"], -0.4, show_value=True)),
    (
        'dimension_between(\n  br, bl, 0.4,\n  side="below",\n  show_value=True)',
        lambda c: M.dimension_between(c["br"], c["bl"], 0.4, side="below", show_value=True),
    ),
])

# A vertical or slanted dimension keeps its label upright, moved out by the
# label's half width; label_orientation="aligned" turns it with the line.
scene.segment("orientation")
boxes([
    (
        'dimension_between(\n  br, tr, 0.4,\n  side="right",\n  show_value=True)',
        lambda c: M.dimension_between(c["br"], c["tr"], 0.4, side="right", show_value=True),
    ),
    ("dimension_between(\n  bl, tr, 0.4,\n  show_value=True)", lambda c: M.dimension_between(c["bl"], c["tr"], 0.4, show_value=True)),
    (
        'dimension_between(\n  bl, tr, 0.4,\n  show_value=True,\n  label_orientation=\n    "aligned")',
        lambda c: M.dimension_between(c["bl"], c["tr"], 0.4, show_value=True, label_orientation="aligned"),
    ),
])

# Style: line width, dashed extension lines, color, type size and face. SHOWN
# holds the shared keywords: label="$L$", show_value=True, unit="m".
scene.segment("style")
SHOWN = {"label": "$L$", "show_value": True, "unit": "m"}
table(
    [
        ("dimension_between(a, b, 0.5, **SHOWN,\n    line_width=0.06)", lambda a, b, s: M.dimension_between(a, b, 0.5, **SHOWN, line_width=0.06)),
        (
            'dimension_between(a, b, 0.5, **SHOWN,\n    extension_style="dashed")',
            lambda a, b, s: M.dimension_between(a, b, 0.5, **SHOWN, extension_style="dashed"),
        ),
        (
            'dimension_between(a, b, 0.5, **SHOWN,\n    extension_style="dashed",\n    dash_length=0.06, gap_length=0.06)',
            lambda a, b, s: M.dimension_between(a, b, 0.5, **SHOWN, extension_style="dashed", dash_length=0.06, gap_length=0.06),
        ),
        ("dimension_between(a, b, 0.5, **SHOWN,\n    color=accent)", lambda a, b, s: M.dimension_between(a, b, 0.5, **SHOWN, color=accent)),
        ("dimension_between(a, b, 0.5, **SHOWN,\n    font_size=0.7)", lambda a, b, s: M.dimension_between(a, b, 0.5, **SHOWN, font_size=0.7)),
        (
            'dimension_between(a, b, 0.5, **SHOWN,\n    font="JetBrains Mono", weight=700)',
            lambda a, b, s: M.dimension_between(a, b, 0.5, **SHOWN, font="JetBrains Mono", weight=700),
        ),
    ],
    columns=("5fr", "3fr"),
)

# A Dimension's line, extensions, label, number and unit are drawables of
# their own.
scene.segment("parts")


def styled(style):
    """A dimension with SHOWN, restyled by ``style``."""

    def build(a, b, stage):
        dimension = M.dimension_between(a, b, 0.5, **SHOWN)
        style(dimension)
        return dimension

    return build


table([
    ("d.line.fill(accent)", styled(lambda d: d.line.fill(accent))),
    ("d.extensions.opacity(0.3)", styled(lambda d: d.extensions.opacity(0.3))),
    ("d.label.fill(accent)", styled(lambda d: d.label.fill(accent))),
    ("d.number.fill(accent)", styled(lambda d: d.number.fill(accent))),
    ("d.unit.fill(muted)", styled(lambda d: d.unit.fill(muted))),
])


# Every segment part-way through its motion and settled.
step = 0.4 + MOVE + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [step * index + t for index in range(5) for t in (0.4 + MOVE / 3, step - 0.1)])
else:
    scene.render()
