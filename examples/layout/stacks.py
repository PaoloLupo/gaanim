"""Stacks: layers that share one area, placed by anchor and offset or fitted to it.

``L.stack`` overlaps its children in the same box. Each child sits by its
``anchor`` (the center by default), moved by an ``offset``, or scales to the
whole area with ``fit``; later children draw on top unless ``z_index`` says
otherwise. Each label is the code that places the layer beside it.
"""

import os

from gaanim import BoxStyle, Scene, stagger

scene = Scene()
L = scene.layout
accent = scene.colors.accent
panel = scene.colors.panel
rule = scene.colors.rule
dark = scene.colors.background
success = scene.colors.success
danger = scene.colors.danger
seeks = []

badge = BoxStyle(padding=("4px", "16px"), background=accent, color=dark)
stage = BoxStyle(width="480px", height="150px", background=panel, border=rule)


def code(label):
    return scene.text(label, role="code")


def show(*objects):
    """Fade the objects in one after another, hold, and note the seeks."""
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in objects], each=0.04))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


def table(rows, header=None):
    """Lay out rows of (label, *boxes) as one grid: the code on the left, the boxes to its right.

    ``header`` labels the box columns. The grid hugs its content and sits in
    the middle of the frame.
    """
    cells = []
    if header:
        cells += [L.box()] + [code(label).item(anchor="left") for label in header]
    for label, *boxes in rows:
        cells += [code(label).item(anchor="right")] + [box.item(anchor="left") for box in boxes]
    L.grid(*cells, columns=["auto"] * len(rows[0]), row_gap="24px", column_gap="48px", align="center")
    show(*cells)


# A layer sits by its anchor, one of nine points of the stack; each chip
# here is labelled with its own anchor.
scene.segment("anchors")
ANCHORS = ["top_left", "top", "top_right", "left", "center", "right", "bottom_left", "bottom", "bottom_right"]
layers = L.stack(
    *[L.box(f'anchor="{anchor}"', style=badge, role="code").item(anchor=anchor) for anchor in ANCHORS],
    background=panel, border=rule, width="fill", height="fill", padding="24px",
)
L.box(layers, within="frame", width="fill", height="fill", padding="page_padding")
show(layers)

# offset moves a layer from its anchor, x to the right and y up.
scene.segment("offsets")
table([
    ('item(anchor="top_right")', L.stack(L.box("badge", style=badge).item(anchor="top_right"), style=stage)),
    ('item(anchor="top_right", offset=(-0.2, -0.2))',
     L.stack(L.box("badge", style=badge).item(anchor="top_right", offset=(-0.2, -0.2)), style=stage)),
    ('item(anchor="bottom_left", offset=(0.2, 0.2))',
     L.stack(L.box("badge", style=badge).item(anchor="bottom_left", offset=(0.2, 0.2)), style=stage)),
    ('item(offset=("160px", 0))', L.stack(L.box("badge", style=badge).item(offset=("160px", 0)), style=stage)),
])

# fit scales a layer to the stack: "contain" fits it whole, "cover" fills
# the area and crops the rest, "stretch" fills it out of proportion and
# "scale_down" only ever shrinks. "none" keeps the layer's own size, and
# clip=True cuts what leaves the stack.
scene.segment("fit")
frame = BoxStyle(width="420px", height="150px", border=rule, clip=True)
table(
    [
        (f'item(fit="{fit}")', *[L.stack(scene.media.svg("assets/character.svg").scale_to(scale).item(fit=fit),
                                         style=frame) for scale in (1.0, 0.25)])
        for fit in ("none", "contain", "cover", "stretch", "scale_down")
    ],
    header=["svg", "svg.scale_to(0.25)"],
)

# Later layers draw on top; z_index lifts one above the others. A cover
# image, a caption and a badge make a card.
scene.segment("layers")
SQUARES = [(accent, (-0.25, 0.12)), (success, (0, 0)), (danger, (0.25, -0.12))]


def squares(lift=None):
    """Three overlapping squares, the one at index ``lift`` given z_index(1)."""
    layers = []
    for index, (color, offset) in enumerate(SQUARES):
        square = scene.geometry.square(0.8).fill(color).item(offset=offset)
        layers.append(square.z_index(1) if index == lift else square)
    return L.stack(*layers, style=stage)


table([
    ("L.stack(a, b, c)", squares()),
    ("L.stack(a.z_index(1), b, c)", squares(lift=0)),
    ('L.stack(svg.item(fit="cover"), caption, badge)', L.stack(
        scene.media.svg("assets/character.svg").item(fit="cover"),
        L.box("Character", background=dark, padding=("2px", "12px")).item(anchor="bottom_left",
                                                                          offset=("12px", "12px")),
        L.box("New", style=badge, radius="full").item(anchor="top_right", offset=("-12px", "-12px")),
        style=stage, height="300px", radius="16px", clip=True,
    )),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
