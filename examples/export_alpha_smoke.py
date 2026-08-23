"""Transparent canvas used by the export alpha smoke test."""

import os

from gaanim import GOLD, Scene


scene = Scene(320, 180, background="#00000000", margin=0)
scene.rect(80, 60).fill(GOLD).no_stroke()
scene.wait(0.2)

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0])

scene.render()
