"""A native osculating circle following a sampled parametric curve."""
import math
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(720, 480, background=WHITE)
curve = scene.polyline([
    (180 * math.cos(t), 100 * math.sin(2 * t))
    for t in (2 * math.pi * index / 240 for index in range(241))
]).no_fill().stroke(BLUE, 4)
tracker = scene.parameter(0.15)
circle = scene.curvature_on_curve(curve, tracker).no_fill().stroke(GOLD, 3)
title = scene.text("osculating circle").fill(BLACK).at(0, 190, anchor=Anchor.CENTER)
scene.play([curve.create().duration(0.7), circle.create().duration(0.3), title.write().duration(0.4)])
scene.play([tracker.animate_to(0.85).duration(2.0)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.2, 2.0, 2.7])
scene.render()
