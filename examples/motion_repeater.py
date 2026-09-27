"""Repeaters and duplicators: one segment per distribution."""

import math
import os

from gaanim import BLUE, CORAL, CYAN, GOLD, PINK, WHITE, Distribution, Easing, Scene


scene = Scene(frame=(16, 9), background="#0d1322")


def title(name):
    scene.text(name, size=0.5).fill(WHITE).move_to(0, 3.6)


scene.segment("repeat")
title("repeat")
petal = scene.geometry.ellipse(0.28, 1.0).fill(PINK).move_to(0, 1.3)
flower = scene.geometry.repeat(petal, 12, rotate=math.tau / 12, about=(0, 0), opacity=(1.0, 0.35))
tunnel_square = scene.geometry.square(5.0).no_fill().stroke(CYAN, 0.05).move_to(0, 0)
tunnel = scene.geometry.repeat(tunnel_square, 8, rotate=0.12, scale=0.82, opacity=(1.0, 0.2))
scene.wait(0.5)

scene.segment("grid")
title("Distribution.grid")
scene.geometry.duplicate(scene.geometry.dot(0.07).fill(CYAN), Distribution.grid(14, 7, 0.9))
scene.wait(0.5)

scene.segment("circle")
title("Distribution.circle")
arrow = scene.geometry.polygon([(-0.3, -0.2), (0.35, 0.0), (-0.3, 0.2)]).fill(GOLD)
scene.geometry.duplicate(arrow, Distribution.circle(16, 2.6, orient=True))
scene.wait(0.5)

scene.segment("along")
title("Distribution.along")
chevron = scene.geometry.polygon([(-0.2, -0.3), (0.25, 0.0), (-0.2, 0.3), (-0.05, 0.0)]).fill(CORAL)
scene.geometry.duplicate(chevron, Distribution.along([(-6, -2), (-2, 2), (2, -2), (6, 2)], 24, orient=True))
scene.wait(0.5)

scene.segment("random")
title("Distribution.random")
star = scene.geometry.star(5, 0.18, 0.08).fill(GOLD)
scene.geometry.duplicate(star, Distribution.random(60, (-7, -3.2, 7, 3.0), seed=3))
scene.wait(0.5)

scene.segment("phyllotaxis")
title("Distribution.phyllotaxis")
seed = scene.geometry.circle(0.08).fill(BLUE)
scene.geometry.duplicate(seed, Distribution.phyllotaxis(320, 0.17))
scene.wait(0.5)

scene.segment("count")
title("animate.count")
leaf = scene.geometry.ellipse(0.12, 0.34).fill(GOLD).move_to(0, 2.4)
spiral = scene.geometry.repeat(leaf, 36, rotate=0.5, scale=0.95, about=(0, 0)).count(0)
scene.play([spiral.animate.count(36).duration(1.2).easing(Easing.LINEAR)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    seeks = [0.25 + 0.5 * index for index in range(6)]
    scene.snapshots(snapshots, seeks + [3.0, 3.55, 4.4])
else:
    scene.render()
