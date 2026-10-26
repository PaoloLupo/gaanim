"""Supports: every support kind, the directions they face, their sizes and their styleable parts.

``scene.mechanics.support_at`` draws the symbol of a structural support at a
connection point: a fixed plate, a pin, rollers, a guide, a cable or a spring.
The direction runs from the base toward the connection, the shortcuts fix the
kind, and each part (joint, body, rollers, guides, ground and hatching) can be
styled on its own. Each label is the code that builds the variant.
"""

import os

from gaanim import Direction, Scene, Zones

DRAW = 1.2
HOLD = 0.6

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success


def cells(rows, columns):
    """A padded grid of ``rows`` x ``columns`` cells, each split into a label strip and a stage."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows([0.5, "1fr"], gap="12px")) for zone in grid]


def reveal(supports):
    """Draw the supports, then hold them."""
    scene.play([support.animate.create() for support in supports], duration=DRAW)
    scene.wait(HOLD)


def gallery(rows, columns, variants):
    """Label each cell with its code and place the support it builds in the cell's stage."""
    supports = []
    for (label_zone, stage), (label, support) in zip(cells(rows, columns), variants):
        scene.text(label, role="code").place(label_zone)
        supports.append(support.place(stage))
    reveal(supports)


def support(kind, direction=Direction.UP, size=1.6):
    """A support built at the origin, with a base wide enough for its size, to be placed in its cell."""
    return scene.mechanics.support_at((0, 0), kind=kind, direction=direction, size=size, ground_length=size * 1.7)


# support_at(point, kind=...) draws one of eight symbols, here at size=1.6
# with ground_length=2.7. Direction.UP puts the ground under the point.
scene.segment("kinds")
KINDS = ["fixed", "pin", "roller", "simple", "guided", "prismatic", "cable", "spring"]
gallery(2, 4, [(f'kind="{kind}"', support(kind)) for kind in KINDS])

# direction runs from the base toward the connection: around the frame, every
# roller (size=1.25) faces the centre. Direction.custom takes any vector.
scene.segment("directions")
COMPASS = ["DOWN_RIGHT", "DOWN", "DOWN_LEFT", "RIGHT", None, "LEFT", "UP_RIGHT", "UP", "UP_LEFT"]
gallery(3, 3, [
    (f"Direction.{name}", support("roller", getattr(Direction, name), size=1.25))
    if name
    else ("Direction.custom(1, 2)", support("roller", Direction.custom(1, 2), size=1.25))
    for name in COMPASS
])

# The shortcuts fix the kind. size scales the symbol, ground_length sets the
# base line, and color replaces the theme's foreground.
scene.segment("sizes")
SIZES = [
    ("pin_support(p, size=0.6)", scene.mechanics.pin_support((0, 0), size=0.6)),
    ("pin_support(p, size=1.2)", scene.mechanics.pin_support((0, 0), size=1.2)),
    ("fixed_support(p, size=1.2, ground_length=4)", scene.mechanics.fixed_support((0, 0), size=1.2, ground_length=4)),
    ("roller_support(p, size=1.2, color=accent)", scene.mechanics.roller_support((0, 0), size=1.2, color=accent)),
    ("guided_support(p, size=1.2, color=success)", scene.mechanics.guided_support((0, 0), size=1.2, color=success)),
]
grid = scene.layout.zones(Zones.grid(rows=len(SIZES), columns=[10, "1fr"], column_gap="48px", padding="page_padding"))
for index, (label, built) in enumerate(SIZES):
    scene.text(label, role="code").place(grid[2 * index], anchor="right")
    built.place(grid[2 * index + 1])
reveal([built for _, built in SIZES])

# Every part of a Support is a drawable of its own, styled like any other.
scene.segment("parts")
PARTS = [
    ("s.joint.stroke(accent, 0.12)", "roller", lambda s: s.joint.stroke(accent, 0.12)),
    ("s.body.fill(accent)", "roller", lambda s: s.body.fill(accent)),
    ("s.rollers.fill(accent)", "roller", lambda s: s.rollers.fill(accent)),
    ("s.ground.stroke(accent, 0.1)", "roller", lambda s: s.ground.stroke(accent, 0.1)),
    ("s.hatching.stroke(accent, 0.06)", "roller", lambda s: s.hatching.stroke(accent, 0.06)),
    ("s.guides.stroke(accent, 0.1)", "guided", lambda s: s.guides.stroke(accent, 0.1)),
]
styled = []
for label, kind, style in PARTS:
    part = support(kind, size=1.5)
    style(part)
    styled.append((label, part))
gallery(3, 2, styled)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawing and drawn, in each of the four segments.
    scene.snapshots(snapshots, [(DRAW + HOLD) * index + t for index in range(4) for t in (0.6, 1.6)])
else:
    scene.render()
