"""3D transforms: turning, moving and scaling drawables in depth with the _3d methods.

Each variant animates an extruded L, whose shape tells every turn apart, away
from the pose a faint copy keeps, and is labelled with the call chained on
``shape.animate``. A perspective camera frames the z = 0 plane exactly like
the 2D layout, so the zones place the shapes and depth shows as size.
"""

import math
import os

from gaanim import Scene, Zones

DURATION = 1.6
HOLD = 0.6
# The L's outline, a unit tall; extruded, it becomes a solid.
L_SHAPE = [(0, 0), (0.8, 0), (0.8, 0.28), (0.28, 0.28), (0.28, 1), (0, 1)]
# A slight turn about x and y, so the L's depth shows before it moves.
TILT = (0.35, -0.45, 0)

scene = Scene()

# At DISTANCE from the z = 0 plane, a vertical field of view that spans the
# frame's height makes that plane line up with the frame.
frame = scene.layout.zones(Zones.rows(1))[0]
DISTANCE = 20
scene.camera.perspective(fov_y=2 * math.atan(frame.height / 2 / DISTANCE))
scene.camera.look_at(eye=(0, 0, DISTANCE), target=(0, 0, 0))


def solid(zone):
    """An extruded L filling most of ``zone``'s height, centered in it and tilted."""
    size = zone.height * 0.75
    outline = scene.geometry.polygon(L_SHAPE).scale_to(size).move_to(*zone.center)
    return scene.geometry.extrude(outline, depth=size * 0.3, bevel=size * 0.03).rotate_to_3d(*TILT)


def pose(zone):
    """A solid L that moves, over a faint copy that keeps its starting pose."""
    solid(zone).opacity(0.2)
    return solid(zone)


def play(moves):
    """Hold the starting poses, play ``moves`` together, then hold the result."""
    scene.wait(HOLD)
    scene.play(moves, duration=DURATION)
    scene.wait(HOLD)


def table(entries):
    """One row per (label, change) entry, its code to the left of its L.

    ``change(shape, zone)`` returns the animation of the row's L.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(entries), columns=["1fr", "1fr"], gap="24px", column_gap="48px", padding="page_padding")
    )
    moves = []
    for index, (label, change) in enumerate(entries):
        scene.text(label, role="code", text_align="right").place(grid[2 * index], anchor="right")
        zone = grid[2 * index + 1]
        moves.append(change(pose(zone), zone))
    play(moves)


def gallery(entries):
    """One column per (label, change) entry, its code under its L."""
    columns = scene.layout.zones(Zones.columns(len(entries), gap="48px", padding="page_padding"))
    moves = []
    for column, (label, change) in zip(columns, entries):
        # The L sits in a band above the middle, with room to grow toward the camera.
        _, art, _, caption = column.split(Zones.rows(["1fr", 2.6, "1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        moves.append(change(pose(art), art))
    play(moves)


# A turn about one of the scene's axes.
scene.segment("axes")
table([
    ('rotate_by_3d("x", math.pi)', lambda shape, zone: shape.animate.rotate_by_3d("x", math.pi)),
    ('rotate_by_3d("y", math.pi)', lambda shape, zone: shape.animate.rotate_by_3d("y", math.pi)),
    ('rotate_by_3d("z", math.pi / 2)', lambda shape, zone: shape.animate.rotate_by_3d("z", math.pi / 2)),
])


# rotate_to_3d sets absolute angles about x, y and z. with_pivot_3d, called
# on the shape before the move, sets the scene point turns go about: here the
# foot of the L, which tips it over onto its base.
def about_foot(shape, zone):
    x, y = zone.center
    foot = (x, y - zone.height * 0.375)
    return shape.with_pivot_3d(*foot, 0).animate.rotate_by_3d("x", -math.pi / 2)


scene.segment("orient")
table([
    ("rotate_to_3d(0, 0, 0)", lambda shape, zone: shape.animate.rotate_to_3d(0, 0, 0)),
    ("rotate_to_3d(0, 1.2, 0.6)", lambda shape, zone: shape.animate.rotate_to_3d(0, 1.2, 0.6)),
    ('with_pivot_3d(*foot, 0)\nrotate_by_3d("x", -math.pi / 2)', about_foot),
])

# Positions in depth: toward the camera a shape grows and drifts away from the
# frame's center, away from it it shrinks toward the center.
scene.segment("move")
gallery([
    ("shift_by_3d(0, 0, -20)", lambda shape, zone: shape.animate.shift_by_3d(0, 0, -20)),
    ("move_to_3d(x, y, 6)", lambda shape, zone: shape.animate.move_to_3d(*zone.center, 6)),
    ("shift_by_3d(-2, 0, 5)", lambda shape, zone: shape.animate.shift_by_3d(-2, 0, 5)),
])

# Per-axis scale, z included: scaling z deepens the extrusion.
scene.segment("scale")
table([
    ("scale_by_3d(1, 1, 4)", lambda shape, zone: shape.animate.scale_by_3d(1, 1, 4)),
    ("scale_to_3d(1.6, 0.6, 1)", lambda shape, zone: shape.animate.scale_to_3d(1.6, 0.6, 1)),
    ("scale_by_3d(0.5, 0.5, 3)", lambda shape, zone: shape.animate.scale_by_3d(0.5, 0.5, 3)),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-move and settled, in each of the four segments.
    scene.snapshots(snapshots, [(DURATION + 2 * HOLD) * index + t for index in range(4) for t in (1.4, 2.7)])
else:
    scene.render()
