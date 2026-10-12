"""Compare retained stroke widths before and during unequal domain zooms."""

import os

from gaanim import Axis, BLACK, CORAL, TEAL, Scene

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")

def plot(width, height, x, title):
    scene.text(title).fill(BLACK).move_to(x, 3.5)
    axes = scene.viz.cartesian_2d(
        Axis.linear(0, 0.4).ticks(0.1).label("Distorsión (%)", position="center"),
        Axis.linear(0, 4).ticks(1).label("Nivel"),
        width=width,
        height=height,
    ).move_to(x, -1.0)
    axes.plot_data([0.1, 0.17, 0.17, 0.12], [1, 2, 3, 4], color=CORAL, width=0.04)
    axes.plot_data([0.12, 0.20, 0.18, 0.15], [1, 2, 3, 4], color=TEAL, width=0.04)
    return axes

reference = plot(4.0, 4.0, -4.0, "Vista de referencia")
zoomed = plot(2.0, 2.0, 2.5, "Acercamiento en X / Y")
scene.wait(0.2)
scene.play([zoomed.animate.view_to((0, 0.2), (0, 4)).duration(1.0)])
scene.play([zoomed.animate.view_to((0, 0.2), (0, 2)).duration(1.0)])
scene.play([zoomed.animate.view_to((0, 0.4), (0, 4)).duration(1.0)])
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.7, 1.2, 1.7, 2.2, 3.2, 0.7])
else:
    scene.render()
