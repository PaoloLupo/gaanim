"""Geometric wheel bounds versus a shared typographic baseline."""
import os
from gaanim import Anchor, BLACK, CORAL, Easing, Scene, TextAnchor, WHITE

scene = Scene(frame=(16, 9), background=WHITE)
for y in [1.4, -1.2]:
    scene.geometry.line(-6, y, 6, y).stroke(CORAL, 0.012)
scene.text("Caja inferior / línea base", size=0.3).fill(BLACK).move_to(-6, 2.5, TextAnchor.BASELINE_LEFT)
scene.text("Ambos sobre la misma línea base", size=0.3).fill(BLACK).move_to(-6, -0.1, TextAnchor.BASELINE_LEFT)
scene.viz.rolling_number(2, min_digits=2, font_size=1, color=CORAL).move_to(-5, 1.4, Anchor.BOTTOM_LEFT)
scene.text("Objetivos", size=1).fill(BLACK).move_to(-3.5, 1.4, TextAnchor.BASELINE_LEFT)
number = scene.viz.rolling_number(2, min_digits=2, font_size=1, color=CORAL).move_to(-5, -1.2, TextAnchor.BASELINE_LEFT)
scene.text("Objetivos", size=1).fill(BLACK).move_to(-3.5, -1.2, TextAnchor.BASELINE_LEFT)
scene.wait(0.5)
scene.play([number.count_to(3, duration=1).easing(Easing.LINEAR)])
scene.wait(0.5)
if directory := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(directory, [0, 1, 1.75, 0])
else:
    scene.render()
