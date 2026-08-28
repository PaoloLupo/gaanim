"""Rotate a grouped mechanism around a scene-space hinge."""

import math
import os

from gaanim import Easing, Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=WHITE)

hinge = (-2.5, -1.125)
arm = scene.geometry.line(hinge[0], hinge[1], 1.75, -1.125).stroke(GRAY, 0.175)
mass = scene.geometry.circle(0.6).fill(GOLD).stroke(BLACK, 0.075).move_to(1.75, -1.125)
mechanism = scene.geometry.group([arm, mass]).with_pivot(*hinge)

scene.geometry.dot(0.275).fill(BLUE).move_to(*hinge)
scene.text("scene-space pivot").fill(GRAY).move_to(-0.875, 2.375, anchor=Anchor.CENTER)
scene.text("hinge").fill(GRAY).move_to(-2.5, -2.2, anchor=Anchor.CENTER)

scene.play([
    arm.animate.create().duration(0.5),
    mass.animate.create().duration(0.5),
])
scene.play([mechanism.animate.rotate_by(math.pi / 2).duration(1.2).easing(Easing.SMOOTH)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.1, 1.7])

scene.render()
