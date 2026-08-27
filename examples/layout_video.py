"""Complete video composition with Layout v2."""
import os

from gaanim import BLUE, GOLD, Scene, lecture

scene = Scene(1280, 720, background="#0b1020", margin=48)
page = scene.layout.template(lecture, title=scene.text("Layout v2", role="title").fill(GOLD), body=scene.layout.row([scene.layout.item(scene.text("Trees, constraints, responsive text and animated reflow."), grow=2), scene.layout.item(scene.geometry.circle(120).fill(BLUE), grow=1, fit="contain")], width="fill", gap=40), footer=scene.text("Gaanim"))
scene.play([page.fade_in().duration(0.6)])
scene.wait(2.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.6, 3.0])
else:
    scene.render()
