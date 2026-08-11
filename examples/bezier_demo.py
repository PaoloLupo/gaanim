"""Native cubic Bézier path and reactive point binding."""
import os
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(720, 480, background=WHITE)
curve = scene.bezier((-220, -80), [(-100, 220), (100, -220)], (220, 80)).no_fill().stroke(BLUE, 4)
tracker = scene.parameter(0.0)
dot = scene.point_on_curve(curve, tracker).fill(GOLD)
title = scene.text("native cubic Bézier").fill(BLACK).at(0, 190)
scene.play([curve.create().duration(0.8), dot.create().duration(0.3), title.write().duration(0.4)])
scene.play([tracker.animate_to(1.0).duration(2.0)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.2, 2.0, 2.8])
scene.render()
