"""Lit solids on 3D axes seen by a camera that circles them, swings up and down, and keeps a readout fixed on the screen.

A metal sphere, a matte cube and a cone sit on the floor of the axes, with a
helix wound around them. The camera orbits twice: once around, and once
precessing as its pitch rises and falls. The readout of the camera's turn is
pinned to the screen with hud(), so it stays in place while the scene turns
under it.
"""

import math
import os

from gaanim import Axis, Easing, Material3D, Scene, Zones

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
muted = scene.colors.muted
geometry = scene.geometry
geometry.lighting_3d("studio", intensity=1.1)

X = Axis.linear(-3, 3).ticks(1)
Y = Axis.linear(-3, 3).ticks(1)
Z = Axis.linear(0, 3).ticks(1)
space = scene.viz.cartesian_3d(X, Y, Z, size=(6.0, 6.0, 3.0), numbers=False, labels=False)
sphere = geometry.sphere(0.8, material=Material3D.metal(gold)).move_to_3d(-1.2, -0.8, 0.8)
cube = geometry.cube(1.2, material=Material3D.matte(violet)).move_to_3d(1.3, -0.6, 0.6)
# A cone points along y; a quarter turn about x stands it on the floor.
cone = geometry.cone(0.6, 1.6, material=Material3D.matte(lilac)).rotate_to_3d(math.pi / 2, 0, 0).move_to_3d(0.2, 1.4, 0.8)
helix = space.parametric(lambda t: (2.4 * math.cos(t), 2.4 * math.sin(t), 0.12 * t), (0, 4 * math.pi)).stroke(gold, 0.04)

turn = scene.viz.parameter(0.0)
band = scene.layout.zones(Zones.rows(["1fr", "6fr"], padding="page_padding"))[0]
readout = scene.viz.readout(turn, label="yaw", format=".0f", suffix="°", font_size=0.5, color=muted).hud()
readout.place(band, anchor="left")

DISTANCE = 12.5
scene.camera.perspective(fov_y=0.65)
scene.camera.look_at(eye=(DISTANCE * 0.85, -DISTANCE * 0.5, 4.5), target=(0, 0, 0.8), up=(0, 0, 1))

scene.play(space.animate.create(), duration=1.0)
scene.play([solid.animate.grow_from_center() for solid in (sphere, cube, cone)], duration=0.8, easing=Easing.back())
scene.play([helix.animate.create(), readout.animate.fade_in()], duration=1.0)
scene.play([scene.camera.animate.orbit(delta_yaw=math.tau, delta_pitch=0.0), turn.animate.set(360)],
           duration=5.0, easing=Easing.SMOOTH)
# The second turn precesses: the pitch rises for half of it and falls back.
for pitch, yaw in ((0.35, 540), (-0.35, 720)):
    scene.play(
        [scene.camera.animate.orbit(delta_yaw=math.pi, delta_pitch=pitch), turn.animate.set(yaw)],
        duration=2.5,
        easing=Easing.SMOOTH,
    )
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Built, around the first turn, and up and down the second.
    scene.snapshots(snapshots, [2.6, 4.6, 6.6, 9.0, 10.4, 13.2])
else:
    scene.render()
