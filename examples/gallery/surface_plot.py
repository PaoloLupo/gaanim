"""A Gaussian hill on 3D axes that the camera circles while the hill narrows into a peak and spreads back out.

The surface is z = exp(-(x^2 + y^2) / s) for a Parameter s, colored by
height with the plasma color map, so it is sampled again every frame as s
changes. The camera starts low and orbits the axes once, rising as it goes.
"""

import math
import os

from gaanim import Axis, Easing, Scene

scene = Scene(theme="gaanim")
scene.geometry.lighting_3d("studio")

X = Axis.linear(-3, 3).ticks(1)
Y = Axis.linear(-3, 3).ticks(1)
Z = Axis.linear(0, 1.5).ticks(0.5)
space = scene.viz.cartesian_3d(X, Y, Z, size=(6.0, 6.0, 3.0), xz_grid=False, yz_grid=False)

spread = scene.viz.parameter(2.0)
hill = space.surface(
    lambda x, y, s: 1.2 * math.exp(-(x * x + y * y) / s), resolution=(56, 56), inputs=[spread], color_map="plasma"
)

DISTANCE = 15.0
scene.camera.perspective(fov_y=0.7)
scene.camera.look_at(eye=(DISTANCE * 0.8, -DISTANCE * 0.5, 3.0), target=(0, 0, 0.4), up=(0, 0, 1))

scene.play(space.animate.create(), duration=1.0)
scene.play(hill.animate.fade_in(), duration=0.8)
scene.play(
    [
        scene.camera.animate.orbit(delta_yaw=math.tau, delta_pitch=0.45),
        spread.animate.set(0.35).easing(Easing.there_and_back()),
    ],
    duration=8.0,
    easing=Easing.SMOOTH,
)
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The axes, the hill, and along the orbit as it narrows and spreads.
    scene.snapshots(snapshots, [0.8, 1.9, 3.8, 5.8, 7.8, 10.2])
else:
    scene.render()
