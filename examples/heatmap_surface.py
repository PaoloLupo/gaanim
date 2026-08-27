"""A keyed rectangular grid morphs from a heatmap into a native 3D surface."""

import math
import os

from gaanim import Axis, BLACK, ChartSpec, Field, Guide, Scale, Scene

scene = Scene(1280, 720, background=BLACK)
xs = [-2, -1, 0, 1, 2]
ys = [-2, -1, 0, 1, 2]
rows = [(x, y, math.sin(x) * math.cos(y)) for y in ys for x in xs]
data = {
    "cell": [f"{x}:{y}" for x, y, _ in rows],
    "x": [x for x, _, _ in rows],
    "y": [y for _, y, _ in rows],
    "value": [value for _, _, value in rows],
}

heatmap = (
    ChartSpec(data, key="cell")
    .mark("heatmap", cell_width=1.0, cell_height=1.0, bands=12)
    .encode(x="x", y="y", color=Field("value", scale=Scale.symlog((-1, 1))))
    .guides(color=Guide.colorbar(title="sin(x) cos(y)"))
)
surface = (
    ChartSpec(data, key="cell")
    .mark("surface", wireframe="overlay", opacity=0.92)
    .encode(x="x", y="y", z="value", color=Field("value", scale=Scale.symlog((-1, 1))))
    .axes(z=Axis.symlog(-1, 1).ticks(0.5).label("z"))
    .guides(color=Guide.colorbar(title="sin(x) cos(y)"))
)

chart = scene.viz.chart(heatmap)
scene.play([chart.animate.create(0.8)])
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.play([
    chart.animate.to(surface).duration(1.5),
    scene.camera.animate.look_at(eye=(11, 9, 11), target=(0, 0, 0)).duration(1.5),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.7, 1.4, 2.4])
else:
    scene.render()
