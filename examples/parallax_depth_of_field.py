"""Depth of field on parallax layers (CA-05): a rack focus from the foreground to the mountains."""

import math
import os

from gaanim import GOLD, WHITE, Scene


scene = Scene(frame=(16, 9), background="#1d2b53")

sky = scene.layer(depth=math.inf)
sky.add(scene.geometry.circle(0.9).fill(GOLD).move_to(4.5, 2.6))

far = scene.layer(depth=3.0)
for index in range(6):
    far.add(
        scene.geometry.polygon([(-2.4, -1.5), (-2.4, 0), (0, 2.6), (2.4, 0), (2.4, -1.5)])
        .fill("#3b4a7a")
        .move_to(-8 + index * 4.2, -1.6)
    )

scene.geometry.rect(40, 2.5).fill("#2f6b4f").move_to(6, -3.6)
for index in range(7):
    scene.geometry.rect(0.5, 1.2).fill("#8a5a3b").move_to(-6.5 + index * 2.6, -2.0)
title = scene.text("Foco", role="title").fill(WHITE).move_to(0, 3.2)

near = scene.layer(depth=0.5)
for index in range(5):
    near.add(scene.geometry.circle(0.9).fill("#1f4d33").move_to(-6 + index * 4.4, -3.4))

# Focused on the near bushes: the scene plane and the mountains blur.
scene.camera.depth_of_field(focus=0.5, aperture=0.12)
scene.wait(0.4)
scene.play([scene.camera.animate.focus_to(3.0).duration(1.6)])
scene.play([scene.camera.animate.pan_to(4, 0).duration(1.2)])
scene.play([scene.camera.animate.depth_of_field(aperture=0.0).duration(0.6)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 1.2, 2.0, 2.8, 3.8])
else:
    scene.render()
