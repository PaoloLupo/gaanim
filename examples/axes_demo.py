"""Comprehensive Axes showcase — manim-compatible API, auto-fit, plot, and sequential create."""

import math
import os

from gaanim import BLACK, BLUE, GOLD, GRAY, GREEN, RED, TEAL, WHITE, Direction, Scene

scene = Scene(800, 480, background=WHITE)

# ── Escena 1: Auto-fit a safe_frame (gaanim idiom) ─────────────────────
scene.segment("auto-fit")
title1 = scene.text("Axes auto-fit — ocupa safe_frame").fill(BLACK).scaled(0.65).at(0, 210)
try:
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
        label_color=BLACK,
        axis_width=2.5,
        grid_width=1,
        tick_width=2,
        tick_length=8,
        auto_fit=True,
    )
except TypeError:
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
        label_color=BLACK,
        axis_width=2.5,
        grid_width=1,
        tick_width=2,
        tick_length=8,
    )
# plot con estilo propio, mapeado al mismo auto_fit
try:
    curve1 = scene.plot(axes1, lambda x: math.sin(x), x=(-3, 3), samples=200).no_fill().stroke(BLUE, 3)
except (TypeError, AttributeError):
    curve1 = scene.function_graph(lambda x: math.sin(x), x=(-3, 3), samples=200).no_fill().stroke(BLUE, 3)
scene.play([axes1.create().duration(1.4)])  # secuencial Grid→Axes→Ticks→Numbers
scene.play([curve1.create().duration(1.0).smooth(), title1.write().duration(0.5)])
scene.wait(0.6)

# ── Escena 2: Manim x_length / y_length / tips ───────────────────────────
scene.segment("manim-size")
scene.play([axes1.fade_out().duration(0.4), curve1.fade_out().duration(0.4), title1.fade_out().duration(0.3)])
try:
    axes2 = scene.axes(
        x=(-7, 7, 1),
        y=(-4, 4, 1),
        x_length=10,
        y_length=5,
        tips=True,
        grid=True,
        ticks=True,
        numbers=True,
        axis_color=TEAL,
        grid_color=GRAY,
        tick_color=TEAL,
        number_color=TEAL,
        auto_fit=False,
        axis_width=2.5,
        grid_width=1,
        tick_width=2,
        tick_length=8,
    )
except TypeError:
    axes2 = scene.axes(
        x=(-7, 7, 1),
        y=(-4, 4, 1),
        grid=True,
        ticks=True,
        numbers=True,
        axis_color=TEAL,
        grid_color=GRAY,
        tick_color=TEAL,
        number_color=TEAL,
        axis_width=2.5,
        grid_width=1,
        tick_width=2,
        tick_length=8,
    )
axes2_title = scene.text("Manim x_length=10, y_length=5, tips=True").fill(BLACK).scaled(0.55).at(0, 210)
try:
    parabola = scene.plot(axes2, lambda x: 0.12 * x * x - 1.2, x=(-6, 6), samples=160).no_fill().stroke(GOLD, 3)
    sine = scene.plot(axes2, lambda x: math.sin(x) * 1.2, x=(-7, 7), samples=200).no_fill().stroke(BLUE, 2.5)
except (TypeError, AttributeError):
    parabola = scene.function_graph(lambda x: 0.12 * x * x - 1.2, x=(-6, 6), samples=160).no_fill().stroke(GOLD, 3)
    sine = scene.function_graph(lambda x: math.sin(x) * 1.2, x=(-7, 7), samples=200).no_fill().stroke(BLUE, 2.5)
scene.play([axes2.create().duration(1.2)])
scene.play([parabola.create().duration(0.9), sine.create().duration(0.9), axes2_title.write().duration(0.4)])
scene.wait(0.5)
# coords_to_point / point_to_coords demo
try:
    dot = scene.dot(6).fill(RED).at(*axes2.coords_to_point(2, 1))
    label = scene.text("(2, 1)").fill(RED).scaled(0.45).at(*axes2.coords_to_point(2, 1)).next_to(dot, Direction.UP, spacing=8)
    scene.play([dot.fade_in().duration(0.3), label.fade_in().duration(0.3)])
    scene.wait(0.4)
    scene.play([dot.fade_out().duration(0.3), label.fade_out().duration(0.3), parabola.fade_out().duration(0.3), sine.fade_out().duration(0.3)])
except Exception:
    scene.play([parabola.fade_out().duration(0.3), sine.fade_out().duration(0.3)])
scene.play([axes2.fade_out().duration(0.4), axes2_title.fade_out().duration(0.3)])

# ── Escena 3: Corazón paramétrico + coordenadas ───────────────────────────
scene.segment("heart")
try:
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
        tick_color=BLACK,
        number_color=BLACK,
        auto_fit=True,
    )
