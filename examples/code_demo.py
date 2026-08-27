"""A compact vector code block for a technical explainer."""

import os

from gaanim import Anchor, BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.text("Integration step", role="title").fill(WHITE).move_to(0, 255, anchor=Anchor.CENTER)
subtitle = scene.text("Explicit Euler update", role="subtitle").fill(GRAY).move_to(0, 200, anchor=Anchor.CENTER)
snippet = scene.text.code(
    "velocity += acceleration * dt\nposition += velocity * dt",
    language="python",
    width=760,
    height=260,
    font_size=22,
).move_to(0, -55)

scene.play([
    title.animate.write().duration(0.55),
    subtitle.animate.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    snippet.animate.fade_in_from(Direction.DOWN, distance=32).duration(0.65),
])
scene.wait(0.35)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
