"""Reactive curve bindings on an arc stored as native Bézier segments."""
import math
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(720, 480, background=WHITE)
curve = scene.geometry.arc(0, 0, 160, 0.0, math.pi * 1.35).no_fill().stroke(BLUE, 4)
tracker = scene.viz.parameter(0.0)
dot = scene.geometry.point_on_curve(curve, tracker).fill(GOLD)
tangent = scene.geometry.tangent_on_curve(curve, tracker, length=80).stroke(BLACK, 4)
title = scene.text("Bézier curve bindings").fill(BLACK).move_to(0, 190, anchor=Anchor.CENTER)
scene.play([curve.animate.create().duration(0.7), dot.animate.create().duration(0.3), tangent.animate.create().duration(0.3), title.animate.write().duration(0.4)])
scene.play([tracker.animate.set(1.0).duration(2.0)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.2, 2.0, 2.7])
scene.render()
