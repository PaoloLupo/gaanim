"""Spot lights: cones of light aimed at a point, as wide and as soft as asked, that sweep across a stage and cast shadows.

light_3d("spot", target=..., angle=..., softness=...) shines from where the
light stands toward target, in a cone angle radians wide on each side,
fading out over the outer softness of that angle. A spot keeps aiming at
its target as it moves, so moving it sweeps its cone across the stage, and
the solids it lights cast their shadows away from it. The label, pinned to
the frame, is the code that makes the spots of the segment.
"""

import os

from gaanim import Material3D, Scene, Zones

FADE = 0.6
SWEEP = 3.0
HOLD = 0.6
SEEKS = []

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
danger = scene.colors.danger
rule = scene.colors.rule
# Dim the sky and ground light, so that the spots shape the stage.
scene.geometry.lighting_3d("studio", intensity=0.25)
HOME = scene.camera.state_3d(eye=(0, 6, 11), target=(0, -0.5, 0), fov_y=0.7)
# Where the three solids stand on the floor.
SPOTS = [(-3.2, -1, 0), (0, -1, 0), (3.2, -1, 0)]


def stage(name, label):
    """Start segment ``name`` on the stage, captioned by ``label``."""
    scene.segment(name)
    scene.camera.to(HOME)
    band = scene.layout.zones(Zones.rows(["1fr", 0.5], padding="page_padding"))[1]
    scene.text(label, role="code").hud().place(band, fit="scale_down")
    scene.geometry.plane(12, 7, subdivisions=(48, 28), material=Material3D.matte(rule)).move_to_3d(0, -1, 0)
    for (x, y, z), solid in zip(SPOTS, ("cube", "sphere", "cone")):
        build = {
            "cube": lambda: scene.geometry.cube(1.2, material=Material3D.matte(accent)),
            "sphere": lambda: scene.geometry.sphere(0.7, material=Material3D.matte(success)),
            "cone": lambda: scene.geometry.cone(0.6, 1.4, material=Material3D.matte(danger)),
        }[solid]
        build().move_to_3d(x, y + 0.6, z)


def light_up(lights):
    """Switch ``lights`` on, and hold."""
    for light in lights:
        light.hidden()
    scene.play([light.animate.fade_in() for light in lights], duration=FADE)
    SEEKS.append(scene.cursor + HOLD / 2)
    scene.wait(HOLD)


# Three spots straight above the three solids, each cone wider.
stage("angle", 'light_3d("spot", target=solid, angle=a) for a in (0.15, 0.3, 0.45)')
light_up([
    scene.geometry.light_3d("spot", target=target, angle=angle, intensity=1.6).move_to_3d(target[0], 4, target[2])
    for target, angle in zip(SPOTS, (0.15, 0.3, 0.45))
])

# The same cone with a hard edge, half faded and faded from its middle.
stage("softness", 'light_3d("spot", angle=0.35, softness=s) for s in (0, 0.5, 1)')
light_up([
    scene.geometry.light_3d("spot", target=target, angle=0.35, softness=softness, intensity=1.6).move_to_3d(target[0], 4, target[2])
    for target, softness in zip(SPOTS, (0.0, 0.5, 1.0))
])

# One spot aimed at the middle solid sweeps from the left to the right, and
# the shadows of the solids turn away from it.
stage("sweep", 'light_3d("spot", target=(0, -1, 0)).animate.move_to_3d(5, 3, 2)')
spot = scene.geometry.light_3d("spot", target=(0, -1, 0), angle=0.5, intensity=1.8).move_to_3d(-5, 3, 2)
light_up([spot])
SEEKS.append(scene.cursor + SWEEP / 2)
scene.play([spot.animate.move_to_3d(5, 3, 2)], duration=SWEEP)
SEEKS.append(scene.cursor + HOLD / 2)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
