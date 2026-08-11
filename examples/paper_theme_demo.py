"""Verify the public paper theme against a white canvas."""

import os

from gaanim import BLUE, Direction, Scene


scene = Scene(1280, 720, margin=72)
scene.canvas.set_theme("paper")

title = scene.text("Technical documentation", role="title").at(0, 170)
subtitle = scene.text("White paper theme with restrained contrast", role="subtitle").at(0, 105)
body = scene.text("Unfilled text follows the theme foreground.").at(0, -10)
equation = scene.text("$F(k) = integral f(x) e^(-i k x) dif x$").at(0, -135)
marker = scene.circle(34).fill(BLUE).at(-360, -8)

scene.play([
    title.write().duration(0.5),
    subtitle.fade_in_from(Direction.DOWN, distance=18).duration(0.4),
    body.fade_in().duration(0.35),
    equation.fade_in().duration(0.45),
    marker.grow_from_center().duration(0.4),
])
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.65])
else:
    scene.render()
