"""3D shapes from points and outlines: tori, arrows, tubes, flat sheets and lines.

Arrows run between two world-space points and tubes along a list of them;
each sits at the center of its points, so it turns and grows about it there.
A 3D line runs between two ends, points or drawables it follows.
A flat sheet is the area a 2D drawable fills, like an extrusion with no
depth, with a face on each side. All of them are lit, closed meshes that
appear with ``create()``. The default camera is orthographic and looks down
the z axis, so the z = 0 plane lines up with the layout zones; each label is
the ``scene.geometry`` call that builds the variant.
"""

import math
import os

from gaanim import Material3D, Scene, Zones

DRAW = 1.2
TURN = 1.4
HOLD = 0.8
# Turn tori about x to look into their hole, and tubes about x and y to show
# their depth.
TIP = (0.9, 0.3, 0)
TILT = (0.45, -0.6, 0)

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
# Times worth a snapshot: while shapes grow, and once each move settles.
moments = []


def reveal(shapes, turns=()):
    """Grow ``shapes``, turn them through each pose of ``turns``, then hold."""
    moments.append(scene.cursor + DRAW / 2)
    scene.play([shape.animate.create() for shape in shapes], duration=DRAW)
    moments.append(scene.cursor)
    for turn in turns:
        scene.play([shape.animate.rotate_to_3d(*turn) for shape in shapes], duration=TURN)
        moments.append(scene.cursor)
    scene.wait(HOLD)


def gallery(variants, turns=()):
    """Two columns of (label, build) variants, each shape above its label, then reveal them.

    ``build(zone)`` returns the variant's shape for its zone.
    """
    rows = math.ceil(len(variants) / 2)
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=2, gap="48px", padding="page_padding"))
    shapes = []
    for cell, (label, build) in zip(grid, variants):
        art, caption = cell.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        shapes.append(build(art))
    reveal(shapes, turns)


def centered(zone, points):
    """``points`` around the origin moved to the center of ``zone``, at the same depth."""
    x, y = zone.center
    return [(x + px, y + py, pz) for px, py, pz in points]


def torus(*args, **options):
    """A torus built by ``scene.geometry.torus(*args, **options)``, moved to its zone and tipped."""
    return lambda zone: scene.geometry.torus(*args, **options).move_to(*zone.center).rotate_to_3d(*TIP)


# A torus lies flat in x and z, like a plane: major_radius is the circle its
# tube runs around and minor_radius the tube. segments divide the circle and
# rings the tube, so few of either facet the ring one way or the other.
scene.segment("tori")
gallery([
    ("torus(1.1, 0.35)", torus(1.1, 0.35)),
    ("torus(1.1, 0.12)", torus(1.1, 0.12)),
    ("torus(1.1, 0.35, segments=10)", torus(1.1, 0.35, segments=10)),
    ("torus(1.1, 0.35, rings=5)", torus(1.1, 0.35, rings=5)),
])


def arrow(**options):
    """An arrow across its zone, from the lower left at the back to the upper right at the front."""

    def build(zone):
        a, b = centered(zone, [(-2.6, -0.6, -1.2), (2.6, 0.6, 1.2)])
        return scene.geometry.arrow_3d(a, b, **options)

    return build


# arrow_3d(a, b) points from a to b in world space. Its shaft radius, head
# radius and head length follow its length unless given in scene units, and
# segments sets how many sides shaft and head have.
scene.segment("arrows")
gallery([
    ("arrow_3d(a, b)", arrow()),
    ("arrow_3d(a, b, shaft_radius=0.06)", arrow(shaft_radius=0.06)),
    ("arrow_3d(a, b, head_length=1.6)", arrow(head_length=1.6)),
    ("arrow_3d(a, b, segments=6)", arrow(segments=6)),
])

# Paths for the tubes, around the origin: a zigzag with sharp corners, a coil
# sampled finely enough to stay smooth, a trefoil knot that closes on itself,
# and a hook whose ends point at the camera.
CORNERS = [(-2.4, -0.7, 0), (-1.2, 0.7, 0.6), (0, -0.7, -0.6), (1.2, 0.7, 0.6), (2.4, -0.7, 0)]
COIL = [(t / 16 - 2.4, 0.7 * math.cos(t / 4), 0.7 * math.sin(t / 4)) for t in range(77)]
KNOT = [
    (
        0.38 * (math.sin(t) + 2 * math.sin(2 * t)),
        0.38 * (math.cos(t) - 2 * math.cos(2 * t)),
        -0.38 * math.sin(3 * t),
    )
    for t in (k * math.tau / 120 for k in range(120))
]
HOOK = [(-1.6, 0.5, 1.2), (-1.6, 0.5, -0.8), (1.6, -0.5, -0.8), (1.6, -0.5, 1.2)]


