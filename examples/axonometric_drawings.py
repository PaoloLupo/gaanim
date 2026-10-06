"""Axonometric drawings of 3D models, and the cases a painter's order finds hard.

Left: a small building (floor, walls, an L-shaped slab, columns and beams)
depth-sorted in isometric and moved through the dimetric, plan, front, side
and an arbitrary view and back. Middle: three slats that overlap in a cycle
and two walls that cross, which no draw order can show exactly, also moved to
plan. Right: degenerate input (collinear and repeated points, a line of zero
length), a closed polyline, a wire cube labelled in a layer above, and a box
seen from below.

Set GAANIM_SNAPSHOTS to capture exact seeks for visual regression.
"""

import math
import os

from gaanim import Scene, parallel

scene = Scene(frame=(16, 9), background="#f5f1ea")
scene.z_layers("model", "labels")

INK = "#2b2b2b"
FLOOR = "#d8d2c6"
WALL = "#c46a45"
WALL_SIDE = "#9c4f32"
SLAB = "#9aa3ad"
STEEL = "#4e535c"


def face(projection, points, color):
    return projection.polygon(points).fill(color).stroke(INK, 0.02)


def box(projection, x0, y0, z0, x1, y1, z1, colors):
    """The six faces of a box, bottom, top, then its four sides."""
    bottom, top, side_x, side_y = colors
    return [
        face(projection, [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0)], bottom),
        face(projection, [(x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)], top),
        face(projection, [(x0, y0, z0), (x0, y1, z0), (x0, y1, z1), (x0, y0, z1)], side_x),
        face(projection, [(x1, y0, z0), (x1, y1, z0), (x1, y1, z1), (x1, y0, z1)], side_x),
        face(projection, [(x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1)], side_y),
        face(projection, [(x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1)], side_y),
    ]


# --- Left: a small building -------------------------------------------------
BUILDING = dict(origin=(-6.6, -1.6), scale=0.55)
building = scene.geometry.axonometric("isometric", **BUILDING)
H = 2.4
drawings = [
    face(building, [(0, 0, 0), (4, 0, 0), (4, 3, 0), (0, 3, 0)], FLOOR),
    # Back walls, on the far planes, and a front wall with a gap.
    face(building, [(0, 3, 0), (4, 3, 0), (4, 3, H), (0, 3, H)], WALL),
    face(building, [(0, 0, 0), (0, 3, 0), (0, 3, H), (0, 0, H)], WALL_SIDE),
    face(building, [(0, 0, 0), (1.5, 0, 0), (1.5, 0, H), (0, 0, H)], WALL),
    face(building, [(2.5, 0, 0), (4, 0, 0), (4, 0, H), (2.5, 0, H)], WALL),
    # An L-shaped slab: not convex.
    face(building, [(0, 0, H), (4, 0, H), (4, 1.5, H), (2, 1.5, H), (2, 3, H), (0, 3, H)], SLAB),
]
columns = [building.line((x, y, 0), (x, y, H + 0.6)).stroke(STEEL, 0.05) for x, y in [(4, 3), (4, 1.5), (2, 3)]]
beams = [
    building.polyline([(2, 3, H + 0.6), (4, 3, H + 0.6), (4, 1.5, H + 0.6)]).no_fill().stroke(STEEL, 0.05),
]
building.depth_sort(drawings + columns + beams, z_index=10)

# --- Middle: overlaps no draw order can show exactly ------------------------
PUZZLE = dict(origin=(-1.8, -1.4), scale=0.7)
puzzle = scene.geometry.axonometric("isometric", **PUZZLE)
slats = []
corners = [(0.0, 0.0), (3.0, 0.0), (1.5, 2.6)]
for index, color in enumerate(["#e4572e", "#29335c", "#f3a712"]):
    (ax, ay), (bx, by) = corners[index], corners[(index + 1) % 3]
    length = math.hypot(bx - ax, by - ay)
    ux, uy = (bx - ax) / length, (by - ay) / length
    nx, ny = -uy * 0.2, ux * 0.2
    # Each slat rises along its length, so each lies on the next at one end.
    start = (ax - ux * 0.4, ay - uy * 0.4, 0.0)
    end = (bx + ux * 0.4, by + uy * 0.4, 1.0)
    slats.append(face(puzzle, [
        (start[0] - nx, start[1] - ny, start[2]),
        (end[0] - nx, end[1] - ny, end[2]),
        (end[0] + nx, end[1] + ny, end[2]),
        (start[0] + nx, start[1] + ny, start[2]),
    ], color))
crossing = [
    face(puzzle, [(0.6, 3.6, 0), (3.6, 3.6, 0), (3.6, 3.6, 1.2), (0.6, 3.6, 1.2)], "#7d9d9c"),
    face(puzzle, [(2.1, 2.4, 0), (2.1, 4.8, 0), (2.1, 4.8, 1.2), (2.1, 2.4, 1.2)], "#576f72"),
]
puzzle.depth_sort(slats + crossing, z_index=10)

# --- Right: degenerate input, a labelled wire cube and a box from below -----
WIRE = dict(origin=(2.0, 0.6), scale=0.6)
wire = scene.geometry.axonometric("isometric", **WIRE)
# Collinear points, a repeated point and a line of zero length.
wire.polygon([(0, 0, 0), (1, 0, 0), (2, 0, 0)]).fill("#e4572e").stroke(INK, 0.04)
wire.polygon([(0, 1, 0), (0, 1, 0), (1, 1, 0), (1, 2, 0)]).fill("#f3a712").stroke(INK, 0.02)
wire.line((2, 2, 0), (2, 2, 0)).stroke(INK, 0.08)
wire.polyline([(3, 0, 0), (4, 0, 0), (4, 1, 0), (3, 1, 0)], closed=True).no_fill().stroke("#29335c", 0.04)
edges = []
cube = [(x, y, z) for x in (0, 1) for y in (0, 1) for z in (0, 1)]
for a in cube:
    for b in cube:
        if a < b and sum(abs(p - q) for p, q in zip(a, b)) == 1:
            edges.append(wire.line(tuple(c + 2.5 for c in a[:2]) + (a[2] + 1.5,), tuple(c + 2.5 for c in b[:2]) + (b[2] + 1.5,)).stroke(INK, 0.03))
wire.depth_sort(edges, z_index=10)
for name, corner in [("A", (2.5, 2.5, 1.5)), ("B", (3.5, 3.5, 2.5))]:
    scene.text(name).fill(INK).scale_to(0.6).move_to(*wire.point(*corner)).z_layer("labels")

BELOW = dict(origin=(4.2, -2.4), scale=0.6)
below = scene.geometry.axonometric("isometric", elevation=-0.5, **BELOW)
below.depth_sort(box(below, 0, 0, 0, 2, 1.5, 1, ["#e4572e", SLAB, WALL_SIDE, FLOOR]), z_index=10)

# --- Views ------------------------------------------------------------------
views = [
    ("dimetric", {}),
    ("plan", {}),
    ("front", {}),
    ("side", {}),
    ("isometric", {"azimuth": 0.3, "elevation": 0.9}),
    ("isometric", {}),
]
scene.wait(0.5)
for view, angles in views:
    targets = [building.animate_to(scene.geometry.axonometric(view, **angles, **BUILDING))]
    if view in ("plan", "isometric") and not angles:
        targets.append(puzzle.animate_to(scene.geometry.axonometric(view, **PUZZLE)))
    scene.play(parallel(*targets).defaults(duration=1.0))
    scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.25, 1.0, 1.75, 3.25, 4.75, 6.25, 7.75, 9.25])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
