"""Text on a path: closed and open paths, alignment, orientation, direction and offset.

``scene.text.on_path(content, path, ...)`` places one line of text along the
outline of another drawable: around a circle, a polygon or a rounded
rectangle, along an open wave or arc. ``align``, ``orient``, ``reverse``
and ``offset`` place the glyphs, and ``animate.path_offset`` slides them
along the path. Each label lists the arguments of ``on_path`` that build
the sample under it, or the animation that moves it.
"""

import math
import os

from gaanim import Easing, Scene, Zones

DURATION = 2.4
HOLD = 0.6
RING = "SPHINX OF BLACK QUARTZ · "
LINE = "Sphinx of black quartz"

scene = Scene()
accent = scene.canvas.color("accent")
rule = scene.canvas.color("rule")


def cells(rows):
    """A rows x 2 grid; each cell is split into a label strip and a stage under it."""
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=2, column_gap="96px", row_gap="48px", padding="page_padding")
    )
    return [cell.split(Zones.rows(["48px", "1fr"], gap="12px")) for cell in grid]


def outline(shape):
    """Draw a path as a thin rule, so the text along it stays readable."""
    return shape.no_fill().stroke(rule, 0.02)


def wave(stage):
    """An open S-curve across ``stage``, left to right."""
    (x0, y), (x1, _) = stage.point("left"), stage.point("right")
    third = (x1 - x0) / 3
    return outline(scene.geometry.path([
        ("move", [(x0, y)]),
        ("cubic", [(x0 + third, y + 1.4), (x1 - third, y - 1.4), (x1, y)]),
    ]))


def smile(stage):
    """An arc along the bottom of a circle, rising to the right at 45 degrees."""
    x, y = stage.center
    return outline(scene.geometry.arc(x, y + 0.9, 1.6, math.pi * 1.25, math.pi * 0.5))


def around(stage, shape, **options):
    """RING along ``shape``, a closed path centered in ``stage``."""
    return scene.text.on_path(RING, outline(shape.move_to(*stage.center)), **options)


def show(variants):
    """Label each (label, build) variant; ``build(stage)`` draws its path and text."""
    texts = []
    for (strip, stage), (code, build) in zip(cells(len(variants) // 2), variants):
        scene.text(code, role="code").place(strip, anchor="left")
        texts.append(build(stage))
    return texts


# Glyphs sit to the left of the direction of travel. A circle runs
# counterclockwise, so text reads around the inside; reverse=True follows
# the path backward and puts it around the outside.
scene.segment("closed")
RADIUS = 1.15
texts = show([
    ("(RING, circle)", lambda stage: around(stage, scene.geometry.circle(RADIUS))),
    ("(RING, circle, reverse=True)", lambda stage: around(stage, scene.geometry.circle(RADIUS), reverse=True)),
    ("(RING, hexagon, reverse=True)",
     lambda stage: around(stage, scene.geometry.regular_polygon(6, RADIUS), reverse=True)),
    ("(RING, box, reverse=True)",
     lambda stage: around(stage, scene.geometry.rounded_rect(4.4, 1.6, 0.5), reverse=True)),
])
scene.play([text.animate.write().duration(DURATION / 2) for text in texts])
scene.wait(HOLD)

# On an open path, align places the text at its start, middle or end;
# orient=False keeps glyphs upright and offset shifts the text by a fraction
# of the path's length.
scene.segment("open")
texts = show([
    ("(LINE, wave)", lambda stage: scene.text.on_path(LINE, wave(stage))),
    ('(LINE, wave, align="center")', lambda stage: scene.text.on_path(LINE, wave(stage), align="center")),
    ('(LINE, wave, align="end")', lambda stage: scene.text.on_path(LINE, wave(stage), align="end")),
    ("(LINE, wave, orient=False)", lambda stage: scene.text.on_path(LINE, wave(stage), orient=False)),
    ("(LINE, wave, offset=0.25)", lambda stage: scene.text.on_path(LINE, wave(stage), offset=0.25)),
    ("(LINE, wave, reverse=True)", lambda stage: scene.text.on_path(LINE, wave(stage), reverse=True)),
])
scene.play([text.animate.write().duration(DURATION / 2) for text in texts])
scene.wait(HOLD)

# animate.path_offset slides the glyphs along the path: one full turn of a
# closed path is 1.0, and on an open path (here the bottom of a circle)
# glyphs past the end go straight on. Text.path_offset(-0.3) sets the wave's
# starting offset, and on_path takes the style keywords of scene.text, like
# color=accent.
scene.segment("motion")
spin, slide, past, styled = show([
    ("animate.path_offset(1.0)", lambda stage: around(stage, scene.geometry.circle(RADIUS), reverse=True)),
    ("animate.path_offset(0.3)",
     lambda stage: scene.text.on_path(LINE, wave(stage), align="center").path_offset(-0.3)),
    ("animate.path_offset(0.6)",
     lambda stage: scene.text.on_path("black quartz", smile(stage))),
    ("animate.path_offset(-1.0)",
     lambda stage: around(stage, scene.geometry.circle(RADIUS), reverse=True, color=accent)),
])
scene.play([
    spin.animate.path_offset(1.0).duration(DURATION).easing(Easing.LINEAR),
    slide.animate.path_offset(0.3).duration(DURATION),
    past.animate.path_offset(0.6).duration(DURATION),
    styled.animate.path_offset(-1.0).duration(DURATION).easing(Easing.LINEAR),
])
scene.wait(HOLD)

STEP = DURATION / 2 + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [
        # Mid-write and settled in the two layout segments ...
        0.6, 1.7, STEP + 0.6, STEP + 1.7,
        # ... and four moments of the motion.
        *[2 * STEP + t for t in (0.1, 0.9, 1.7, 2.9)],
    ])
else:
    scene.render()
