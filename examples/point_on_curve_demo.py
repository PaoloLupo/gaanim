"""A native marker following a sampled parametric curve by arc length."""
import math
import os

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(720, 480, background=WHITE)
curve = (
    scene.parametric_curve(
        lambda t: (210 * math.cos(t), 120 * math.sin(2 * t)),
        t=(0, 2 * math.pi),
    )
    .no_fill()
    .stroke(BLUE, 4)
)
tracker = scene.value_tracker(0.0)
dot = scene.point_on_curve(curve, tracker).fill(GOLD)
title = scene.text("point on curve").fill(BLACK).at(0, 190)
scene.play([curve.create().duration(0.7), dot.create().duration(0.3), title.write().duration(0.4)])
scene.play([tracker.animate_to(1.0).duration(2.0)])

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.2, 2.0, 2.7])
scene.render()
