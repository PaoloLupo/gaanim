"""Minimal native-3D scene used by the isolated export smoke test."""

from gaanim import BLUE, NAVY, Material3D, Scene


scene = Scene(frame=(16, 9), background=NAVY)
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=False)

cube = scene.geometry.cube(1.6, material=Material3D.matte(BLUE))
scene.camera.perspective(fov_y=0.785, near=0.1, far=100.0)
scene.camera.look_at(eye=(3.5, 2.5, 4.5), target=(0, 0, 0))
scene.play([cube.animate.create().duration(0.2)])
scene.render()
