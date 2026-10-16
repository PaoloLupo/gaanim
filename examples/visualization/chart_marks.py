"""Chart marks: points, lines, steps, areas, bars, histograms, box plots, violins, error bars, heatmaps and arcs.

``ChartSpec`` is an immutable recipe of data, a mark, encodings, axes and
guides; ``scene.viz.chart(spec)`` draws it, inferring the axes it is not
given from the data. Each segment draws one mark at the chart's full size,
labelled with the spec that builds it; the last one sweeps the marks in
with a wipe instead of fading them.
"""

import math
import os
import random

from gaanim import Axis, ChartSpec, Direction, Scene, Value, Zones

DRAW = 0.8
HOLD = 0.8
# A segment draws the axes, then the marks, then holds.
STEP = 2 * DRAW + HOLD

scene = Scene()
chart_color = scene.canvas.color("chart")

# One observation per row: x, y and how far its error reaches below and
# above y.
SERIES = {
    "x": [0, 1, 2, 3, 4, 5],
    "y": [1.0, 2.4, 1.6, 3.2, 2.1, 2.8],
    "below": [0.4, 0.5, 0.4, 0.6, 0.5, 0.5],
    "above": [0.5, 0.4, 0.5, 0.5, 0.4, 0.6],
}
# Elapsed time of four methods, for bars on a category axis.
METHODS = {"method": ["Baseline", "Cached", "GPU", "Optimized"], "elapsed": [48, 32, 21, 15]}
# Two hundred draws from a normal distribution, for the distribution marks.
draws = random.Random(7)
SAMPLES = {"value": [draws.gauss(5, 1.5) for _ in range(200)]}
# A 9 x 6 grid of cells with a value each, for the heatmap.
GRID = [(x, y, math.sin(x / 2) * math.cos(y / 2)) for y in range(6) for x in range(9)]
CELLS = {"x": [x for x, _, _ in GRID], "y": [y for _, y, _ in GRID], "value": [v for _, _, v in GRID]}
# The share of each source in a power grid, and how much each one grew, for
# the arcs.
ENERGY = {
    "source": ["Solar", "Wind", "Hydro", "Gas"],
    "share": [36, 27, 20, 17],
    "growth": [9, 6, 2, 3],
}

series = ChartSpec(SERIES)


def show(label, spec, style=None, entry=None):
    """Draw ``spec`` at its default size and place, its label above it.

    ``style(marks)`` restyles the marks layer when given, and
    ``entry(marks)`` is the animation that brings the marks in (a fade
    by default).
    """
    top, _ = scene.layout.zones(Zones.rows(["1fr", "8fr"], padding="page_padding"))
    text = scene.text(label, role="code").place(top, anchor="bottom")
    chart = scene.viz.chart(spec)
    if style:
        style(chart.layer("marks"))
    scene.play([chart.layer("axes").animate.create(), text.animate.fade_in()], duration=DRAW)
    marks = chart.layer("marks")
    scene.play([entry(marks) if entry else marks.animate.fade_in()], duration=DRAW)
    scene.wait(HOLD)


def show_arc(label, spec):
    """Grow the arc chart ``spec`` around its center, its label above it."""
    top, _ = scene.layout.zones(Zones.rows(["1fr", "8fr"], padding="page_padding"))
    text = scene.text(label, role="code").place(top, anchor="bottom")
    chart = scene.viz.chart(spec)
    scene.play([chart.animate.grow(), text.animate.fade_in()], duration=2 * DRAW)
    scene.wait(HOLD)


# point, line, step and area draw one series of (x, y) rows; radius sizes
# the points.
scene.segment("point")
show('series.mark("point", radius=0.12).encode(x="x", y="y")', series.mark("point", radius=0.12).encode(x="x", y="y"))

scene.segment("line")
show('series.mark("line").encode(x="x", y="y")', series.mark("line").encode(x="x", y="y"))

scene.segment("step")
show('series.mark("step").encode(x="x", y="y")', series.mark("step").encode(x="x", y="y"))

scene.segment("area")
show('series.mark("area").encode(x="x", y="y")', series.mark("area").encode(x="x", y="y"))

