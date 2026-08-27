"""Deterministic Python callbacks with explicit reactive inputs."""

import math
import os

from gaanim import BLACK, Axis, RED, Scene

scene = Scene(1280, 720)
k = scene.viz.variable(1.0, label="$k$", format=".1f", color=RED)
radius = scene.viz.parameter(1.0)
axes = scene.viz.cartesian_2d(Axis.linear(-6, 6), Axis.linear(-2, 2))
curve = axes.plot(lambda x, frequency: math.sin(frequency * x), inputs=[k]).stroke(RED, 3)
area = scene.viz.readout(
    lambda current_radius: math.pi * current_radius**2,
    inputs=[radius],
    label="$A$",
    unit="$m^2$",
)
area.move_to(360, 220).fill(BLACK)
k.move_to(-360, 220)

scene.play([k.animate.create(), area.animate.create(), axes.animate.create(), curve.animate.write()])
scene.play([k.animate.set(4.0).duration(2), radius.animate.set(3.0).duration(2)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.5, 2.25, 3.0])
else:
    scene.render()
