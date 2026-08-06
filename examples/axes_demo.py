"""Configurable scientific axes with heart curve — auto-fit + plot."""

import math
import os

from gaanim import BLACK, BLUE, GRAY, RED, WHITE, Scene


scene = Scene(800, 480, background=WHITE)

axes = scene.axes(
    x=(-17, 17, 4),
    y=(-17, 15, 4),
    grid=True,
    ticks=True,
    numbers=True,
    x_grid=True,
    y_grid=True,
    x_ticks=True,
    y_ticks=True,
    x_numbers=True,
    y_numbers=True,
    x_label="x",
    y_label="y",
    axis_color=BLACK,
    grid_color=GRAY,
    tick_color=BLACK,
    number_color=BLACK,
    label_color=BLACK,
    axis_width=2.5,
    grid_width=1,
    tick_width=2,
    tick_length=8,
    auto_fit=True,  # ocupa safe_frame
)

# Corazón paramétrico clásico: x=16 sin^3 t, y=13 cos t -5 cos2t -2 cos3t - cos4t, t∈[0,2π]
# Generamos 240 puntos y los mapeamos al mismo scale que ejes (auto_fit)
samples = 240
heart_data = []
for i in range(samples):
    t = 2 * math.pi * i / (samples - 1)
    x = 16 * math.sin(t) ** 3
    y = 13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t)
    heart_data.append((x, y))

# Mapeo data -> escena usando el mismo scale que styled_axes (min(safe_w/data_w, safe_h/data_h))
# safe_frame 800x480, data 34x32 => scale 15 (ocupa safe_frame)
data_w = 34  # 17 - (-17)
data_h = 32  # 15 - (-17)
avail_w, avail_h = 800, 480  # Scene(800,480) sin margen
scale = min(avail_w / data_w, avail_h / data_h)  # mismo que Rust auto_fit
x_center, y_center = 0.0, -1.0  # centro datos: (0, -1)
heart_scaled = [((x - x_center) * scale, (y - y_center) * scale) for x, y in heart_data]

heart = scene.polyline(heart_scaled).no_fill().stroke(RED, 3.5)
title = scene.text("corazón — x=16 sin³t, y=13cos t-5cos2t-2cos3t-cos4t").fill(BLACK).scaled(0.7).at(0, 210)
subtitle = scene.text("ejes auto-fit + curva paramétrica").fill(GRAY).scaled(0.5).at(0, 185)

# create secuencial por capas: Grid → Axes → Ticks → Numbers/Labels
scene.play([axes.write().duration(3)])
scene.play([heart.create().duration(1.2).smooth(), title.write().duration(0.6)])
scene.play([subtitle.fade_in().duration(0.4)])
scene.wait(1.2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.7, 1.4, 2.2, 3.2])

scene.render()
