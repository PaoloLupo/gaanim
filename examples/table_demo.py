"""A compact technical table suitable for research-style explanations."""

import os

from gaanim import BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.title("Solver comparison").fill(WHITE).at(0, 250)
subtitle = scene.subtitle("Residual after 200 iterations").fill(GRAY).at(0, 195)
results = scene.table(
    ["Method", "Residual", "Elapsed"],
    [
        ["Baseline", "1.8e-1", "48 ms"],
        ["Cached", "7.2e-2", "32 ms"],
        ["GPU", "4.0e-2", "15 ms"],
    ],
    width=760,
    row_height=62,
).at(0, -45)

scene.play([
    title.write().duration(0.55),
    subtitle.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    results.fade_in_from(Direction.DOWN, distance=32).duration(0.7),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
