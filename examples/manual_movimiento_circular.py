"""Ejemplo acumulativo de la guía de usuario: del círculo a la curva seno."""

import math
import os

from gaanim import Anchor, BLUE, WHITE, YELLOW, Axis, Color, Scene, computed, stagger

# Paleta semántica.
BACKGROUND = Color(15, 23, 42)
PRIMARY = BLUE
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

# Escena y geometría base.
scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.4)
circle_center = (-3.0, -0.166667)
radius_value = 1.0
line_center = (1.5, -0.166667)
line_length = 5.0
theta = scene.viz.parameter(0.0)

title = scene.text("Movimiento circular y curva seno", role="title")
title.fill(WHITE).move_to(0, 2.208333, anchor=Anchor.CENTER)
caption = scene.text("Un solo ángulo gobierna ambas representaciones", role="subtitle")
caption.fill(MUTED).move_to(0, 1.833333, anchor=Anchor.CENTER)

orbit = (
    scene.geometry.circle(radius_value)
    .stroke(PRIMARY, 0.033333)
    .no_fill()
    .move_to(*circle_center)
)
circle_ref = scene.geometry.polar_point(circle_center, radius_value, theta)
circle_dot = scene.geometry.dot(0.083333).fill(ACCENT).follow(circle_ref)
radius = scene.geometry.tracking_line(circle_center, circle_ref).stroke(MUTED, 0.016667).no_fill()

# Recta tipada: el cero está en el inicio y las etiquetas nacen de Axis.
axis = (
    Axis.linear(0.0, 3.0 * math.pi)
    .ticks(math.pi)
    .numbers("pi", denominator=1)
    .style(
        color=MUTED,
        width=0.016667,
        tick_width=0.016667,
        number_color=MUTED,
        label_color=MUTED,
    )
)
timeline = scene.viz.number_line(axis, length=line_length)
timeline.drawable().move_to(*line_center)

# La función se traza una vez y se muestrea de forma nativa. ±1 ocupa ±radius.
sine_curve = timeline.function(
    lambda value: math.sin(value),
    normal_scale=radius_value,
    reveal=theta,
)
sine_curve.stroke(PRIMARY, 0.025).no_fill()

# El mismo theta determina la coordenada sobre la recta y su altura normal.
wave_ref = timeline.point_ref(
    theta,
    normal_offset=computed(
        lambda angle: radius_value * math.sin(angle),
        inputs=[theta],
    ),
)
wave_dot = scene.geometry.dot(0.066667).fill(ACCENT).follow(wave_ref)
projection_line = scene.geometry.tracking_line(circle_ref, wave_ref)
projection_line.stroke(ACCENT, 0.016667).no_fill()

formula = (
    scene.text.equation("y(theta) = r sin(theta)", role="subtitle")
    .fill(WHITE)
    .move_to(0, -1.791667, anchor=Anchor.CENTER)
)
explanation = scene.text("La fase y la altura comparten el mismo parámetro", role="body")
explanation.fill(MUTED).move_to(0, -2.208333, anchor=Anchor.CENTER)

# Timeline narrativa y determinista.
scene.play(stagger(title.animate.write().duration(0.8), caption.animate.fade_in().duration(0.6), each=0.12))
scene.play(stagger(
        orbit.animate.create().duration(1.0),
        circle_dot.animate.fade_in().duration(0.3),
        radius.animate.fade_in().duration(0.3),
    each=0.12,
))
scene.play(stagger(formula.animate.write().duration(0.8), explanation.animate.fade_in().duration(0.5), each=0.1))
scene.play(stagger(
        timeline.animate.create().duration(0.8),
        sine_curve.animate.fade_in().duration(0.01),
        wave_dot.animate.fade_in().duration(0.3),
        projection_line.animate.fade_in().duration(0.3),
    each=0.08,
))
scene.play(
    [
        theta.animate.set(3.0 * math.pi).duration(8.0),
    ]
)

# Editor o regresión visual desde el mismo archivo.
if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 1.0, 2.4, 3.4, 4.733, 6.067, 8.733, 11.3])
else:
    scene.render()
