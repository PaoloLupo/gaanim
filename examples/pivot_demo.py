"""Rotate a grouped mechanism around a scene-space hinge."""

import math
import os

from gaanim import Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(640, 360, background=WHITE)

hinge = (-100, -45)
arm = scene.line(hinge[0], hinge[1], 70, -45).stroke(GRAY, 7)
mass = scene.circle(24).fill(GOLD).stroke(BLACK, 3).at(70, -45)
mechanism = scene.group([arm, mass]).with_pivot(*hinge)

scene.dot(11).fill(BLUE).at(*hinge)
scene.text("scene-space pivot").fill(GRAY).at(-35, 95, anchor=Anchor.CENTER)
scene.text("hinge").fill(GRAY).at(-100, -88, anchor=Anchor.CENTER)

scene.play([
    arm.create().duration(0.5),
    mass.create().duration(0.5),
])
scene.play([mechanism.rotate(math.pi / 2).duration(1.2).smooth()])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.1, 1.7])

scene.render()
