"""Regression demo for late glTF visibility and segment cross-fades."""

import os

from gaanim import BLACK, Scene, Theme, Transition


paper = Theme("paper", colors={"background": "#fffbe6"})
scene = Scene(1920, 1080)
scene.assets_dir("examples/assets")
scene.canvas.set_theme(paper)

scene.segment("Introduction")
title = scene.text("glTF segment visibility").fill(BLACK).hud().at(0, 420)
scene.play([title.write(0.6)])
scene.wait(1.0)

scene.segment("Character", Transition.cross_fade(1.0))
scene.reuse(title)
scene.play([title.move_to(-250, 400)])

# The model is declared after the title move. It must not leak into the first
# second of this segment while the camera is still orthographic.
fox = scene.gltf("Fox.glb").at_3d(2.0, -1.0, 0.0).scaled_3d(0.04, 0.04, 0.04)
scene.play(
    [
        scene.camera.perspective(fov_y=1.0, near=0.1, far=500.0, duration=0.0),
        scene.camera.look_at(
            eye=(10.5, 3.8, 0.0), target=(0.0, 0.15, 0.0), duration=0.0
        ),
        fox.animation("Survey", duration=2.4),
    ]
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [1.8, 2.55, 2.8, 3.2, 3.6, 4.0, 4.4, 4.8])

scene.render()
