"""Camera motion: exponential zoom, fixed-point framing, and trauma shake."""

import os

from gaanim import BLACK, BLUE, CORAL, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Camera motion", role="title").fill(WHITE).move_to(0, 3.6)

# Concentric rings make the zoom rate visible: exponential zoom passes them at a steady pace.
for index, radius in enumerate([3.2, 1.6, 0.8, 0.4, 0.2, 0.1]):
    scene.geometry.circle(radius).no_fill().stroke([CYAN, BLUE][index % 2], radius * 0.06)
core = scene.geometry.dot(0.05).fill(GOLD)
detail = scene.text("x8").fill(WHITE).scale_by(0.08).move_to(0, -0.16)

card = scene.geometry.rect(1.2, 0.8).fill(CORAL).move_to(5.2, -2.4)
scene.text("frame_to").fill(GRAY).scale_by(0.4).move_to(5.2, -3.1)

scene.wait(0.3)
scene.play([scene.camera.animate.zoom_to(8.0).duration(1.5)])
scene.play([scene.camera.animate.zoom_to(1.0, interpolation="linear").duration(1.0)])
scene.play([scene.camera.animate.frame_to(card, 0.4).duration(1.2)])
scene.play([scene.camera.animate.reset().duration(0.6)])
scene.play([scene.camera.animate.shake(trauma=0.9, decay=1.5, frequency=12, rotation=0.03, seed=2)])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid exponential zoom, mid linear zoom-out, mid fixed-point frame, peak shake, rest.
    scene.snapshots(snapshots, [1.05, 2.3, 3.4, 4.7, 5.4])
else:
    scene.render()
