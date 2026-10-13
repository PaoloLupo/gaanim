"""The logo built from primitives: a square that turns into a circle as it moves, and a wordmark cut from ellipses and bars.

The symbol is three frames of one interpolation drawn as onion skin: a pixel
square advances two units a frame while it becomes a pixel circle, and each
frame it leaves behind fades toward the background. Every point of the three
outlines comes from one function, so the square morphs point by point. Each
letter of the wordmark is a union of rings, half rings and bars, and the i
gets its gold dot last.
"""

import os

from gaanim import Easing, Scene, stagger

scene = Scene(theme="gaanim")
gold = scene.canvas.color("title")
violet = scene.canvas.color("accent")
mist = scene.canvas.color("rule")
paper = scene.canvas.color("foreground")
geometry = scene.geometry

# The logo is drawn on a grid of units, 71 wide and 16 tall, y pointing down;
# S scene units per unit, centered on the origin.
S = 0.17


def at(x, y):
    """Scene coordinates of the grid point (x, y)."""
    return (x - 35.5) * S, (8 - y) * S


# A frame's outline is 20 points: five per corner, each corner cut by the
# same profile of (inset along the edge, inset across it) steps.
SQUARE = [(0, 0)] * 5
MIDDLE = [(1, 0), (1, 1), (1, 1), (0, 1), (0, 1)]
CIRCLE = [(2, 0), (2, 1), (1, 1), (1, 2), (0, 2)]


def frame(left, profile):
    """The outline of an 8 x 8 frame whose left edge is at ``left``, its corners cut by ``profile``."""
    corners = [
        [(8 - ix, iy) for ix, iy in profile],
        [(8 - iy, 8 - ix) for ix, iy in profile],
        [(ix, 8 - iy) for ix, iy in profile],
        [(iy, ix) for ix, iy in profile],
    ]
    return [at(left + x, 4 + y) for corner in corners for x, y in corner]


def operand(shape):
    """A piece of a letter: filled like the letters and never drawn on its own."""
    return shape.fill(paper).no_stroke().hidden()


def bar(x0, y0, x1, y1):
    cx, cy = at((x0 + x1) / 2, (y0 + y1) / 2)
    return operand(geometry.rect((x1 - x0) * S, (y1 - y0) * S).move_to(cx, cy))


def oval(cx, cy, rx, ry):
    return operand(geometry.ellipse(rx * S, ry * S).move_to(*at(cx, cy)))


def ring(cx, cy, rx, ry):
    """An oval ring two units thick."""
    return operand(geometry.difference(oval(cx, cy, rx, ry), oval(cx, cy, rx - 2, ry - 2)))


def arch(cx, cy, rx, ry):
    """The upper half of a ring."""
    return operand(geometry.intersection(ring(cx, cy, rx, ry), bar(cx - rx, cy - ry - 1, cx + rx, cy)))


def letter(*pieces):
    glyph = geometry.union(*pieces) if len(pieces) > 1 else pieces[0]
    return glyph.fill(paper).no_stroke().hidden()


# The g's tail: the lower half of a ring around (20.5, 12.5), cut at x = 17.5.
tail = operand(geometry.intersection(
    operand(geometry.difference(oval(20.5, 12.5, 3.5, 3.5), oval(20.5, 12.5, 1.5, 1.5))),
    bar(17.5, 12.5, 24, 16.5),
))
word = [
    letter(ring(20, 8, 4, 4.12), bar(22, 4, 24, 12.5), tail),
    letter(ring(29.5, 8, 4, 4.12), bar(31.5, 4, 33.5, 12)),
    letter(ring(39, 8, 4, 4.12), bar(41, 4, 43, 12)),
    letter(bar(45, 4, 47, 12), arch(49, 8, 4, 4.12), bar(51, 8, 53, 12)),
    letter(bar(55, 4, 57, 12)),
    letter(bar(59, 4, 61, 12), arch(62.5, 7.5, 3.5, 3.62), bar(64, 7.5, 66, 12),
           arch(67.5, 7.5, 3.5, 3.62), bar(69, 7.5, 71, 12)),
]
dot_x, dot_y = at(56, 1)
dot = geometry.rect(2 * S, 2 * S).fill(gold).no_stroke().move_to(dot_x, dot_y + 4).hidden()

# The symbol: the current frame is gold, the one before violet and the oldest
# mist. Each frame starts as a copy of the previous one and morphs ahead.
first = geometry.polygon(frame(0, SQUARE)).fill(gold).no_stroke().hidden()
second = geometry.polygon(frame(0, SQUARE)).fill(gold).no_stroke().hidden()
third = geometry.polygon(frame(2, MIDDLE)).fill(gold).no_stroke().hidden()

STEP = 0.7
scene.wait(0.3)
scene.play(first.animate.grow_from_center().duration(0.5).easing(Easing.back()))
scene.wait(0.3)
scene.play([
    second.animate.points(frame(2, MIDDLE)).duration(STEP),
    first.animate.fill(violet).duration(STEP),
], easing=Easing.SMOOTH)
scene.play([
    third.animate.points(frame(4, CIRCLE)).duration(STEP),
    second.animate.fill(violet).duration(STEP),
    first.animate.fill(mist).duration(STEP),
], easing=Easing.SMOOTH)
scene.play(stagger(*(glyph.animate.write().duration(0.9) for glyph in word), each=0.18))
scene.play(dot.animate.move_to(dot_x, dot_y).easing(Easing.bounce()).duration(0.8))
scene.wait(1.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The square, each step of the morph, the wordmark writing and the logo.
    scene.snapshots(snapshots, [0.9, 1.5, 2.2, 3.2, 4.1, 6.1])
else:
    scene.render()
