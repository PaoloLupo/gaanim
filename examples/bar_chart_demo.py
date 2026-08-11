"""A reactive, table-backed bar chart for educational videos."""

import os

from gaanim import Axis, BLACK, BLUE, DataSource, GRAY, WHITE, Direction, Scene


scene = Scene(1280, 720, background=BLACK, margin=56)
labels = ["Baseline", "Cached", "GPU", "Optimized"]
data = DataSource(
    {"method": labels, "x": [0, 1, 2, 3], "elapsed": [48, 32, 21, 15]},
    key="method",
)

space = scene.axes(
    Axis.category(labels).style(color=WHITE, number_color=WHITE),
    Axis.linear(0, 55).ticks(10).label("ms").style(color=WHITE, number_color=WHITE),
    width=780,
    height=340,
).at(0, -35)
bars = space.bars(data, "x", "elapsed", width=0.72).fill(BLUE)

title = scene.text("Convergence benchmark", role="title").fill(WHITE).at(0, 245)
subtitle = scene.text("Elapsed time (ms) — lower is better", role="subtitle").fill(GRAY).at(0, 190)
scene.play([
    title.write().duration(0.55),
    subtitle.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    space.create().duration(0.7),
    bars.grow_from_center().duration(0.7),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
