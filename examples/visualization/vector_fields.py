"""Vector fields: arrows, streamlines, flowing highlights, particles, advection and color maps.

``plane.field(f)`` wraps a function (x, y) -> (u, v) in a ``VectorField``
that draws nothing by itself: arrows, streamlines and particles are views
of it. The first segments show fields and their arrow and streamline
options, the next ones set them in motion, and the last one colors them.
"""

import os

from gaanim import Axis, ColorMap, Scene, Zones

DRAW = 1.2
HOLD = 0.8
FLOW = 3.0
# A gallery draws its planes, then their views, then holds.
STEP = 2 * DRAW + HOLD

scene = Scene()
accent = scene.canvas.color("accent")
danger = scene.canvas.color("danger")
X = Axis.linear(-3, 3).ticks(1)
Y = Axis.linear(-2, 2).ticks(1)


def vortex(x, y):
    return -y, x


def source(x, y):
    return x, y


def saddle(x, y):
    return x, -y


def spiral(x, y):
    return -y - 0.3 * x, x - 0.3 * y


def cells(count, bands=("2fr", "7fr")):
    """A padded 2-column grid of ``count`` cells, each split into a label band and a body."""
    grid = scene.layout.zones(
        Zones.grid(rows=-(-count // 2), columns=2, column_gap="96px", row_gap="48px", padding="page_padding")
    )
    return [cell.split(Zones.rows(list(bands))) for cell in list(grid)[:count]]


def plane_in(body):
    """A plane without numbers, its plot area 3:2 and as large as ``body`` allows."""
    height = min(body.height * 0.92, body.width * 0.92 * 2 / 3)
    plane = scene.viz.cartesian_2d(X, Y, width=height * 1.5, height=height, numbers=False)
    return plane.move_to(*body.center)


def gallery(variants):
    """Draw a plane per (label, build) cell and the view ``build(plane)`` returns on it."""
    planes, views = [], []
    for (top, body), (label, build) in zip(cells(len(variants)), variants):
        scene.text(label, role="code").place(top, anchor="bottom")
        plane = plane_in(body)
        planes.append(plane)
        views.append(build(plane))
    scene.play([plane.animate.create() for plane in planes], duration=DRAW)
    scene.play([view.animate.write() for view in views], duration=DRAW)
    scene.wait(HOLD)


# Arrows sample the field on a regular grid; their color follows the
# magnitude through the viridis color map unless told otherwise.
scene.segment("fields")
gallery([
    ("plane.field(vortex).arrows()", lambda plane: plane.field(vortex).arrows()),
    ("plane.field(source).arrows()", lambda plane: plane.field(source).arrows()),
    ("plane.field(saddle).arrows()", lambda plane: plane.field(saddle).arrows()),
    ("plane.field(spiral).arrows()", lambda plane: plane.field(spiral).arrows()),
])

# resolution= is the number of arrows per axis, max_length= and
# length_scale= size them, width= thickens the shafts and color= replaces
# the color map with one color.
scene.segment("arrows")
gallery([
    ("arrows(resolution=(9, 6))", lambda plane: plane.field(spiral).arrows(resolution=(9, 6))),
    ("arrows(max_length=0.5,\n       width=0.04)", lambda plane: plane.field(spiral).arrows(max_length=0.5, width=0.04)),
    ("arrows(length_scale=0.3,\n       min_length=0.1)", lambda plane: plane.field(spiral).arrows(length_scale=0.3, min_length=0.1)),
    ("arrows(color=accent)", lambda plane: plane.field(spiral).arrows(color=accent)),
])

# Streamlines integrate the field from a grid of seeds, kept apart by
# separation=; direction= follows the field forward, backward or both.
scene.segment("streamlines")
gallery([
    ("streamlines()", lambda plane: plane.field(spiral).streamlines()),
    ("streamlines(seeds=(6, 4),\n            separation=0.2)", lambda plane: plane.field(spiral).streamlines(seeds=(6, 4), separation=0.2)),
    ('streamlines(direction="forward",\n            max_time=1)', lambda plane: plane.field(spiral).streamlines(direction="forward", max_time=1)),
    ("streamlines(width=0.05, opacity=0.6)", lambda plane: plane.field(spiral).streamlines(width=0.05, opacity=0.6)),
])

# flow() runs brighter highlights along the streamlines; particles() seeds
# points that flow() carries along the field, and advect() moves any
# object's center along the path from one seed.
scene.segment("flow")
top, body = scene.layout.zones(Zones.rows(["1fr", "6fr"], padding="page_padding"))
scene.text(
    "scene.play(streams.flow(3, time_width=0.15) + particles.flow()\n"
    "           + [field.advect(dot, (2.5, 0), duration=3)])",
    role="code",
).place(top, anchor="bottom_left")
plane = scene.viz.cartesian_2d(X, Y, width=body.height * 1.5 * 0.92, height=body.height * 0.92, numbers=False)
plane.move_to(*body.center)
field = plane.field(spiral)
streams = field.streamlines(seeds=(14, 9), opacity=0.5)
particles = field.particles(24, radius=0.06, duration=FLOW)
dot = scene.geometry.dot(0.16).fill(accent)
scene.play([plane.animate.create(), streams.animate.write(), particles.animate.fade_in()], duration=DRAW)
scene.play(streams.flow(FLOW, time_width=0.15) + particles.flow() + [field.advect(dot, (2.5, 0), duration=FLOW)])
scene.wait(HOLD)

# colormap= takes a ColorMap or a name (Matplotlib and Scientific Colour
# Maps); color_range= fixes the magnitudes at its two ends.
scene.segment("color_maps")
gallery([
    ('arrows(colormap="batlow")', lambda plane: plane.field(spiral).arrows(colormap="batlow")),
    ('arrows(colormap=\n    ColorMap("viridis").reversed())', lambda plane: plane.field(spiral).arrows(colormap=ColorMap("viridis").reversed())),
    ("arrows(colormap=ColorMap.from_colors(\n    [accent, danger]))", lambda plane: plane.field(spiral).arrows(colormap=ColorMap.from_colors([accent, danger]))),
    ('streamlines(colormap="vik",\n            color_range=(0, 2))', lambda plane: plane.field(spiral).streamlines(colormap="vik", color_range=(0, 2))),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each gallery while its views draw and once drawn, the flow at three
    # moments and the color maps.
    scene.snapshots(
        snapshots,
        [STEP * index + t for index in range(3) for t in (1.8, 2.8)]
        + [STEP * 3 + t for t in (1.0, 2.2, 3.6)]
        + [STEP * 3 + DRAW + FLOW + HOLD + 2.8],
    )
else:
    scene.render()
