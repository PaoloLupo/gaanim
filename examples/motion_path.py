"""Oriented motion along a path and arcs instead of straight moves."""

import math
import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("move_along(orient=True) · path_arc", role="title").fill(WHITE).move_to(0, 3.6)

route = scene.geometry.polyline([(x / 10, 1.2 * math.sin(x / 10 * 1.3) + 0.8) for x in range(-65, 66)])
route.no_fill().stroke(GRAY, 0.03)
plane = scene.geometry.polygon([(0.45, 0.0), (-0.3, 0.25), (-0.15, 0.0), (-0.3, -0.25)]).fill(CYAN)
plane.move_to(-6.5, 0.8)

ball = scene.geometry.dot(0.22).fill(GOLD).move_to(-4.0, -2.4)
straight = scene.geometry.dot(0.22).fill(CORAL).move_to(-4.0, -2.4)
scene.geometry.dot(0.08).fill(WHITE).move_to(4.0, -2.4)

scene.play([
    plane.animate.move_along(route, orient=True).duration(3.0),
    ball.animate.move_to(4.0, -2.4).path_arc(-math.pi / 2).duration(3.0),
    straight.animate.move_to(4.0, -2.4).duration(3.0),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.6, 1.5, 2.4])
else:
    scene.render()
