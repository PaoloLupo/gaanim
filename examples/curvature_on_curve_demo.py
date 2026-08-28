"""A native osculating circle following a sampled parametric curve."""
import math
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(13.5, 9), background=WHITE)
curve = scene.geometry.polyline([
    (3.375 * math.cos(t), 1.875 * math.sin(2 * t))
    for t in (2 * math.pi * index / 240 for index in range(241))
]).no_fill().stroke(BLUE, 0.075)
tracker = scene.viz.parameter(0.15)
circle = scene.geometry.curvature_on_curve(curve, tracker).no_fill().stroke(GOLD, 0.05625)
title = scene.text("osculating circle").fill(BLACK).move_to(0, 3.5625, anchor=Anchor.CENTER)
scene.play([curve.animate.create().duration(0.7), circle.animate.create().duration(0.3), title.animate.write().duration(0.4)])
scene.play([tracker.animate.set(0.85).duration(2.0)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.2, 2.0, 2.7])
scene.render()
