"""Two ellipses and their union, intersection, exclusion and difference, which reshape live as one ellipse turns and slides.

The pair on the left is drawn; each result on the right is computed with
``live=True`` from its own copy of the pair, scaled down and left unpainted, so
when the right ellipse of every pair turns and slides, all four results are
cut again every frame.
"""

import math
import os

from gaanim import Easing, Scene, Zones, stagger

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
lilac = scene.canvas.color("chart")
paper = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
geometry = scene.geometry

left, right = scene.layout.zones(Zones.columns(["5fr", "6fr"], gap="48px", padding="page_padding"))
cells = right.split(Zones.grid(rows=2, columns=2, column_gap="48px", row_gap="24px"))


def pair(center, size, hidden=False):
    """Ellipses a and b side by side around ``center``, ``size`` times the drawn pair.

    A hidden pair has neither fill nor stroke: it only shapes its results.
    """
    x, y = center
    a = geometry.ellipse(1.9 * size, 1.25 * size).move_to(x - 0.85 * size, y)
    b = geometry.ellipse(1.9 * size, 1.25 * size).move_to(x + 0.85 * size, y)
    if hidden:
        return a.no_fill().no_stroke(), b.no_fill().no_stroke()
    a.fill(violet).opacity(0.55).stroke(violet, 0.04)
    b.fill(gold).opacity(0.55).stroke(gold, 0.04)
    return a, b


a, b = pair(left.center, 1.0)
OPERATIONS = [
    ("union(a, b)", geometry.union, violet),
    ("intersection(a, b)", geometry.intersection, gold),
    ("xor(a, b)", geometry.xor, lilac),
    ("difference(a, b)", geometry.difference, paper),
]
SMALL = 0.5
results, labels, movers = [], [], []
for cell, (label, operation, color) in zip(cells, OPERATIONS):
    body, caption = cell.split(Zones.rows(["5fr", "1fr"]))
    small_a, small_b = pair(body.center, SMALL, hidden=True)
    results.append(operation(small_a, small_b, live=True).fill(color).no_stroke().hidden())
    labels.append(scene.text(label, role="code", color=muted).place(caption, anchor="center").hidden())
    movers.append(small_b)

scene.play([a.animate.create(), b.animate.create()], duration=1.0)
scene.play(stagger(
    *(result.animate.grow_from_center().easing(Easing.back()).duration(0.6) for result in results),
    *(label.animate.fade_in().duration(0.4) for label in labels),
    each=0.15,
))
scene.wait(0.6)
# The right ellipse of every pair turns and slides by its pair's size; the
# results follow.
for turn, slide in ((math.pi / 2, -0.6), (math.pi / 2, 1.1), (0.0, -0.5)):
    scene.play(
        [b.animate.rotate_by(turn), b.animate.shift_by(slide, 0)]
        + [m.animate.rotate_by(turn) for m in movers]
        + [m.animate.shift_by(slide * SMALL, 0) for m in movers],
        duration=1.4,
        easing=Easing.SMOOTH_SPRING,
    )
    scene.wait(0.3)
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The pair, the results, and after each move.
    scene.snapshots(snapshots, [0.8, 2.4, 3.6, 4.6, 6.3, 7.9])
else:
    scene.render()
