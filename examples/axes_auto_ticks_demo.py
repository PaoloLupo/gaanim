"""Automatic axis ticks regenerate for every `view_to` window."""

import os

from gaanim import Axis, BLACK, BLUE, Scene

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
scene.text("Ticks automáticos al cambiar la vista").fill(BLACK).move_to(0, 4.0)
axes = scene.viz.cartesian_2d(
    Axis.linear(0, 0.4).minor_ticks(2).label("x"),
    Axis.linear(0, 4).label("y"),
    width=9.0,
    height=6.0,
).move_to(0, -0.4)
axes.plot(lambda x: 10 * x).stroke(BLUE, 0.03)
scene.wait(0.2)
scene.play([axes.animate.view_to((0, 0.2), (0, 4)).duration(1.0)])    # zoom in x
scene.play([axes.animate.view_to((0, 0.2), (0, 2)).duration(1.0)])    # zoom in y
scene.play([axes.animate.view_to((0, 1.6), (0, 16)).duration(1.0)])   # zoom out
scene.play([axes.animate.view_to((0, 0.4), (0, 4)).duration(1.0)])    # back
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.2, 2.2, 2.7, 3.2, 4.4, 0.7])
else:
    scene.render()
