"""Minimal reproducible scene used by the repository quickstart."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK, margin=48)

circle = scene.circle(96).fill(BLUE).stroke(WHITE, 4)
title = scene.title("Hola, Gaanim").fill(GOLD).at(0, 180)

scene.play([
    circle.create().duration(0.8),
    title.write().duration(0.6),
])
scene.play([circle.move(240, 0).duration(1.0).smooth()])
scene.render()
