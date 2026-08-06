"""Minimal exact-seek visual regression example.

Set GAANIM_SNAPSHOTS to capture this scene before opening the viewer.
"""

import os

from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(960, 540)
circle = scene.circle(72).fill(BLUE).stroke(WHITE, 4).at(-220, 0)

scene.play([circle.create(1.0).smooth()])
scene.play([circle.move(440, 0).duration(1.5).smooth()])
scene.play([circle.rotate(3.14159).duration(0.5), circle.fade_to(0.65).duration(0.5)])
circle.stroke(GOLD, 7)
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0, 1.75, 2.5, 3.0, 3.5])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
