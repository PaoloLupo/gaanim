"""Cancel one semantic term, then replace an equation state."""

import os

from gaanim import BLACK, GOLD, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLACK)
title = scene.title("Resolver una ecuación por términos").fill(WHITE).at(0, 230)

before = scene.equation(
    "x + 3 = 7",
    tags={"variable": "x", "constant": "3", "result": "7"},
).at(0, 40)
before.tag("variable").fill(GOLD)

after = scene.equation(
    "x = 4",
    tags={"variable": "x", "result": "4"},
).at(0, 40)
after.tag("variable").fill(GOLD)

caption = scene.text("Cancelamos el término que se elimina").fill(GRAY).at(0, -150)

scene.play([title.write(), before.write(), caption.fade_in()])
scene.wait(0.4)

# La línea hereda el color del término, se dibuja y luego este desaparece.
before.cancel_term("constant", duration=0.65)

caption.fade_out(duration=0.2)
caption = scene.text("Reemplazamos el paso y conservamos x").fill(GRAY).at(0, -150)
scene.play([caption.fade_in()])

# El tag `variable` fija la correspondencia x → x en la transición.
scene.replace_term(before, after, tag="variable", duration=0.8)
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.72, 3.25, 3.65, 4.05, 5.05])

scene.render()
