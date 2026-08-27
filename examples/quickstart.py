"""Minimal reproducible scene used by the repository quickstart."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK, margin=48)

circle = scene.geometry.circle(96).fill(BLUE).stroke(WHITE, 4)
title = scene.text("Hola, Gaanim", role="title").fill(GOLD).move_to(0, 180)

scene.play([
    circle.animate.create().duration(0.8),
    title.animate.write().duration(0.6),
])
scene.play([circle.animate.shift_by(240, 0).duration(1.0).smooth()])
scene.render()
