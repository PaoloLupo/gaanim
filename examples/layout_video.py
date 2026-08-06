"""A reusable editorial layout with header, content columns, and footer."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene

scene = Scene(1280, 720, background=BLUE, margin=48)
layout = scene.frame_layout(header=132, footer=56, gap=24)

title = layout.header.place(scene.title("Cómo funciona el layout"), Anchor.TOP_LEFT)
subtitle = layout.header.place(scene.subtitle("Una composición estable para cada segmento"), Anchor.BOTTOM_LEFT)

grid = layout.content.grid(rows=1, columns=12, column_gap=24)
left = grid.area(0, 0, column_span=5)
right = grid.area(0, 5, column_span=7).inset(12)
explanation = left.place(
    scene.paragraph(
        "Una idea principal explicada en varias líneas, con ancho controlado y texto justificado.",
        width=left.width,
        align="justify",
        line_spacing=1.40,
    ),
    Anchor.TOP_LEFT,
)
diagram = right.place(scene.circle(110).fill(WHITE), Anchor.CENTER)
footer = layout.footer.place(scene.text("gaanim • capítulo 1").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), subtitle.fade_in(), explanation.fade_in(), diagram.create(), footer.draw_border_then_fill()])
scene.wait(2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 0.8, 1.6, 3.0])

scene.render()
