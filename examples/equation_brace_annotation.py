"""Semantic brace labels and annotations attached to equation tags."""

import os

from gaanim import Anchor, BLACK, CORAL, GOLD, GRAY, WHITE, Scene, part


scene = Scene(frame=(16, 9), background=BLACK)

title = scene.text("Explicar los términos de una ecuación", role="title").fill(WHITE).move_to(0, 3.125, anchor=Anchor.CENTER)
formula = scene.text.equation(
    part("energy", "E"), "=", part("mass", "m"), part("light_speed", "c^2")
).move_to(0, 0.75, anchor=Anchor.CENTER)
formula["mass"].fill(GOLD)
formula["light_speed"].fill(CORAL)

caption = scene.text("La llave nombra la masa").fill(GRAY).move_to(0, -2.75, anchor=Anchor.CENTER)

scene.play([title.animate.write(), formula.animate.write(), caption.animate.fade_in()])
scene.wait(0.4)

# El énfasis se limita a los glifos del tag ``mass``.
scene.play([formula["mass"].animate.highlight().duration(0.65)])
scene.wait(0.5)

scene.play([caption.animate.fade_out().duration(0.2)])
caption = scene.text("La línea sigue al término anotado").fill(GRAY).move_to(0, -2.75, anchor=Anchor.CENTER)
scene.play([caption.animate.fade_in()])

# La onda conserva la selección semántica aunque cambie el layout del texto.
scene.play([formula["light_speed"].animate.wave().duration(0.65)])
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 2.2, 4.0, 4.5])

scene.render()
