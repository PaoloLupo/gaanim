"""A compact vector code block for a technical explainer."""

import os

from gaanim import BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.title("Integration step").fill(WHITE).at(0, 255)
subtitle = scene.subtitle("Explicit Euler update").fill(GRAY).at(0, 200)
snippet = scene.code(
    "velocity += acceleration * dt\nposition += velocity * dt",
    language="python",
    width=760,
    height=260,
    font_size=22,
).at(0, -55)

scene.play([
    title.write().duration(0.55),
    subtitle.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    snippet.fade_in_from(Direction.DOWN, distance=32).duration(0.65),
])
scene.wait(0.35)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
