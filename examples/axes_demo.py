"""Configurable scientific axes: grid, ticks, numbers, and independent styles."""

import math
import os

from gaanim import BLACK, BLUE, GRAY, WHITE, Scene


scene = Scene(800, 480, background=WHITE)

axes = scene.axes(
    x=(-6, 6, 1),
    y=(-1, 1, 0.5),
    grid=True,
    ticks=True,
    numbers=True,
    x_grid=True,
    y_grid=True,
    x_ticks=True,
    y_ticks=True,
    x_numbers=True,
    y_numbers=True,
    x_label="x",
    y_label="f(x)",
    axis_color=BLACK,
    grid_color=GRAY,
    tick_color=BLUE,
    number_color=BLUE,
    label_color=BLACK,
    axis_width=3,
    grid_width=1,
    tick_width=3,
    tick_length=14,
    auto_fit=True,  # ocupa safe_frame (manim-like width)
)
# axes.plot mapea datos -> escena usando el mismo auto_fit, estilo propio
curve = scene.plot(axes, lambda x: math.cos(x), x=(-3, 3), samples=200).no_fill().stroke(BLUE, 4)
title = scene.text("configurable axes — auto-fit + plot").fill(BLACK).at(0, 210)

# create secuencial por capas: Grid → Axes → Ticks → Numbers/Labels
scene.play([axes.create().duration(1.2)])
scene.play([curve.create().duration(0.9), title.write().duration(0.5)])
scene.wait(1.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.6, 1.2, 2.1, 3.0])

scene.render()
