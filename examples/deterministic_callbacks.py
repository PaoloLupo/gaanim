"""Pure Python reactive callbacks remain identical under direct and repeated seeks."""

import math
import os

from gaanim import BLUE, GOLD, RED, Axis, Scene, computed


scene = Scene(1280, 720)
amplitude = scene.viz.parameter(1.0)
phase = scene.viz.parameter(0.0)
plane = scene.viz.cartesian_2d(
    Axis.linear(-math.pi, math.pi).ticks(math.pi / 2),
    Axis.linear(-3.0, 3.0).ticks(1.0),
    width=960,
    height=500,
)

curve = plane.plot(
    lambda x, scale, offset: scale * math.sin(x + offset) if x >= 0 else scale * math.cos(x - offset),
    inputs=[amplitude, phase],
).stroke(BLUE, 4)

pulse = computed(
    lambda scale, t: 42.0 + 8.0 * scale * math.sin(t),
    inputs=[amplitude, scene.viz.time],
)
angle = computed(lambda offset, t: offset + 0.35 * t, inputs=[phase, scene.viz.time])
marker = scene.geometry.dot(9).fill(GOLD).follow(scene.geometry.polar_point((0.0, 0.0), pulse, angle))
value = scene.viz.readout(
    lambda scale, offset: scale**2 + offset if scale >= 0 else float("nan"),
    inputs=[amplitude, phase],
    label="$q$",
    invalid="invalid",
).fill(RED).at(430, 260)

scene.play([plane.create(), curve.create(), marker.fade_in(), value.fade_in()])
scene.play([amplitude.animate_to(2.0, duration=2.0), phase.animate_to(math.pi, duration=2.0)])
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.75, 1.5, 2.25, 3.0, 4.0])
else:
    scene.render()
