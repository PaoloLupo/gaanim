"""A region-bound persistent layout stays anchored during reflow."""

import os

from gaanim import Anchor, BLUE, GRAY, Scene


scene = Scene(1280, 720, background=BLUE, margin=64)
frame = scene.frame_layout(header=112, footer=48, gap=24)

title = frame.header.place(scene.title("Layout anclado a content"), Anchor.TOP_LEFT)
agenda = frame.content.layout("column", gap=20, fit="shrink")

first = agenda.add(scene.text("Introducción").scaled(1.15))
second = agenda.add(scene.text("Idea principal").scaled(1.15))
third = agenda.add(scene.text("Demostración").scaled(1.15))
footer = frame.footer.place(scene.text("content.layout(...)").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), first.fade_in(), second.fade_in(), third.fade_in(), footer.fade_in()])
scene.wait(0.8)
agenda.add(scene.text("Nueva sección").scaled(1.15), at=1, animate=0.5)
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 2.0, 3.0])

scene.render()
