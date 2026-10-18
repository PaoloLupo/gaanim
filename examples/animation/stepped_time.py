"""Stepped time: drawables and segments that move like stop-motion, and outlines and jitter that boil.

``drawable.step_rate(rate)`` evaluates a drawable's animations, updaters and
path modifiers ``rate`` times a second and holds them in between, while the
rest of the scene stays smooth. ``hold=True`` makes ``wiggle_path`` and
``Updater.wiggle`` jump between unrelated shapes instead of flowing, the boil
of hand-drawn lines. ``scene.segment(..., step_rate=rate)`` steps everything
in a segment, the camera included. Every row is labelled with its code.
"""

import os
from math import pi

from gaanim import Easing, Scene, Updater, Zones

RUN = 3.0

scene = Scene()
rule = scene.canvas.color("rule")


def rows(count, columns):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(Zones.grid(rows=count, columns=columns, column_gap="48px", padding="page_padding"))


# step_rate holds the drawable between steps on a grid from second 0, so
# drawables with the same rate change together, and every move still ends
# on its final value.
rates_start = scene.cursor
scene.segment("step_rate")
RATES = [None, 24, 12, 6]
grid = rows(len(RATES), ["2fr", "3fr"])
moves = []
for row, rate in enumerate(RATES):
    scene.text(f"step_rate({rate})", role="code").place(grid[2 * row], anchor="right")
    stage = grid[2 * row + 1]
    (left, y), (right, _) = stage.point("left"), stage.point("right")
    square = scene.geometry.square(0.5).move_to(left + 0.4, y).step_rate(rate)
    moves.append(square.animate.move_to(right - 0.4, y).rotate_by(pi).duration(RUN).easing(Easing.SMOOTH))
scene.play(moves)
scene.wait(0.5)

# A flowing wiggle drifts; with hold=True the outline keeps each shape and
# jumps to another frequency times a second. frequency=0 keeps one rough
# shape still, like torn paper. The same applies to an Updater.wiggle, and
# a stepped drawable samples a flowing one at its own rate.
boil_start = scene.cursor
scene.segment("boil")
# The first three rows are strip.modifiers.wiggle_path(...), the others
# strip.add_updater(...).
BOILS = [
    ("wiggle_path(0.08, frequency=8)", {"frequency": 8}, None),
    ("wiggle_path(0.08, frequency=8, hold=True)", {"frequency": 8, "hold": True}, None),
    ("wiggle_path(0.08, frequency=0)", {"frequency": 0}, None),
    ("Updater.wiggle(frequency=8)", None, {"frequency": 8}),
    ("Updater.wiggle(frequency=8, hold=True)", None, {"frequency": 8, "hold": True}),
    ("step_rate(8) + Updater.wiggle()", None, {}),
]
grid = rows(len(BOILS), ["2fr", "1fr"])
for row, (label, path_options, jitter_options) in enumerate(BOILS):
    scene.text(label, role="code").place(grid[2 * row], anchor="right")
    center = grid[2 * row + 1].center
    scene.geometry.rect(2.4, 0.5).no_fill().stroke(rule, 0.02).move_to(*center)
    strip = scene.geometry.rect(2.4, 0.5).move_to(*center)
    if path_options is not None:
        strip.modifiers.wiggle_path(0.08, detail=8, seed=row, **path_options)
    else:
        if label.startswith("step_rate"):
            strip.step_rate(8)
        strip.add_updater(Updater.wiggle(seed=row, **jitter_options))
scene.wait(RUN)

# A stepped segment shows the world at the last of rate steps a second,
# counted from its start and again from each stop: the camera, every
# animation and the updaters jump together, while audio stays smooth. The
# label is a HUD, so the zoom leaves it in place.
segment_start = scene.cursor
scene.segment("stepped segment", step_rate=8)
zones = scene.layout.zones(Zones.rows([0.5, "1fr"], gap="24px", padding="page_padding"))
head, stage = zones[0], zones[1]
scene.text("scene.segment(name, step_rate=8)", role="code").hud().place(head, anchor="left")
shapes = [
    scene.geometry.circle(0.5),
    scene.geometry.square(0.9),
    scene.geometry.star(5, 0.6, 0.25),
    scene.geometry.regular_polygon(3, 0.6),
]
columns = [stage.left + (index + 0.5) * stage.width / len(shapes) for index in range(len(shapes))]
for shape, x in zip(shapes, columns):
    shape.move_to(x, stage.center[1] - 0.8).add_updater(Updater.rotate(1.5))
scene.play(
    [shape.animate.move_to(x, stage.center[1] + 0.8).duration(RUN / 2) for shape, x in zip(shapes, columns)]
    + [scene.camera.animate.zoom_to(1.3).duration(RUN / 2).easing(Easing.SMOOTH)]
)
scene.play([scene.camera.animate.zoom_to(1.0).duration(RUN / 2).easing(Easing.SMOOTH)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The rows mid-move, off every rate's grid so each shows another step;
    # the boil at two instants an eighth of a second apart; and the stepped
    # segment mid-zoom.
    scene.snapshots(
        snapshots,
        [rates_start + t for t in (1.13, 1.63)]
        + [boil_start + t for t in (1.0, 1.125)]
        + [segment_start + t for t in (0.8, 2.3)],
    )
else:
    scene.render()
