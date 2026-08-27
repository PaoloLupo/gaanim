"""A reactive, table-backed bar chart for educational videos."""

import os

from gaanim import Anchor, Axis, BLACK, BLUE, ChartSpec, GRAY, WHITE, Direction, Scene, Value


scene = Scene(1920, 1080, background=BLACK, margin=56)
labels = ["Baseline", "Cached", "GPU", "Optimized"]
data = {"method": labels, "elapsed": [48, 32, 21, 15]}
spec = (
    ChartSpec(data, key="method")
    .mark("bar", width=0.72, label_offset=20)
    .encode(x="method", y="elapsed", color=Value(BLUE), label="elapsed")
    .axes(
        x=Axis.category(labels).style(
            color=WHITE, tick_color=WHITE, number_color=WHITE, label_color=WHITE
        ),
        y=Axis.linear(0, 55).ticks(10).label("ms").style(
            color=WHITE, tick_color=WHITE, number_color=WHITE, label_color=WHITE
        ),
    )
)
chart = scene.viz.chart(spec).at(0, -35)

title = scene.text("Convergence benchmark", role="title").fill(WHITE).at(0, 400, anchor=Anchor.CENTER)
subtitle = scene.text("Elapsed time (ms) — lower is better", role="subtitle").fill(GRAY).at(0, 350, anchor=Anchor.CENTER)
scene.play([
    title.write().duration(0.55),
    subtitle.fade_in_from(Direction.DOWN, distance=24).duration(0.45),
    chart.layer("axes").create().duration(0.7),
    chart.layer("marks").write().duration(0.7),
    chart.layer("labels").write().duration(0.7),
])
scene.wait(0.31)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.55, 1.0])
else:
    scene.render()
