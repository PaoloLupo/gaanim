"""Scatter: seeded, non-overlapping placement of free objects in a region.

``scene.layout.scatter`` (``L.scatter`` below) moves objects to random but repeatable places
inside a region (the safe area, the frame, a zone or an object's box),
at least ``gap`` apart and clear of every shape in ``avoid``. Larger objects
go first, each where it has the most room. Each label is the call that
scattered the objects under it.
"""

import os

from gaanim import Scene, Zones, stagger

scene = Scene()
L = scene.layout
header = scene.canvas.color("header")
rule = scene.canvas.color("rule")
danger = scene.canvas.color("danger")
seeks = []


def code(label):
    return scene.text(label, role="code")


def outline(zone, color=rule):
    """A frame drawn around ``zone``, to make it visible."""
    return scene.geometry.rect(zone.width, zone.height).no_fill().stroke(color, 0.02).place(zone)


# Formulas scattered around a character: avoiding its parts, rather than
# its whole box, lets them come close to its outline.
scene.segment("avoid")
page = L.zones(Zones.rows(["1fr", "96px"], names=["stage", "caption"], gap="16px", padding="page_padding"))
character = scene.media.svg("assets/character.svg").scale_to(2.2).place(page["stage"])
parts = [character.part(name) for name in ("head", "torso", "legs", "left-arm", "right-arm")]
FORMULAS = [
    "E = m c^2", "a^2 + b^2 = c^2", "e^(i pi) + 1 = 0", "F = m a", "pi r^2", "sqrt(2)",
    "integral_0^1 x dif x", "sum_(n=1)^oo 1/n^2", "d/(d x) sin x", "lim_(x -> 0) (sin x)/x",
    "nabla dot E", "p V = n R T", "Delta x", "x!", "log_2 8 = 3", "(a + b)^2", "v = d / t",
    "phi", "f'(x)", "binom(n, k)", "cos^2 + sin^2 = 1", "lambda",
]
formulas = [scene.text.equation(formula) for formula in FORMULAS]
L.scatter(formulas, page["stage"], avoid=parts, gap=0.2, seed=1)
caption = code('L.scatter(formulas, page["stage"], avoid=parts, gap=0.2, seed=1)').place(page["caption"])
start = scene.cursor
scene.play([character.animate.fade_in().duration(0.5), caption.animate.fade_in().duration(0.5)])
scene.play(stagger(*[formula.animate.write().duration(0.6) for formula in formulas], each=0.05))
scene.wait(0.6)
seeks.extend([start + 0.6, start + 1.2, scene.cursor - 0.1])

# The same objects and region give the same layout for the same seed; gap
# sets the clearance, avoid takes zones too, and the region can be any
# object's box.
scene.segment("options")
cells = L.zones(Zones.grid(rows=3, columns=2, gap="32px", padding="page_padding"))
SIZES = [0.26, 0.24, 0.22, 0.2, 0.18, 0.16, 0.15, 0.14, 0.12, 0.12, 0.1, 0.1, 0.08, 0.08]


def dots():
    return [scene.geometry.circle(size) for size in SIZES]


def grains():
    """Many small dots, to fill a region up to the gap."""
    return [scene.geometry.circle(0.07) for _ in range(90)]


shown, scattered = [], []
for cell, label, scatter in [
    (cells[0], "scatter(dots, zone, seed=1)", lambda items, zone: L.scatter(items, zone, seed=1)),
    (cells[1], "scatter(dots, zone, seed=2)", lambda items, zone: L.scatter(items, zone, seed=2)),
    (cells[2], "scatter(dots, zone, gap=0.4)", lambda items, zone: L.scatter(items, zone, gap=0.4)),
    (cells[3], 'scatter(grains, zone, gap="6px")', lambda items, zone: L.scatter(items, zone, gap="6px")),
]:
    title, zone = cell.split(Zones.rows(["56px", "1fr"], gap="8px"))
    items = grains() if "grains" in label else dots()
    scatter(items, zone)
    shown += [code(label).place(title, anchor="left"), outline(zone)]
    scattered += items
# A zone to keep clear, and an object whose box is the region.
title, zone = cells[4].split(Zones.rows(["56px", "1fr"], gap="8px"))
hole = zone.inset(("32px", "300px"))
items = dots()
L.scatter(items, zone, avoid=[hole])
shown += [code("scatter(dots, zone, avoid=[hole])").place(title, anchor="left"), outline(zone), outline(hole, danger)]
scattered += items
title, zone = cells[5].split(Zones.rows(["56px", "1fr"], gap="8px"))
card = scene.geometry.rounded_rect(zone.width * 0.7, zone.height, 0.2).fill(header).stroke(rule, 0.02).place(zone)
items = dots()
L.scatter(items, card)
shown += [code("scatter(dots, card)").place(title, anchor="left"), card]
scattered += items
start = scene.cursor
scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in shown], each=0.05))
scene.play(stagger(*[item.animate.grow_from_center().duration(0.4) for item in scattered], each=0.01))
scene.wait(0.6)
seeks.extend([start + 0.6, start + 1.2, scene.cursor - 0.1])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
