"""Plots: functions, parametric and implicit curves, contours, calculus constructions and reactive curves.

Every curve lives in the data coordinates of the plane that draws it. The
first segments draw curves of each kind, the next ones the calculus
constructions (tangents, normals, secants, projections, areas and Riemann
sums) and the last one curves that follow a parameter or the time. Each
curve is stroked with the theme's accent.
"""

import math
import os

from gaanim import Axis, Scene, Zones

DRAW = 1.2
HOLD = 0.8
# Each gallery draws its planes, then their marks, and holds.
STEP = 2 * DRAW + HOLD
# Planes are drawn larger than their default size so their numbers read
# as easily as the labels.
ZOOM = 1.3

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
# crossing("min") keeps both axes on the edges of the plot, clear of the curves.
ANGLE = Axis.linear(0, 2 * math.pi).ticks(math.pi / 2).numbers("pi", denominator=2)
WAVE = Axis.linear(-1.5, 1.5).ticks(1).crossing("min")
SQUARE = Axis.linear(-2, 2).ticks(1).crossing("min")
X = Axis.linear(0, 4).ticks(1)
Y = Axis.linear(0, 3).ticks(1)


def f(x):
    """The function of the calculus segments."""
    return 0.15 * x * x + 0.3


def lissajous(t):
    return 1.8 * math.sin(3 * t), 1.5 * math.sin(2 * t)


def quartic(x, y):
    return x**4 + y**4 - 2 * x * y - 1


def bowl(x, y):
    return x * x + y * y


def saddle(x, y):
    return x * y


