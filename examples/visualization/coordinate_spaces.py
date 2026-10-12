"""Coordinate spaces: Cartesian, polar and complex planes, number lines, layers, coordinates and views.

A coordinate space draws axes, grids, ticks, numbers and titles as separate
layers and maps data to the frame. The first segments switch layers on and
off and bring them in one by one; the next ones place objects at data
coordinates and change the view window, then come number lines, polar
spaces and the complex plane.
"""

import math
import os

from gaanim import Anchor, Axis, Scene, Zones, computed

DRAW = 1.0
HOLD = 0.8
MOVE = 1.4
# Planes are drawn larger than their default size so their numbers read
# as easily as the labels.
ZOOM = 1.3

scene = Scene()
accent = scene.canvas.color("accent")
rule = scene.canvas.color("rule")
X = Axis.linear(0, 6).ticks(1).label("x")
Y = Axis.linear(0, 4).ticks(1).label("y")


def cells(count, columns=2, bands=("1fr", "4fr")):
    """A padded grid of ``count`` cells, each split into a label band and a body."""
    rows = -(-count // columns)
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=columns, column_gap="96px", row_gap="48px", padding="page_padding")
    )
    return [zone.split(Zones.rows(list(bands))) for zone in list(grid)[:count]]


def plane_in(body, x=X, y=Y, size=(0.7, 0.55), **options):
    """A Cartesian plane whose plot area takes ``size`` of ``body``, centered in it."""
    width, height = body.width * size[0] / ZOOM, body.height * size[1] / ZOOM
    plane = scene.viz.cartesian_2d(x, y, width=width, height=height, **options)
    # A plane's origin is the middle of its plot area.
    return plane.scale_to(ZOOM).move_to(*body.center)


def gallery(variants):
    """Draw a plane per (label, options) cell of a 2 x 2 grid, its label above it."""
    planes = []
    for (top, body), (label, options) in zip(cells(len(variants)), variants):
        scene.text(label, role="code").place(top, anchor="bottom")
        planes.append(plane_in(body, **options))
    scene.play([plane.animate.create() for plane in planes], duration=DRAW)
    scene.wait(HOLD)


# Global switches (grid, axes, ticks, numbers, labels) turn a kind of layer
# off on both axes; x_* and y_* switches override them for one axis.
scene.segment("visibility")
gallery([
    ("cartesian_2d(X, Y)", {}),
    ("grid=False, numbers=False", {"grid": False, "numbers": False}),
    ("grid=False, x_grid=True", {"grid": False, "x_grid": True}),
    ("numbers=False, y_numbers=True", {"numbers": False, "y_numbers": True}),
])

scene.segment("more_visibility")
gallery([
    ("labels=False", {"labels": False}),
    ("axes=False, ticks=False", {"axes": False, "ticks": False}),
    ("ticks=False, y_ticks=True", {"ticks": False, "y_ticks": True}),
    ("x_axis=False, x_numbers=False", {"x_axis": False, "x_numbers": False}),
])

# Every layer is a drawable of its own: bring each one in with its own
# animation, or style it apart from the rest.
scene.segment("layers")
grid = scene.layout.zones(Zones.columns(["5fr", "9fr"], gap="72px", padding="page_padding"))
names = ["minor_grid", "major_grid", "axes", "ticks", "numbers", "labels"]
rows = grid[0].split(Zones.rows(len(names) + 1))
plane = plane_in(
    grid[1],
    x=Axis.linear(0, 6).ticks(1).minor_ticks(2).label("x"),
    y=Axis.linear(0, 4).ticks(1).minor_ticks(2).label("y"),
    size=(0.8, 0.7),
)
for row, name in zip(rows, names):
    label = scene.text(f'plane.layer("{name}")', role="code").place(row, anchor="left").hidden()
    layer = plane.layer(name)
    entrance = layer.animate.fade_in() if name in ("numbers", "labels") else layer.animate.create()
    scene.play([entrance, label.animate.fade_in()], duration=0.5)
styled = scene.text('plane.layer("major_grid")\n    .opacity(0.3)', role="code").place(rows[-1], anchor="left").hidden()
scene.play([plane.layer("major_grid").animate.opacity(0.3), styled.animate.fade_in()], duration=0.5)
scene.wait(HOLD)

