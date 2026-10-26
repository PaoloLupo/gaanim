"""3D morphs: animate.transform_to turns one mesh into another where that one stands.

Both meshes are sampled on one grid, and each grid point moves from one to the
other. A mesh whose texture coordinates cover it once (a sphere, a torus, a
revolved profile) is sampled by them, so a sphere opens its poles into a
torus's hole and a vase rounds into a sphere along its profile; any other (a
cube, a capped cylinder or cone)
by rays from its center, so a sphere swells into a cube point by point. The
target is a template that stays hidden: the morphing mesh also takes its
place, turn, size and material. Each label is the call that plays the morph,
and before and after it every mesh is drawn exactly as built.
"""

import os

from gaanim import Material3D, Scene, Zones

DRAW = 0.8
MORPH = 2.0
HOLD = 0.8
# Turn every mesh about x to show its top and about y to show a side.
TILT = (0.45, -0.6, 0)
# A vase's (r, y) profile, from the axis at its foot up to the axis at its
# mouth, in fractions of its zone around its middle.
VASE = [(0, -0.3), (0.2, -0.3), (0.25, -0.12), (0.12, 0.12), (0.16, 0.3), (0, 0.3)]
SEEKS = []

scene = Scene()
accent = Material3D.matte(scene.colors.accent)
success = Material3D.matte(scene.colors.success)
danger = Material3D.matte(scene.colors.danger)


def morph(plays):
    """Play ``plays`` together and hold the result, noting the instants to capture."""
    SEEKS.append(scene.cursor + MORPH / 2)
    scene.play(plays, duration=MORPH)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


def morphs(variants):
    """Morph each (label, source, target) pair in its own column, above its label.

    A pair's meshes stand in the same zone, sized to it by ``build(side)``;
    the sources grow in, then morph together.
    """
    columns = scene.layout.zones(Zones.columns(len(variants), gap="48px", padding="page_padding"))
    sources = []
    plays = []
    for zone, (label, build_source, build_target) in zip(columns, variants):
        art, caption = zone.split(Zones.rows(["1fr", 0.4]))
        side = min(art.width, art.height)
        source = build_source(side).move_to(*art.center).rotate_to_3d(*TILT)
        target = build_target(side).move_to(*art.center).rotate_to_3d(*TILT)
        scene.text(label, role="code").place(caption)
        sources.append(source)
        plays.append(source.animate.transform_to(target))
    scene.play([source.animate.create() for source in sources], duration=DRAW)
    SEEKS.append(scene.cursor)
    morph(plays)


# Spheres, tori and revolved profiles are sampled by their texture coordinates.
scene.segment("coordinates")
morphs([
    ("ball.animate.transform_to(ring)",
     lambda side: scene.geometry.sphere(side * 0.3, material=accent),
     lambda side: scene.geometry.torus(side * 0.28, side * 0.11, material=success)),
    ("vase.animate.transform_to(ball)",
     lambda side: scene.geometry.revolve(
         [(r * side, y * side) for r, y in VASE], material=accent,
     ),
     lambda side: scene.geometry.sphere(side * 0.28, material=success)),
])

# Cubes and capped cylinders and cones are sampled by rays from their center.
scene.segment("rays")
morphs([
    ("ball.animate.transform_to(box)",
     lambda side: scene.geometry.sphere(side * 0.3, material=accent),
     lambda side: scene.geometry.cube(side * 0.46, material=success)),
    ("can.animate.transform_to(cone)",
     lambda side: scene.geometry.cylinder(side * 0.24, side * 0.56, material=accent),
     lambda side: scene.geometry.cone(side * 0.3, side * 0.56, material=danger)),
])

# One mesh morphs through three targets in turn, each from the last: it moves
# to where each one stands and takes its turn, size and material. The caption
# below names each call as it plays.
scene.segment("chain")
page = scene.layout.zones(Zones.rows(["1fr", "80px"]))
stops = page[0].inset("page_padding").split(Zones.columns(3, gap="48px"))
strip = page[1].inset((0, "page_padding"))
side = min(stops[0].width, stops[0].height)
ball = scene.geometry.sphere(side * 0.3, material=accent).move_to(*stops[0].center).rotate_to_3d(*TILT)
calls = [
    ("ball.animate.transform_to(box)",
     scene.geometry.cube(side * 0.46, material=success).move_to(*stops[1].center).rotate_to_3d(0.6, 0.5, 0)),
    ("ball.animate.transform_to(ring)",
     scene.geometry.torus(side * 0.28, side * 0.11, material=danger).move_to(*stops[2].center).rotate_to_3d(1.1, 0, 0)),
    ("ball.animate.transform_to(home)",
     scene.geometry.sphere(side * 0.3, material=accent).move_to(*stops[0].center)),
]
captions = [scene.text(label, role="code").place(strip, anchor="left").hidden() for label, _ in calls]
scene.play([ball.animate.create()], duration=DRAW)
for index, (_, target) in enumerate(calls):
    swap = [captions[index].animate.fade_in().duration(0.2)]
    if index:
        swap.append(captions[index - 1].animate.fade_out().duration(0.15))
    morph(swap + [ball.animate.transform_to(target).duration(MORPH)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
