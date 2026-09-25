"""repeat, yoyo, loop, and a repeated composition."""

import math
import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene, parallel


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("repeat · yoyo · loop", role="title").fill(WHITE).move_to(0, 3.6)

spinner = scene.geometry.square(1.2).fill(CYAN).move_to(-4.5, 0.5)
badge = scene.geometry.circle(0.7).fill(GOLD).move_to(-1.5, 0.5)
arrow = scene.geometry.arrow(0.8, 0.5, 2.4, 0.5, head_length=0.4, head_width=0.4, body_width=0.12).fill(CORAL).stroke(CORAL, 0.02)
pair = [scene.geometry.dot(0.2).fill(WHITE).move_to(4.5 + dx, 0.5) for dx in (-0.6, 0.6)]
for x, label in [(-4.5, "repeat(4)"), (-1.5, "repeat(4, yoyo)"), (1.6, 'loop("pingpong")'), (4.5, "parallel().repeat(2)")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -2.4)

scene.play([
    spinner.animate.rotate_by(math.pi / 2).duration(0.5).repeat(4),
    badge.animate.scale_to(1.3).duration(0.4).repeat(4, yoyo=True, delay=0.1),
    arrow.animate.shift_by(1.0, 0).duration(0.5).loop("pingpong", until=2.0),
    parallel(pair[0].animate.shift_by(0, 0.7).duration(0.5),
             pair[1].animate.shift_by(0, -0.7).duration(0.5)).repeat(2, delay=0.5),
])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-cycle, second cycle, and the rest pose after the even yoyo.
    scene.snapshots(snapshots, [0.25, 0.75, 1.3, 2.2])
else:
    scene.render()
