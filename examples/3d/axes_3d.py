"""3D axes: scene.viz.cartesian_3d with its grid planes, box sizes, axis scales and layers.

A distant perspective camera looks at every space from the front right with
z pointing up. Segments cover which planes and axes a space draws, the size
of its box, the scales of its axes, and animating its layers one by one.
"""

import math
import os

from gaanim import Axis, Scene, Zones

DRAW = 1.4
HOLD = 0.8

X = Axis.linear(-2, 2).ticks(1).label("x")
Y = Axis.linear(-2, 2).ticks(1).label("y")
Z = Axis.linear(-2, 2).ticks(1).label("z")

scene = Scene()
accent = scene.canvas.color("accent")

# The camera looks at the origin from AZIMUTH radians around z and ELEVATION
# radians above the floor, with z up. From DISTANCE away, a field of view as
# tall as the frame draws the plane through the origin that faces the camera
# at the layout's scale, so on_screen() finds the 3D point under a zone.
AZIMUTH, ELEVATION, DISTANCE = 0.65, 0.45, 40
RIGHT = (math.cos(AZIMUTH), math.sin(AZIMUTH), 0)
ABOVE = (-math.sin(AZIMUTH) * math.sin(ELEVATION), math.cos(AZIMUTH) * math.sin(ELEVATION), math.cos(ELEVATION))
TOWARD_EYE = (math.sin(AZIMUTH) * math.cos(ELEVATION), -math.cos(AZIMUTH) * math.cos(ELEVATION), math.sin(ELEVATION))
frame = scene.layout.zones(Zones.rows(1))[0]
scene.camera.perspective(fov_y=2 * math.atan(frame.height / 2 / DISTANCE))
scene.camera.look_at(eye=tuple(DISTANCE * c for c in TOWARD_EYE), target=(0, 0, 0), up=(0, 0, 1))


def on_screen(zone):
    """The 3D point the camera draws at the center of ``zone``."""
    x, y = zone.center
    return tuple(x * right + y * above for right, above in zip(RIGHT, ABOVE))


def gallery(variants, rows=2):
    """Lay out (label, build) pairs in a grid, each space above its label.

    ``build()`` returns a new space, which moves under its zone.
    """
    columns = -(-len(variants) // rows)
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    spaces = []
    for zone, (label, build) in zip(grid, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.5 * (label.count("\n") + 1)], gap="72px"))
        scene.text(label, role="code").place(caption)
        spaces.append(build().move_to_3d(*on_screen(art)))
    scene.play([space.animate.create() for space in spaces], duration=DRAW)
    scene.wait(HOLD)


def space(size=(2.6, 2.6, 1.8), **options):
    """A space of X, Y and Z in a box of ``size``, with ``options``."""
    return lambda: scene.viz.cartesian_3d(X, Y, Z, size=size, **options)


# Each label is the options passed to scene.viz.cartesian_3d(X, Y, Z, size=(2.6, 2.6, 1.8), ...).
# grid switches the three planes at once; xy_grid, xz_grid and yz_grid
# override it for one plane.
scene.segment("planes")
gallery([
    ("grid=True", space()),
    ("xy_grid=False", space(xy_grid=False)),
    ("xz_grid=False, yz_grid=False", space(xz_grid=False, yz_grid=False)),
    ("xy_grid=False, yz_grid=False", space(xy_grid=False, yz_grid=False)),
])

# axes switches the three axis lines, which run from the box's lowest
# corner; x_axis, y_axis and z_axis override it for one line.
scene.segment("axes")
gallery(
    [
        ("grid=False", space((3.2, 3.2, 2.2), grid=False)),
        ("grid=False,\nz_axis=False", space((3.2, 3.2, 2.2), grid=False, z_axis=False)),
        ("axes=False", space((3.2, 3.2, 2.2), axes=False)),
    ],
    rows=1,
)

# size is the box's extent along x, y and z in scene units, whatever the
# data ranges: the same axes stretch to fill it.
scene.segment("size")
gallery(
    [
        ("size=(3.2, 3.2, 1.2)", space((3.2, 3.2, 1.2), xz_grid=False)),
        ("size=(2.2, 2.2, 4)", space((2.2, 2.2, 4), xz_grid=False)),
        ("size=(4, 2, 1.4)", space((4, 2, 1.4), xz_grid=False)),
    ],
    rows=1,
)


# Any Axis scale works on any axis; here x changes and y and z stay linear,
# so the grid lines across x show how each scale spaces its ticks.
def scaled(x_axis):
    return lambda: scene.viz.cartesian_3d(x_axis, Y, Z, size=(2.6, 2.6, 1.8), yz_grid=False)


scene.segment("scales")
gallery([
    ("Axis.linear(0, 1000).ticks(250)", scaled(Axis.linear(0, 1000).ticks(250))),
    ("Axis.log(1, 1000)", scaled(Axis.log(1, 1000))),
    ("Axis.symlog(-100, 100).ticks(100)", scaled(Axis.symlog(-100, 100).ticks(100))),
    ("Axis.power(0, 100, 0.5).ticks(50)", scaled(Axis.power(0, 100, 0.5).ticks(50))),
])

# Every space keeps its parts in layers that animate on their own: "grid",
# "axes", "ticks", "numbers" and "labels". Axis.style colors the axis lines
# and their ticks. The axes draw the box's lowest edges, so they come first.
scene.segment("layers")
label_column, stage = scene.layout.zones(Zones.columns(["4fr", "3fr"], gap="48px", padding="page_padding"))
styled = [axis.style(color=accent, tick_color=accent) for axis in (X, Y, Z)]
world = scene.viz.cartesian_3d(*styled, size=(4, 4, 2.8)).move_to_3d(*on_screen(stage))
steps = [
    ('world.layer("axes").animate.create()', world.layer("axes").animate.create()),
    ('world.layer("grid").animate.fade_in()', world.layer("grid").animate.fade_in()),
    ('world.layer("ticks").animate.create()', world.layer("ticks").animate.create()),
    ('world.layer("numbers").animate.fade_in()', world.layer("numbers").animate.fade_in()),
    ('world.layer("labels").animate.fade_in()', world.layer("labels").animate.fade_in()),
]
rows = label_column.split(Zones.rows(len(steps), gap="24px"))
for row, (label, _) in zip(rows, steps):
    scene.text(label, role="code").place(row, anchor="right")
for _, step in steps:
    scene.play([step], duration=DRAW / 2)
scene.wait(HOLD)

# Drawing and settled in the first four segments, then each layer step.
segment = DRAW + HOLD
seeks = [segment * index + t for index in range(4) for t in (0.6, DRAW + 0.4)]
seeks += [4 * segment + DRAW / 2 * step + 0.6 for step in range(len(steps))]
seeks += [4 * segment + DRAW / 2 * len(steps) + 0.4]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
