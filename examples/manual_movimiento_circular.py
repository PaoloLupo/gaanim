"""Ejemplo acumulativo de la guía de usuario: del círculo a la curva seno."""

import math
import os

from gaanim import Anchor, BLUE, WHITE, YELLOW, Axis, Color, Scene, math as gm

# Paleta semántica.
BACKGROUND = Color(15, 23, 42)
PRIMARY = BLUE
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

# Escena y geometría base.
scene = Scene(1920, 1080, background=BACKGROUND, margin=48)
circle_center = (-360.0, -20.0)
radius_value = 120.0
line_center = (180.0, -20.0)
line_length = 600.0
theta = scene.parameter(0.0)

title = scene.text("Movimiento circular y curva seno", role="title")
title.fill(WHITE).at(0, 265, anchor=Anchor.CENTER)
caption = scene.text("Un solo ángulo gobierna ambas representaciones", role="subtitle")
caption.fill(MUTED).at(0, 220, anchor=Anchor.CENTER)

orbit = (
    scene.circle(radius_value)
    .stroke(PRIMARY, 4)
    .no_fill()
    .at(*circle_center)
)
circle_ref = scene.polar_point(circle_center, radius_value, theta)
circle_dot = scene.dot(10).fill(ACCENT).follow(circle_ref)
radius = scene.tracking_line(circle_center, circle_ref).stroke(MUTED, 2).no_fill()

# Recta tipada: el cero está en el inicio y las etiquetas nacen de Axis.
axis = (
    Axis.linear(0.0, 3.0 * math.pi)
    .ticks(math.pi)
    .numbers("pi", denominator=1)
    .style(
        color=MUTED,
        width=2.0,
        tick_width=2.0,
        number_color=MUTED,
        label_color=MUTED,
    )
)
timeline = scene.number_line(axis, length=line_length)
timeline.drawable().at(*line_center)

# La función se traza una vez y se muestrea de forma nativa. ±1 ocupa ±radius.
sine_curve = timeline.function(
    lambda value: gm.sin(value),
    normal_scale=radius_value,
    reveal=theta,
)
sine_curve.stroke(PRIMARY, 3).no_fill()

# El mismo theta determina la coordenada sobre la recta y su altura normal.
wave_ref = timeline.point_ref(
    theta,
    normal_offset=radius_value * gm.sin(theta),
)
wave_dot = scene.dot(8).fill(ACCENT).follow(wave_ref)
projection_line = scene.tracking_line(circle_ref, wave_ref)
projection_line.stroke(ACCENT, 2).no_fill()

formula = (
    scene.equation("y(theta) = r sin(theta)", role="subtitle")
    .fill(WHITE)
    .at(0, -215, anchor=Anchor.CENTER)
)
explanation = scene.text("La fase y la altura comparten el mismo parámetro", role="body")
explanation.fill(MUTED).at(0, -265, anchor=Anchor.CENTER)

# Timeline narrativa y determinista.
scene.play([title.write().duration(0.8), caption.fade_in().duration(0.6)], lag=0.12)
scene.play(
    [
        orbit.create().duration(1.0),
        circle_dot.fade_in().duration(0.3),
        radius.fade_in().duration(0.3),
    ],
    lag=0.12,
)
scene.play([formula.write().duration(0.8), explanation.fade_in().duration(0.5)], lag=0.1)
scene.play(
    [
        timeline.create().duration(0.8),
        sine_curve.fade_in().duration(0.01),
        wave_dot.fade_in().duration(0.3),
        projection_line.fade_in().duration(0.3),
    ],
    lag=0.08,
)
scene.play(
    [
        theta.animate_to(3.0 * math.pi, 8.0),
    ]
)

# Editor o regresión visual desde el mismo archivo.
if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 1.0, 2.4, 3.4, 4.733, 6.067, 8.733, 11.3])
else:
    scene.render()
