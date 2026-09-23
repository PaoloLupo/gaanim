"""Section dividers with an agenda and a persistent segmented progress rail.

Every section opens with a divider that lists the agenda; the marker slides
from the previous section to the current one. The HUD rail persists across
segments: past sections are full, and the current one fills step by step.
"""
import os

from gaanim import Anchor, Scene, Section, SectionStep, TextStyle, Transition

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
BRICK = "#B4532A"
INK, SOFT, MUTED = "#1F2328", "#57606A", "#A0A7B0"


def slide(title):
    def build(scene):
        text = scene.text(title, size=0.6).move_to(0, 0.4)
        scene.play(text.animate.fade_in(), duration=0.3)
        scene.wait(0.3)
    return build


fade = Transition.cross_fade(0.2)
sections = [
    Section("context", [SectionStep(name="Problema", build=slide("El problema"),
                                    transition=fade)], title="Contexto"),
    Section("method", [
        SectionStep(name="Modelo", build=slide("El modelo"), transition=fade),
        SectionStep(name="Datos", build=slide("Los datos"), transition=fade),
    ], title="Método"),
    Section("results", [SectionStep(name="Hallazgos", build=slide("Hallazgos"),
                                    transition=fade)], title="Resultados"),
]


def row(scene, entry, state):
    """Numbered agenda row; each state may change weight and color."""
    color = {"done": SOFT, "current": BRICK, "upcoming": MUTED}[state]
    number = scene.text(f"{entry.index + 1:02d}", size=0.2, color=color)
    name = scene.text(entry.title, size=0.3, weight=900 if state == "current" else 400,
                      color=INK if state == "current" else color)
    number.move_to(0, 0, Anchor.LEFT)
    name.move_to(0.65, 0, Anchor.LEFT)
    return scene.geometry.group([number, name])


def marker(scene):
    return scene.geometry.rounded_rect(0.07, 0.42, 0.035).fill(BRICK).no_stroke()


scene.segment("Portada")
rail = scene.sections.progress_rail(
    sections, segmented=True, length=14.6, thickness=0.04, fill_color=BRICK,
    captions=True, caption_colors={"current": BRICK},
    caption=lambda scene, entry: scene.text(entry.title.upper(), size=0.115, weight=900))
rail.root.move_to(0, -4.3).hud()
scene.persist(rail.root)
scene.play(rail.root.animate.fade_in(), duration=0.3)

previous = None
for index, section in enumerate(sections):
    scene.segment(f"Índice · {section.title}", fade)
    agenda = scene.sections.agenda(sections, current=previous, pitch=0.66, item=row,
                                   marker=marker)
    agenda.root.move_to(-2, 1.4, Anchor.TOP_LEFT)
    scene.play(agenda.root.animate.fade_in(), duration=0.3)
    scene.play([agenda.animate.focus(section), rail.animate.enter(section)], duration=0.6)
    scene.wait(0.3)
    section.build(scene, on_enter=lambda scene, progress: scene.play(
        rail.animate.to(progress), duration=0.3))
    previous = index

if "GAANIM_SNAPSHOTS" in os.environ:
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.3, 1.1, 3.2, 4.3, 6.4])
scene.render()
