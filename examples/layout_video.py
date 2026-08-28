"""Complete video composition with Layout v2."""
import os

from gaanim import BLUE, GOLD, Scene, lecture

scene = Scene(frame=(16, 9), background="#0b1020", margin=0.6)
page = scene.layout.template(lecture, title=scene.text("Layout v2", role="title").fill(GOLD), body=scene.layout.row([scene.layout.item(scene.text("Trees, constraints, responsive text and animated reflow."), grow=2), scene.layout.item(scene.geometry.circle(1.5).fill(BLUE), grow=1, fit="contain")], width="fill", gap=0.5), footer=scene.text("Gaanim"))
scene.play([page.animate.fade_in().duration(0.6)])
scene.wait(2.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
