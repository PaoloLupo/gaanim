"""A vector parametric curve sampled once during scene construction."""
import math
import os
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(720, 480, background=WHITE)
curve = scene.polyline([
    (210 * math.cos(t), 120 * math.sin(2 * t))
    for t in (2 * math.pi * index / 240 for index in range(241))
]).no_fill().stroke(BLUE, 4)
dot = scene.dot(10).fill(GOLD).at(210, 0)
title = scene.text("parametric curve").fill(BLACK).at(0, 190)
scene.play([curve.create().duration(1.0), dot.create().duration(0.4), title.write().duration(0.5)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.4, 0.7, 1.0])
scene.render()
