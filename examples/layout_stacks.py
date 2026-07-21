"""Regression fixture for layout regions combined with vstack/hstack."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene

scene = Scene(1280, 720, background=BLUE, margin=48)
layout = scene.layout(header=120, footer=56, gap=24)

title = layout.header.place(scene.title("Stacks dentro del grid"), Anchor.TOP_LEFT)
grid = layout.content.grid(rows=1, columns=12, column_gap=24)
copy_region = grid.area(0, 0, column_span=5).inset(12)
visual_region = grid.area(0, 5, column_span=7).inset(12)

headline = scene.text("Tres pasos").scaled(1.25)
step_one = scene.text("1. Define una region")
step_two = scene.text("2. Agrupa el contenido")
step_three = scene.text("3. Apila y anima")
steps = scene.group([headline, step_one, step_two, step_three]).vstack(gap=22)
steps = copy_region.place(steps, Anchor.TOP_LEFT)

visual = visual_region.place(scene.circle(120).fill(WHITE), Anchor.CENTER)
footer = layout.footer.place(scene.text("gaanim - stack").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), steps.fade_in(), visual.create(), footer.fade_in()])
scene.wait(2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.8, 1.6, 3.0])

scene.render()
