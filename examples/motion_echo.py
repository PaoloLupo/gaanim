"""Echo: fading copies trail a ball, a spinning bar and a written title."""

import os

from gaanim import CORAL, GOLD, TEAL, WHITE, Scene


scene = Scene(frame=(16, 9), background="#0e1422")

ball = scene.geometry.circle(0.45).fill(CORAL).move_to(-6.0, 1.5).echo(6, delay=0.05, decay=0.7)
bar = scene.geometry.rect(3.0, 0.3).fill(TEAL).move_to(0.0, -1.8).echo(4, delay=0.06)
title = scene.text("Echo", role="title").fill(WHITE).move_to(0, 3.3).echo(3, delay=0.1, decay=0.5)
dot = scene.geometry.circle(0.2).fill(GOLD).move_to(5.0, -3.0)

scene.play([title.animate.write().duration(1.0)])
scene.play(
    [
        ball.animate.move_to(6.0, 1.5).duration(1.2),
        bar.animate.rotate_by(3.1416).duration(1.2),
        dot.animate.move_to(-5.0, -3.0).duration(1.2),
    ]
)
scene.play([ball.animate.move_to(0.0, -0.2).fill(GOLD).duration(0.6)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Title copies while it writes, copies mid-motion, the color change
    # reaching the copies one by one, and the copies caught up at rest.
    scene.snapshots(snapshots, [0.5, 1.5, 2.0, 2.45, 3.25])
else:
    scene.render()
