"""Trace a Lottie composition before starting its own animation."""

import os

from gaanim import Scene, sequence

scene = Scene(background="#0b1020")
clip = scene.media.lottie("examples/assets/lottie_balls.json", width=12.5)
scene.play(sequence(clip.animate.create().duration(1.0), clip))
scene.wait(0.5)

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, [0.0, 0.35, 0.85, 1.0, 2.5, 0.35, 1.0])

scene.render()
