"""Expand a single tagged term into a longer expression."""

import os

from gaanim import BLACK, GOLD, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)

title = scene.title("Descomponer una masa").fill(WHITE).at(0, 220)

compact = (
    scene.equation("E = m c^2", tags={"mass": "m"})
    .at(0, 20)
)
expanded = (
    scene.equation("E = (m_1 + m_2) c^2", tags={"mass": "(m_1 + m_2)"})
    .at(0, 20)
)
caption = scene.text("La masa se abre; los términos nuevos emergen desde ella.").at(0, -170)

compact.tag("mass").fill(GOLD)
expanded.tag("mass").fill(GOLD)

scene.play([title.write(), compact.write(), caption.fade_in()])
scene.wait(0.4)
scene.expand_equation(compact, expanded, tag="mass", duration=0.9)
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.85, 2.65])

scene.render()
