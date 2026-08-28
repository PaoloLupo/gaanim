"""Incremental content with persistent Layout v2."""
import os

from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#101827", margin=0.5)
page = scene.layout.column([scene.text("Incremental flow", role="title").fill(GOLD)], within="safe", width="fill", height="fill", padding=0.4, gap=0.25, align="start")
page.add(scene.text("Measure").fill(WHITE))
page.add(scene.text("Solve").fill(WHITE))
page.add(scene.text("Place").fill(WHITE))
scene.wait(2.25)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
