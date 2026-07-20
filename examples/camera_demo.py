"""Camera pan and zoom over ordinary vector mobjects."""

import os

from gaanim import BLACK, BLUE, CORAL, GOLD, WHITE, Scene, Updater


scene = Scene(960, 540, background=BLACK)
scene.title("Camera controls").fill(WHITE).at(0, 220)
left = scene.circle(85).fill(BLUE).at(-260, -10)
right = scene.rect(180, 110).fill(GOLD).at(250, -10)
guide = scene.dot(18).fill(CORAL).at(180, 0)
guide.add_updater(Updater.orbit(0, 0, 180, 1.6))

scene.wait(0.3)
scene.camera_frame_to(left, margin=70, duration=0.9)
scene.camera_frame_to(right, margin=70, duration=0.9)
scene.camera_zoom_to(1.0, 0.7)
scene.camera_rotate_to(0.16, 0.6)
scene.camera_rotate_to(0.0, 0.4)
scene.camera_follow(guide, duration=1.2)
guide.remove_updater()
scene.camera_shake(amplitude=14, frequency=7, duration=0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.1, 1.9, 3.2, 4.1, 5.15, 5.5])

scene.render()
