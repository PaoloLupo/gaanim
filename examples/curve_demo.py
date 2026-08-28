"""A Typst-inspired composed curve with relative and automatic controls."""
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(13.5, 9), background=WHITE)
curve = scene.geometry.curve([
    ("move", [(-4.125, 1.5)]),
    ("line_rel", [(3.375, 0)]),
    ("cubic_rel", [(None), ("auto"), (4.125, -0.9375)]),
    ("quad", [(1.875, 3.0), (-4.125, 1.5)]),
    ("close_smooth", []),
]).fill(BLUE).stroke(BLACK, 0.075)
title = scene.text("composed curve").fill(BLACK).move_to(0, 3.5625, anchor=Anchor.CENTER)
marker = scene.geometry.dot(0.16875).fill(GOLD).move_to(-4.125, 1.5)
scene.play([curve.animate.create().duration(1.0), title.animate.write().duration(0.4), marker.animate.create().duration(0.3)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0])
scene.render()
