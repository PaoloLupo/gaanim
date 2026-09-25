# %% circulo_al_seno
"""Del círculo al seno: el proyecto final del tutorial de Gaanim.

Un punto gira sobre un círculo y su altura dibuja una onda seno. Un único
parámetro, ``theta``, gobierna el punto, el radio, la onda y su proyección.
"""

import math

from gaanim import WHITE, YELLOW, Axis, Color, Easing, Scene, computed, stagger

# Paleta
BACKGROUND = Color(15, 23, 42)
PRIMARY = Color(96, 165, 250)
ACCENT = YELLOW
MUTED = Color(148, 163, 184)

scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)

CENTER = (-4.5, -1.0)
R = 1.5

# Texto
title = scene.text("Del círculo al seno", role="title")
title.fill(WHITE).move_to(0, 3.4)
caption = scene.text("Un punto que gira a radio constante", role="subtitle")
caption.fill(MUTED).move_to(0, 2.8)

formula = scene.text("$y = r sin(theta)$", role="subtitle").fill(WHITE)
explanation = scene.text("La altura del punto dibuja la onda.", role="body")
explanation.fill(MUTED)
panel = scene.layout.column([formula, explanation], gap=0.2, width=6.5)
panel.move_to(3.5, 1.6)

# Círculo
theta = scene.viz.parameter(0.0)
orbit = scene.geometry.circle(R).stroke(PRIMARY, 0.05).no_fill().move_to(*CENTER)
circle_ref = scene.geometry.polar_point(CENTER, R, theta)
point = scene.geometry.dot(0.125).fill(ACCENT).follow(circle_ref)
radius = scene.geometry.line(CENTER, point).stroke(MUTED, 0.025)
point.z_index(1)

# Recta y onda
axis = (
    Axis.linear(0, 2 * math.pi)
    .ticks(math.pi)
    .numbers("pi", denominator=1)
    .style(color=MUTED, number_color=MUTED)
)
number_line = scene.viz.number_line(axis, length=8)
number_line.drawable().move_to(2, CENTER[1])

wave = number_line.function(math.sin, normal_scale=R, reveal=theta)
wave.stroke(PRIMARY, 0.04).no_fill()

height = computed(lambda angle: R * math.sin(angle), inputs=[theta])
wave_dot = scene.geometry.dot(0.1).fill(ACCENT)
wave_dot.follow(number_line.point_ref(theta, normal_offset=height))
projection = scene.geometry.line(point, wave_dot).stroke(ACCENT, 0.025)

# Línea de tiempo
scene.play(stagger(
    title.animate.write().duration(0.8),
    caption.animate.fade_in().duration(0.5),
    each=0.5,
))
scene.play(stagger(
    orbit.animate.create().duration(0.8),
    radius.animate.create().duration(0.4),
    point.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
scene.play(stagger(
    formula.animate.write().duration(0.8),
    explanation.animate.write().duration(0.8),
    each=0.2,
))
scene.play(stagger(
    number_line.animate.create().duration(0.8),
    projection.animate.create().duration(0.4),
    wave_dot.animate.grow_from_center().duration(0.5).easing(Easing.SNAPPY),
    each=0.25,
))
scene.play([theta.animate.set(2 * math.pi).duration(4).easing(Easing.LINEAR)])
scene.play([projection.animate.fade_out().duration(0.5)])
scene.wait(1.5)
scene.render()
