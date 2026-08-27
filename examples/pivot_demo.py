"""Rotate a grouped mechanism around a scene-space hinge."""

import math
import os

from gaanim import Easing, Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(640, 360, background=WHITE)

hinge = (-100, -45)
arm = scene.geometry.line(hinge[0], hinge[1], 70, -45).stroke(GRAY, 7)
mass = scene.geometry.circle(24).fill(GOLD).stroke(BLACK, 3).move_to(70, -45)
mechanism = scene.geometry.group([arm, mass]).with_pivot(*hinge)

scene.geometry.dot(11).fill(BLUE).move_to(*hinge)
scene.text("scene-space pivot").fill(GRAY).move_to(-35, 95, anchor=Anchor.CENTER)
scene.text("hinge").fill(GRAY).move_to(-100, -88, anchor=Anchor.CENTER)

scene.play([
    arm.animate.create().duration(0.5),
    mass.animate.create().duration(0.5),
])
scene.play([mechanism.animate.rotate_by(math.pi / 2).duration(1.2).easing(Easing.SMOOTH)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.1, 1.7])

scene.render()
