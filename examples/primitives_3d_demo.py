"""Native PBR primitives, material animation, and a deterministic 3D camera."""

import os

from gaanim import BLUE, CORAL, CYAN, GOLD, NAVY, WHITE, Material3D, Scene


scene = Scene(1280, 720, background=NAVY)
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)

floor = scene.geometry.plane(
    14,
    10,
    subdivisions=(8, 6),
    material=Material3D.matte(NAVY),
).move_to_3d(0, -2.2, 0)
cube = scene.geometry.cube(2.2, material=Material3D.matte(BLUE)).move_to_3d(-4.2, -1.0, 0)
sphere = scene.geometry.sphere(1.35, material=Material3D.metal(GOLD)).move_to_3d(-1.4, -0.85, 0)
cylinder = scene.geometry.cylinder(1.1, 2.8, material=Material3D.matte(CYAN)).move_to_3d(1.5, -0.8, 0)
cone = scene.geometry.cone(1.25, 3.0, material=Material3D.matte(CORAL)).move_to_3d(4.2, -0.7, 0)

scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.camera.look_at(eye=(10, 7, 13), target=(0, -0.5, 0))

scene.play([floor.animate.fade_in(0.6)])
scene.play([cube.animate.create(0.9), sphere.animate.create(0.9), cylinder.animate.create(0.9), cone.animate.create(0.9)])
scene.play(
    [
        cube.animate
        .material(Material3D.metal(CORAL))
        .rotate_by_3d("y", 1.2)
        .duration(1.2),
        sphere.animate
        .material(Material3D.emissive(WHITE, strength=2.5))
        .animate.scale_by(1.15)
        .duration(1.2),
        cylinder.animate.color(GOLD).rotate_by_3d("y", 1.2).duration(1.2),
        cone.animate.shift_by_3d(0, 0.4, 0).rotate_by_3d("y", -1.2).duration(1.2),
    ]
)
scene.play([scene.camera.animate.orbit(delta_yaw=0.55, delta_pitch=0.08).duration(1.5)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 1.1, 2.0, 3.0, 4.4])
else:
    scene.render()
