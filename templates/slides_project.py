"""Starter de slides generado por `gaanim init slides`."""

import os
from pathlib import Path

from gaanim import Anchor, Direction, Scene


ROOT = Path(__file__).resolve().parent

scene = Scene(1920, 1080, margin=72)
scene.load_project(str(ROOT / "gaanim.toml"))
scene.canvas.set_theme("presentation")
scene.brand(
    footer="MIS SLIDES",
    slide_numbers=True,
    rule=True,
)

opening = scene.segment(
    "Portada",
    layout="cover",
    notes="Presenta el tema y explica por qué importa.",
)
opening.region("title").place(scene.text("Mis slides", role="title"), Anchor.CENTER)
opening.region("subtitle").place(
    scene.text(
        "Una presentación construida con estructura semántica",
        role="subtitle",
    ),
    Anchor.CENTER,
)
scene.wait(0.5)
scene.stop("lista")

idea = scene.segment(
    "Idea principal",
    layout="content",
    notes="Explica una sola idea y revela el detalle cuando sea necesario.",
)
idea.region("title").place(scene.text("Una idea por slide", role="title"), Anchor.LEFT)
message = idea.region("content").place(
    scene.text(
        "Usa regiones para mantener jerarquía, alineación y márgenes consistentes.",
        wrap=1120,
        text_align="center",
        size=42,
    ),
    Anchor.CENTER,
)
scene.play([message.fade_in_from(Direction.DOWN).duration(0.5)])
scene.stop("mensaje")

closing = scene.segment(
    "Cierre",
    layout="conclusion",
    notes="Resume el mensaje e invita a preguntas.",
)
closing.region("title").place(scene.text("Gracias", role="title"), Anchor.CENTER)
closing.region("subtitle").place(
    scene.text("¿Preguntas?", role="subtitle"),
    Anchor.CENTER,
)
scene.wait(0.5)
scene.stop("preguntas")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2, 0.7, 1.2])
scene.render()