# coord(x, y) is a data point owned by the space: at_coordinate() makes an
# object its child and coord().place() puts one there. data_to_scene() is a
# frame point that follows the data for objects outside the space, such as
# connectors and labels. All of them follow the plane when it turns.
scene.segment("coordinates")
left, right = scene.layout.zones(Zones.columns(["8fr", "6fr"], gap="72px", padding="page_padding"))
plane = plane_in(left, x=Axis.linear(-4, 4).ticks(1), y=Axis.linear(-3, 3).ticks(1), size=(0.8, 0.75))
dot = scene.geometry.dot(0.14).at_coordinate(plane.coord(2, 2))
square = plane.coord(-2, 1).place(scene.geometry.square(0.3))
ring = scene.geometry.circle(0.2).no_fill().stroke(accent, 0.05).follow(plane.data_to_scene(2, -2))
notes = []
for row, (label, target) in zip(
    right.split(Zones.rows(3)),
    [
        ("dot.at_coordinate(\n    plane.coord(2, 2))", dot),
        ("plane.coord(-2, 1)\n    .place(square)", square),
        ("ring.follow(\n    plane.data_to_scene(2, -2))", plane.data_to_scene(2, -2)),
    ],
):
    note = scene.text(label, role="code").place(row, anchor="left")
    notes.append((note, scene.geometry.connector(note.anchor_point(Anchor.LEFT), target)))
scene.play(
    [plane.animate.create()] + [part.animate.fade_in() for part in (dot, square, ring)],
    duration=DRAW,
)
scene.play([anim for note, link in notes for anim in (note.animate.fade_in(), link.animate.grow_arrow())], duration=0.6)
scene.play([plane.animate.rotate_to(0.25)], duration=MOVE)
scene.play([plane.animate.rotate_to(0)], duration=MOVE)
scene.wait(HOLD)

# view_to() changes the data window while the plot area stays put. Axes
# with automatic ticks regenerate their step for the new window; a fixed
# ticks(step) is kept.
scene.segment("view")
panels = cells(2, bands=("1fr", "5fr"))
views = []
for (top, body), (label, x) in zip(
    panels,
    [("Axis.linear(-6, 6)", Axis.linear(-6, 6)), ("Axis.linear(-6, 6).ticks(2)", Axis.linear(-6, 6).ticks(2))],
):
    scene.text(label, role="code").place(top, anchor="bottom")
    space = plane_in(body, x=x, y=Axis.linear(-3, 3), size=(0.8, 0.55))
    wave = space.plot(lambda value: 2 * math.sin(value)).stroke(accent, 0.04)
    views.append((space, wave))
caption = scene.layout.zones(Zones.rows(["5fr", "1fr"], padding="page_padding"))[1]
scene.play([part.animate.create() for space, wave in views for part in (space, wave)], duration=DRAW)
shown = None
for window in [(-1.5, 1.5), (-10, 10), (-6, 6)]:
    text = scene.text(f"plane.animate.view_to({window}, (-3, 3))", role="code").place(caption).hidden()
    swap = [text.animate.fade_in()] + ([shown.animate.fade_out()] if shown else [])
    scene.play(swap + [space.animate.view_to(window, (-3, 3)) for space, _ in views], duration=MOVE)
    shown = text
scene.wait(HOLD)

# A number line is a one-dimensional space: point_ref() follows a value,
# function() draws a curve across it and reveal= draws it up to a value.
scene.segment("number_line")
rows = scene.layout.zones(Zones.rows(["1fr", "3fr", "1fr", "2fr"], padding="page_padding"))
theta = scene.viz.parameter(0.0)
line = scene.viz.number_line(
    Axis.linear(0, 3 * math.pi).ticks(math.pi / 2).numbers("pi", denominator=2),
    length=rows[1].width * 0.9 / ZOOM,
)
line.drawable().scale_to(ZOOM).move_to(*rows[1].center)
curve = line.function(math.sin, normal_scale=1.0, reveal=theta).stroke(accent, 0.05)
point = scene.geometry.dot(0.12).follow(
    line.point_ref(theta, normal_offset=computed(lambda t: math.sin(t), inputs=[theta]))
)
start = rows[0].inset((0, rows[0].width * 0.05))
scene.text("line.function(math.sin, reveal=theta)", role="code").place(start, anchor="bottom_left")
# Number lines take axis_visible, ticks, numbers and labels switches; the
# dot on this one follows a point_ref of a value computed from theta.
bare = scene.viz.number_line(Axis.linear(0, 10).ticks(1), length=rows[3].width * 0.9 / ZOOM, numbers=False)
bare.drawable().scale_to(ZOOM).move_to(*rows[3].center)
start = rows[2].inset((0, rows[2].width * 0.05))
scene.text("number_line(Axis.linear(0, 10).ticks(1), numbers=False)", role="code").place(start, anchor="bottom_left")
marker = scene.geometry.dot(0.12).follow(bare.point_ref(computed(lambda t: 10 * t / (3 * math.pi), inputs=[theta])))
scene.play(
    [line.animate.create(), bare.animate.create(), curve.animate.fade_in(), point.animate.fade_in(), marker.animate.fade_in()],
    duration=DRAW,
)
scene.play([theta.animate.set(3 * math.pi)], duration=3.0)
scene.wait(HOLD)

