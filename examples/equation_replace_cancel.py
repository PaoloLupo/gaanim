"""Cancel one semantic term, then replace an equation state."""

import os

from gaanim import Anchor, BLACK, GOLD, GRAY, WHITE, Scene, part


scene = Scene(1280, 720, background=BLACK)
title = scene.text("Resolver una ecuación por términos", role="title").fill(WHITE).at(0, 230, anchor=Anchor.CENTER)

before = scene.equation(
    part("variable", "x"), "+", part("constant", "3"), "=", part("result", "7")
).at(0, 40, anchor=Anchor.CENTER)
before["variable"].fill(GOLD)

after = scene.equation(
    part("variable", "x"), "=", part("result", "4")
).at(0, 40, anchor=Anchor.CENTER)
after["variable"].fill(GOLD)

caption = scene.text("Cancelamos el término que se elimina").fill(GRAY).at(0, -150, anchor=Anchor.CENTER)

scene.play([title.write(), before.write(), caption.fade_in()])
scene.wait(0.4)

# La línea hereda el color del término, se dibuja y luego este desaparece.
scene.play([before["constant"].cancel(duration=0.65)])

scene.play([caption.fade_out(duration=0.2)])
caption = scene.text("Reemplazamos el paso y conservamos x").fill(GRAY).at(0, -150, anchor=Anchor.CENTER)
scene.play([caption.fade_in()])

# `step_to` reemplaza el Text completo y prioriza las rutas semánticas compartidas.
scene.play([before.step_to(after, duration=0.8)])
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.72, 3.25, 3.65, 4.05, 5.05])

scene.render()
