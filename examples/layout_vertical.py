"""Vertical 9:16 regression fixture for the editorial layout API."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene

scene = Scene(720, 1280, background=BLUE, margin=48)
layout = scene.layout_preset("vertical_short")

title = layout.header.place(scene.title("Layout vertical"), Anchor.TOP_LEFT)
subtitle = layout.header.place(
    scene.subtitle("Texto, grid y safe areas"),
    Anchor.BOTTOM_LEFT,
)

grid = layout.content.grid(rows=12, columns=4, row_gap=20, column_gap=20)
copy_region = grid.area(0, 0, row_span=4, column_span=4).inset(8)
visual_region = grid.area(5, 0, row_span=6, column_span=4).inset(8)

copy = copy_region.place(
    scene.paragraph(
        "La misma composicion se adapta a un formato vertical sin coordenadas manuales.",
        width=copy_region.width,
        align="justify",
        line_spacing=1.25,
    ),
    Anchor.TOP_LEFT,
)
visual = visual_region.place(scene.circle(125).fill(WHITE), Anchor.CENTER)
footer = layout.footer.place(
    scene.text("gaanim - vertical").fill(GRAY),
    Anchor.BOTTOM_RIGHT,
)

scene.play(
    [
        title.write(),
        subtitle.fade_in(),
        copy.fade_in(),
        visual.create(),
        footer.draw_border_then_fill(),
    ]
)
scene.wait(2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.8, 1.6, 3.0])

scene.render()
