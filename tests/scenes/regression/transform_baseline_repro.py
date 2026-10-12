"""Regression fixture for baseline-preserving equation transforms."""

import os

from gaanim import BLACK, RED, WHITE, Scene


scene = Scene(frame=(16, 9), background=WHITE)
scene.geometry.line(-8, 0, 8, 0).stroke(RED, 0.02)
scene.geometry.circle(0.04).fill(BLACK).move_to(0, 0)

equation = (
    scene.text.equation(r"integral_(-infinity)^infinity = x^2 d x", role="title")
    .fill(BLACK)
    .scale_by(3)
    .move_to(0, 0)
)
equation_2 = (
    scene.text.equation(r"x^2 d x", role="title")
    .fill(BLACK)
    .scale_by(3)
    .move_to(0, 0)
)
scene.play([equation.animate.write().duration(2)])
scene.play([equation.animate.transform_to(equation_2).duration(1)])
scene.wait(0.25)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [2.0, 2.5, 3.0])

scene.render()
