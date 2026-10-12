"""Extrusion: turning the area a 2D drawable fills into a lit 3D solid.

``scene.geometry.extrude(source)`` reads the outline of text, equations,
imported SVGs, shapes and groups, keeps their holes, and builds a closed mesh
in the source's fill color, facing the camera. Each solid then turns so its
depth shows. Segments cover the sources, the depth and bevel of the mesh, and
its remaining options.
"""

import os

from gaanim import Material3D, Scene, Zones

DRAW = 1.0
TURN = 1.4
HOLD = 0.6
# The pose every solid turns to, about x and y.
TILT = (0.4, -0.7, 0)

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")


def show(solids):
    """Grow ``solids`` facing the camera, turn them so their depth shows, then hold."""
    scene.play([solid.animate.create() for solid in solids], duration=DRAW)
    scene.play([solid.animate.rotate_to_3d(*TILT) for solid in solids], duration=TURN)
    scene.wait(HOLD)


def table(entries):
    """One row per (label, build) entry, its code to the left: ``build(zone)`` returns the row's solid."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(entries), columns=["5fr", "4fr"], gap="24px", column_gap="48px", padding="page_padding")
    )
    solids = []
    for index, (label, build) in enumerate(entries):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        solids.append(build(grid[2 * index + 1]))
    show(solids)


def extrude(source):
    """Scale ``source`` to fill its zone, with room to turn, and extrude it half a unit deep."""
    return lambda zone: scene.geometry.extrude(
        source.place(zone.inset(zone.height * 0.08), fit="contain"), depth=0.5, bevel=0.04
    )


# Each label is the source passed to scene.geometry.extrude(source, depth=0.5, bevel=0.04).
# Text and equations keep the holes of their letters; an SVG becomes one solid
# in the color of its fill.
scene.segment("outlines")
table([
    ('scene.text("Bg8")', extrude(scene.text("Bg8"))),
    ('scene.text.equation("e^(i pi)")', extrude(scene.text.equation("e^(i pi)"))),
    ('scene.media.svg("assets/icon.svg")', extrude(scene.media.svg("assets/icon.svg"))),
])

# Shapes fill with the nonzero rule, so an annulus keeps its hole and a group
# extrudes as the union of its members.
disc = scene.geometry.circle(1).move_to(-0.7, 0)
block = scene.geometry.square(1.8).move_to(0.7, 0)
scene.segment("shapes")
table([
    ("scene.geometry.annulus(1, 0.5)", extrude(scene.geometry.annulus(1, 0.5))),
    ("scene.geometry.star(5, 1, 0.4)", extrude(scene.geometry.star(5, 1, 0.4))),
    ("scene.geometry.group([disc, block])", extrude(scene.geometry.group([disc, block]))),
])


# depth is the solid's thickness, centered on z = 0; bevel cuts a 45° chamfer
# that wide around both caps and must stay under half the depth:
# scene.geometry.extrude(star, depth=0.6, bevel=0.05).
def star_solid(depth, bevel):
    return lambda zone: scene.geometry.extrude(
        scene.geometry.star(5, zone.height * 0.48, zone.height * 0.22).place(zone), depth=depth, bevel=bevel
    )


scene.segment("depth")
DEPTHS = [("depth=0.3", 0.3), ("depth=0.6", 0.6), ("depth=1.2", 1.2)]
BEVELS = [("bevel=0", 0.0), ("bevel=0.07", 0.07), ("bevel=0.14", 0.14)]
grid = scene.layout.zones(
    Zones.grid(rows=[0.5, "1fr", "1fr", "1fr"], columns=["1fr", "2fr", "2fr", "2fr"], gap="24px", padding="page_padding")
)
for index, (label, _) in enumerate(BEVELS, start=1):
    scene.text(label, role="code").place(grid[index])
solids = []
for row, (label, depth) in enumerate(DEPTHS, start=1):
    scene.text(label, role="code").place(grid[4 * row], anchor="right")
    for column, (_, bevel) in enumerate(BEVELS, start=1):
        solids.append(star_solid(depth, bevel)(grid[4 * row + column]))
show(solids)

# Each label is an option of scene.geometry.extrude(circle, depth=0.5, ...):
# tolerance sets how finely curves flatten, keep_source keeps the 2D source
# visible over the solid (2D always draws over 3D), and material replaces
# the matte fill color.
def disc_solid(**options):
    return lambda zone: scene.geometry.extrude(
        scene.geometry.circle(zone.height * 0.42).place(zone), depth=0.5, **options
    )


def kept_outline(zone):
    """A solid whose kept source, outlined and faded, stays flat over it."""
    circle = scene.geometry.circle(zone.height * 0.42).place(zone)
    solid = scene.geometry.extrude(circle, depth=0.5, keep_source=True)
    circle.stroke(foreground, 0.05).opacity(0.4)
    return solid


scene.segment("options")
table([
    ("tolerance=0.25", disc_solid(tolerance=0.25)),
    ("keep_source=True", kept_outline),
    ("material=Material3D.metal(accent)", disc_solid(material=Material3D.metal(accent))),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Facing the camera, turning and turned, in each of the four segments.
    scene.snapshots(snapshots, [(DRAW + TURN + HOLD) * index + t for index in range(4) for t in (0.9, 1.7, 2.8)])
else:
    scene.render()
