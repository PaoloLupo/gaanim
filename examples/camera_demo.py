"""Camera pan and zoom over ordinary vector mobjects."""

import os

from gaanim import Anchor, BLACK, BLUE, CORAL, GOLD, WHITE, Scene, Updater


scene = Scene(960, 540, background=BLACK)
scene.text("Camera controls", role="title").fill(WHITE).move_to(0, 220, anchor=Anchor.CENTER)
left = scene.geometry.circle(85).fill(BLUE).move_to(-260, -10)
right = scene.geometry.rect(180, 110).fill(GOLD).move_to(250, -10)
guide = scene.geometry.dot(18).fill(CORAL).move_to(180, 0)
guide.add_updater(Updater.orbit(0, 0, 180, 1.6))

scene.wait(0.3)
scene.play([scene.camera.animate.frame_to(left, margin=70).duration(0.9)])
scene.play([scene.camera.animate.frame_to(right, margin=70).duration(0.9)])
scene.play([scene.camera.animate.zoom_to(1.0).duration(0.7)])
scene.play([scene.camera.animate.rotate_to(0.16).duration(0.6)])
scene.play([scene.camera.animate.rotate_to(0.0).duration(0.4)])
scene.play([scene.camera.animate.follow(guide).duration(1.6)])
scene.play([scene.camera.animate.shake(amplitude=30, frequency=4).duration(1.2)])
guide.remove_updater()
scene.play([scene.camera.animate.frame_to([left, right], margin=(48, 72), dynamic=True).duration(1.0)])
scene.play([scene.camera.animate.reset().duration(0.7)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(
        snapshot_dir,
        [0.0, 0.5, 1.1, 1.9, 3.2, 4.05, 4.6, 5.4, 5.9, 6.3, 6.75, 7.1],
    )

scene.render()
