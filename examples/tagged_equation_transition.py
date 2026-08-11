"""Transform matching semantic parts between two equations."""

import os

from gaanim import BLACK, BLUE, GOLD, GRAY, WHITE, Scene, GREEN, part


scene = Scene(1280, 720, background=BLACK)

title = scene.text("Una variable, dos ecuaciones", role="title").fill(WHITE).at(0, 230)

energy = (
    scene.text("$E = ", part("mass", "m"), " ", part("light_speed", "c^2"), "$")
    .at(0, 70)
)

momentum = (
    scene.text("$p = ", part("mass", "m"), " ", part("velocity", "v"), "$")
    .at(0, -90)
)

caption = scene.text("La etiqueta 'mass' conecta ambos términos.").fill(GRAY).at(0, -240)

# El color también puede aplicarse usando el nombre semántico.
energy["mass"].fill(GOLD)
# momentum["velocity"].fill(BLUE)

scene.play([title.write(), energy.write(), momentum.fade_in(), caption.fade_in()])
scene.wait(0.5)

# La fuente permanece y una copia semántica viaja hasta la ecuación destino.
scene.play([energy["mass"].copy_to(momentum["mass"], duration=0.9)])
scene.wait(0.25)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    # Include the exact handoff frame: both `m` glyphs must overlap here.
    scene.snapshots(snapshot_dir, [0.0, 1.0, 1.5, 1.8, 2.55])

scene.render()
