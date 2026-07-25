"""Demostración de Layout persistente, anidado y con reflow animado."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLUE, margin=64)
frame = scene.frame_layout(header=112, footer=48, gap=24)

title = frame.header.place(
    scene.title("Layouts que se reordenan solos"),
    Anchor.TOP_LEFT,
)

# Un padre horizontal contiene dos layouts. Cada layout es un elemento más del
# árbol: no se necesitan coordenadas para los paneles ni para sus filas.
body = scene.layout("row", gap=72)

agenda = scene.layout("column", gap=20)
first = agenda.add(scene.text("1. Introducción").scaled(1.15))
second = agenda.add(scene.text("2. Idea principal").scaled(1.15))
third = agenda.add(scene.text("3. Demostración").scaled(1.15))

notes = scene.layout("column", gap=16)
notes.add(scene.text("Panel de apoyo").scaled(1.2).fill(WHITE))
notes.add(scene.text("El panel se mueve con el padre.").fill(GRAY))

body.add(agenda)
body.add(notes)
content = frame.content.place(body.drawable, Anchor.CENTER)
footer = frame.footer.place(
    scene.text("Layout.add(..., at=..., animate=...)").fill(GRAY),
    Anchor.BOTTOM_RIGHT,
)

scene.play([title.write(), first.fade_in(), second.fade_in(), third.fade_in(), footer.fade_in()])
scene.wait(0.8)

# La nueva fila entra entre las existentes. `agenda` mueve sus filas y luego
# propaga el reflow a `body`, que recoloca el panel derecho.
agenda.add(
    scene.text("Nueva fila: ejemplo interactivo").fill(WHITE).scaled(1.15),
    at=1,
    animate=0.6,
)
scene.wait(1.2)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.2, 2.0, 3.2])

scene.render()
