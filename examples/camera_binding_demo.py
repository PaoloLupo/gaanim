"""Native reactive camera bindings in orthographic and perspective modes."""

import math
import os

from gaanim import Anchor, BLACK, BLUE, CYAN, GOLD, WHITE, Material3D, Scene, computed


scene = Scene(960, 540, background=BLACK)
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)
scene.text("Reactive camera rig", role="title").fill(WHITE).move_to(0, 220, anchor=Anchor.CENTER)
marker = scene.geometry.dot(18).fill(GOLD)
scene.geometry.circle(150).no_fill().stroke(BLUE, 3)
cube = scene.geometry.cube(2.2, material=Material3D.matte(BLUE)).move_to_3d(-1.8, -0.5, 0)
sphere = scene.geometry.sphere(1.25, material=Material3D.metal(CYAN)).move_to_3d(1.8, -0.5, 0)
theta = scene.viz.parameter(0.0)
focus = scene.geometry.point_ref(
    computed(lambda value: value * 260 - 130, inputs=[theta]),
    computed(lambda value: math.sin(value * math.pi) * 90, inputs=[theta]),
)
marker.follow(focus)

rig_2d = scene.camera.bind_2d(
    center=focus,
    zoom=computed(lambda value: 1.0 + value * 0.35, inputs=[theta]),
    rotation=computed(lambda value: (value - 0.5) * 0.12, inputs=[theta]),
)
scene.play([theta.animate.set(1.0).duration(2.0), marker.animate.fade_in().duration(0.3)])
rig_2d.disable()

target_x = scene.viz.parameter(0.0)
focus_3d = scene.geometry.point_ref(
    computed(lambda value: (value - 0.5) * 3.0, inputs=[target_x]),
    -0.5,
)
rig_3d = scene.camera.bind_3d(
    eye=(7.0, 4.5, 10.0), target=focus_3d, fov_y=0.72,
)
scene.play(
    [
        target_x.animate.set(1.0).duration(2.0),
        cube.animate.create().duration(0.5),
        sphere.animate.create().duration(0.5),
    ]
)
rig_3d.disable()
scene.play([scene.camera.animate.reset().duration(0.7)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0, 1.8, 2.1, 2.8, 3.8, 4.4, 4.7])

scene.render()
