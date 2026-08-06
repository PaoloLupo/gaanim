"""A wrapped row lays cards out on multiple animated lines."""

import os

from gaanim import Anchor, BLUE, GRAY, WHITE, Scene


scene = Scene(1280, 720, background=BLUE, margin=64)
frame = scene.frame_layout(header=112, footer=48, gap=24)
title = frame.header.place(scene.title("Fila con wrapping"), Anchor.TOP_LEFT)

cards = frame.content.layout("row", width=660, gap=24, wrap=True)
for label in ["Contexto", "Modelo", "Resultado", "Ejercicio", "Resumen"]:
    cards.add(scene.text(label).scaled(1.15).fill(WHITE))

footer = frame.footer.place(scene.text("wrap=True").fill(GRAY), Anchor.BOTTOM_RIGHT)
scene.play([title.write(), footer.fade_in()])
scene.wait(0.8)
cards.add(scene.text("Nueva tarjeta").scaled(1.15).fill(WHITE), at=2, animate=0.5)
scene.wait(1.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 2.0, 3.0])

scene.render()
