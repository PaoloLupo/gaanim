"""Seeded randomness and coherent noise evaluated natively."""

import os

from gaanim import BLACK, CYAN, GOLD, GRAY, WHITE, Scene, computed


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("scene.random · scene.noise", role="title").fill(WHITE).move_to(0, 3.6)

rng = scene.random(seed=42)
for _ in range(70):
    size = rng.uniform(0.02, 0.07)
    scene.geometry.dot(size).fill(rng.choice([GRAY, WHITE, CYAN])).move_to(rng.uniform(-7.5, 7.5), rng.uniform(-4.2, 3.0))

drift = scene.noise(frequency=0.8, amplitude=1.0, octaves=3, seed=5)
sway = scene.noise(frequency=0.5, amplitude=2.5, octaves=2, seed=9)
leaf = scene.geometry.rect(2.4, 0.5).fill(GOLD).move_to(-3.0, 0.0)
leaf.rotate_to(computed(lambda v: 1.5 * v, inputs=[drift]))
firefly = scene.geometry.dot(0.18).fill(CYAN)
firefly.move_to(computed(lambda v: 3.0 + v, inputs=[sway]), computed(lambda v: 0.8 * v, inputs=[drift]))
scene.wait(3.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 1.5, 2.5])
else:
    scene.render()
