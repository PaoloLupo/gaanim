"""Axes: scales, tick steps, number formats, styles, titles and crossings.

``Axis`` describes one axis and is immutable: every method returns a copy.
Each row draws a number line from the axis its label builds; the last
segments put pairs of axes on planes for their titles and crossings.
"""

import math
import os

from gaanim import Axis, Scene, Zones

DRAW = 1.0
HOLD = 0.8
# Number lines and planes are drawn larger than their default size, so their
# numbers read as easily as the labels.
ZOOM = 1.6
PLANE_ZOOM = 1.3

scene = Scene()
accent = scene.colors.accent


def table(rows):
    """Draw one number line per (label, axis) row across the frame, its label above it."""
    grid = scene.layout.zones(Zones.rows(len(rows), gap="24px", padding="page_padding"))
    lines = []
    for zone, (label, axis) in zip(grid, rows):
        top, body = zone.split(Zones.rows(["2fr", "3fr"]))
        length = zone.width * 0.9
        # The label starts where the line does.
        start = top.inset((0, (zone.width - length) / 2))
        scene.text(label, role="code").place(start, anchor="bottom_left")
        line = scene.viz.number_line(axis, length=length / ZOOM)
        # A number line's origin is the middle of the axis, above its numbers.
        x, y = body.center
        line.drawable().scale_to(ZOOM).move_to(x, y + 0.2)
        lines.append(line)
    scene.play([line.animate.create() for line in lines], duration=DRAW)
    scene.wait(HOLD)


