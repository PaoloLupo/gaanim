"""Axes 3D demo — perspective real, 3 planes, billboard HUD, surface triangulada."""
import math
import os
from gaanim import Scene, WHITE, BLACK, BLUE, RED, GREEN, GOLD

scene = Scene(1280, 720, background=BLACK)
# Ejes 3D con 3 planos y labels billboard (frente a cámara)
axes = scene.axes_3d(
    x_range=(-5, 5, 1),
    y_range=(-5, 5, 1),
    z_range=(-3, 3, 1),
    x_label="x", y_label="y", z_label="z",
    label_mode="billboard",
    grid=True, ticks=True, numbers=True, labels=True,
    axis_color=WHITE, grid_color=WHITE, tick_color=WHITE,
    auto_fit=True,
).at_3d(0, 0, 0)

# HUD fijo (no escala con cámara) — título
title = scene.text("Axes 3D — 3 planos + billboard + HUD").fill(WHITE).hud().at(0, 300)
title2 = scene.text("Right-drag: orbit | Middle/Shift+Left: pan | Wheel: dolly | I/Esc: toggle").fill(GOLD).hud().at(0, 270)

# Billboard en mundo 3D (mira a cámara)
label_world = scene.text("origen").fill(GREEN).at_3d(0, 0, 0.5).billboard()

# Superficie triangulada z = sin(x)*cos(y)
surf = scene.surface(
    lambda x, y: math.sin(x) * math.cos(y),
    x_range=(-5, 5), y_range=(-5, 5), x_samples=30, y_samples=30,
    color=BLUE,
)
# El surface se crea en data coords; coincide con axes si rangos iguales

# Polilínea 3D helicoidal
helix_points = [(math.cos(t)*2, math.sin(t)*2, t*0.3) for t in [i*0.2 for i in range(60)]]
helix = scene.polyline_3d(helix_points, color=RED)

# Cámara 3D: perspectiva y look_at
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000, duration=0.0)  # 45°
scene.camera.look_at(eye=(8, 6, 8), target=(0, 0, 0), duration=1.0)

scene.play([axes.create(duration=1.2), title.write(duration=0.8)])
scene.play([surf.create(duration=1.0), helix.create(duration=1.0), label_world.fade_in(0.6)])

# Órbita programática
scene.camera.orbit(delta_yaw=0.9, delta_pitch=0.2, duration=1.5)
scene.wait(0.5)
scene.camera.orbit(delta_yaw=-0.5, delta_pitch=-0.1, duration=1.0)

# Dolly
scene.camera.dolly(factor=0.7, duration=0.8)
scene.wait(0.5)
scene.camera.dolly(factor=1.4, duration=0.8)

if os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 1.0, 2.5, 4.0])

scene.render()
