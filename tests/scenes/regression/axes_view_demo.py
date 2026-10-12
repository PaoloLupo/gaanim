"""Unequal domain zooms keep Cartesian axis text legible, including on rewind."""

import os

from gaanim import Axis, BLACK, BLUE, Scene

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
scene.text("Zoom del dominio: X e Y independientes").fill(BLACK).move_to(0, 4.0)
axes = scene.viz.cartesian_2d(
    Axis.linear(0, 0.4).ticks(0.1).label("Distorsión (%)", position="center"),
    Axis.linear(0, 4).ticks(1).label("Nivel"),
    width=4.0,
    height=2.5,
).move_to(-1.0, -0.7)
axes.plot(lambda x: 10 * x, samples=40).stroke(BLUE, 0.03)
scene.wait(0.2)
scene.play([axes.animate.view_to((0, 0.2), (0, 4)).duration(1.0)])
scene.play([axes.animate.view_to((0, 0.4), (0, 2)).duration(1.0)])
scene.play([axes.animate.view_to((0, 0.4), (0, 4)).duration(1.0)])
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.2, 1.7, 2.2, 3.2, 0.7])
else:
    scene.render()
