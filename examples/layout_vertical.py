"""The same tree adapts to a vertical viewport."""
import os

from gaanim import BLUE, GOLD, Scene, vertical_short

scene = Scene(720, 1280, background="#0f172a", margin=48)
page = scene.template(vertical_short, title=scene.text("Vertical", role="title").fill(GOLD), body=scene.circle(180).fill(BLUE), caption=scene.text("9:16 · no at()"))
scene.play([page.fade_in().duration(0.5)])
scene.wait(2.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
