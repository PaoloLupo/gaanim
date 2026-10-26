"""Pivots: the point a drawable turns, scales and leans about, and drawables that aim.

Without a pivot a shape turns about its own center. ``with_pivot(x, y)`` (or
its alias ``pivot``) moves that point anywhere in the scene, for hinges,
corners, orbits and the parts of an SVG;
``animate.rotate_by(...).pivot(x, y)`` turns about a point once; ``look_at``
keeps a drawable aimed at a moving target. Dots mark the pivots, and each
label is the code that moves its cell.
"""

import os
from math import pi

from gaanim import Scene, Zones

TURN = 1.4
HOLD = 0.6

scene = Scene()
foreground = scene.colors.foreground
success = scene.colors.success
danger = scene.colors.danger
rule = scene.colors.rule
geometry = scene.geometry


def cells(labels, label_height="96px", outer=False):
    """A 2 x 2 grid of cells, each label above its stage; return the stages.

    With ``outer`` the labels of the bottom row sit below their stages, so
    the middle of the frame stays free.
    """
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))
    stages = []
    for index, (cell, label) in enumerate(zip(grid, labels)):
        below = outer and index >= 2
        first, second = cell.split(Zones.rows(["1fr", label_height] if below else [label_height, "1fr"]))
        top, stage = (second, first) if below else (first, second)
        scene.text(label, role="code").place(top, anchor="bottom_left" if below else "top_left")
        stages.append(stage)
    return stages


def mark(x, y):
    """A dot on the pivot ``(x, y)``."""
    geometry.dot(0.08).fill(foreground).move_to(x, y)


def bar(x, y, length=1.6):
    """A bar centered on ``(x, y)``."""
    return geometry.rounded_rect(length, 0.3, 0.08).move_to(x, y)


# rotate_by turns about the drawable's pivot: its own center unless
# with_pivot sets another, which then holds for later animations too.
# .pivot(x, y) on the animation turns about a point for that rotation only.
# (x0, y0) is the left end of the bar, (px, py) the dot.
scene.segment("rotation")
stages = cells([
    "bar.animate.rotate_by(pi / 2)",
    "bar.with_pivot(x0, y0)\nbar.animate.rotate_by(pi / 2)",
    "bar.animate.rotate_by(pi / 2)\n    .pivot(px, py)",
    "bar.with_pivot(x0, y0)\nbar.animate.rotate_by(-2 * pi)",
])
turns = []
x, y = stages[0].center
mark(x, y)
turns.append(bar(x, y).animate.rotate_by(pi / 2))
x, y = stages[1].center
x0, y0 = x - 0.8, y - 0.7
mark(x0, y0)
turns.append(bar(x, y0).with_pivot(x0, y0).animate.rotate_by(pi / 2))
x, y = stages[2].center
px, py = x - 1.0, y - 0.7
mark(px, py)
turns.append(bar(px + 1.5, py).animate.rotate_by(pi / 2).pivot(px, py))
x, y = stages[3].center
x0, y0 = x - 0.8, y
mark(x0, y0)
turns.append(bar(x, y0).with_pivot(x0, y0).animate.rotate_by(-2 * pi))
scene.play(turns, duration=TURN)
scene.wait(HOLD)


# Scales and skews use the pivot too: about the center a box grows or leans
# both ways; pinned at a corner it grows away from it, and pinned at its
# base it leans like a falling card. (x0, y0) is the box's bottom-left corner.
def box(x, y, size=1.2):
    """A square whose bottom-left corner is ``(x, y)``."""
    return geometry.square(size).move_to(x + size / 2, y + size / 2)


scene.segment("scale_skew")
stages = cells([
    "box.animate.scale_to(1.7)",
    "box.with_pivot(x0, y0)\nbox.animate.scale_to(1.7)",
    "box.animate.skew_to(0.6, 0)",
    "box.with_pivot(x0, y0)\nbox.animate.skew_to(0.6, 0)",
])
changes = []
for index, stage in enumerate(stages):
    x, y = stage.center
    pinned = index % 2 == 1
    x0, y0 = (x - 1.0, stage.bottom + 0.2) if pinned else (x - 0.6, y - 0.6)
    square = box(x0, y0)
    if pinned:
        square.with_pivot(x0, y0)
        mark(x0, y0)
    else:
        mark(x, y)
    changes.append(square.animate.scale_to(1.7) if index < 2 else square.animate.skew_to(0.6, 0))
scene.play(changes, duration=TURN)
scene.wait(HOLD)


# A group turns as one about its pivot, like an arm on a hinge, and the
# pivot alias makes a moon orbit its planet. Once a drawable is on screen,
# with_pivot moves the pivot at the cursor without moving the drawable: the
# bar flips over its right end, then over its new right end. A text pivots
# like any shape. The other cells then turn back, or on for the moon.
# (hx, hy) is the hinge, (x1, y) and (x2, y) the bar's right ends and
# (x1, y0) the word's bottom-right corner.
scene.segment("hinges")
stages = cells(
    [
        "arm = geometry.group([rod, bob])\narm.with_pivot(hx, hy)\narm.animate.rotate_by(-pi / 2)",
        "moon.pivot(*planet)\nmoon.animate.rotate_by(pi)  # twice",
        "bar.with_pivot(x1, y)\nbar.animate.rotate_by(-pi)\nbar.with_pivot(x2, y)  # again",
        "word.with_pivot(x1, y0)\nword.animate.rotate_by(pi / 6)",
    ],
    label_height="128px",
)
x, y = stages[0].center
hx, hy = x - 1.2, y + 0.9
mark(hx, hy)
rod = geometry.line(hx, hy, hx + 1.5, hy).stroke(rule, 0.12)
bob = geometry.circle(0.3).move_to(hx + 1.5, hy)
arm = geometry.group([rod, bob]).with_pivot(hx, hy)

