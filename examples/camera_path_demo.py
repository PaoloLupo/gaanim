"""Camera along a path, a whip pan and a dolly zoom (CA-03)."""

import os

from gaanim import BLUE, CORAL, GOLD, GRAY, NAVY, WHITE, Material3D, Scene


scene = Scene(frame=(16, 9), background=NAVY)

# A winding road: the camera drives along it, turned to its direction.
road = scene.geometry.path(
    [("move", [(-8, -2)]), ("cubic", [(-4, 4), (0, -6), (4, 0)]), ("cubic", [(6, 3), (9, 2), (12, -1)])]
).no_fill().stroke(GRAY, 0.12)
for index, (x, y) in enumerate([(-6, 0), (-1, -2.5), (3.5, -1), (8, 2)]):
    scene.geometry.circle(0.5).fill([BLUE, CORAL, GOLD, WHITE][index]).move_to(x, y)
section_b = scene.text("B", role="title").fill(WHITE).move_to(30, 0)

scene.play([scene.camera.animate.zoom_to(1.6).duration(0.5)])
scene.play([scene.camera.animate.follow_path(road, orient=True).duration(3.0)])
scene.play([scene.camera.animate.reset().duration(0.6)])
scene.play([scene.camera.animate.whip_pan(section_b)])
scene.wait(0.4)

# A dolly zoom: the near cube keeps its size while the corridor stretches away.
scene.segment("vertigo")
for depth in range(6):
    for side in (-2.5, 2.5):
        scene.geometry.cube(1.0, material=Material3D.matte(BLUE if depth % 2 else CORAL)).move_to_3d(side, 0, -depth * 3.0)
scene.geometry.cube(1.2, material=Material3D.metal(GOLD)).move_to_3d(0, 0, 2)
scene.camera.perspective(fov_y=0.5, near=0.1, far=100)
scene.camera.look_at(eye=(0, 0.6, 9), target=(0, 0, 2))
scene.play([scene.camera.animate.dolly_zoom(0.35).duration(2.0)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.6, 2.0, 3.4, 4.25, 4.35, 4.6, 5.0, 5.9, 6.85])
else:
    scene.render()
