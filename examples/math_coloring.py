"""Glyph-level colors for text and Typst equations."""

import os

from gaanim import Anchor, BLACK, BLUE, GOLD, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=64)
layout = scene.frame_layout(header=150, footer=64, gap=24)

title = layout.header.place(
    scene.title("Colores que explican la fórmula").color_by("fórmula", GOLD),
    Anchor.TOP_LEFT,
)

equation = layout.content.place(
    scene.equation("E = m c^2").color_by("m", GOLD).color_by("c", BLUE),
    Anchor.CENTER,
)

caption = layout.content.place(
    scene.text("La masa aporta oro; la velocidad de la luz, azul.")
    .color_by("masa", GOLD)
    .color_by("velocidad", BLUE),
    Anchor.BOTTOM,
)

footer = layout.footer.place(scene.text("gaanim · fragment colors").fill(GRAY), Anchor.BOTTOM_RIGHT)

scene.play([title.write(), equation.write(), caption.fade_in(), footer.fade_in()])
equation.select("m").indicate(duration=0.8)
equation.select("c").color_to(BLUE, duration=0.8)
scene.wait(1.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 2.0, 4.0])

scene.render()
