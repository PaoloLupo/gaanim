"""Animated structural reflow in a nested Layout v2 tree."""
import os

from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background="#0b1020", margin=36)
agenda = scene.layout.column([scene.text("Measure").fill(WHITE), scene.text("Solve").fill(WHITE)], gap=18)
notes = scene.layout.column([scene.text("Deterministic").fill(BLUE)], gap=16)
body = scene.layout.row([scene.layout.item(agenda, grow=1), scene.layout.item(notes, grow=1)], gap=56, width="fill", align="center")
page = scene.layout.column([scene.text("Reflow", role="title").fill(GOLD), scene.layout.item(body, grow=1)], within="safe", width="fill", height="fill", padding=32, gap=28, align="stretch")
agenda.add(scene.text("Place").fill(WHITE))
scene.wait(2.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.2, 2.0, 3.2])
else:
    scene.render()
