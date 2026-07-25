"""Regression fixture for the deferred Flow layout builder."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene

scene = Scene(1280, 720, background=BLUE, margin=48)
layout = scene.frame_layout(header=120, footer=56, gap=24)
title = layout.header.place(scene.title("Flow de contenido"), Anchor.TOP_LEFT)

grid = layout.content.grid(rows=1, columns=12, column_gap=24)
copy_region = grid.area(0, 0, column_span=5).inset(12)
visual_region = grid.area(0, 5, column_span=7).inset(12)

flow = scene.flow(gap=20)
flow.add(scene.text("Sin grupos manuales").scaled(1.25))
flow.add(scene.text("1. Agrega contenido"))
flow.add(scene.text("2. Construye el flow"))
flow.add(scene.text("3. Colocalo en una region"))
content = copy_region.place(flow.build(), Anchor.TOP_LEFT)

visual = visual_region.place(scene.circle(120).fill(WHITE), Anchor.CENTER)
footer = layout.footer.place(scene.text("gaanim - flow").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), content.fade_in(), visual.create(), footer.fade_in()])
scene.wait(2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.8, 1.6, 3.0])

scene.render()
