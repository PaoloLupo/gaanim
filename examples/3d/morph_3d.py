"""3D morphs: animate.transform_to turns one mesh into another where that one stands.

Both meshes are sampled on one grid, and each grid point moves from one to the
other. A mesh whose texture coordinates cover it once (a sphere, a torus, a
plane) is sampled by them, so a sphere opens its poles into a torus's hole and
a plane rolls up into a sphere; any other (a cube, a capped cylinder) by rays
from its center, so a sphere swells into a cube point by point. The target is
a template that stays hidden: the morphing mesh also takes its place, turn,
size and material. Each label is the call that plays the morph, and before and
after it every mesh is drawn exactly as built.
"""

import os

from gaanim import Material3D, Scene, Zones

DRAW = 0.8
MORPH = 2.0
HOLD = 0.8
# Turn every mesh about x to show its top and about y to show a side.
TILT = (0.45, -0.6, 0)
SEEKS = []

scene = Scene()
accent = Material3D.matte(scene.canvas.color("accent"))
success = Material3D.matte(scene.canvas.color("success"))
danger = Material3D.matte(scene.canvas.color("danger"))


def cells(count):
    """``count`` zones side by side, each split into its art and its caption."""
    row = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows(["1fr", 0.4])) for zone in row]


def morphs(variants):
    """Morph each (label, source, target) pair in place, above its label.

    Sources and targets of a pair stand in the same zone, sized to it by
    ``build(side)``; the sources grow in, morph together and hold.
    """
    sources = []
    plays = []
    for (art, caption), (label, build_source, build_target) in zip(cells(len(variants)), variants):
        side = min(art.width, art.height)
        source = build_source(side).move_to(*art.center).rotate_to_3d(*TILT)
        target = build_target(side).move_to(*art.center).rotate_to_3d(*TILT)
        scene.text(label, role="code").place(caption)
        sources.append(source)
        plays.append(source.animate.transform_to(target))
    scene.play([source.animate.create() for source in sources], duration=DRAW)
    SEEKS.append(scene.cursor)
    SEEKS.append(scene.cursor + MORPH / 2)
    scene.play(plays, duration=MORPH)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# Spheres, tori and planes are sampled by their texture coordinates.
scene.segment("coordinates")
morphs([
    ("ball.animate.transform_to(ring)",
     lambda side: scene.geometry.sphere(side * 0.32, material=accent),
     lambda side: scene.geometry.torus(side * 0.3, side * 0.12, material=success)),
    ("sheet.animate.transform_to(ball)",
     lambda side: scene.geometry.plane(side * 0.8, side * 0.55, subdivisions=(24, 16), material=accent),
     lambda side: scene.geometry.sphere(side * 0.3, material=success)),
    ("ring.animate.transform_to(ball)",
     lambda side: scene.geometry.torus(side * 0.3, side * 0.12, material=accent),
     lambda side: scene.geometry.sphere(side * 0.32, material=danger)),
])

# Cubes and capped cylinders and cones are sampled by rays from their center.
scene.segment("rays")
morphs([
    ("ball.animate.transform_to(box)",
     lambda side: scene.geometry.sphere(side * 0.32, material=accent),
     lambda side: scene.geometry.cube(side * 0.5, material=success)),
    ("box.animate.transform_to(can)",
     lambda side: scene.geometry.cube(side * 0.5, material=accent),
     lambda side: scene.geometry.cylinder(side * 0.25, side * 0.6, material=success)),
    ("can.animate.transform_to(cone)",
     lambda side: scene.geometry.cylinder(side * 0.25, side * 0.6, material=accent),
     lambda side: scene.geometry.cone(side * 0.32, side * 0.6, material=danger)),
])

# One mesh morphs through three targets in turn, each from the last: it moves
# to where each one stands and takes its turn, size and material.
scene.segment("chain")
stops = cells(3)
arts = [art for art, _ in stops]
sides = [min(art.width, art.height) for art in arts]
ball = scene.geometry.sphere(sides[0] * 0.32, material=accent)
ball.move_to(*arts[0].center).rotate_to_3d(*TILT)
targets = [
    scene.geometry.cube(sides[1] * 0.5, material=success).move_to(*arts[1].center).rotate_to_3d(0.6, 0.5, 0),
    scene.geometry.torus(sides[2] * 0.3, sides[2] * 0.12, material=danger).move_to(*arts[2].center).rotate_to_3d(1.1, 0, 0),
    scene.geometry.sphere(sides[0] * 0.32, material=accent).move_to(*arts[0].center),
]
labels = ("ball.animate.transform_to(home)", "ball.animate.transform_to(box)", "ball.animate.transform_to(ring)")
for (_, caption), label in zip(stops, labels):
    scene.text(label, role="code").place(caption)
scene.play([ball.animate.create()], duration=DRAW)
for target in targets:
    SEEKS.append(scene.cursor + MORPH / 2)
    scene.play([ball.animate.transform_to(target)], duration=MORPH)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
