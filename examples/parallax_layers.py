"""2.5D parallax: layers at different depths drift at their own speed as the camera pans."""

import math
import os

from gaanim import GOLD, Scene


scene = Scene(frame=(16, 9), background="#1d2b53")

sky = scene.layer(depth=math.inf)
sky.add(scene.geometry.circle(0.9).fill(GOLD).move_to(4.5, 2.6))

far = scene.layer(depth=3.0)
for index in range(6):
    far.add(
        scene.geometry.polygon([(-2.4, 0), (0, 2.6), (2.4, 0)])
        .fill("#3b4a7a")
        .move_to(-8 + index * 4.2, -1.4)
    )

ground = scene.geometry.rect(40, 2.5).fill("#2f6b4f").move_to(6, -3.6)
for index in range(8):
    scene.geometry.rect(0.5, 1.2).fill("#8a5a3b").move_to(-6 + index * 3, -2.0)

near = scene.layer(depth=0.6)
for index in range(5):
    near.add(
        scene.geometry.circle(0.9).fill("#1f4d33").move_to(-7 + index * 5, -3.3)
    )

scene.wait(0.2)
scene.play([scene.camera.animate.pan_to(6, 0).duration(3)])
scene.play([scene.camera.animate.zoom_to(1.4).duration(1)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.7, 3.2, 4.2])
else:
    scene.render()
