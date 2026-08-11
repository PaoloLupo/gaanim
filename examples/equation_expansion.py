"""Expand a single tagged term into a longer expression."""

import os

from gaanim import BLACK, GOLD, WHITE, Scene, part


scene = Scene(1280, 720, background=BLACK)

title = scene.text("Descomponer una masa", role="title").fill(WHITE).at(0, 220)

compact = (
    scene.text("$E = ", part("mass", "m"), " c^2$")
    .at(0, 20)
)
expanded = (
    scene.text("$E = ", part("mass", "(m_1 + m_2)"), " c^2$")
    .at(0, 20)
)
caption = scene.text("La masa se abre; los términos nuevos emergen desde ella.").at(0, -170)

compact["mass"].fill(GOLD)
expanded["mass"].fill(GOLD)

scene.play([title.write(), compact.write(), caption.fade_in()])
scene.wait(0.4)
scene.play([compact.expand_to(expanded, anchor="mass", duration=0.9)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.85, 2.65])

scene.render()
