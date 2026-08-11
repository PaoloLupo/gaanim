"""Native reactive parameters, visible variables, and traced plotting."""

import os

from gaanim import BLACK, GREEN, Axis, RED, Scene, math as gm

scene = Scene(1280, 720)
k = scene.variable(1.0, label="$k$", format=".1f", color=RED)
radius = scene.parameter(1.0)
axes = scene.axes(Axis.linear(-6, 6), Axis.linear(-2, 2))
curve = axes.plot(lambda x: gm.sin(k * x)).stroke(RED, 3)
area = scene.readout(lambda: gm.pi * radius**2, label="$A$", unit="$m ^2$")
area.at(360, 220).fill(BLACK)
k.at(-360, 220)

scene.play([k.create(), area.create(), axes.create(), curve.write()])
scene.play([k.animate_to(4.0, duration=2), radius.animate_to(3.0, duration=2)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.5, 2.25, 3.0])
else:
    scene.render()
