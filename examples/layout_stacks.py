"""Rows and columns replace group vstack/hstack helpers."""
import os

from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.5)
steps = scene.layout.column([scene.text("Pipeline", role="title").fill(GOLD), scene.text("Measure").fill(WHITE), scene.text("Solve").fill(WHITE), scene.text("Place").fill(BLUE)], gap=0.275, align="start")
page = scene.layout.stack([steps], within="safe", width="fill", height="fill", align="center")
scene.play([page.animate.fade_in().duration(0.5)])
scene.wait(2.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
