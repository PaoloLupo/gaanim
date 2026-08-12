"""Stable-key point storytelling from a 2D chart into a 3D chart."""

import os

from gaanim import Axis, BLACK, ChartSpec, Field, GOLD, Guide, Scale, Scene, Value

scene = Scene(1280, 720, background=BLACK)
data = {
    "id": ["a", "b", "c", "d", "e", "f"],
    "x": [-4, -2, -1, 1, 2.5, 4],
    "y": [1.2, -1.4, 2.1, -0.5, 1.4, -2.0],
    "z": [-2.0, 1.2, 2.4, -1.0, 0.6, 1.8],
    "group": ["A", "A", "B", "B", "C", "C"],
}

base = (
    ChartSpec(data, key="id")
    .mark("point")
    .encode(
        x="x",
        y="y",
        color=Field("group", scale=Scale.category(["A", "B", "C"])),
        size=Value(10),
    )
    .axes(
        x=Axis.linear(-5, 5).ticks(1).label("x"),
        y=Axis.linear(-3, 3).ticks(1).label("y"),
    )
    .guides(color=Guide.legend(title="Grupo"))
)
target = base.encode(z="z").axes(z=Axis.symlog(-3, 3).ticks(1).label("z"))

chart = scene.chart(base).inspect(("id", "group", "x", "y", "z"), format="{id}: {group}")
title = scene.text("Identidad estable: 2D → 3D").fill(GOLD).hud().at(0, 310)

scene.play([chart.create(0.9), title.write(0.6)])
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000, duration=0.0)
scene.play([
    chart.to(target).duration(1.4),
    scene.camera.look_at(eye=(8, 6, 8), target=(0, 0, 0)).duration(1.4),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 2.3])
else:
    scene.render()
