"""A compact vector code block for a technical explainer."""

import os

from gaanim import Anchor, BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.7)

title = scene.text("Integration step", role="title").fill(WHITE).move_to(0, 3.1875, anchor=Anchor.CENTER)
subtitle = scene.text("Explicit Euler update", role="subtitle").fill(GRAY).move_to(0, 2.5, anchor=Anchor.CENTER)
snippet = scene.text.code(
    "velocity += acceleration * dt\nposition += velocity * dt",
    language="python",
    width=9.5,
    height=3.25,
    font_size=0.275,
).move_to(0, -0.6875)

scene.play([
    title.animate.write().duration(0.55),
    subtitle.animate.fade_in_from(Direction.DOWN, distance=0.3).duration(0.45),
    snippet.animate.fade_in_from(Direction.DOWN, distance=0.4).duration(0.65),
])
scene.wait(0.35)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
