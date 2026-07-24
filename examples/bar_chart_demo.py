"""A restrained bar chart for technical and scientific explainers."""

import os

from gaanim import BLACK, BLUE, GRAY, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)

title = scene.title("Convergence benchmark").fill(WHITE).at(0, 245)
subtitle = scene.subtitle("Elapsed time (ms) — lower is better").fill(GRAY).at(0, 190)
chart = scene.bar_chart(
    [48, 32, 21, 15],
    labels=["Baseline", "Cached", "GPU", "Optimized"],
    width=780,
    height=340,
    color=BLUE,
).at(0, -35)

scene.play([
    title.write().duration(0.55),
    subtitle.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    chart.grow_from_center().duration(0.7),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
