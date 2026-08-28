"""Expand a single tagged term into a longer expression."""

import os

from gaanim import Anchor, BLACK, GOLD, WHITE, Scene, part


scene = Scene(frame=(16, 9), background=BLACK)

title = scene.text("Descomponer una masa", role="title").fill(WHITE).move_to(0, 2.75, anchor=Anchor.CENTER)

compact = (
    scene.text.equation("E =", part("mass", "m"), "c^2")
    .move_to(0, 0.25, anchor=Anchor.CENTER)
)
expanded = (
    scene.text.equation("E =", part("mass", "(m_1 + m_2)"), "c^2")
    .move_to(0, 0.25, anchor=Anchor.CENTER)
)
caption = scene.text("La masa se abre; los términos nuevos emergen desde ella.").move_to(0, -2.125, anchor=Anchor.CENTER)

compact["mass"].fill(GOLD)
expanded["mass"].fill(GOLD)

scene.play([title.animate.write(), compact.animate.write(), caption.animate.fade_in()])
scene.wait(0.4)
scene.play([compact.animate.transform_to(expanded).duration(0.9)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.85, 2.65])

scene.render()