def cells(count, columns=2, bands=("1fr", "5fr"), zone=None):
    """A grid of ``count`` cells in ``zone`` (the padded frame by default), each split into a label band and a body."""
    rows = -(-count // columns)
    template = Zones.grid(rows=rows, columns=columns, column_gap="72px", row_gap="48px")
    grid = zone.split(template) if zone else scene.layout.zones(
        Zones.grid(rows=rows, columns=columns, column_gap="72px", row_gap="48px", padding="page_padding")
    )
    return [cell.split(Zones.rows(list(bands))) for cell in list(grid)[:count]]


def plane_in(body, x, y, size):
    """A Cartesian plane whose plot area is ``size`` scene units, centered in ``body``."""
    plane = scene.viz.cartesian_2d(x, y, width=size[0] / ZOOM, height=size[1] / ZOOM)
    # A plane's origin is the middle of its plot area.
    return plane.scale_to(ZOOM).move_to(*body.center)


def gallery(variants, x, y, columns=2, square=False, bands=("1fr", "5fr"), zone=None):
    """Draw a plane per (label, build) cell, its label above it, and what ``build(plane)`` returns on it.

    ``build`` returns one mark or a list of them; ``x`` and ``y`` may be
    lists with one axis per cell. ``square`` plot areas keep circles round.
    """
    count = len(variants)
    xs = x if isinstance(x, list) else [x] * count
    ys = y if isinstance(y, list) else [y] * count
    planes, marks = [], []
    for (top, body), (label, build), x_axis, y_axis in zip(cells(count, columns, bands, zone), variants, xs, ys):
        scene.text(label, role="code").place(top, anchor="bottom")
        if square:
            side = min(body.width, body.height) * 0.75
            size = (side, side)
        else:
            size = (body.width * 0.75, body.height * 0.6)
        plane = plane_in(body, x_axis, y_axis, size)
        planes.append(plane)
        built = build(plane)
        marks.extend(built if isinstance(built, list) else [built])
    scene.play([plane.animate.create() for plane in planes], duration=DRAW)
    scene.play([mark.animate.create() for mark in marks], duration=DRAW)
    scene.wait(HOLD)


def curve(mark):
    """Stroke a curve with the accent."""
    return mark.stroke(accent, 0.05)


def line(mark):
    """Stroke a construction line with the foreground color."""
    return mark.no_fill().stroke(foreground, 0.035)


def area(mark):
    """Fill an area with a translucent accent."""
    return mark.fill(accent).opacity(0.45)


def over_f(plane, mark):
    """``mark`` drawn over the curve y = f(x)."""
    return [mark, curve(plane.plot(f))]


# plot() samples y = f(x) adaptively over the plane's x range, or over a
# domain; samples= fixes the number of samples instead. Marks are clipped
# to the plot area unless no_clip() lets them overflow.
scene.segment("functions")
gallery(
    [
        ("plane.plot(math.sin)", lambda plane: curve(plane.plot(math.sin))),
        ("plane.plot(math.sin, domain=(0, pi))", lambda plane: curve(plane.plot(math.sin, domain=(0, math.pi)))),
        ("plane.plot(math.sin, samples=7)", lambda plane: curve(plane.plot(math.sin, samples=7))),
        (
            "plane.plot(lambda x: 2 * sin(x))\n    .no_clip()",
            lambda plane: curve(plane.plot(lambda x: 2 * math.sin(x))).no_clip(),
        ),
    ],
    ANGLE,
    WAVE,
)

# parametric() draws t -> (x, y) and implicit() the zero set of f(x, y),
# sampled on a grid of resolution cells.
scene.segment("curves")
gallery(
    [
        ("plane.parametric(lissajous, (0, tau))", lambda plane: curve(plane.parametric(lissajous, (0, math.tau)))),
        ("plane.implicit(quartic)", lambda plane: curve(plane.implicit(quartic))),
    ],
    SQUARE,
    SQUARE,
    square=True,
    bands=("1fr", "9fr"),
)

# contour() draws one curve for each level of f(x, y).
scene.segment("contours")
gallery(
    [
        ("plane.contour(bowl,\n    levels=[0.5, 1, 2, 3])", lambda plane: curve(plane.contour(bowl, levels=[0.5, 1, 2, 3]))),
        (
            "plane.contour(saddle,\n    levels=[-1, -0.2, 0.2, 1])",
            lambda plane: curve(plane.contour(saddle, levels=[-1, -0.2, 0.2, 1])),
        ),
    ],
    SQUARE,
    SQUARE,
    square=True,
    bands=("1fr", "9fr"),
)

# Curves are sampled through the axes' scales: on a logarithmic x axis
# y = log10(x) is a straight line.
scene.segment("scales")
LOG = Axis.log(1, 1000)
LINEAR = Axis.linear(1, 1000)
gallery(
    [
        ("cartesian_2d(LOG, Y)\n    .plot(math.log10)", lambda plane: curve(plane.plot(math.log10))),
        ("cartesian_2d(LINEAR, Y)\n    .plot(math.log10)", lambda plane: curve(plane.plot(math.log10))),
    ],
    [LOG, LINEAR],
    Y,
)

# Tangent and normal lines at x, a secant through two points and the
# projections of a point onto both axes, drawn over y = f(x).
scene.segment("tangents")
gallery(
    [
        ("plane.tangent(f, 2, length=3)", lambda plane: over_f(plane, line(plane.tangent(f, 2, length=3)))),
        ("plane.normal(f, 2, length=1.5)", lambda plane: over_f(plane, line(plane.normal(f, 2, length=1.5)))),
        ("plane.secant(f, 1, 3.5)", lambda plane: over_f(plane, line(plane.secant(f, 1, 3.5)))),
        ("plane.projections(2.5, f(2.5))", lambda plane: over_f(plane, line(plane.projections(2.5, f(2.5))))),
    ],
    X,
    Y,
)

# The area between y = f(x) and a baseline, and Riemann sums of
# equal-width rectangles (twelve, at the midpoints, unless told otherwise).
scene.segment("areas")
gallery(
    [
        ("plane.area_under(f, (1, 3))", lambda plane: over_f(plane, area(plane.area_under(f, (1, 3))))),
        (
            "plane.area_under(f, (0, 4),\n    baseline=1)",
            lambda plane: over_f(plane, area(plane.area_under(f, (0, 4), baseline=1))),
        ),
        ("plane.riemann_sum(f, (0, 4))", lambda plane: over_f(plane, area(plane.riemann_sum(f, (0, 4))))),
        (
            "plane.riemann_sum(f, (0, 4),\n    rectangles=4)",
            lambda plane: over_f(plane, area(plane.riemann_sum(f, (0, 4), rectangles=4))),
        ),
    ],
    X,
    Y,
)

# method picks where each rectangle meets the curve.
scene.segment("riemann_methods")
header, body = scene.layout.zones(Zones.rows(["1fr", "7fr"], gap="24px", padding="page_padding"))
scene.text("plane.riemann_sum(f, (0, 4), rectangles=6, method=...)", role="code").place(header, anchor="bottom")
gallery(
    [
        (
            f'method="{method}"',
            lambda plane, method=method: over_f(plane, area(plane.riemann_sum(f, (0, 4), rectangles=6, method=method))),
        )
        for method in ("left", "midpoint", "right")
    ],
    X,
    Y,
    columns=3,
    bands=("1fr", "6fr"),
    zone=body,
)

# inputs= passes reactive values after x, so the curve is rebuilt whenever
# they change: a parameter, or the time itself.
scene.segment("reactive")
amplitude = scene.viz.parameter(0.5)
waves = []
for (top, body), (label, build) in zip(
    cells(2, bands=("1fr", "3fr")),
    [
        (
            "plane.plot(\n    lambda x, a: a * sin(x),\n    inputs=[amplitude])",
            lambda plane: plane.plot(lambda x, a: a * math.sin(x), inputs=[amplitude]),
        ),
        (
            "plane.plot(\n    lambda x, t: sin(x - 2 * t),\n    inputs=[scene.viz.time])",
            lambda plane: plane.plot(lambda x, t: math.sin(x - 2 * t), inputs=[scene.viz.time]),
        ),
    ],
):
    scene.text(label, role="code").place(top, anchor="bottom_left")
    plane = plane_in(body, ANGLE, WAVE, (body.width * 0.75, body.height * 0.6))
    waves.append((plane, curve(build(plane))))
scene.play([part.animate.create() for pair in waves for part in pair], duration=DRAW)
scene.play([amplitude.animate.set(1.4)], duration=1.5)
scene.play([amplitude.animate.set(0.5)], duration=1.5)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each gallery drawn (and the first one while its curves draw), then the
    # reactive curves at both amplitudes.
    scene.snapshots(snapshots, [1.8] + [STEP * index + 3.1 for index in range(7)] + [STEP * 7 + 2.7, STEP * 7 + 4.2])
else:
    scene.render()
