"""Camera pan and zoom over ordinary vector mobjects."""

import os

from gaanim import BLUE, GOLD, WHITE, Scene


scene = Scene(960, 540)
scene.title("Camera controls").fill(WHITE).at(0, 220)
scene.circle(85).fill(BLUE).at(-260, -10)
scene.rect(180, 110).fill(GOLD).at(250, -10)

scene.wait(0.3)
scene.camera_pan_to(-260, -10, 0.8)
scene.camera_zoom_to(1.7, 0.8)
scene.camera_pan_to(250, -10, 0.8)
scene.camera_zoom_to(1.0, 0.8)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.1, 1.9, 3.1])

scene.render()
