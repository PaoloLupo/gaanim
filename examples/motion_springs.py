"""Perceptual springs: named presets and Easing.spring(bounce=...)."""

import os

from gaanim import BLACK, CYAN, GOLD, GRAY, WHITE, Easing, Scene


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Springs perceptuales", role="title").fill(WHITE).move_to(0, 3.6)
target = scene.geometry.line(4.0, 2.9, 4.0, -2.9).stroke(GRAY, 0.02)

rows = [
    ("SMOOTH_SPRING", Easing.SMOOTH_SPRING),
    ("GENTLE", Easing.GENTLE),
    ("QUICK", Easing.QUICK),
    ("SNAPPY", Easing.SNAPPY),
    ("BOUNCY", Easing.BOUNCY),
    ("spring(bounce=0.35)", Easing.spring(bounce=0.35)),
]
dots = []
for index, (name, easing) in enumerate(rows):
    y = 2.3 - index * 0.9
    scene.text(name).fill(GRAY).scale_by(0.6).move_to(-5.2, y)
    dot = scene.geometry.dot(0.16).fill(GOLD if index == 4 else CYAN).move_to(-2.5, y)
    dots.append((dot, easing))

# The same duration for every row: only the spring's character differs.
scene.play([dot.animate.move_to(4.0, dot_y).duration(1.4).easing(easing)
            for (dot, easing), dot_y in zip(dots, [2.3 - i * 0.9 for i in range(6)])])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Rising, peak overshoot, and settled.
    scene.snapshots(snapshots, [0.25, 0.55, 1.6])
else:
    scene.render()
