"""Equation annotations: emphasis, braces, leader-line notes and hand-drawn marks on single terms.

A selection of an equation, such as the part ``eq["rhs"]``, points at its
glyphs without breaking the equation apart: transient emphasis, braces and
notes that stay, and hand-drawn marks that follow the term. Each label is
the call that marks the term beside it.
"""

import os

from gaanim import Scene, Zones, part

WRITE = 0.8
MARK = 1.2
HOLD = 0.8
SEGMENT = WRITE + MARK + HOLD
SEGMENTS = 6
# Equations are set larger than the theme's math size (0.44) so that each
# mark reads at a glance.
SIZE = 1.0

scene = Scene()
accent = scene.canvas.color("accent")


def energy():
    """E = m c^2, with the parts mass and light inside the part rhs."""
    return scene.text.equation(
        part("energy", "E"), "=", part("rhs", part("mass", "m"), part("light", "c^2")), size=SIZE
    )


def table(rows, tracks=("1fr", "1fr"), anchor="left"):
    """Write E = m c^2 in each row, labels on the left, then play every row's mark.

    ``rows`` are (label, mark) pairs, where ``mark(eq)`` returns the
    animation that marks the row's equation; ``anchor`` places the equation
    in its cell.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    equations = []
    for index, (label, _) in enumerate(rows):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        equations.append(energy().place(grid[2 * index + 1], anchor=anchor))
    scene.play([eq.animate.write().duration(WRITE) for eq in equations])
    scene.play([mark(eq).duration(MARK) for (_, mark), eq in zip(rows, equations)])
    scene.wait(HOLD)


# Transient emphasis: the term moves and returns to rest...
scene.segment("emphasis")
table([
    ('eq["rhs"].animate.indicate()', lambda eq: eq["rhs"].animate.indicate()),
    ('eq["rhs"].animate.pulse()', lambda eq: eq["rhs"].animate.pulse()),
    ('eq["rhs"].animate.wiggle()', lambda eq: eq["rhs"].animate.wiggle()),
    ('eq["rhs"].animate.wave()', lambda eq: eq["rhs"].animate.wave()),
])

# ...or light passes around or under it. focus keeps the rest of the
# equation dimmed, and the marker band stays.
scene.segment("highlights")
table([
    ('eq["rhs"].animate.flash_around()', lambda eq: eq["rhs"].animate.flash_around()),
    ('eq["rhs"].animate.flash_under()', lambda eq: eq["rhs"].animate.flash_under()),
    ('eq["rhs"].animate.focus()', lambda eq: eq["rhs"].animate.focus()),
    ('eq["rhs"].animate.marker(accent)', lambda eq: eq["rhs"].animate.marker(accent)),
])

# A brace over the term (above=True) or under it, and its label, stay on
# screen, colored like the term.
scene.segment("braces")
table(
    [
        ('eq["rhs"].animate.brace("rest energy", above=True)', lambda eq: eq["rhs"].animate.brace("rest energy", above=True)),
        ('eq["rhs"].animate.brace()', lambda eq: eq["rhs"].animate.brace()),
        ('eq["rhs"].animate.brace("rest energy")', lambda eq: eq["rhs"].animate.brace("rest energy")),
        ('eq["rhs"]["mass"].animate.brace("mass")', lambda eq: eq["rhs"]["mass"].animate.brace("mass")),
    ],
    tracks=("5fr", "2fr"),
)

# A note sits at offset from the term's center, joined to it by a leader
# line; each label abbreviates eq["energy"].animate.annotate(...).
scene.segment("notes")
table(
    [
        ('annotate("energy")', lambda eq: eq["energy"].animate.annotate("energy")),
        ('annotate("energy", offset=(0, -0.6))', lambda eq: eq["energy"].animate.annotate("energy", offset=(0, -0.6))),
        ('annotate("energy", offset=(-2, 0.6))', lambda eq: eq["energy"].animate.annotate("energy", offset=(-2, 0.6))),
        ('annotate("energy", offset=(-2, -0.6))', lambda eq: eq["energy"].animate.annotate("energy", offset=(-2, -0.6))),
    ],
    tracks=("3fr", "2fr"),
    anchor="right",
)


def drawn(mark):
    """The animation that draws ``mark(eq["rhs"].annotate)`` on, for table()."""
    return lambda eq: mark(eq["rhs"].annotate).animate.create()


# Hand-drawn marks are drawables that follow the term; mark.animate.create()
# draws them on, one pass after another. Without a color they take the
# theme's stroke. Each label abbreviates eq["rhs"].annotate.<mark>(...).
scene.segment("hand_drawn")
table([
    ("underline()", drawn(lambda notes: notes.underline())),
    ("box()", drawn(lambda notes: notes.box())),
    ("circle(padding=(0.3, 0.15))", drawn(lambda notes: notes.circle(padding=(0.3, 0.15)))),
    ("strike_through()", drawn(lambda notes: notes.strike_through())),
])

# Strikes and brackets, and the options every mark takes.
scene.segment("hand_drawn_options")
table([
    ("crossed_off()", drawn(lambda notes: notes.crossed_off())),
    ('bracket("right")', drawn(lambda notes: notes.bracket("right"))),
    ("box(roughness=0)", drawn(lambda notes: notes.box(roughness=0))),
    ("underline(color=accent, passes=1)", drawn(lambda notes: notes.underline(color=accent, passes=1))),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-mark and settled, in each segment.
    scene.snapshots(snapshots, [SEGMENT * index + t for index in range(SEGMENTS) for t in (WRITE + MARK / 2, SEGMENT - 0.1)])
else:
    scene.render()
