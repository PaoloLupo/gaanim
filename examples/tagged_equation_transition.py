"""Transform matching semantic parts between two equations."""

import os

from gaanim import BLACK, BLUE, GOLD, GRAY, WHITE, Scene, GREEN


scene = Scene(1280, 720, background=BLACK)

title = scene.title("Una variable, dos ecuaciones").fill(WHITE).at(0, 230)

energy = (
    scene.equation(
        "E = m c^2",
        tags={
            "mass": "m",
            "light_speed": "c^2",
        },
    )
    .at(0, 70)
)

momentum = (
    scene.equation(
        "p = m v",
        tags={
            "mass": "m",
            "velocity": "v",
        },
    )
    .at(0, -90)
)

caption = scene.text("La etiqueta 'mass' conecta ambos términos.").fill(GRAY).at(0, -240)

# El color también puede aplicarse usando el nombre semántico.
energy.tag("mass").fill(GOLD)
# momentum.tag("velocity").fill(BLUE)

scene.play([title.write(), energy.write(), momentum.fade_in(), caption.fade_in()])
scene.wait(0.5)

# La fuente permanece y una copia semántica viaja hasta la ecuación destino.
scene.copy_equation_terms(energy, momentum, tags=["mass"], duration=0.9)
scene.wait(0.25)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    # Include the exact handoff frame: both `m` glyphs must overlap here.
    scene.snapshots(snapshot_dir, [0.0, 1.0, 1.5, 1.8, 2.55])

scene.render()
