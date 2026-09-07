"""Ordered section content with progress and scene-unit arrows."""
import os

from gaanim import Scene, Section, SectionStep, Transition

scene = Scene(frame=(16, 9))
scene.canvas.set_theme("paper")
scene.segment("Cover")
rail = scene.geometry.rect(12, 0.08).fill("#DDE0E6").no_stroke().move_to(0, -3.8).hud()
progress = scene.geometry.fill_level(rail, "#e26d5c", 0, direction="left",
    keep_outline=False).hud()
scene.persist(rail, progress)
scene.play(rail.animate.fade_in(), duration=0.2)


def arrows(scene):
    title = scene.text("Flechas en unidades de escena", size=0.5).move_to(0, 2)
    glyphs = [scene.geometry.arrow(x1, y1, x2, y2,
        head_length=0.18, head_width=0.15, body_width=0.036, max_head_ratio=0.3
    ).fill("#e26d5c").no_stroke()
        for x1, y1, x2, y2 in [(-4, 0, -2, 0), (0, -1, 0, 1),
                               (2, -1, 4, 1), (5, 0, 5.2, 0)]]
    scene.play([title.animate.fade_in(), *[a.animate.create() for a in glyphs]], duration=0.5)
    scene.stop("arrows")


def conclusion(scene):
    text = scene.text("Las pausas no avanzan la sección", size=0.5)
    scene.play(text.animate.write(), duration=0.5)
    scene.stop("explain")
    scene.wait(0.2)
    scene.stop("continue")


section = Section("demo", [
    SectionStep(name="Flechas", build=arrows, transition=Transition.cross_fade(0.2)),
    SectionStep(name="Pausas", build=conclusion, notes="Dos pausas, un solo paso.",
                transition=Transition.cross_fade(0.2)),
])


def enter(scene, context):
    scene.play(progress.animate.fill_level(context.fraction), duration=0.2)


section.build(scene, on_enter=enter)
if "GAANIM_SNAPSHOTS" in os.environ:
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.9, 1.6])
scene.render()
