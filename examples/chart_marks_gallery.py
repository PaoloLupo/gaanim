"""Visual gallery for every declarative ChartSpec mark."""

import os

from gaanim import Anchor, BLACK, ChartSpec, Scene

scene = Scene(1600, 900, background=BLACK)
series = {
    "id": ["a", "b", "c", "d", "e"],
    "x": [-2, -1, 0, 1, 2],
    "y": [1.0, 2.4, 1.6, 3.2, 2.1],
    "low": [0.7, 2.0, 1.1, 2.8, 1.7],
    "high": [1.4, 2.8, 2.0, 3.7, 2.6],
}
grid = {
    "id": [f"{x}:{y}" for y in (-1, 0, 1) for x in (-1, 0, 1)],
    "x": [x for y in (-1, 0, 1) for x in (-1, 0, 1)],
    "y": [y for y in (-1, 0, 1) for x in (-1, 0, 1)],
    "value": [x * x - y for y in (-1, 0, 1) for x in (-1, 0, 1)],
}

specs = [
    ("point", ChartSpec(series, key="id").mark("point", radius=18).encode(x="x", y="y")),
    ("line", ChartSpec(series, key="id").mark("line").encode(x="x", y="y")),
    ("step", ChartSpec(series, key="id").mark("step").encode(x="x", y="y")),
    ("area", ChartSpec(series, key="id").mark("area").encode(x="x", y="y")),
    ("bar", ChartSpec(series, key="id").mark("bar").encode(x="x", y="y")),
    ("histogram", ChartSpec(series, key="id").mark("histogram", bins=5).encode(x="y")),
    ("box", ChartSpec(series, key="id").mark("box", center=0.5, width=0.6).encode(y="y")),
    ("violin", ChartSpec(series, key="id").mark("violin", center=0.5, width=0.6).encode(y="y")),
    (
        "error_bar",
        ChartSpec(series, key="id")
        .mark("error_bar", low="low", high="high")
        .encode(x="x", y="y"),
    ),
    (
        "heatmap",
        ChartSpec(grid, key="id").mark("heatmap").encode(x="x", y="y", color="value"),
    ),
    (
        "surface",
        ChartSpec(grid, key="id").mark("surface").encode(x="x", y="y", z="value"),
    ),
]

charts = []
for index, (name, spec) in enumerate(specs):
    column, row = index % 6, index // 6
    x, y = -650 + column * 260, 205 - row * 390
    if name == "surface":
        # The perspective camera uses world units while the 2D panels use
        # canvas pixels. At eye.z=18 this conversion aligns the hybrid cells.
        chart = scene.chart(spec).scaled(0.2).at_3d(x / 60, y / 60, 0)
    else:
        chart = scene.chart(spec).scaled(0.17).at(x, y)
    label = scene.text(name).hud().scaled(0.55).at(x, y + 150, anchor=Anchor.CENTER)
    charts.extend([chart.drawable(), label])

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000, duration=0.0)
scene.camera.look_at(eye=(0, 0, 18), target=(0, 0, 0), duration=0.0)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Native 3D resources are guaranteed ready after the first rendered frame.
    scene.snapshots(snapshots, [0.5, 1.0])
else:
    scene.render()
