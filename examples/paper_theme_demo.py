"""Verify the public paper theme against a white canvas."""

import os

from gaanim import Anchor, BLUE, Direction, Scene


scene = Scene(1280, 720, margin=72)
scene.canvas.set_theme("paper")

title = scene.text("Technical documentation", role="title").move_to(0, 170, anchor=Anchor.CENTER)
subtitle = scene.text("White paper theme with restrained contrast", role="subtitle").move_to(0, 105, anchor=Anchor.CENTER)
body = scene.text("Unfilled text follows the theme foreground.").move_to(0, -10, anchor=Anchor.CENTER)
equation = scene.text.equation("F(k) = integral f(x) e^(-i k x) dif x").move_to(0, -135, anchor=Anchor.CENTER)
marker = scene.geometry.circle(34).fill(BLUE).move_to(-360, -8)

scene.play([
    title.animate.write().duration(0.5),
    subtitle.animate.fade_in_from(Direction.DOWN, distance=18).duration(0.4),
    body.animate.fade_in().duration(0.35),
    equation.animate.fade_in().duration(0.45),
    marker.animate.grow_from_center().duration(0.4),
])
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.65])
else:
    scene.render()
