"""Layout v2 fit modes without manual coordinates."""
import os

from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.45)
formula = scene.layout.column(
    [scene.text("Fit modes", role="title").fill(GOLD), scene.layout.item(scene.geometry.circle(1.375).fill(BLUE), fit="contain", grow=1)],
    within="safe", width="fill", height="fill", padding=0.4, gap=0.35, align="center",
)
scene.play([formula.animate.fade_in().duration(0.5)])
scene.wait(1.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.0])
else:
    scene.render()
