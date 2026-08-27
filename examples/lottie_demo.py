"""Minimal vector Lottie playback synchronized with the Gaanim timeline."""

import os

from gaanim import GOLD, Scene

scene = Scene(background="#0b1020")
title = scene.text("Lottie + Velato", role="title").fill(GOLD).move_to(0, 230)
composition = scene.media.lottie(
    "examples/assets/lottie_balls.json",
    width=1000,
    fit="contain",
)

scene.play([title.animate.fade_in().duration(0.5), composition])
scene.wait(0.5)

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    count = scene.snapshots(snapshot_dir, [0.0, 0.5, 1.5, 3.0, 4.1])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
