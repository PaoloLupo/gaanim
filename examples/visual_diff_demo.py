"""Minimal exact-seek visual regression example.

Set GAANIM_SNAPSHOTS to capture this scene before opening the viewer.
"""

import os

from gaanim import Easing, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9))
circle = scene.geometry.circle(1.2).fill(BLUE).stroke(WHITE, 0.066667).move_to(-3.666667, 0)

scene.play([circle.animate.create().duration(1.0).easing(Easing.SMOOTH)])
scene.play([circle.animate.shift_by(7.333333, 0).duration(1.5).easing(Easing.SMOOTH)])
scene.play([circle.animate.rotate_by(3.14159).duration(0.5), circle.animate.opacity(0.65).duration(0.5)])
circle.stroke(GOLD, 0.116667)
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0, 1.75, 2.5, 3.0, 3.5])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
