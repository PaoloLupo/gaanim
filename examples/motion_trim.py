"""Trim paths: draw from the center, a travelling segment, sequential dashes."""

import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Trim paths", role="title").fill(WHITE).move_to(0, 3.6)

ring = scene.geometry.circle(1.2).no_fill().stroke(CYAN, 0.12).move_to(-4.8, 0.4)
ring.trim(start=0.5, end=0.5)
orbit = scene.geometry.circle(1.2).no_fill().stroke(GOLD, 0.12).move_to(-1.6, 0.4)
orbit.trim(start=0.0, end=0.15)
dashes = scene.geometry.dashed_line(0.6, 0.4, 3.4, 0.4, dash_length=0.3, gap_length=0.2).stroke(CORAL, 0.1)
dashes.trim(end=0.0, mode="sequential")
star = scene.geometry.star(5, 1.2, 0.5).no_fill().stroke(WHITE, 0.08).move_to(5.4, 0.4)
star.trim(end=0.0)
for x, label in [(-4.8, "desde el centro"), (-1.6, "segmento viajero"), (2.0, '"sequential"'), (5.4, "dibujar")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -1.6)

scene.play([
    ring.animate.trim(start=0.0, end=1.0).duration(1.5),
    orbit.animate.trim(offset=1.0).duration(1.5),
    dashes.animate.trim(end=1.0).duration(1.5),
    star.animate.trim(end=1.0).duration(1.5),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 0.8, 1.2])
else:
    scene.render()
