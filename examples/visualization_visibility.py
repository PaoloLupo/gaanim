"""Visibility controls for typed Cartesian, polar, and number-line spaces."""

import os

from gaanim import Anchor, Axis, BLACK, CYAN, GOLD, GREEN, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)

plane = scene.viz.cartesian_2d(
    Axis.linear(-4, 4).ticks(1).minor_ticks(2).label("x").style(color=WHITE),
    Axis.linear(-2, 2).ticks(1).label("y").style(color=WHITE),
    width=480,
    height=260,
    grid=False,
    x_grid=True,
    numbers=False,
    y_numbers=True,
    labels=False,
    x_labels=True,
).at(-330, 120)
curve = plane.plot(lambda x: 0.12 * x * x - 1).stroke(CYAN, 3)

polar = scene.viz.polar(
    Axis.linear(0, 4).ticks(1).label("r").style(color=WHITE),
    radius=125,
    angle_divisions=10,
    grid=False,
    rings=True,
    axes=False,
    numbers=False,
).drawable().at(330, 120)

line = scene.viz.number_line(
    Axis.linear(0, 6).ticks(1).label("t").style(color=WHITE),
    length=620,
    ticks=False,
    numbers=True,
    labels=False,
).drawable().at(0, -245)

title = scene.text("Visibility: per-axis, rings, and annotations").fill(GOLD).at(
    0, 315, anchor=Anchor.CENTER
)
scene.play([plane.write(0.9), curve.create(0.9), polar.create(0.9), line.create(0.9), title.write(0.5)])
scene.wait(0.4)
scene.play([
    plane.fade_out(0.4),
    curve.fade_out(0.4),
    polar.fade_out(0.4),
    line.fade_out(0.4),
    title.fade_out(0.4),
])

space_3d = scene.viz.cartesian_3d(
    Axis.linear(-3, 3).ticks(1).label("x").style(color=WHITE),
    Axis.linear(-2, 2).ticks(1).label("y").style(color=WHITE),
    Axis.linear(-2, 2).ticks(1).label("z").style(color=WHITE),
    size=(6, 4, 4),
    grid=False,
    xy_grid=True,
    axes=False,
    x_axis=True,
    y_axis=True,
    numbers=False,
    z_numbers=True,
    labels=False,
    z_labels=True,
)
space_3d.parametric(lambda t: (t, 0.35 * t * t - 1, 0.5 * t), (-3, 3), samples=120).stroke(
    GREEN, 3
)
title_3d = scene.text("Visibility: XY grid with selected 3D axes").fill(GOLD).hud().at(
    0, 315, anchor=Anchor.CENTER
)
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000, duration=0.0)
scene.camera.look_at(eye=(9, 7, 9), target=(0, 0, 0), duration=0.0)
scene.play([space_3d.create(1.0), title_3d.write(0.5)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.45, 1.2, 1.9, 2.6])
else:
    scene.render()
