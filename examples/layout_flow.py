"""Incremental content with persistent Layout v2."""
import os

from gaanim import GOLD, WHITE, Scene

scene = Scene(1280, 720, background="#101827", margin=40)
page = scene.column([scene.title("Incremental flow").fill(GOLD)], within="safe", width="fill", height="fill", padding=32, gap=20, align="start")
page.add(scene.text("Measure").fill(WHITE), animate=0.25)
page.add(scene.text("Solve").fill(WHITE), animate=0.25)
page.add(scene.text("Place").fill(WHITE), animate=0.25)
scene.wait(2.25)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
