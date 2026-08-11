"""Layout v2 fit modes without manual coordinates."""
import os

from gaanim import BLUE, GOLD, Scene

scene = Scene(1280, 720, background="#0f172a", margin=36)
formula = scene.column(
    [scene.title("Fit modes").fill(GOLD), scene.item(scene.circle(110).fill(BLUE), fit="contain", grow=1)],
    within="safe", width="fill", height="fill", padding=32, gap=28, align="center",
)
scene.play([formula.fade_in().duration(0.5)])
scene.wait(1.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.0])
else:
    scene.render()
