"""Geometry primitives: closed shapes, lines and marks drawn by scene.geometry.

Each segment draws one family of primitives with ``create()``. They keep the
default theme's look, shapes filled with the accent and lines stroked with the
foreground, and each label is the ``scene.geometry`` call that builds the
variant.
"""

import math
import os

from gaanim import Direction, Scene, Zones

DRAW = 1.2
HOLD = 0.6

scene = Scene()
accent = scene.canvas.color("accent")

# Polygons and point clouds take their vertices in scene coordinates.
TRIANGLE = [(-1.4, -1), (1.4, -1), (0.4, 1.2)]
RING = [(math.cos(k * math.tau / 12), math.sin(k * math.tau / 12)) for k in range(12)]
ZIGZAG = [(x * 0.6 - 2.1, 0.3 if x % 2 else -0.3) for x in range(8)]
TRAPEZOID = [(-2, -0.35), (2, -0.35), (1.4, 0.35), (-1.4, 0.35)]


def reveal(shapes):
    """Draw ``shapes`` together, then hold them."""
    scene.play([shape.animate.create() for shape in shapes], duration=DRAW)
    scene.wait(HOLD)


def gallery(variants, columns):
    """Place each (label, shape) pair in a grid cell, the shape above its label.

    ``columns`` are the grid's column tracks, wider where a label is longer.
    """
    rows = math.ceil(len(variants) / len(columns))
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    for zone, (label, shape) in zip(grid, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        shape.place(art)
    reveal([shape for _, shape in variants])


def table(variants):
    """Place each (label, shape) pair in a row, the label right-aligned to the left of its shape."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=["5fr", "3fr"], gap="24px", column_gap="48px", padding="page_padding")
    )
    for index, (label, shape) in enumerate(variants):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        shape.place(grid[2 * index + 1])
    reveal([shape for _, shape in variants])


# Round shapes are centered at the origin; place() moves them into their cell.
scene.segment("round")
gallery(
    [
        ("circle(1)", scene.geometry.circle(1)),
        ("ellipse(1.6, 1)", scene.geometry.ellipse(1.6, 1)),
        ("annulus(1, 0.6)", scene.geometry.annulus(1, 0.6)),
        ("sector(0, 0, 1, 1, 5)", scene.geometry.sector(0, 0, 1, 1, 5)),
        ("dot(0.15)", scene.geometry.dot(0.15)),
        ("points(RING, 0.08)", scene.geometry.points(RING, 0.08)),
    ],
    ["1fr", "1fr", "1fr"],
)

# Rectangles, arbitrary and regular polygons, and stars.
scene.segment("polygons")
gallery(
    [
        ("square(2)", scene.geometry.square(2)),
        ("rounded_rect(3.2, 2, 0.5)", scene.geometry.rounded_rect(3.2, 2, 0.5)),
        ("rect(3.2, 2)", scene.geometry.rect(3.2, 2)),
        ("polygon(TRIANGLE)", scene.geometry.polygon(TRIANGLE)),
        ("regular_polygon(6, 1.2)", scene.geometry.regular_polygon(6, 1.2)),
        ("star(5, 1.3, 0.55)", scene.geometry.star(5, 1.3, 0.55)),
    ],
    ["1fr", "1.3fr", "1fr"],
)

# Lines, arcs and polylines are born without fill: only their stroke shows.
# A line takes two endpoints, or a length and a direction to place it later.
scene.segment("lines")
table([
    ("line((-2, -0.3), (2, 0.3))", scene.geometry.line((-2, -0.3), (2, 0.3))),
    ("line(length=4)", scene.geometry.line(length=4)),
    ("line(length=0.8, direction=Direction.UP)", scene.geometry.line(length=0.8, direction=Direction.UP)),
    ("dashed_line((-2, 0), (2, 0))", scene.geometry.dashed_line((-2, 0), (2, 0))),
    ("arc(0, 0, 0.8, 0, math.pi)", scene.geometry.arc(0, 0, 0.8, 0, math.pi)),
    ("ellipse_arc(0, 0, 2, 0.7, 0, math.pi)", scene.geometry.ellipse_arc(0, 0, 2, 0.7, 0, math.pi)),
    ("polyline(ZIGZAG)", scene.geometry.polyline(ZIGZAG)),
    ("polyline(TRAPEZOID, closed=True)", scene.geometry.polyline(TRAPEZOID, closed=True)),
])

# Marks for diagrams. A brace's tip points to the right of its direction, and
# a negative height flips it; a cross and a right angle are strokes only.
scene.segment("marks")
table([
    ("brace(0, 0, 3, 0, 0.5)", scene.geometry.brace(0, 0, 3, 0, 0.5)),
    ("brace(0, 0, 3, 0, -0.5)", scene.geometry.brace(0, 0, 3, 0, -0.5)),
    ("checkmark(1)", scene.geometry.checkmark(1)),
    ("cross(0.9).stroke(accent, 0.08)", scene.geometry.cross(0.9).stroke(accent, 0.08)),
    ("right_angle(0.9).stroke(accent, 0.08)", scene.geometry.right_angle(0.9).stroke(accent, 0.08)),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawing and settled, in each of the four segments.
    scene.snapshots(snapshots, [(DRAW + HOLD) * index + t for index in range(4) for t in (0.5, 1.6)])
else:
    scene.render()
