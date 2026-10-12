"""Arrows: straight, double and curved arrows, growing heads, tips and live connectors.

The first segments show the arrow factories and their head and body sizes,
then grow_arrow against create(). The tips segment puts heads and dots on any
stroked path, and the last one links boxes with connectors and lines that
follow them as they move. Each label is the ``scene.geometry`` call that
builds the variant.
"""

import os

from gaanim import Anchor, Scene, Zones

DRAW = 1.2
HOLD = 0.6

scene = Scene()

# A route for the tipped paths, in scene coordinates, and bolder arrow sizes.
ROUTE = [(0, 0), (1, 0.8), (2, 0), (3, 0.8)]
BOLD = {"head_length": 0.4, "head_width": 0.32, "body_width": 0.08}


def rows(tracks, columns=("3fr", "1fr")):
    """A padded grid of ``tracks`` rows (a count or a list) over ``columns``, the label column first."""
    return scene.layout.zones(
        Zones.grid(rows=tracks, columns=list(columns), gap="24px", padding="page_padding")
    )


def table(variants, animate=lambda drawable: drawable.animate.create()):
    """Place each (label, drawable) pair in a row and play ``animate`` on every drawable."""
    grid = rows(len(variants))
    for index, (label, drawable) in enumerate(variants):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        drawable.place(grid[2 * index + 1])
    scene.play([animate(drawable) for _, drawable in variants], duration=DRAW)
    scene.wait(HOLD)


def boxes(zone, drop=0.0):
    """Boxes a and b at the ends of ``zone``, b ``drop`` lower, and the corner above b.

    a has an "out" port on its right; b has "in" on its left and "up" on top.
    """
    inner = zone.inset((0, 0.4))
    (left, y), (right, _) = inner.point("left"), inner.point("right")
    a = scene.geometry.square(0.6).move_to(left + 0.3, y + drop / 2).with_port("out", Anchor.RIGHT)
    b = scene.geometry.square(0.6).move_to(right - 0.3, y - drop / 2)
    b.with_port("in", Anchor.LEFT).with_port("up", Anchor.TOP)
    return a, b, (right - 0.3, y + drop / 2)


# Omitted sizes are a 0.18 head length, a 0.15 head width and a 0.036 body.
# max_head_ratio caps the head to a share of a short arrow's length.
scene.segment("straight")
table([
    ("arrow(0, 0, 3, 0)", scene.geometry.arrow(0, 0, 3, 0)),
    ("arrow(0, 0, 3, 0, body_width=0.1)", scene.geometry.arrow(0, 0, 3, 0, body_width=0.1)),
    ("arrow(0, 0, 3, 0, head_length=0.5)", scene.geometry.arrow(0, 0, 3, 0, head_length=0.5)),
    ("arrow(0, 0, 3, 0, head_width=0.4)", scene.geometry.arrow(0, 0, 3, 0, head_width=0.4)),
    ("arrow(0, 0, 0.8, 0, **BOLD)", scene.geometry.arrow(0, 0, 0.8, 0, **BOLD)),
    (
        "arrow(0, 0, 0.8, 0, **BOLD, max_head_ratio=0.3)",
        scene.geometry.arrow(0, 0, 0.8, 0, **BOLD, max_head_ratio=0.3),
    ),
    ("double_arrow(0, 0, 3, 0)", scene.geometry.double_arrow(0, 0, 3, 0)),
    ("double_arrow(0, 0, 3, 0, body_width=0.012)", scene.geometry.double_arrow(0, 0, 3, 0, body_width=0.012)),
])

# A curved arrow bends by an angle whose sign picks the side; an arc arrow
# follows a circle from a start angle through a sweep, clockwise when negative.
scene.segment("curved")
table([
    ("curved_arrow(0, 0, 3, 0, 0.8)", scene.geometry.curved_arrow(0, 0, 3, 0, 0.8)),
    ("curved_arrow(0, 0, 3, 0, -0.8)", scene.geometry.curved_arrow(0, 0, 3, 0, -0.8)),
    (
        "curved_arrow(0, 0, 3, 0, 1.2, body_width=0.08)",
        scene.geometry.curved_arrow(0, 0, 3, 0, 1.2, body_width=0.08),
    ),
    ("curved_arrow_arc(0, 0, 0.6, 0, 4.5)", scene.geometry.curved_arrow_arc(0, 0, 0.6, 0, 4.5)),
    ("curved_arrow_arc(0, 0, 0.6, 0, -4.5)", scene.geometry.curved_arrow_arc(0, 0, 0.6, 0, -4.5)),
])

