"""Minimal reproducible scene used by the repository quickstart."""

from gaanim import Easing, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background=BLACK, margin=0.6)

circle = scene.geometry.circle(1.2).fill(BLUE).stroke(WHITE, 0.05)
title = scene.text("Hola, Gaanim", role="title").fill(GOLD).move_to(0, 2.25)

scene.play([
    circle.animate.create().duration(0.8),
    title.animate.write().duration(0.6),
])
scene.play([circle.animate.shift_by(3, 0).duration(1.0).easing(Easing.SMOOTH)])
scene.render()
