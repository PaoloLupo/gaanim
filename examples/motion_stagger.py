"""Spatial stagger over a 12×7 grid: center, edges, random and a point."""

import os

from gaanim import BLACK, CYAN, GOLD, WHITE, Scene, distribute, stagger


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Stagger espacial", role="title").fill(WHITE).move_to(0, 3.9)
dots = [scene.geometry.dot(0.2).fill(CYAN).move_to(column - 5.5, 3.0 - row)
        for row in range(7) for column in range(12)]

# Distances from the grid center shape each wave; values can be distributed too.
sizes = distribute(dots, 1.0, 0.55, origin="center")

scene.play(stagger(*[d.animate.grow_from_center().duration(0.4) for d in dots], total=0.8, origin="center"))
scene.play(stagger(*[d.animate.opacity(0.25).duration(0.4) for d in dots], total=0.8, origin="edges"))
scene.play(stagger(*[d.animate.opacity(1.0).duration(0.4) for d in dots], total=0.8, origin="random", seed=7))
scene.play(stagger(*[d.animate.scale_to(s).fill(GOLD).duration(0.4) for d, s in zip(dots, sizes)],
                   total=0.8, origin=(-5.5, -3.0)))
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Middle of each 1.2 s wave: center, edges, random, from the corner.
    scene.snapshots(snapshots, [0.5, 1.7, 2.9, 4.1])
else:
    scene.render()