def planes(cells, size=(0.8, 0.75), **options):
    """Draw a plane per (label, x, y) cell of a 2 x 2 grid, its label above it.

    ``size`` is the share of the cell's width and height the plot area takes.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=2, columns=2, column_gap="96px", row_gap="48px", padding="page_padding")
    )
    spaces = []
    for zone, (label, x, y) in zip(grid, cells):
        top, body = zone.split(Zones.rows(["1fr", "4fr"]))
        scene.text(label, role="code").place(top, anchor="bottom")
        width, height = body.width * size[0] / PLANE_ZOOM, body.height * size[1] / PLANE_ZOOM
        plane = scene.viz.cartesian_2d(x, y, width=width, height=height, **options)
        # A plane's origin is the middle of its plot area; leave its numbers
        # and titles room below it.
        x, y = body.center
        plane.scale_to(PLANE_ZOOM).move_to(x, y + body.height * (1 - size[1]) / 6)
        spaces.append(plane)
    scene.play([plane.animate.create() for plane in spaces], duration=DRAW)
    scene.wait(HOLD)


# A scale maps data to position: logarithmic scales space orders of
# magnitude evenly and a power scale stretches values by an exponent.
scene.segment("scales")
table([
    ("Axis.linear(0, 100)", Axis.linear(0, 100)),
    ("Axis.log(1, 10000)", Axis.log(1, 10000)),
    ("Axis.log(1, 64, base=2)", Axis.log(1, 64, base=2)),
    ("Axis.power(0, 100, 0.5)", Axis.power(0, 100, 0.5)),
])

# symlog is logarithmic on both sides of zero and linear within
# ``threshold`` of it; category places named groups; time takes seconds.
scene.segment("more_scales")
table([
    ("Axis.symlog(-10, 10)", Axis.symlog(-10, 10)),
    ("Axis.symlog(-100, 100, threshold=10)", Axis.symlog(-100, 100, threshold=10)),
    ('Axis.category(["low", "mid", "high"])', Axis.category(["low", "mid", "high"])),
    ("Axis.time(0, 3600).ticks(900)", Axis.time(0, 3600).ticks(900)),
])

# Without ticks() the step is chosen for the domain; ticks(step) fixes it
# and auto_ticks() goes back to the chosen one.
scene.segment("ticks")
table([
    ("Axis.linear(0, 10)", Axis.linear(0, 10)),
    ("Axis.linear(0, 0.05)", Axis.linear(0, 0.05)),
    ("Axis.linear(0, 10).ticks(5)", Axis.linear(0, 10).ticks(5)),
    ("Axis.linear(0, 10).ticks(5).auto_ticks()", Axis.linear(0, 10).ticks(5).auto_ticks()),
])

# minor_ticks(n) splits each step into n parts with shorter, unnumbered ticks.
scene.segment("minor_ticks")
table([
    ("Axis.linear(0, 10).ticks(2).minor_ticks(2)", Axis.linear(0, 10).ticks(2).minor_ticks(2)),
    ("Axis.linear(0, 10).ticks(2).minor_ticks(4)", Axis.linear(0, 10).ticks(2).minor_ticks(4)),
    ("Axis.linear(0, 10).ticks(5).minor_ticks(5)", Axis.linear(0, 10).ticks(5).minor_ticks(5)),
])

# numbers() formats the tick text; negative numbers use a typographic minus.
scene.segment("numbers")
axis = Axis.linear(-1, 1).ticks(0.5)
table([
    ("axis = Axis.linear(-1, 1).ticks(0.5)", axis),
    ('axis.numbers("fixed", precision=2)', axis.numbers("fixed", precision=2)),
    ('axis.numbers("percent", precision=0)', axis.numbers("percent", precision=0)),
    ('axis.numbers("scientific", precision=1)', axis.numbers("scientific", precision=1)),
])

# fraction and pi round each value to the nearest multiple of
# 1/denominator and reduce it.
scene.segment("fractions")
angle = Axis.linear(0, math.tau).ticks(math.tau / 4)
table([
    ('axis.numbers("fraction", denominator=2)', axis.numbers("fraction", denominator=2)),
    (
        'Axis.linear(0, 2).ticks(0.25).numbers("fraction", denominator=4)',
        Axis.linear(0, 2).ticks(0.25).numbers("fraction", denominator=4),
    ),
    ("angle = Axis.linear(0, tau).ticks(tau / 4)", angle),
    ('angle.numbers("pi", denominator=2)', angle.numbers("pi", denominator=2)),
])

# style() replaces only the properties it names and keeps the theme's for
# the rest.
scene.segment("style")
axis = Axis.linear(0, 4).ticks(1)
table([
    ("axis = Axis.linear(0, 4).ticks(1)", axis),
    ("axis.style(color=accent)", axis.style(color=accent)),
    ("axis.style(width=0.05)", axis.style(width=0.05)),
    ("axis.style(number_color=accent)", axis.style(number_color=accent)),
])

scene.segment("tick_style")
table([
    ("axis.style(tick_length=0.3)", axis.style(tick_length=0.3)),
    ("axis.style(tick_width=0.05)", axis.style(tick_width=0.05)),
    ('axis.label("t").style(label_color=accent)', axis.label("t").style(label_color=accent)),
])

# label() titles an axis past its positive end, past its negative end
# ("start") or along its middle ("center", the y title turned).
scene.segment("titles")
X = Axis.linear(0, 4).ticks(1)
Y = Axis.linear(0, 3).ticks(1)
planes(
    [
        ('X.label("x"), Y.label("y")', X.label("x"), Y.label("y")),
        ('X.label("x", position="start")', X.label("x", position="start"), Y),
        (
            'X.label("time", position="center")\nY.label("speed", position="center")',
            X.label("time", position="center"),
            Y.label("speed", position="center"),
        ),
        (
            "X.style(tick_color=accent)\nY.style(tick_color=accent)",
            X.style(tick_color=accent),
            Y.style(tick_color=accent),
        ),
    ],
    size=(0.6, 0.45),
)

# crossing() is the value of this axis where the other axis crosses it:
# "auto" (zero when in range), "zero", "min", "max" or a data value. The
# numbers are off here, so the grid shows where each axis runs.
scene.segment("crossings")
X = Axis.linear(-1, 4).ticks(1)
Y = Axis.linear(-1, 3).ticks(1)
planes(
    [
        ('X.crossing("zero")\nY.crossing("zero")', X.crossing("zero"), Y.crossing("zero")),
        ('X.crossing("min")\nY.crossing("min")', X.crossing("min"), Y.crossing("min")),
        ('X.crossing("max")\nY.crossing("max")', X.crossing("max"), Y.crossing("max")),
        ("X.crossing(3)\nY.crossing(2)", X.crossing(3), Y.crossing(2)),
    ],
    numbers=False,
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The first number lines drawing, then every segment once drawn.
    scene.snapshots(snapshots, [0.5] + [(DRAW + HOLD) * index + 1.6 for index in range(10)])
else:
    scene.render()
