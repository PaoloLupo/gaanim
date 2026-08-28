"""Native cubic Bézier path and reactive point binding."""
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(13.5, 9), background=WHITE)
curve = scene.geometry.bezier((-4.125, -1.5), [(-1.875, 4.125), (1.875, -4.125)], (4.125, 1.5)).no_fill().stroke(BLUE, 0.075)
tracker = scene.viz.parameter(0.0)
dot = scene.geometry.point_on_curve(curve, tracker).fill(GOLD)
title = scene.text("native cubic Bézier").fill(BLACK).move_to(0, 3.5625, anchor=Anchor.CENTER)
scene.play([curve.animate.create().duration(0.8), dot.animate.create().duration(0.3), title.animate.write().duration(0.4)])
scene.play([tracker.animate.set(1.0).duration(2.0)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.2, 2.0, 2.8])
scene.render()
