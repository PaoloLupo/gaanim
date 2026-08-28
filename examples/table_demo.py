"""A compact technical table suitable for research-style explanations."""

import os

from gaanim import Anchor, BLACK, GRAY, WHITE, Direction, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.7)

title = scene.text("Solver comparison", role="title").fill(WHITE).move_to(0, 3.125, anchor=Anchor.CENTER)
subtitle = scene.text("Residual after 200 iterations", role="subtitle").fill(GRAY).move_to(0, 2.4375, anchor=Anchor.CENTER)
results = scene.slides.table(
    ["Method", "Residual", "Elapsed"],
    [
        ["Baseline", "1.8e-1", "48 ms"],
        ["Cached", "7.2e-2", "32 ms"],
        ["GPU", "4.0e-2", "15 ms"],
    ],
    width=9.5,
    row_height=0.775,
).move_to(0, -0.5625)

scene.play([
    title.animate.write().duration(0.55),
    subtitle.animate.fade_in_from(Direction.DOWN, distance=0.3).duration(0.45),
    results.animate.fade_in_from(Direction.DOWN, distance=0.4).duration(0.7),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