except TypeError:
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
        tick_color=BLACK,
        number_color=BLACK,
        axis_width=2.5,
        grid_width=1,
        tick_width=2,
        tick_length=8,
    )
# corazón clásico paramétrico
samples = 240
heart_data = []
for i in range(samples):
    t = 2 * math.pi * i / (samples - 1)
    x = 16 * math.sin(t) ** 3
    y = 13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t)
    heart_data.append((x, y))
# mapeo manual igual que scene.plot (auto_fit)
data_w, data_h = 34, 32
avail_w, avail_h = 800, 480
scale = min(avail_w / data_w, avail_h / data_h)
x_center, y_center = 0.0, -1.0
heart_scaled = [((x - x_center) * scale, (y - y_center) * scale) for x, y in heart_data]
heart = scene.polyline(heart_scaled).no_fill().stroke(RED, 3.2)
# alternativa manim: axes.plot_parametric_curve via scene.plot_parametric_curve
try:
    spiral = scene.plot_parametric_curve(
        axes3, lambda t: (6 * math.cos(t), 6 * math.sin(t) * 0.6), t=(0, 2 * math.pi), samples=180
    ).no_fill().stroke(GREEN, 2).opacity(0.0)
except (AttributeError, TypeError):
    spiral = scene.parametric_curve(lambda t: (6 * math.cos(t), 6 * math.sin(t) * 0.6), t=(0, 2 * math.pi), samples=180).no_fill().stroke(GREEN, 2).opacity(0.0)
title3 = scene.text("corazón paramétrico — plot + coords_to_point").fill(BLACK).scaled(0.6).at(0, 210)
scene.play([axes3.create().duration(1.3)])
scene.play([heart.create().duration(1.4).smooth(), title3.write().duration(0.5)])
# add_coordinates y get_x_axis demo (compat)
try:
    axes3.add_coordinates()
    x_axis = axes3.get_x_axis()
    y_axis = axes3.get_y_axis()
except Exception:
    pass
try:
    highlight = scene.dot(5).fill(GOLD).at(*axes3.coords_to_point(0, 0))
    scene.play([highlight.fade_in().duration(0.3)])
    scene.wait(0.6)
    scene.play([heart.fade_out().duration(0.4), highlight.fade_out().duration(0.3)])
except Exception:
    scene.play([heart.fade_out().duration(0.4)])
# morph a espiral paramétrica dentro del mismo sistema
scene.play([spiral.fade_to(1.0).duration(0.4), spiral.create().duration(1.0)])
scene.wait(0.5)
scene.play([spiral.fade_out().duration(0.3), axes3.fade_out().duration(0.4), title3.fade_out().duration(0.3)])

# ── Escena 4: Múltiples funciones y estilos ───────────────────────────────
scene.segment("multiplot")
try:
    axes4 = scene.axes(
        x=(-5, 5, 1),
        y=(-3, 3, 1),
        grid=True,
        x_grid=True,
        y_grid=True,
        ticks=True,
        numbers=True,
        tips=True,
        auto_fit=True,
    )
except TypeError:
    axes4 = scene.axes(
        x=(-5, 5, 1),
        y=(-3, 3, 1),
        grid=True,
        ticks=True,
        numbers=True,
        auto_fit=True,
    )
axes4_title = scene.text("múltiples plot() con estilo propio").fill(BLACK).scaled(0.55).at(0, 210)
try:
    g1 = scene.plot(axes4, lambda x: math.sin(x), x=(-5, 5), samples=200).no_fill().stroke(BLUE, 3)
    g2 = scene.plot(axes4, lambda x: math.cos(x), x=(-5, 5), samples=200).no_fill().stroke(GOLD, 3)
    g3 = scene.plot(axes4, lambda x: 0.2 * x, x=(-5, 5), samples=80).no_fill().stroke(TEAL, 2.5)
except (TypeError, AttributeError):
    g1 = scene.function_graph(lambda x: math.sin(x), x=(-5, 5), samples=200).no_fill().stroke(BLUE, 3)
    g2 = scene.function_graph(lambda x: math.cos(x), x=(-5, 5), samples=200).no_fill().stroke(GOLD, 3)
    g3 = scene.function_graph(lambda x: 0.2 * x, x=(-5, 5), samples=80).no_fill().stroke(TEAL, 2.5)
scene.play([axes4.create().duration(1.2)])
scene.play([g1.create().duration(0.8), g2.create().duration(0.8), g3.create().duration(0.7), axes4_title.write().duration(0.4)])
scene.wait(0.5)
# point_to_coords inverso
try:
    pt = axes4.coords_to_point(1.5, math.sin(1.5))
    dot2 = scene.dot(5).fill(RED).at(*pt)
    scene.play([dot2.fade_in().duration(0.3)])
    scene.wait(0.4)
except Exception:
    pass

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.5, 1.8, 3.2, 5.0, 7.0, 9.5])
else:
    scene.render()
