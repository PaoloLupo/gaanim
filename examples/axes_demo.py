"""Configurable scientific axes: grid, ticks, numbers, and independent styles."""

import os

from gaanim import BLACK, BLUE, GRAY, WHITE, Scene


scene = Scene(800, 480, background=WHITE)

axes = scene.axes(
    x=(-300, 300, 100),
    y=(-180, 180, 60),
    grid=True,
    ticks=True,
    numbers=True,
    x_grid=True,
    y_grid=False,
    x_ticks=True,
    y_ticks=True,
    x_numbers=True,
    y_numbers=False,
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
)
curve = scene.function_graph(lambda x: 0.008 * x * x - 60, x=(-250, 150), samples=160).no_fill().stroke(BLUE, 4)
title = scene.text("configurable axes").fill(BLACK).at(190, 210)

scene.play([curve.create().duration(0.8), title.write().duration(0.5)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.25, 0.5, 0.8])

scene.render()
