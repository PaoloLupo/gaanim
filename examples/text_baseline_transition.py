"""Stable typographic baselines across text and equation transitions."""

import os

from gaanim import Anchor, BLACK, BLUE, GOLD, GRAY, TEAL, WHITE, Scene, TextAnchor, part


scene = Scene(1280, 720, background=BLACK)

title = (
    scene.text("Text and Equation share a stable baseline", role="title")
    .fill(WHITE)
    .move_to(0, 275, anchor=Anchor.CENTER)
)

baseline_y = 65
guide = scene.geometry.line(-555, baseline_y, 555, baseline_y).stroke(GRAY, 2)
guide_label = (
    scene.text("typographic baseline", size=22)
    .fill(GRAY)
    .move_to(-555, baseline_y + 105, anchor=TextAnchor.BASELINE_LEFT)
)

fade_from = scene.text("HAPPY", size=72).fill(BLUE).move_to(-390, baseline_y)
fade_to = scene.text("gyp", size=54).fill(GOLD).move_to(-390, baseline_y)

morph_from = scene.text("Baseline", size=64).fill(WHITE).move_to(0, baseline_y)
morph_to = scene.text("descenders", size=52).fill(TEAL).move_to(0, baseline_y)

equation_from = scene.text.equation(
    part("left", "frac(x_1^2, y_2)"), "=", part("right", "4"), size=64
).fill(WHITE).move_to(390, baseline_y)
equation_to = scene.text.equation(
    part("left", "x_1"), "=", part("right", "2"), size=52
).fill(GOLD).move_to(390, baseline_y)

multiline_y = -165
multiline_guide = scene.geometry.line(-555, multiline_y, -85, multiline_y).stroke(GRAY, 2)
multiline = (
    scene.text("first baseline\nsecond visual line", size=38)
    .fill(WHITE)
    .move_to(-555, multiline_y, anchor=TextAnchor.BASELINE_LEFT)
)
multiline_label = (
    scene.text("explicit TextAnchor uses the first line", size=22)
    .fill(GRAY)
    .move_to(-555, multiline_y - 100, anchor=TextAnchor.BASELINE_LEFT)
)

scene.play(
    [
        fade_from.animate.fade_in(0.5),
        morph_from.animate.fade_in(0.5),
        equation_from.animate.fade_in(0.5),
    ]
)
scene.wait(0.25)
scene.play(
    [
        fade_from.animate.fade_out(0.8),
        fade_to.animate.fade_in(0.8),
        morph_from.animate.transform_to(morph_to).duration(0.8),
        equation_from.animate.transform_to(equation_to).duration(0.8),
    ]
)
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.15])

scene.render()
