"""Falloffs: a cursor sweeps a 20x12 grid that scales, tints, fades and turns toward it."""

import math
import os

from gaanim import BLUE, GOLD, WHITE, Distribution, Easing, Falloff, Scene

scene = Scene(frame=(16, 9), background="#0f172a")

arrow = scene.geometry.regular_polygon(3, 0.17).fill(BLUE).no_stroke()
grid = scene.geometry.duplicate(arrow, Distribution.grid(20, 12, 0.7))
cursor = scene.geometry.dot(0.12).fill(WHITE).move_to(-6.5, 0)

# One falloff per idea; each drive connects it to a channel of all 240 members.
near = Falloff.distance(cursor, radius=2.5, falloff="smooth")
grid.drive("scale", near.remap(1.0, 1.9))
grid.drive("fill", near.gradient(BLUE, GOLD))
grid.drive("opacity", Falloff.index(easing=Easing.SMOOTH).remap(0.35, 1.0))
# A triangle points up; the offset makes its tip look at the cursor.
grid.look_at(cursor, offset=-math.pi / 2)

scene.play([cursor.animate.move_to(6.5, 0).duration(3.0).easing(Easing.SMOOTH)])
scene.play([cursor.animate.move_to(0, 3.0).duration(1.5).easing(Easing.SMOOTH)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.75, 1.5, 2.25, 3.0, 4.0, 5.0])
else:
    scene.render()