planet = stages[1].center
geometry.circle(0.36).fill(success).move_to(*planet)
moon = geometry.circle(0.16).move_to(planet[0] + 1.2, planet[1]).pivot(*planet)

x, y = stages[2].center
LENGTH = 1.1
x1, x2, y = x - LENGTH / 2, x + LENGTH / 2, y - 0.5
step = geometry.rounded_rect(LENGTH, 0.3, 0.08).move_to(x1 - LENGTH / 2, y)
step.with_pivot(x1, y)
mark(x1, y)
mark(x2, y)

x, y = stages[3].center
word = scene.text("Pivot", size=0.9).move_to(x, y + 0.3)
x1, y0 = word.bounds().right, word.bounds().bottom
word.with_pivot(x1, y0)
mark(x1, y0)

scene.play(
    [
        arm.animate.rotate_by(-pi / 2),
        moon.animate.rotate_by(pi),
        step.animate.rotate_by(-pi),
        word.animate.rotate_by(pi / 6),
    ],
    duration=TURN,
)
step.with_pivot(x2, y)
scene.play(
    [
        arm.animate.rotate_by(pi / 2),
        moon.animate.rotate_by(pi),
        step.animate.rotate_by(-pi),
        word.animate.rotate_by(-pi / 6),
    ],
    duration=TURN,
)
scene.wait(HOLD)


# look_at turns a drawable about its pivot so its x axis points at a target,
# every frame from the cursor on; offset adds an angle for shapes drawn
# pointing elsewhere, and a group turns each member. clear_drive ends it at
# the cursor and gives the drawable back its own rotation.
scene.segment("look_at")
stages = cells(
    [
        "arrow.with_pivot(*tail)\narrow.look_at(dot)",
        "arrows.look_at(dot)",
        "cone.look_at(dot, offset=-pi / 2)",
        "arrow.look_at(dot)\n...\narrow.clear_drive()",
    ],
    label_height="128px",
    outer=True,
)
BOLD = {"head_length": 0.3, "head_width": 0.28, "body_width": 0.08}
track = geometry.circle(0.9).no_fill().stroke(rule, 0.03)
dot = geometry.dot(0.16).fill(danger).move_to(0.9, 0)

x, y = stages[0].center
tail = (x - 0.8, y)
mark(*tail)
geometry.arrow(x - 0.8, y, x + 0.8, y, **BOLD).with_pivot(*tail).look_at(dot)

x, y = stages[1].center
small = [geometry.arrow(x + dx - 0.4, y + dy, x + dx + 0.4, y + dy, **BOLD) for dx in (-1.3, 0, 1.3) for dy in (-0.5, 0.5)]
geometry.group(small).look_at(dot)

x, y = stages[2].center
geometry.polygon([(x - 0.35, y - 0.6), (x + 0.35, y - 0.6), (x, y + 0.7)]).look_at(dot, offset=-pi / 2)

x, y = stages[3].center
stopping = geometry.arrow(x - 0.8, y, x + 0.8, y, **BOLD)
stopping.look_at(dot)

scene.play([dot.animate.move_along(track, start=0.0, end=0.5)], duration=2 * TURN)
stopping.clear_drive()
scene.play([dot.animate.move_along(track, start=0.5, end=1.0)], duration=2 * TURN)
scene.wait(HOLD)

# The part of an SVG turns about a scene point too, however the SVG is
# scaled and placed: the character raises both arms about its shoulders,
# the inner ends of the arms' boxes, and lowers them again.
scene.segment("parts")
top, stage = scene.layout.zones(Zones.rows(["200px", "1fr"], gap="24px", padding="page_padding"))
scene.text(
    'character = scene.media.svg("assets/character.svg").scale_to(1.8)\n'
    'left = character.part("left-arm").with_pivot(*left_shoulder)\n'
    'right = character.part("right-arm").with_pivot(*right_shoulder)\n'
    "scene.play([left.animate.rotate_by(-pi / 3),\n"
    "            right.animate.rotate_by(pi / 3)])",
    role="code",
).place(top, anchor="top_left")
character = scene.media.svg("assets/character.svg").scale_to(1.8).place(stage)
left, right = character.part("left-arm"), character.part("right-arm")
left_box, right_box = left.bounds(), right.bounds()
left_shoulder, right_shoulder = (left_box.right, left_box.y), (right_box.left, right_box.y)
left.with_pivot(*left_shoulder)
right.with_pivot(*right_shoulder)
mark(*left_shoulder)
mark(*right_shoulder)
scene.play([left.animate.rotate_by(-pi / 3), right.animate.rotate_by(pi / 3)], duration=TURN)
scene.play([left.animate.rotate_by(pi / 3), right.animate.rotate_by(-pi / 3)], duration=TURN)
scene.wait(HOLD)

SEGMENT = TURN + HOLD
HINGES = 2 * TURN + HOLD
LOOK_AT = 4 * TURN + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Rotations, scales and hinges midway and settled; the aimed shapes at
    # four places of the dot; the arms midway up, raised and lowered.
    scene.snapshots(
        snapshots,
        [0.6, SEGMENT - 0.1, SEGMENT + 0.6, 2 * SEGMENT - 0.1]
        + [2 * SEGMENT + t for t in (0.7, TURN + 0.1, TURN + 0.7, HINGES - 0.1)]
        + [2 * SEGMENT + HINGES + t for t in (0.1, 1.4, 2.8, 4.2, 5.5)]
        + [2 * SEGMENT + HINGES + LOOK_AT + t for t in (TURN / 2, TURN, 2 * TURN + HOLD - 0.1)],
    )
else:
    scene.render()
