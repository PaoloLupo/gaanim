"""Animated vector fill percentages, suitable for visual snapshot comparison."""

import os

from gaanim import Scene


scene = Scene(frame=(13.714286, 9), background="#101827")
scene.text("Vector fill level", role="title").move_to(0, 3.214286)
mask = scene.geometry.circle(2.142857).no_fill().stroke("#dbeafe", 0.085714).move_to(0, -0.428571).opacity(0)
water = scene.geometry.fill_level(mask, "#38bdf8", 0.0, direction="up", keep_outline=True)
scene.play([water.animate.fill_level(0.75).duration(1.2)])
scene.play([mask.animate.rotate_by(0.45).duration(0.8)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.2, 2.0])
else:
    scene.render()
