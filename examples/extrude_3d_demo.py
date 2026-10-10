"""3D extrusion (CA-06): text, a shape with holes and a beveled star turn into lit meshes."""

import os

from gaanim import BLUE, CORAL, GOLD, NAVY, Material3D, Scene


scene = Scene(frame=(16, 9), background=NAVY)
scene.geometry.lighting_3d("studio", intensity=1.0)

word = scene.geometry.extrude(
    scene.text("HOLA", size=1.8).move_to(0, 1.6), depth=0.5, bevel=0.05, material=Material3D.metal(GOLD)
)
# A ring keeps its hole; its fill color becomes the material.
ring = scene.geometry.extrude(scene.geometry.annulus(1.1, 0.5).fill(CORAL).move_to(-4, -1.6), depth=0.4)
star = scene.geometry.extrude(
    scene.geometry.star(5, 1.2, 0.5).fill(BLUE).move_to(4, -1.6), depth=0.3, bevel=0.06
)

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.camera.look_at(eye=(0, 0, 16), target=(0, 0, 0))

scene.play([word.animate.create(), ring.animate.create(), star.animate.create()], duration=1.0)
scene.play([scene.camera.animate.orbit(delta_yaw=0.7, delta_pitch=0.35).duration(1.5)])
scene.play([star.animate.rotate_by_3d("y", 1.2), word.animate.material(Material3D.matte(GOLD))], duration=1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.5, 1.0, 1.8, 2.5, 3.5])
else:
    scene.render()
