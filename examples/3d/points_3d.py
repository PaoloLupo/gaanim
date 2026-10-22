"""Point clouds and copies of a mesh: one drawable draws one mesh at many points.

geometry.points_3d puts a lit sphere at each point, and geometry.instances
copies a primitive's mesh and material to each position, scaled, turned and
colored copy by copy. The mesh uploads once and every copy draws in one
instanced draw, so thousands of points stay one drawable that moves, turns
and is lit as a whole. create() grows the copies in one after another, in
the order given, and transform_to moves each copy to the copy at its place in
another set of as many. Each label is the call that builds or plays its
variant.
"""

import math
import os

from gaanim import ColorMap, Material3D, Scene, Zones

DRAW = 1.6
MOVE = 2.0
HOLD = 0.8
# Turn every drawable about x to show its top and about y to show a side.
TILT = (0.45, -0.6, 0)
GOLDEN = math.pi * (3 - math.sqrt(5))
SEEKS = []

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
blend = ColorMap.from_colors([accent, success])


def ball(count, radius):
    """``count`` points spread evenly over a sphere of ``radius``, from bottom to top."""
    points = []
    for k in range(count):
        y = 2 * (k + 0.5) / count - 1
        ring = math.sqrt(1 - y * y)
        angle = k * GOLDEN
        points.append((radius * ring * math.cos(angle), radius * y, radius * ring * math.sin(angle)))
    return points


def helix(count, radius, height):
    """Two strands of ``count`` points each, interleaved from bottom to top."""
    points = []
    for k in range(count):
        t = k / (count - 1)
        for strand in (0, math.pi):
            angle = 4 * math.pi * t + strand
            points.append((radius * math.cos(angle), height * (t - 0.5), radius * math.sin(angle)))
    return points


def lattice(per_side, side):
    """A cube of ``per_side``³ points ``side`` across, from bottom to top."""
    step = side / (per_side - 1)
    return [
        (x * step - side / 2, y * step - side / 2, z * step - side / 2)
        for y in range(per_side)
        for z in range(per_side)
        for x in range(per_side)
    ]


def field(per_side, side, swirl):
    """Arrows on a ``per_side``² grid ``side`` across: their places, turns and lengths.

    Each arrow points away from the middle, turned ``swirl`` radians towards
    going around it; its length grows away from the middle.
    """
    step = side / (per_side - 1)
    places, turns, lengths = [], [], []
    for row in range(per_side):
        for column in range(per_side):
            x, y = column * step - side / 2, row * step - side / 2
            out = math.atan2(-x, y) if (x, y) != (0, 0) else 0.0
            places.append((x, y, 0))
            turns.append((0, 0, out + swirl))
            lengths.append(0.5 + math.hypot(x, y) / side)
    return places, turns, lengths


def columns(count):
    """The art and caption zones of ``count`` columns side by side."""
    zones = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows(["1fr", 0.4])) for zone in zones]


def place(build, art):
    """``build(side)`` sized to ``art`` and placed in it, turned by TILT to show its depth."""
    return build(min(art.width, art.height)).move_to(*art.center).rotate_to_3d(*TILT)


def variants(pairs):
    """Build each (label, build) pair in its own column, above its label, which shrinks to fit."""
    built = []
    for (art, caption), (label, build) in zip(columns(len(pairs)), pairs):
        built.append(place(build, art))
        scene.text(label, role="code").place(caption, fit="scale_down")
    return built


def grow(drawables):
    """Grow ``drawables`` in together and hold them, noting the instants to capture."""
    SEEKS.append(scene.cursor + DRAW / 2)
    scene.play([drawable.animate.create() for drawable in drawables], duration=DRAW)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# A sphere at each point, in the material's color or one color per point.
scene.segment("points")
grow(variants([
    ("points_3d(ball, radius=...)",
     lambda side: scene.geometry.points_3d(
         ball(1200, side * 0.36), radius=side * 0.012, material=Material3D.matte(accent),
     )),
    ("points_3d(helix, colors=shades)",
     lambda side: scene.geometry.points_3d(
         helix(240, side * 0.2, side * 0.7), radius=side * 0.016,
         colors=[blend.sample(k % 2) for k in range(480)],
     )),
]))


def voxels(side):
    """The cells of a ball on a grid, from the middle out, shaded by height."""
    cube = scene.geometry.cube(side * 0.07, material=Material3D.matte(accent))
    step = side * 0.08
    cells = [
        (x * step, y * step, z * step)
        for x in range(-4, 5) for y in range(-4, 5) for z in range(-4, 5)
        if x * x + y * y + z * z <= 18
    ]
    cells.sort(key=lambda cell: cell[0] ** 2 + cell[1] ** 2 + cell[2] ** 2)
    shades = [blend.sample(0.5 + y / (8 * step)) for _, y, _ in cells]
    return scene.geometry.instances(cube, cells, colors=shades)


def arrows(side, swirl, scaled):
    """An arrow field across the art, each arrow turned by ``swirl`` from pointing out.

    The arrows are all as long unless ``scaled``.
    """
    arrow = scene.geometry.arrow_3d((0, -side * 0.04, 0), (0, side * 0.04, 0), material=Material3D.matte(success))
    places, turns, lengths = field(7, side * 0.7, swirl)
    if scaled:
        return scene.geometry.instances(arrow, places, rotations=turns, scales=lengths)
    return scene.geometry.instances(arrow, places, rotations=turns)


# Copies of a primitive's mesh, each colored, turned or scaled.
scene.segment("copies")
grow(variants([
    ("instances(cube, cells, colors=shades)", voxels),
    ("instances(arrow, places, rotations=turns)", lambda side: arrows(side, 0, scaled=False)),
]))

# Each copy moves to the copy at its place in another set of as many: points
# move along straight lines, and arrows turn the shorter way round and take
# the lengths of the swirl's.
scene.segment("transform")
cloud, spread = variants([
    ("cloud.animate.transform_to(globe)",
     lambda side: scene.geometry.points_3d(
         lattice(9, side * 0.5), radius=side * 0.014, material=Material3D.matte(accent),
     )),
    ("spread.animate.transform_to(swirl)", lambda side: arrows(side, 0, scaled=False)),
])
# The targets are templates in the same places, which stay hidden.
(cloud_art, _), (spread_art, _) = columns(2)
globe = place(
    lambda side: scene.geometry.points_3d(
        ball(729, side * 0.32), radius=side * 0.014, material=Material3D.matte(success),
    ),
    cloud_art,
)
swirl = place(lambda side: arrows(side, math.pi / 2, scaled=True), spread_art)
grow([cloud, spread])
SEEKS.append(scene.cursor + MOVE / 2)
scene.play([cloud.animate.transform_to(globe), spread.animate.transform_to(swirl)], duration=MOVE)
SEEKS.append(scene.cursor + HOLD / 2)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
