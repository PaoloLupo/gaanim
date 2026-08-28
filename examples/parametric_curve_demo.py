"""A vector parametric curve sampled once during scene construction."""
import math
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(13.5, 9), background=WHITE)
curve = scene.geometry.polyline([
    (3.9375 * math.cos(t), 2.25 * math.sin(2 * t))
    for t in (2 * math.pi * index / 240 for index in range(241))
]).no_fill().stroke(BLUE, 0.075)
dot = scene.geometry.dot(0.1875).fill(GOLD).move_to(3.9375, 0)
title = scene.text("parametric curve").fill(BLACK).move_to(0, 3.5625, anchor=Anchor.CENTER)
scene.play([curve.animate.create().duration(1.0), dot.animate.create().duration(0.4), title.animate.write().duration(0.5)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.4, 0.7, 1.0])
scene.render()
