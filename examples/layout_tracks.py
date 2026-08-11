"""Fixed, auto, and fractional grid tracks."""
import os

from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background="#111827", margin=36)
grid = scene.grid([scene.text("auto").fill(GOLD), scene.rect(120, 90).fill(BLUE), scene.text("2fr").fill(WHITE)], columns=["auto", 180, "2fr"], rows=["1fr"], gap=24, width="fill", height="fill", align="center")
page = scene.column([scene.text("Grid tracks", role="title").fill(GOLD), scene.item(grid, grow=1)], within="safe", width="fill", height="fill", padding=28, gap=24, align="stretch")
scene.play([page.fade_in().duration(0.5)])
scene.wait(2.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
