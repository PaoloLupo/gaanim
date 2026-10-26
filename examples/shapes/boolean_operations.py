"""Boolean operations: union, intersection, difference and xor of vector shapes.

Dashed outlines are the operands a, b and c, and the filled shape is the
result. The first segment shows each operation, the second results that follow
their operands with live=True, and the third the fill rule, the flattening
tolerance and several clips. Each label is the ``scene.geometry`` call that
builds the result.
"""

import math
import os

from gaanim import Scene, StrokeStyle, Zones

DRAW = 1.2
MOVE = 1.6
HOLD = 0.6

scene = Scene()
accent = scene.colors.accent
muted = scene.colors.muted
OUTLINE = StrokeStyle(muted, 0.02, dashes=[0.08, 0.06])

# A five-pointed star drawn as one self-crossing polygon.
PENTAGRAM = [(math.cos(math.pi / 2 + k * 0.8 * math.pi), math.sin(math.pi / 2 + k * 0.8 * math.pi)) for k in range(5)]


def gallery(labels, columns):
    """Lay out one grid cell per label, the label at the bottom, and return the zones above them."""
    rows = math.ceil(len(labels) / columns)
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    zones = []
    for zone, label in zip(grid, labels):
        art, caption = zone.split(Zones.rows(["1fr", 0.5]))
        scene.text(label, role="code").place(caption)
        zones.append(art)
    return zones


def reveal(results):
    """Fill the results with the accent and trace them in together."""
    scene.play([result.fill(accent).no_stroke().animate.create() for result in results], duration=DRAW)


def outline(shape):
    """Show an operand as a dashed outline, above the results."""
    return shape.no_fill().stroke_style(OUTLINE).z_index(1)


def pair(zone):
    """A circle a and a square b overlapping at the center of ``zone``."""
    x, y = zone.center
    a = outline(scene.geometry.circle(0.9).move_to(x - 0.45, y))
    b = outline(scene.geometry.square(1.5).move_to(x + 0.45, y))
    return a, b


def trio(zone):
    """Three circles a, b and c overlapping like a Venn diagram."""
    x, y = zone.center
    return [
        outline(scene.geometry.circle(0.75).move_to(x + 0.45 * math.cos(angle), y + 0.45 * math.sin(angle)))
        for angle in (math.pi / 2, math.pi * 7 / 6, math.pi * 11 / 6)
    ]


def star_and_dot(zone):
    """A pentagram a with a small circle b on one of its tips."""
    x, y = zone.center
    a = outline(scene.geometry.polygon(PENTAGRAM).scale_by(1.3).move_to(x, y))
    b = outline(scene.geometry.circle(0.35).move_to(x + 1.15, y + 0.45))
    return a, b


def plate(zone):
    """A rectangle a with two circles b and c over it."""
    x, y = zone.center
    a = outline(scene.geometry.rect(3, 1.8).move_to(x, y))
    b = outline(scene.geometry.circle(0.5).move_to(x - 0.7, y))
    c = outline(scene.geometry.circle(0.5).move_to(x + 0.7, y + 0.5))
    return a, b, c


# Results keep their operands, which stay drawable on their own. difference
# subtracts every later operand from the first, so the order matters; xor
# keeps what an odd number of operands cover.
scene.segment("operations")
zones = gallery(
    ["union(a, b)", "intersection(a, b)", "difference(a, b)", "difference(b, a)", "xor(a, b)", "xor(a, b, c)"],
    columns=3,
)
a, b = pair(zones[3])
reveal([
    scene.geometry.union(*pair(zones[0])),
    scene.geometry.intersection(*pair(zones[1])),
    scene.geometry.difference(*pair(zones[2])),
    scene.geometry.difference(b, a),
    scene.geometry.xor(*pair(zones[4])),
    scene.geometry.xor(*trio(zones[5])),
])
scene.wait(HOLD)

# A result is computed once from where its operands are declared; with
# live=True it is rebuilt whenever an operand moves or changes shape.
scene.segment("live")
zones = gallery(
    ["union(a, b)", "union(a, b, live=True)", "difference(b, a, live=True)", "intersection(a, b, live=True)"],
    columns=2,
)
pairs = [pair(zone) for zone in zones]
reveal([
    scene.geometry.union(*pairs[0]),
    scene.geometry.union(*pairs[1], live=True),
    scene.geometry.difference(*reversed(pairs[2]), live=True),
    scene.geometry.intersection(*pairs[3], live=True),
])
scene.play(
    [
        pairs[0][0].animate.shift_by(0, 0.8),
        pairs[1][0].animate.shift_by(0, 0.8),
        pairs[2][0].animate.shift_by(1.2, 0),
        pairs[3][1].animate.rotate_by(math.pi / 4),
    ],
    duration=MOVE,
)
scene.wait(HOLD)

# Operands are filled by the nonzero rule unless rule="evenodd", which leaves
# the middle of a self-crossing star empty. tolerance is how far flattened
# curves may stray, in scene units: raise it only for heavy live results.
scene.segment("options")
zones = gallery(
    ["union(a, b)", 'union(a, b, rule="evenodd")', "union(a, b, tolerance=0.12)", "difference(a, b, c)"],
    columns=2,
)
reveal([
    scene.geometry.union(*star_and_dot(zones[0])),
    scene.geometry.union(*star_and_dot(zones[1]), rule="evenodd"),
    scene.geometry.union(*pair(zones[2]), tolerance=0.12),
    scene.geometry.difference(*plate(zones[3])),
])
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Tracing and settled in each segment, and the live results before, while
    # and after their operands move.
    scene.snapshots(snapshots, [0.6, 1.7, 3.0, 3.8, 5.1, 5.8, 6.9])
else:
    scene.render()
