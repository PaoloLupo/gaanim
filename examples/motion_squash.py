"""Squash and stretch: shapes stretch along their velocity and keep their area."""

import os

from gaanim import BLUE, CORAL, GOLD, WHITE, Easing, Scene


scene = Scene(frame=(16, 9), background="#0f1729")
scene.text("Squash & stretch", role="title").fill(WHITE).move_to(0, 3.3)
ball = scene.geometry.circle(0.5).fill(CORAL).move_to(-6.0, 1.2).squash_stretch(0.08, max_ratio=1.8)
tile = scene.geometry.square(0.9).fill(BLUE).move_to(-6.0, -1.4).squash_stretch(0.05, max_ratio=1.5)
still = scene.geometry.circle(0.5).fill(GOLD).move_to(5.5, -1.4).squash_stretch(0.08)

scene.play(
    [
        ball.animate.move_to(6.0, 1.2).duration(1.0).easing(Easing.SMOOTH),
        tile.animate.move_to(2.0, 1.2).duration(1.0).easing(Easing.LINEAR),
    ]
)
scene.play([ball.animate.move_to(0.0, -2.5).duration(0.5).easing(Easing.LINEAR)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Speeding up, fastest mid-course, falling diagonally and at rest.
    scene.snapshots(snapshots, [0.2, 0.5, 0.95, 1.3, 1.9])
else:
    scene.render()
