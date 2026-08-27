"""Typed 3D axes with native surface and parametric-curve sampling."""

import math
import os

from gaanim import Anchor, Axis, BLACK, GOLD, RED, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)
axes = scene.viz.cartesian_3d(
    Axis.linear(-5, 5).ticks(1).label("x").style(color=WHITE),
    Axis.linear(-5, 5).ticks(1).label("y").style(color=WHITE),
    Axis.linear(-3, 3).ticks(1).label("z").style(color=WHITE),
    size=(10, 10, 6),
)

surface = axes.surface(lambda x, y: math.sin(x) * math.cos(y), resolution=(36, 36))
helix = axes.parametric(
    lambda t: (2 * math.cos(t), 2 * math.sin(t), 0.2 * t - 2),
    (0, 8 * math.pi),
    samples=320,
).stroke(RED, 3)
title = scene.text("Cartesian3D + surface + parametric").fill(GOLD).hud().at(0, 310, anchor=Anchor.CENTER)

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000, duration=0.0)
scene.camera.look_at(eye=(11, 8, 11), target=(0, 0, 0), duration=1.0)
scene.play([axes.create(1.0), title.write(0.6)])
scene.play([surface.create(1.0), helix.create(1.0)])
scene.camera.orbit(delta_yaw=0.8, delta_pitch=0.2, duration=1.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 1.5, 2.8, 4.4])
else:
    scene.render()
