"""Procedural wiggle and oscillators layered over ordinary animation."""

import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene, Updater


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Updater.wiggle · Updater.oscillate", role="title").fill(WHITE).move_to(0, 3.6)

logo = scene.geometry.star(5, 1.0, 0.45).fill(GOLD).move_to(-4.0, 0.3)
light = scene.geometry.circle(0.8).fill(CYAN).move_to(0.0, 0.3)
buoy = scene.geometry.square(1.0).fill(CORAL).move_to(4.0, 0.3)
for x, label in [(-4.0, "wiggle"), (0.0, 'oscillate("opacity")'), (4.0, 'oscillate("y", "triangle")')]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -1.6)

logo.add_updater(Updater.wiggle(position=0.25, rotation=0.25, frequency=1.5, seed=1))
light.add_updater(Updater.oscillate("opacity", waveform="sine", frequency=0.8, low=0.2, high=1.0))
buoy.add_updater(Updater.oscillate("y", waveform="triangle", frequency=1.0, low=-0.4, high=0.4))

scene.wait(1.0)
# The wiggle keeps adding to the move instead of replacing it.
scene.play([logo.animate.move_to(-4.0, -0.8).duration(1.5)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 0.9, 1.75, 2.8])
else:
    scene.render()