# A bar per row; on a category axis width is the share of each band. The
# label channel writes a value at the end of each bar.
scene.segment("bar")
show(
    'ChartSpec(METHODS).mark("bar", width=0.6)\n'
    '    .encode(x="method", y="elapsed", label="elapsed")\n'
    '    .axes(y=Axis.linear(0, 55).ticks(10).label("ms"))',
    ChartSpec(METHODS)
    .mark("bar", width=0.6)
    .encode(x="method", y="elapsed", label="elapsed")
    .axes(y=Axis.linear(0, 55).ticks(10).label("ms")),
)

# The distribution marks summarize one column: a histogram counts it in
# bins, a box plot draws its quartiles and a violin its estimated density.
# Box and violin stand at x = center (0 by default), the first band of a
# category axis. The box is outlined here so its median and whiskers show.
scene.segment("histogram")
show('ChartSpec(SAMPLES).mark("histogram", bins=12).encode(x="value")', ChartSpec(SAMPLES).mark("histogram", bins=12).encode(x="value"))

scene.segment("box")
show(
    'ChartSpec(SAMPLES).mark("box", width=0.4).encode(y="value")\n    .axes(x=Axis.category(["value"]))',
    ChartSpec(SAMPLES).mark("box", width=0.4).encode(y="value").axes(x=Axis.category(["value"])),
    style=lambda marks: marks.no_fill().stroke(chart_color, 0.04),
)

scene.segment("violin")
show(
    'ChartSpec(SAMPLES).mark("violin", bandwidth=0.5).encode(y="value")\n    .axes(x=Axis.category(["value"]))',
    ChartSpec(SAMPLES).mark("violin", bandwidth=0.5).encode(y="value").axes(x=Axis.category(["value"])),
)

# Error bars reach the low column's value below each y and the high
# column's above it.
scene.segment("error_bar")
show(
    'series.mark("error_bar", low="below", high="above", cap_width=0.2)\n    .encode(x="x", y="y")',
    series.mark("error_bar", low="below", high="above", cap_width=0.2).encode(x="x", y="y"),
)

# A heatmap colors each cell by the color column, quantized into bands of
# the theme's heatmap palette.
scene.segment("heatmap")
show(
    'ChartSpec(CELLS).mark("heatmap", bands=8)\n    .encode(x="x", y="y", color="value")',
    ChartSpec(CELLS).mark("heatmap", bands=8).encode(x="x", y="y", color="value"),
)

# An arc gives each row its share of the theta total, clockwise from 12
# o'clock, with no axes; grow() turns a hand from there and opens each
# slice, then its label, as the hand crosses it.
# inner_radius leaves the middle empty for a donut and pad_angle separates
# the slices; a radius field draws a rose, each slice reaching out in
# proportion to its value.
scene.segment("arc")
show_arc(
    'ChartSpec(ENERGY).mark("arc")\n    .encode(theta="share", color="source", label="source")',
    ChartSpec(ENERGY).mark("arc").encode(theta="share", color="source", label="source"),
)

scene.segment("donut")
show_arc(
    'ChartSpec(ENERGY).mark("arc", inner_radius=0.6, pad_angle=0.03)\n    .encode(theta="share", color="source", label="share")',
    ChartSpec(ENERGY).mark("arc", inner_radius=0.6, pad_angle=0.03).encode(theta="share", color="source", label="share"),
)

scene.segment("rose")
show_arc(
    'ChartSpec(ENERGY).mark("arc", inner_radius=0.2)\n    .encode(theta=Value(1), radius="growth", color="source")',
    ChartSpec(ENERGY).mark("arc", inner_radius=0.2).encode(theta=Value(1), radius="growth", color="source"),
)

# Any mark can enter behind an edge that sweeps across it; a Parameter
# passed to marks.wipe(progress, ...) moves the edge instead, at the pace of
# a simulation.
scene.segment("wipe")
show(
    'chart.layer("marks").animate.wipe(Direction.RIGHT)',
    ChartSpec(CELLS).mark("heatmap", bands=8).encode(x="x", y="y", color="value"),
    entry=lambda marks: marks.animate.wipe(Direction.RIGHT),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The first chart while its marks fade in, then every chart drawn, the
    # first arc halfway through its growth, and the wipe halfway across.
    scene.snapshots(
        snapshots,
        [1.2]
        + [STEP * index + 2.2 for index in range(13)]
        + [STEP * 10 + 0.8, STEP * 13 + 1.2, STEP * 13 + 2.2],
    )
else:
    scene.render()
