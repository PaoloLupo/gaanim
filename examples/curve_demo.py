"""A Typst-inspired composed curve with relative and automatic controls."""
import os
from gaanim import Anchor, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(720, 480, background=WHITE)
curve = scene.geometry.curve([
    ("move", [(-220, 80)]),
    ("line_rel", [(180, 0)]),
    ("cubic_rel", [(None), ("auto"), (220, -50)]),
    ("quad", [(100, 160), (-220, 80)]),
    ("close_smooth", []),
]).fill(BLUE).stroke(BLACK, 4)
title = scene.text("composed curve").fill(BLACK).at(0, 190, anchor=Anchor.CENTER)
marker = scene.geometry.dot(9).fill(GOLD).at(-220, 80)
scene.play([curve.create().duration(1.0), title.write().duration(0.4), marker.create().duration(0.3)])
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.0])
scene.render()
