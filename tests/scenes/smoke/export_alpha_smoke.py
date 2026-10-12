"""Transparent canvas used by the export alpha smoke test."""

import os

from gaanim import GOLD, Scene


scene = Scene(frame=(16, 9), background="#00000000", margin=0)
scene.geometry.rect(4, 3).fill(GOLD).no_stroke()
scene.wait(0.2)

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0])

scene.render()
