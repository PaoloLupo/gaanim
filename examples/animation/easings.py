"""Easing: named presets, curve families, overshoot, springs and shaping factories.

Every dot crosses the same track in the same time, so only the easing differs.
Each segment covers one part of the Easing API, and each label is the code
that builds the easing.
"""

import os

from gaanim import Easing, EasingCurve, Scene, Zones

DURATION = 1.6
HOLD = 0.4

scene = Scene()
rule = scene.colors.rule


def inner(zone):
    """Leave a tenth of ``zone`` free at each end, room for easings that overshoot."""
    return zone.inset((0, zone.width * 0.1))


def track(zone, easing):
    """Draw a track across ``zone`` and return the move of its dot."""
    (start, y), (end, _) = zone.point("left"), zone.point("right")
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    dot = scene.geometry.dot(0.12).move_to(start, y)
    return dot.animate.move_to(end, y).duration(DURATION).easing(easing)


def table(rows, columns=None, build=None):
    """Race one dot per cell, with each row's label to the left of its tracks.

    ``rows`` are (label, value) pairs. Without ``columns``, each row's value is
    its easing. Otherwise ``columns`` are (label, value) pairs too, labelled
    in a header row, and ``build(row_value, column_value)`` makes each cell's
    easing.
    """
    header = [] if columns is None else columns
    columns = columns or [(None, None)]
    tracks = ["4fr"] + ["3fr"] * len(columns)
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + bool(header), columns=tracks, column_gap="48px", padding="page_padding")
    )
    for index, (label, _) in enumerate(header, start=1):
        scene.text(label, role="code").place(inner(grid[index]), anchor="left")
    moves = []
    for row_index, (label, row) in enumerate(rows, start=bool(header)):
        first = row_index * len(tracks)
        scene.text(label, role="code").place(grid[first], anchor="right")
        for index, (_, column) in enumerate(columns, start=1):
            easing = row if build is None else build(row, column)
            moves.append(track(inner(grid[first + index]), easing))
    scene.play(moves)
    scene.wait(HOLD)


scene.segment("presets")
table([
    ("Easing.LINEAR", Easing.LINEAR),
    ("Easing.SMOOTH", Easing.SMOOTH),
    ("Easing.DOUBLE_SMOOTH", Easing.DOUBLE_SMOOTH),
    ("Easing.LINGERING", Easing.LINGERING),
    ("Easing.RUNNING_START", Easing.RUNNING_START),
    ("Easing.EXPONENTIAL_DECAY", Easing.EXPONENTIAL_DECAY),
    ("Easing.NOT_QUITE_THERE", Easing.NOT_QUITE_THERE),
    ("Easing.THERE_AND_BACK", Easing.THERE_AND_BACK),
])

# Every EasingCurve family eases in, out, or both ways: Easing.ease_in(EasingCurve.CUBIC).
scene.segment("curves")
CURVES = ["QUADRATIC", "CUBIC", "QUARTIC", "QUINTIC", "EXPONENTIAL", "SINE", "CIRCULAR", "BACK", "ELASTIC", "BOUNCE"]
table(
    [(name, getattr(EasingCurve, name)) for name in CURVES],
    [("ease_in", Easing.ease_in), ("ease_out", Easing.ease_out), ("ease_in_out", Easing.ease_in_out)],
    lambda curve, ease: ease(curve),
)

# back, elastic and bounce take a strength and the same three modes:
# Easing.back(3.0, mode="in_out").
scene.segment("overshoot")
table(
    [
        ("back()", lambda mode: Easing.back(mode=mode)),
        ("back(3.0)", lambda mode: Easing.back(3.0, mode=mode)),
        ("elastic()", lambda mode: Easing.elastic(mode=mode)),
        ("elastic(1.5, 0.5)", lambda mode: Easing.elastic(1.5, 0.5, mode=mode)),
        ("bounce()", lambda mode: Easing.bounce(mode=mode)),
        ("bounce(0.4)", lambda mode: Easing.bounce(0.4, mode=mode)),
    ],
    [('mode="in"', "in"), ('mode="out"', "out"), ('mode="in_out"', "in_out")],
    lambda factory, mode: factory(mode),
)

# Perceptual presets settle within the animation; physical springs take
# stiffness, damping, mass and an initial velocity.
scene.segment("springs")
table([
    ("Easing.SMOOTH_SPRING", Easing.SMOOTH_SPRING),
    ("Easing.GENTLE", Easing.GENTLE),
    ("Easing.QUICK", Easing.QUICK),
    ("Easing.SNAPPY", Easing.SNAPPY),
    ("Easing.BOUNCY", Easing.BOUNCY),
    ("Easing.spring(bounce=0.35)", Easing.spring(bounce=0.35)),
    ("Easing.spring(160, 8, mass=2)", Easing.spring(160, 8, mass=2)),
    ("Easing.spring(90, 12, velocity=3)", Easing.spring(90, 12, velocity=3)),
])

# Factories that reshape time: holds, steps, jitter, and curves of your own.
scene.segment("shaping")
OVERSHOOT_PATH = "M0,0 C0.3,0 0.2,1.25 1,1"


def cube(t):
    """A function of t in [0, 1] for Easing.custom, which samples it into a table."""
    return t**3


table([
    ("Easing.slow_mo(0.6)", Easing.slow_mo(0.6)),
    ("Easing.rough(0.8, seed=4)", Easing.rough(0.8, seed=4)),
    ("Easing.steps(4)", Easing.steps(4)),
    ('Easing.steps(6, jump="both")', Easing.steps(6, jump="both")),
    ("Easing.squish(SMOOTH, 0.3, 0.7)", Easing.squish(Easing.SMOOTH, 0.3, 0.7)),
    ("Easing.mirror(SMOOTH)", Easing.mirror(Easing.SMOOTH)),
    ("Easing.there_and_back(pause=0.3)", Easing.there_and_back(pause=0.3)),
    ("Easing.cubic_bezier(0.7, 0, 0.3, 1)", Easing.cubic_bezier(0.7, 0, 0.3, 1)),
    ("Easing.from_svg(OVERSHOOT_PATH)", Easing.from_svg(OVERSHOOT_PATH)),
    ("Easing.custom(cube, samples=6)", Easing.custom(cube, samples=6)),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Early, mid-race and settled, in each of the five segments.
    scene.snapshots(snapshots, [(DURATION + HOLD) * index + t for index in range(5) for t in (0.5, 1.0, 1.9)])
else:
    scene.render()
