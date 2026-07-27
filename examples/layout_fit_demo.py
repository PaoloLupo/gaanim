"""Visual regression fixture for a layout constrained with fit='shrink'."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLUE, margin=64)
frame = scene.frame_layout(header=112, footer=48, gap=24)

title = frame.header.place(scene.title("Contenido que cabe en su región"), Anchor.TOP_LEFT)

# The children are intentionally wider than the requested region. The layout
# measures the resulting group and scales it down as one visual unit.
formula = scene.layout("column", gap=28, width=320, height=180, fit="shrink")
formula.add(scene.text("Una ecuación larga no debe romper el slide").scaled(1.2))
formula.add(scene.equation("sum_(n=1)^infinity 1/n^2 = pi^2 / 6"))

content = frame.content.place(formula.drawable, Anchor.CENTER)
footer = frame.footer.place(scene.text("fit='shrink'").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), content.fade_in(), footer.fade_in()])
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 2.0])

scene.render()
