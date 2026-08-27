"""Animated vector fill percentages, suitable for visual snapshot comparison."""

import os

from gaanim import Scene


scene = Scene(640, 420, background="#101827")
scene.text("Vector fill level", role="title").at(0, 150)
mask = scene.geometry.circle(100).no_fill().stroke("#dbeafe", 4).at(0, -20).opacity(0)
water = scene.geometry.fill_level(mask, "#38bdf8", 0.0, direction="up", keep_outline=True)
scene.play([water.animate().fill_level(0.75).duration(1.2)])
scene.play([mask.rotate(0.45).duration(0.8)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.2, 2.0])
else:
    scene.render()
