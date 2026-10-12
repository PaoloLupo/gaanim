"""Equation parts: named terms, selections, per-term style and writing term by term.

``part`` and ``parts`` name pieces of an equation; indexing the equation
selects a part, a nested part, or every occurrence of a fragment, and the
selection takes a color or an animation without breaking the equation apart.
Each label is the code that differs from row to row.
"""

import os

from gaanim import Scene, Zones, part, parts

DURATION = 1.4
HOLD = 0.8
PARTS_END = DURATION + HOLD
FRAGMENTS_END = PARTS_END + DURATION + HOLD
STYLING_END = FRAGMENTS_END + 2 * DURATION + HOLD
# Equations are set larger than the theme's math size (0.44) so that they
# read at a glance.
SIZE = 0.8

scene = Scene()
accent = scene.canvas.color("accent")

# The pendulum's equation of motion. parts() names the mass and gravity
# terms; "inertia" nests parts given as a mapping, whose names need not be
# Python identifiers.
PENDULUM = (
    "-",
    parts(mass="m", gravity="g sin(theta)"),
    "=",
    part("inertia", parts({"mass": "m", "length": "L", "angle''": "theta''"})),
)
# a^2 + b^2 = c^2, one part per square, to write term by term.
PYTHAGORAS = (part("a", "a^2"), "+", part("b", "b^2"), "=", part("c", "c^2"))


def pendulum():
    return scene.text.equation(*PENDULUM, size=SIZE)


def energy(mass=None):
    """E = m c^2, with ``mass`` (or a plain part) as the term named "mass"."""
    return scene.text.equation("E =", mass or part("mass", "m"), "c^2", size=SIZE)


def table(rows, header=None, tracks=("1fr", "1fr")):
    """Lay out (label, equation) rows, labels on the left, and return the equations.

    ``header`` labels the equation column.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + bool(header), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    if header:
        scene.text(header, role="code").place(grid[1], anchor="left")
    equations = []
    for index, (label, equation) in enumerate(rows, start=bool(header)):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        equations.append(equation.place(grid[2 * index + 1], anchor="left"))
    return equations


def selections(rows):
    """Write one pendulum per (label, select) row, its selection filled with the accent."""
    equations = table([(label, pendulum()) for label, _ in rows], header=".fill(accent)")
    for (_, select), eq in zip(rows, equations):
        select(eq).fill(accent)
    scene.play([eq.animate.write().duration(DURATION) for eq in equations])
    scene.wait(HOLD)


# A string index selects a top-level part and indexing again goes down a
# level...
scene.segment("parts")
selections([
    ('eq["gravity"]', lambda eq: eq["gravity"]),
    ('eq["mass"]', lambda eq: eq["mass"]),
    ('eq["inertia"]', lambda eq: eq["inertia"]),
    ('eq["inertia"]["mass"]', lambda eq: eq["inertia"]["mass"]),
    ('eq["inertia"]["angle\'\'"]', lambda eq: eq["inertia"]["angle''"]),
])

# ...and a string that names no part selects that fragment wherever it
# appears, with Typst symbol names and primes resolved to the glyphs they
# render.
scene.segment("fragments")
selections([
    ('eq["m"]', lambda eq: eq["m"]),
    ('eq["sin(theta)"]', lambda eq: eq["sin(theta)"]),
    ('eq["theta\'\'"]', lambda eq: eq["theta''"]),
])

# A selection restyles its glyphs at once or animates them; a part can also
# carry its own style when the equation is built.
scene.segment("styling")
still, filled, faded, both = energy(), energy(), energy(), energy()
equations = table(
    [
        ('eq["mass"].fill(accent)', still),
        ('eq["mass"].animate.fill(accent)', filled),
        ('eq["mass"].animate.opacity(0.3)', faded),
        ('eq["mass"].animate.fill(accent).opacity(0.6)', both),
        ('part("mass", "m", color=accent)', energy(part("mass", "m", color=accent))),
        ('part("mass", "m", size=1.2)', energy(part("mass", "m", size=1.2))),
    ],
    tracks=("5fr", "2fr"),
)
still["mass"].fill(accent)
scene.play([eq.animate.write().duration(DURATION) for eq in equations])
scene.play([
    filled["mass"].animate.fill(accent).duration(DURATION),
    faded["mass"].animate.opacity(0.3).duration(DURATION),
    both["mass"].animate.fill(accent).opacity(0.6).duration(DURATION),
])
scene.wait(HOLD)

# write() traces the glyphs left to right; by="part" writes the terms of
# a^2 + b^2 = c^2 part by part, in the order given. Each label abbreviates
# eq.animate.write(...).
scene.segment("writing")
WRITES = [
    ("write()", {}),
    ("write(stagger=0.5)", {"stagger": 0.5}),
    ('write(by="part")', {"by": "part"}),
    ('write(by="part", order="reverse")', {"by": "part", "order": "reverse"}),
    ('write(by="part", order="center")', {"by": "part", "order": "center"}),
    ('write(by="part", order="random")', {"by": "part", "order": "random"}),
    ('write(brush="chalk")', {"brush": "chalk"}),
]
equations = table([(label, scene.text.equation(*PYTHAGORAS, size=SIZE)) for label, _ in WRITES], tracks=("8fr", "7fr"))
scene.play([eq.animate.write(**options).duration(DURATION) for (_, options), eq in zip(WRITES, equations)])
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-write and settled for parts and fragments; mid-way and settled
    # while styling; early, mid and settled while writing.
    scene.snapshots(snapshots, [
        0.7, PARTS_END - 0.1, PARTS_END + 0.7, FRAGMENTS_END - 0.1,
        FRAGMENTS_END + DURATION + 0.7, STYLING_END - 0.1,
        STYLING_END + 0.3, STYLING_END + 0.7, STYLING_END + DURATION + HOLD - 0.1,
    ])
else:
    scene.render()
