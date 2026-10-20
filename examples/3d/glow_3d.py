"""Glow on 3D content: a halo around what the camera sees of primitives, 3D lines and glTF models.

glow() gives a 3D drawable a halo in scene units, as it gives a 2D one, and
animate.glow() grows and fades it. The halo surrounds the visible silhouette
and lies under the drawable and under what lies in front of it: the part of
the sphere the cube hides does not glow, and the cube covers the rest of the
halo. Each label is the call that gives the variant its glow.
"""

import math
import os

from gaanim import Scene, Zones

DRAW = 1.0
WALK = 2.0
HOLD = 0.6
# Turn the solids about x to show their top and about y to show a side.
TILT = (0.45, -0.6, 0)

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
rule = scene.canvas.color("rule")


def cells(count):
    """``count`` columns, each an area for the drawable above a caption."""
    columns = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [column.split(Zones.rows(["1fr", 0.5])) for column in columns]


def helix_points(zone):
    """Points of a helix of three turns that fills ``zone``."""
    x, y = zone.center
    radius, height = zone.width * 0.25, zone.height * 0.7
    points = []
    for k in range(181):
        t = k / 180
        angle = 6 * math.pi * t
        points.append((x + radius * math.cos(angle), y + height * (t - 0.5), radius * math.sin(angle)))
    return points


# The glow of a primitive and of a 3D line.
scene.segment("glow")
(sphere_art, sphere_caption), (helix_art, helix_caption) = cells(2)
sphere = scene.geometry.sphere(min(sphere_art.width, sphere_art.height) * 0.3)
sphere.move_to(*sphere_art.center).glow(accent, 0.3, 2.0)
helix = scene.geometry.polyline_3d(helix_points(helix_art), rule).stroke(rule, 0.05).glow(accent, 0.25, 2.0)
scene.text("sphere.glow(accent, 0.3, 2.0)", role="code").place(sphere_caption)
scene.text("helix.glow(accent, 0.25, 2.0)", role="code").place(helix_caption)
scene.play([shape.animate.fade_in() for shape in (sphere, helix)], duration=DRAW)
scene.wait(HOLD)

# An imported model keeps its glow while its animation deforms it.
scene.segment("model")
((wolf_art, wolf_caption),) = cells(1)
# The wolf is about 1.8 tall and, in three-quarter view, 2.3 long.
scale = 0.8 * min(wolf_art.height / 1.8, wolf_art.width / 2.3)
wolf = scene.media.gltf("Wolf.glb").scale_to(scale).rotate_to_3d(0.25, 0.9, 0)
wolf.move_to(*wolf_art.point("bottom")).glow(accent, 0.2, 2.0)
scene.text("wolf.glow(accent, 0.2, 2.0)", role="code").place(wolf_caption)
scene.play(wolf.animate.fade_in(), duration=DRAW)
scene.play(wolf.animation("Walk", loop=True), duration=WALK)
scene.wait(HOLD)

# animate.glow grows a halo and glow(None) fades it; a drawable in front
# hides the part it covers, which does not glow.
scene.segment("animate")
(cube_art, cube_caption), (hidden_art, hidden_caption) = cells(2)
size = min(cube_art.width, cube_art.height) * 0.4
cube = scene.geometry.cube(size).move_to(*cube_art.center).rotate_to_3d(*TILT)
behind = scene.geometry.sphere(size * 0.6).move_to(*hidden_art.center).shift_by(-size * 0.35, 0)
behind.glow(accent, 0.3, 2.0)
front = scene.geometry.cube(size * 0.8).move_to(*hidden_art.center).shift_by(size * 0.35, 0)
front.rotate_to_3d(*TILT).shift_by_3d(0, 0, size)
scene.text("cube.animate.glow(success, 0.5, 3.0)", role="code").place(cube_caption)
scene.text("behind.glow(accent, 0.3, 2.0)", role="code").place(hidden_caption)
scene.play([shape.animate.fade_in() for shape in (cube, behind, front)], duration=DRAW)
scene.play(cube.animate.glow(success, 0.5, 3.0), duration=DRAW)
scene.wait(HOLD)
scene.play(cube.animate.glow(None), duration=DRAW)
scene.wait(HOLD)

# The sphere's and the helix's glows, the wolf walking, and the cube's halo
# growing, whole and faded out.
model = DRAW + HOLD
animate = model + DRAW + WALK + HOLD
SEEKS = [
    DRAW + 0.2,
    model + DRAW + 1.3,
    animate + DRAW + 0.5,
    animate + 2 * DRAW + 0.3,
    animate + 3 * DRAW + HOLD + 0.3,
]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
