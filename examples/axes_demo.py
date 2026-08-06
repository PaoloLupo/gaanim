"""Axes showcase — manim-compatible, auto-fit, plot y create secuencial."""

import math
import os

from gaanim import BLACK, BLUE, GOLD, GRAY, GREEN, RED, TEAL, WHITE, Direction, Scene

scene = Scene(1920, 1080)
scene.canvas.set_theme("paper")

# ── 1. Auto-fit a safe_frame ──────────────────────────────────────────
scene.segment("auto-fit")
title1 = scene.text("Axes auto-fit — ocupa safe_frame").fill(BLACK).scaled(0.65).at(0, 480)
axes1 = scene.axes(
    x=(-3, 3, 1),
    y=(-2, 2, 1),
    grid=True,
    ticks=True,
    numbers=True,
    x_label="x",
    y_label="y",
    axis_color=BLACK,
    grid_color=GRAY,
    tick_color=BLACK,
    number_color=BLACK,
    auto_fit=True,  # escala el rango de datos a safe_frame (ticks sin escalar)
)
curve1 = scene.plot(axes1, lambda x: math.sin(x), x=(-3, 3), samples=200).no_fill().stroke(BLUE, 3)

scene.play([axes1.create().duration(1.4)])  # Grid → Ejes → Ticks → Números
scene.play([curve1.create().duration(1.0).smooth(), title1.write().duration(0.5)])
scene.wait(0.6)

# ── 2. Tamaño explícito estilo Manim ──────────────────────────────────
scene.segment("manim-size")
scene.play([axes1.fade_out().duration(0.4), curve1.fade_out().duration(0.4), title1.fade_out().duration(0.3)])

axes2 = scene.axes(
    x=(-7, 7, 1),
    y=(-4, 4, 1),
    x_length=10,  # Manim: longitud en unidades escena
    y_length=5,
    tips=True,  # puntas de flecha
    grid=True,
    ticks=True,
    numbers=True,
    axis_color=TEAL,
    grid_color=GRAY,
    tick_color=TEAL,
    number_color=TEAL,
    auto_fit=False,
)
title2 = scene.text("Manim x_length=10, y_length=5, tips=True").fill(BLACK).scaled(0.55).at(0, 480)
parabola = scene.plot(axes2, lambda x: 0.12 * x * x - 1.2, x=(-6, 6), samples=160).no_fill().stroke(GOLD, 3)
sine = scene.plot(axes2, lambda x: math.sin(x) * 1.2, x=(-7, 7), samples=200).no_fill().stroke(BLUE, 2.5)

scene.play([axes2.create().duration(1.2)])
scene.play([parabola.create().duration(0.9), sine.create().duration(0.9), title2.write().duration(0.4)])
scene.wait(0.3)

# coords_to_point: dato → escena
dot = scene.dot(6).fill(RED).at(*axes2.coords_to_point(2, 1))
label = scene.text("(2, 1)").fill(RED).scaled(0.45).at(*axes2.coords_to_point(2, 1)).next_to(dot, Direction.UP, spacing=8)
scene.play([dot.fade_in().duration(0.3), label.fade_in().duration(0.3)])
scene.wait(0.4)
scene.play([dot.fade_out().duration(0.3), label.fade_out().duration(0.3), parabola.fade_out().duration(0.3), sine.fade_out().duration(0.3)])
scene.play([axes2.fade_out().duration(0.4), title2.fade_out().duration(0.3)])

# ── 3. Corazón paramétrico ────────────────────────────────────────────
scene.segment("heart")
axes3 = scene.axes(
    x=(-17, 17, 4),
    y=(-17, 15, 4),
    grid=True,
    ticks=True,
    numbers=True,
    x_label="x",
    y_label="y",
    axis_color=BLACK,
    grid_color=GRAY,
    auto_fit=True,
)
# x(t)=16 sin³t, y(t)=13cos t-5cos2t-2cos3t-cos4t
heart = scene.plot_parametric_curve(
    axes3,
    lambda t: (
        16 * math.sin(t) ** 3,
        13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t),
    ),
    t=(0, 2 * math.pi),
    samples=240,
).no_fill().stroke(RED, 3.2)

spiral = scene.plot_parametric_curve(
    axes3,
    lambda t: (6 * math.cos(t), 6 * math.sin(t) * 0.6),
    t=(0, 2 * math.pi),
    samples=180,
).no_fill().stroke(GREEN, 2).opacity(0.0)

title3 = scene.text("Corazón paramétrico — plot_parametric_curve").fill(BLACK).scaled(0.6).at(0, 480)
scene.play([axes3.create().duration(1.3)])
scene.play([heart.create().duration(1.4).smooth(), title3.write().duration(0.5)])

# helpers manim: get_x_axis / add_coordinates
x_axis = axes3.get_x_axis()
y_axis = axes3.get_y_axis()
axes3.add_coordinates()

highlight = scene.dot(5).fill(GOLD).at(*axes3.coords_to_point(0, 0))
scene.play([highlight.fade_in().duration(0.3)])
scene.wait(0.6)
scene.play([heart.fade_out().duration(0.4), highlight.fade_out().duration(0.3)])
scene.play([spiral.fade_to(1.0).duration(0.4), spiral.create().duration(1.0)])
scene.wait(0.5)
scene.play([spiral.fade_out().duration(0.3), axes3.fade_out().duration(0.4), title3.fade_out().duration(0.3)])

# ── 4. Múltiples gráficas y point_to_coords ───────────────────────────
scene.segment("multiplot")
axes4 = scene.axes(
    x=(-5, 5, 1),
    y=(-3, 3, 1),
    grid=True,
    ticks=True,
    numbers=True,
    tips=True,
    auto_fit=True,
)
title4 = scene.text("Múltiples plot() con estilo propio").fill(BLACK).scaled(0.55).at(0, 480)

g1 = scene.plot(axes4, lambda x: math.sin(x), x=(-5, 5), samples=200).no_fill().stroke(BLUE, 3)
g2 = scene.plot(axes4, lambda x: math.cos(x), x=(-5, 5), samples=200).no_fill().stroke(GOLD, 3)
g3 = scene.plot(axes4, lambda x: 0.2 * x, x=(-5, 5), samples=80).no_fill().stroke(TEAL, 2.5)

scene.play([axes4.create().duration(1.2)])
scene.play([g1.create().duration(0.8), g2.create().duration(0.8), g3.create().duration(0.7), title4.write().duration(0.4)])
scene.wait(0.5)

# point_to_coords inverso: escena → dato
pt = axes4.coords_to_point(1.5, math.sin(1.5))
dot2 = scene.dot(5).fill(RED).at(*pt)
scene.play([dot2.fade_in().duration(0.3)])
scene.wait(0.4)
# ejemplo: recuperar dato desde punto escena
# print(axes4.point_to_coords(pt))

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.5, 1.8, 3.2, 5.0, 7.0, 9.5])
else:
    scene.render()
