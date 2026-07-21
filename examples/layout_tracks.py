"""Regression fixture for fixed and fractional grid tracks."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene

scene = Scene(1280, 720, background=BLUE, margin=48)
layout = scene.layout(header=120, footer=56, gap=24)
title = layout.header.place(scene.title("Tracks fijos y flexibles"), Anchor.TOP_LEFT)

grid = layout.content.grid_tracks(
    rows=["1fr"],
    columns=[260, "1fr", "2fr"],
    column_gap=24,
)

fixed = grid.cell(0, 0).inset(12)
middle = grid.cell(0, 1).inset(12)
wide = grid.cell(0, 2).inset(12)

fixed_text = fixed.place(scene.text("260 px fijos"), Anchor.TOP_LEFT)
middle_text = middle.place(scene.text("1fr"), Anchor.CENTER)
wide_visual = wide.place(scene.circle(110).fill(WHITE), Anchor.CENTER)
footer = layout.footer.place(scene.text("gaanim - tracks").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), fixed_text.fade_in(), middle_text.fade_in(), wide_visual.create(), footer.fade_in()])
scene.wait(2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.8, 1.6, 3.0])

scene.render()
