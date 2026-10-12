"""Stable typographic baselines across text and equation transitions."""

import os

from gaanim import Anchor, BLACK, BLUE, GOLD, GRAY, TEAL, WHITE, Scene, TextAnchor, part


scene = Scene(frame=(16, 9), background=BLACK)

title = (
    scene.text("Text and Equation share a stable baseline", role="title")
    .fill(WHITE)
    .move_to(0, 3.4375, anchor=Anchor.CENTER)
)

baseline_y = 0.8125
guide = scene.geometry.line(-6.9375, baseline_y, 6.9375, baseline_y).stroke(GRAY, 0.025)
guide_label = (
    scene.text("typographic baseline", size=0.275)
    .fill(GRAY)
    .move_to(-6.9375, baseline_y + 1.3125, anchor=TextAnchor.BASELINE_LEFT)
)

fade_from = scene.text("HAPPY", size=0.9).fill(BLUE).move_to(-4.875, baseline_y)
fade_to = scene.text("gyp", size=0.675).fill(GOLD).move_to(-4.875, baseline_y)

morph_from = scene.text("Baseline", size=0.8).fill(WHITE).move_to(0, baseline_y)
morph_to = scene.text("descenders", size=0.65).fill(TEAL).move_to(0, baseline_y)

equation_from = scene.text.equation(
    part("left", "frac(x_1^2, y_2)"), "=", part("right", "4"), size=0.8
).fill(WHITE).move_to(4.875, baseline_y)
equation_to = scene.text.equation(
    part("left", "x_1"), "=", part("right", "2"), size=0.65
).fill(GOLD).move_to(4.875, baseline_y)

multiline_y = -2.0625
multiline_guide = scene.geometry.line(-6.9375, multiline_y, -1.0625, multiline_y).stroke(GRAY, 0.025)
multiline = (
    scene.text("first baseline\nsecond visual line", size=0.475)
    .fill(WHITE)
    .move_to(-6.9375, multiline_y, anchor=TextAnchor.BASELINE_LEFT)
)
multiline_label = (
    scene.text("explicit TextAnchor uses the first line", size=0.275)
    .fill(GRAY)
    .move_to(-6.9375, multiline_y - 1.25, anchor=TextAnchor.BASELINE_LEFT)
)

scene.play(
    [
        fade_from.animate.fade_in().duration(0.5),
        morph_from.animate.fade_in().duration(0.5),
        equation_from.animate.fade_in().duration(0.5),
    ]
)
scene.wait(0.25)
scene.play(
    [
        fade_from.animate.fade_out().duration(0.8),
        fade_to.animate.fade_in().duration(0.8),
        morph_from.animate.transform_to(morph_to).duration(0.8),
        equation_from.animate.transform_to(equation_to).duration(0.8),
    ]
)
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.15])

scene.render()
