"""Dynamic force vectors driven by physical magnitudes and components."""

import os
from math import pi, sin

from gaanim import Anchor, BLACK, CYAN, GOLD, GREEN, Scene


scene = Scene(1280, 720, background=BLACK, margin=52, theme="technical")
title = scene.text("Fuerzas dinámicas reactivas", role="title").at(0, 285, anchor=Anchor.CENTER)

body = scene.geometry.rounded_rect(150, 82, 14).fill(BLACK).stroke(CYAN, 4).at(-120, -20)
force_origin = body.anchor_point(Anchor.TOP_RIGHT)
resultant_origin = body.anchor_point(Anchor.BOTTOM_RIGHT)

magnitude = scene.viz.parameter(45.0)
direction = scene.viz.parameter(pi / 7)
force = scene.mechanics.force_at(
    force_origin,
    magnitude,
    direction=direction,
    visual_scale=2.3,
    label="$F(t)$",
    show_value=True,
    format=".1f",
    unit="N",
    label_gap=28,
    color=GREEN,
)

fx = scene.viz.parameter(-35.0)
fy = scene.viz.parameter(25.0)
resultant = scene.mechanics.force_from_components(
    resultant_origin,
    fx,
    fy,
    visual_scale=2.8,
    label="$R$",
    show_value=True,
    unit="N",
    label_gap=45,
    color=GOLD,
)

caption = scene.text(
    "La escala visual no altera la magnitud física",
    role="caption",
).at(0, -265, anchor=Anchor.CENTER)

scene.play([
    title.write(),
    body.fade_in(),
    force.fade_in(),
    resultant.fade_in(),
    caption.write(),
])
scene.play([
    magnitude.animate_to(100.0, duration=2.0),
    direction.animate_to(pi * 0.72, duration=2.0),
    fx.animate_to(55.0, duration=2.0),
    fy.animate_to(-45.0, duration=2.0),
    body.move(220, 35).duration(2.0),
])

# No invisible drawable is needed for a continuously evaluated scalar.
magnitude.add_updater_fn(
    lambda _current, _dt, elapsed: 68.0 + 24.0 * sin(elapsed * 2.4),
)
scene.wait(2.5)
magnitude.remove_updater()

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.6, 4.2, 5.4])
else:
    scene.render()