def tube(points, radius, **options):
    """A tube along ``points``, moved to the center of its zone."""
    return lambda zone: scene.geometry.tube(centered(zone, points), radius, **options)


# tube(points, radius) follows a polyline without twisting: corners keep the
# radius and crease, sampled curves stay smooth, closed=True joins the last
# point to the first, and caps=False leaves the ends open. Lit meshes hide
# the faces turned away from the viewer, so an open end shows a ring.
scene.segment("tubes")
gallery(
    [
        ("tube(corners, 0.15)", tube(CORNERS, 0.15)),
        ("tube(coil, 0.08)", tube(COIL, 0.08, material=Material3D.matte(accent))),
        ("tube(knot, 0.16, closed=True)", tube(KNOT, 0.16, closed=True, material=Material3D.metal(success))),
        ("tube(hook, 0.3, caps=False)", tube(HOOK, 0.3, caps=False)),
    ],
    turns=[TILT],
)


# Each label is the source passed to scene.geometry.flat_3d(source), which
# lays the area it fills flat in 3D, in its fill color, keeping the holes of
# letters and rings. The sheets face the camera, lie back like a sign on the
# floor, then turn round: from behind they read mirrored.
def sheet(source):
    """A flat sheet of ``source``, scaled to fill its zone with room to turn."""
    return lambda zone: scene.geometry.flat_3d(source.place(zone.inset(zone.height * 0.12), fit="contain"))


scene.segment("sheets")
entries = [
    ('scene.text("Bg8")', sheet(scene.text("Bg8"))),
    ('scene.text.equation("e^(i pi)")', sheet(scene.text.equation("e^(i pi)"))),
    ("scene.geometry.annulus(1, 0.5)", sheet(scene.geometry.annulus(1, 0.5))),
]
table = scene.layout.zones(
    Zones.grid(rows=len(entries), columns=["5fr", "4fr"], gap="24px", column_gap="48px", padding="page_padding")
)
sheets = []
for index, (label, build) in enumerate(entries):
    scene.text(label, role="code").place(table[2 * index], anchor="right")
    sheets.append(build(table[2 * index + 1]))
reveal(sheets, turns=[(-1.0, 0, 0), (0.2, 2.6, 0)])


def line(*args, **options):
    """A line across its zone, from the lower left at the back to the upper right at the front."""

    def build(zone):
        a, b = centered(zone, [(-2.4, -0.5, -1.0), (2.4, 0.5, 1.0)])
        return scene.geometry.line_3d(a, b, *args, **options)

    return build


# line_3d(start, end, thickness) runs a lit cylinder between two ends that
# keeps its thickness, and create() grows it from start. segments sets how
# many sides it has. An end may be a drawable: the line follows it as it
# moves, like a bond between two atoms, so these two balls drag theirs along.
scene.segment("lines")
grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))
lines, balls, moves = [], [], []
for cell, (label, build) in zip(
    grid,
    [
        ("line_3d(a, b)", line()),
        ("line_3d(a, b, 0.2)", line(0.2)),
        ("line_3d(a, b, 0.3, segments=5)", line(0.3, segments=5)),
        ("line_3d(left, right, 0.12)", None),
    ],
):
    art, caption = cell.split(Zones.rows(["1fr", 0.5]))
    scene.text(label, role="code").place(caption)
    if build:
        lines.append(build(art))
        continue
    ends = centered(art, [(-2.2, 0, 0), (2.2, 0, 0)])
    left, right = (
        scene.geometry.sphere(0.35, material=Material3D.matte(color)).move_to_3d(*end)
        for end, color in zip(ends, (accent, success))
    )
    balls += [left, right]
    lines.append(scene.geometry.line_3d(left, right, 0.12))
    to_left, to_right = centered(art, [(-1.6, 0.8, 1.2), (2.0, -0.7, -1.2)])
    moves += [left.animate.move_to_3d(*to_left), right.animate.move_to_3d(*to_right)]
moments.append(scene.cursor + DRAW / 2)
scene.play([shape.animate.create() for shape in balls + lines], duration=DRAW)
moments.append(scene.cursor)
scene.play(moves, duration=TURN)
moments.append(scene.cursor)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, moments)
else:
    scene.render()