# A polar space has rings and spokes, which rings= and spokes= switch apart
# from grid=; angle_divisions sets the number of spokes.
radial = Axis.linear(0, 3).ticks(1).label("r")


def polar_in(body, axis=radial, **options):
    """A polar space as large as ``body`` allows, its grid stroked with the theme's rule color."""
    radius = min(body.width, body.height) * 0.42 / ZOOM
    polar = scene.viz.polar(axis, radius=radius, **options)
    polar.layer("grid").stroke(rule, 0.015)
    polar.drawable().scale_to(ZOOM).move_to(*body.center)
    return polar


for name, variants in [
    ("polar", [("polar(radial)", {}), ("polar(radial, angle_divisions=6)", {"angle_divisions": 6})]),
    ("polar_grid", [("polar(radial, rings=False)", {"rings": False}), ("polar(radial, spokes=False)", {"spokes": False})]),
]:
    scene.segment(name)
    polars = []
    for (top, body), (label, options) in zip(cells(2, bands=("1fr", "6fr")), variants):
        scene.text(label, role="code").place(top, anchor="bottom")
        polars.append(polar_in(body, **options))
    scene.play([polar.animate.create() for polar in polars], duration=DRAW)
    scene.wait(HOLD)

# plot() draws r = f(angle) over one turn, or over a domain of angles.
scene.segment("polar_plot")
curves = []
for (top, body), (label, function, domain) in zip(
    cells(2, bands=("1fr", "6fr")),
    [
        ("polar.plot(lambda a:\n    2 * abs(cos(3 * a)))", lambda a: 2 * abs(math.cos(3 * a)), (0, math.tau)),
        ("polar.plot(lambda a: a / 6,\n           (0, 12))", lambda a: a / 6, (0, 12)),
    ],
):
    scene.text(label, role="code").place(top, anchor="bottom")
    polar = polar_in(body, axis=Axis.linear(0, 2).ticks(0.5).label("r"))
    curves.append((polar, polar.plot(function, domain).stroke(accent, 0.05)))
scene.play([polar.animate.create() for polar, _ in curves], duration=DRAW)
scene.play([curve.animate.create() for _, curve in curves], duration=1.6)
scene.wait(HOLD)

# The complex plane is a Cartesian space titled Re and Im whose coordinates
# are the real and imaginary parts; here, the cube roots of 8i. Without
# axes it spans -5 to 5 and -3 to 3.
scene.segment("complex")
top, body = cells(1, columns=1, bands=("1fr", "9fr"))[0]
scene.text("scene.viz.complex()", role="code").place(top, anchor="bottom")
plane = scene.viz.complex(width=body.width * 0.75 / ZOOM, height=body.height * 0.75 / ZOOM)
plane.scale_to(ZOOM).move_to(*body.center)
roots = []
for k in range(3):
    angle = math.pi / 6 + k * math.tau / 3
    z = (2 * math.cos(angle), 2 * math.sin(angle))
    roots.append(scene.geometry.dot(0.14).at_coordinate(plane.coord(*z)))
    sign = "+" if z[1] >= 0 else "-"
    value = scene.text(f"{round(z[0], 2) + 0:.2f} {sign} {abs(z[1]):.2f}i", role="code")
    roots.append(plane.coord(z[0] + (1.7 if z[0] > -0.01 else -1.7), z[1]).place(value))
scene.play([plane.animate.create()] + [root.animate.fade_in() for root in roots], duration=DRAW)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [1.6, 3.4, 5.6, 7.7, 9.4, 10.9, 15.5, 16.9, 18.2, 21.6, 23.8, 25.6, 27.4, 29.3, 30.8, 32.6])
else:
    scene.render()
