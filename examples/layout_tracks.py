"""Fixed, auto, and fractional grid tracks."""
import os

from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#111827", margin=0.45)
grid = scene.layout.grid([scene.text("auto").fill(GOLD), scene.geometry.rect(1.5, 1.125).fill(BLUE), scene.text("2fr").fill(WHITE)], columns=["auto", 2.25, "2fr"], rows=["1fr"], gap=0.3, width="fill", height="fill", align="center")
page = scene.layout.column([scene.text("Grid tracks", role="title").fill(GOLD), scene.layout.item(grid, grow=1)], within="safe", width="fill", height="fill", padding=0.35, gap=0.3, align="stretch")
scene.play([page.animate.fade_in().duration(0.5)])
scene.wait(2.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