# grow_arrow keeps the tail still and slides the head along the spine at its
# full size; create() traces the outline and then fades the fill in.
scene.segment("grow")
ARROWS = [
    ("arrow(0, 0, 2.4, 0, **BOLD)", lambda: scene.geometry.arrow(0, 0, 2.4, 0, **BOLD)),
    ("curved_arrow(0, 0, 2.4, 0, 1.2, **BOLD)", lambda: scene.geometry.curved_arrow(0, 0, 2.4, 0, 1.2, **BOLD)),
    ("curved_arrow_arc(0, 0, 0.8, 0, 4.5, **BOLD)", lambda: scene.geometry.curved_arrow_arc(0, 0, 0.8, 0, 4.5, **BOLD)),
]
GROWTHS = [
    ("grow_arrow()", lambda arrow: arrow.animate.grow_arrow()),
    ("create()", lambda arrow: arrow.animate.create()),
]
# A short header row names the animation of each column.
grid = rows([0.6] + ["1fr"] * len(ARROWS), ("13fr", "4fr", "4fr"))
for column, (label, _) in enumerate(GROWTHS, start=1):
    scene.text(label, role="code").place(grid[column])
growths = []
for row, (label, build) in enumerate(ARROWS, start=1):
    scene.text(label, role="code").place(grid[3 * row], anchor="right")
    for column, (_, grow) in enumerate(GROWTHS, start=1):
        growths.append(grow(build().place(grid[3 * row + column])))
scene.play(growths, duration=DRAW)
scene.wait(HOLD)

# tip() puts arrowheads or dots on the ends of any stroked path. The tips ride
# the path's current ends, so grow_arrow and trims carry them along.
scene.segment("tips")
tipped = [
    ("polyline(ROUTE).tip()", scene.geometry.polyline(ROUTE).tip()),
    ('polyline(ROUTE).tip(start="dot")', scene.geometry.polyline(ROUTE).tip(start="dot")),
    ('polyline(ROUTE).tip("dot", "dot")', scene.geometry.polyline(ROUTE).tip("dot", "dot")),
    ("polyline(ROUTE).tip(length=0.4, width=0.3)", scene.geometry.polyline(ROUTE).tip(length=0.4, width=0.3)),
    ('arc(0, 0, 0.5, 0, 4.5).tip(start="arrow")', scene.geometry.arc(0, 0, 0.5, 0, 4.5).tip(start="arrow")),
    ("bezier((0, 0), [(1, 2), (2, -1)], (3, 1)).tip()", scene.geometry.bezier((0, 0), [(1, 2), (2, -1)], (3, 1)).tip()),
]
table(tipped, animate=lambda path: path.animate.grow_arrow())
scene.play([path.animate.trim(start=0.4) for _, path in tipped], duration=DRAW)
scene.wait(HOLD)

# A connector is a filled arrow between endpoints that follow other objects,
# through optional via waypoints; line() and dashed_line() take the same
# endpoints. Ports name anchor points of a drawable.
scene.segment("connectors")
grid = rows(4, ("5fr", "3fr"))
for index, label in enumerate([
    'connector(a.port("out"), b.port("in"))',
    'connector(a.port("out"), b.port("up"),\n          via=[corner])',
    'line(a.port("out"), b.port("in"))',
    'dashed_line(a.port("out"), b.port("in"))',
]):
    scene.text(label, role="code").place(grid[2 * index], anchor="right")
a, b, corner = boxes(grid[1])
direct = scene.geometry.connector(a.port("out"), b.port("in"))
moves = [b.animate.shift_by(-1.2, 0.3)]
a, b, corner = boxes(grid[3], drop=0.5)
routed = scene.geometry.connector(a.port("out"), b.port("up"), via=[corner])
moves.append(b.animate.shift_by(0, -0.25))
a, b, corner = boxes(grid[5])
line = scene.geometry.line(a.port("out"), b.port("in"))
moves.append(b.animate.shift_by(-1.2, -0.1))
a, b, corner = boxes(grid[7])
dashed = scene.geometry.dashed_line(a.port("out"), b.port("in"))
moves.append(b.animate.shift_by(-1.2, -0.3))
scene.play(
    [direct.animate.grow_arrow(), routed.animate.grow_arrow(), line.animate.create(), dashed.animate.create()],
    duration=DRAW,
)
scene.play(moves, duration=DRAW)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawing and settled in the first four segments, then the tips after
    # their trim and the connectors growing, following and settled.
    scene.snapshots(
        snapshots,
        [(DRAW + HOLD) * index + t for index in range(4) for t in (0.5, 1.6)]
        + [(DRAW + HOLD) * 4 + 1.6]
        + [(DRAW + HOLD) * 5 + t for t in (0.6, DRAW + 0.6, 2 * DRAW + 0.4)],
    )
else:
    scene.render()
