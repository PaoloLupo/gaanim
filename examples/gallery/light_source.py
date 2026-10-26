"""A gold point light that sweeps over three solids like a sun, lighting each from the side it passes.

light_3d makes a light, a drawable that draws nothing: while the scene shows
it, it replaces the fixed key light. The lamp and a small emissive bulb both
follow a polar point of one Parameter, so they arc over the solids together
in front of them, and a faint violet light stays on the far side as a fill.
The scene's own light is turned down, so the lamp shapes the solids.
"""

import math
import os

from gaanim import Easing, Material3D, Scene

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
paper = scene.colors.foreground
panel = scene.colors.panel
geometry = scene.geometry
geometry.lighting_3d("studio", intensity=0.35)

floor = geometry.plane(12, 8, subdivisions=(36, 24), material=Material3D.matte(panel)).move_to_3d(0, -1.2, 0)
solids = [
    geometry.cube(1.6, material=Material3D.matte(violet)).move_to_3d(-3.0, -0.4, 0),
    geometry.sphere(1.2, segments=48, rings=24, material=Material3D.matte(paper)).move_to_3d(0, 0, 0),
    geometry.cone(0.9, 2.2, material=Material3D.matte(lilac)).move_to_3d(3.0, -0.1, 0),
]

# The lamp and its bulb share a point that turns on an arc above the solids,
# in front of them: a follower takes the point's depth, which is its center's.
angle = scene.viz.parameter(math.pi * 0.95)
sun = geometry.polar_point((0.0, -0.6, 2.5), 3.9, angle)
lamp = geometry.light_3d(color=gold, intensity=1.8).follow(sun).hidden()
bulb = geometry.sphere(0.16, material=Material3D.emissive(gold, 2.0)).follow(sun).hidden()
fill = geometry.light_3d(color=violet, intensity=0.5).move_to_3d(2.0, 3.0, -6.0)

scene.camera.perspective(fov_y=0.7)
scene.camera.look_at(eye=(0, 3.2, 12), target=(0, 0, 0))

scene.play([floor.animate.fade_in(), *(solid.animate.create() for solid in solids)], duration=1.2)
scene.play([lamp.animate.fade_in(), bulb.animate.fade_in()], duration=0.8)
scene.play(angle.animate.set(math.pi * 0.05), duration=5.0, easing=Easing.SMOOTH)
scene.play(angle.animate.set(math.pi * 0.5), duration=2.0, easing=Easing.SMOOTH)
scene.play([lamp.animate.fade_out(), bulb.animate.fade_out()], duration=0.8)
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Lit by the dim scene, the lamp rising on the left, overhead, setting
    # on the right, back above, and switched off.
    scene.snapshots(snapshots, [1.0, 2.2, 4.4, 6.6, 9.0, 10.4])
else:
    scene.render()
