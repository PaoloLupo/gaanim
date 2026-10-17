"""Chart transitions: morphing between specs by key or index, new mark families, changing data, 2D to 3D and arcs.

``chart.animate.to(spec)`` animates a chart into another recipe: marks
matched by their key column (or by row index) move and reshape, the rest
fades, and the target chart replaces the first when the clip ends. Each
segment runs one transition, labelled with its code.
"""

import math
import os

from gaanim import Axis, ChartSpec, DataSource, Field, Scale, Scene, Value, Zones

DRAW = 0.8
MORPH = 1.6
HOLD = 0.8
# A segment draws a chart, holds, swaps its label, transitions and holds.
STEP = 2 * DRAW + 2 * HOLD + 0.4 + MORPH

scene = Scene()
accent = scene.canvas.color("accent")

# The same six cities in two years: their key column, "city", ties each row
# of one spec to the row of the other.
BEFORE = {
    "city": ["Lima", "Cusco", "Arequipa", "Puno", "Piura", "Iquitos"],
    "visitors": [82, 61, 38, 15, 24, 9],
    "growth": [2.0, 4.5, 3.1, 6.2, 1.4, 5.0],
}
AFTER = {
    "city": ["Lima", "Cusco", "Arequipa", "Puno", "Piura", "Iquitos"],
    "visitors": [112, 121, 70, 52, 56, 79],
    "growth": [3.1, 6.8, 4.0, 7.5, 2.2, 9.4],
}
AXES = {
    "x": Axis.linear(0, 140).ticks(20).label("visitors", position="center"),
    "y": Axis.linear(0, 10).ticks(2).label("growth", position="center"),
}


def label(text):
    """Write a code label above the chart, hidden until faded in."""
    top, _ = scene.layout.zones(Zones.rows(["1fr", "9fr"], padding="page_padding"))
    return scene.text(text, role="code").place(top, anchor="bottom_left").hidden()


def transition(first, setup, target, then, **options):
    """Draw ``first`` with the ``setup`` label, then run ``to(target)`` under the ``then`` label."""
    before, after = label(setup), label(then)
    chart = scene.viz.chart(first)
    scene.play([chart.layer("axes").animate.create(), before.animate.fade_in()], duration=DRAW)
    scene.play([chart.layer("marks").animate.fade_in()], duration=DRAW)
    scene.wait(HOLD)
    scene.play([before.animate.fade_out(), after.animate.fade_in()], duration=0.4)
    scene.play([chart.animate.to(target, **options)], duration=MORPH)
    scene.wait(HOLD)
    return chart


# Matched by key, each point moves from its old values to its new ones.
scene.segment("by_key")
points = ChartSpec(BEFORE, key="city").mark("point", radius=0.14).encode(x="visitors", y="growth").axes(**AXES)
moved = ChartSpec(AFTER, key="city").mark("point", radius=0.14).encode(x="visitors", y="growth").axes(**AXES)
transition(
    points,
    'points = ChartSpec(BEFORE, key="city").mark("point", radius=0.14)\n    .encode(x="visitors", y="growth")',
    moved,
    'chart.animate.to(ChartSpec(AFTER, key="city").mark("point", radius=0.14)\n    .encode(x="visitors", y="growth"))',
)

# A new encoding is a new spec too: encode() replaces the channels it names
# and keeps the others, and the axes follow the new values.
scene.segment("encodings")
bars = ChartSpec(BEFORE, key="city").mark("bar", width=0.6).encode(x="city", y="visitors")
transition(
    bars,
    'bars = ChartSpec(BEFORE, key="city").mark("bar", width=0.6)\n    .encode(x="city", y="visitors")',
    bars.encode(y="growth", color=Value(accent)),
    'chart.animate.to(bars.encode(y="growth", color=Value(accent)))',
)

# Specs without a key match their rows in order with match_="index".
scene.segment("by_index")
WAVE = {"t": [i / 4 for i in range(25)], "y": [math.sin(i / 4) for i in range(25)]}
WAVE2 = {"t": WAVE["t"], "y": [0.5 * math.sin(i / 2) + 0.4 for i in range(25)]}
transition(
    ChartSpec(WAVE).mark("line").encode(x="t", y="y"),
    'ChartSpec(WAVE).mark("line").encode(x="t", y="y")',
    ChartSpec(WAVE2).mark("line").encode(x="t", y="y"),
    'chart.animate.to(ChartSpec(WAVE2).mark("line").encode(x="t", y="y"),\n    match_="index")',
    match_="index",
)

# Marks of different families cannot morph into each other; fallback=
# "crossfade" fades from one chart to the other instead of failing.
scene.segment("crossfade")
transition(
    bars,
    'bars = ChartSpec(BEFORE, key="city").mark("bar", width=0.6)\n    .encode(x="city", y="visitors")',
    points,
    'chart.animate.to(points, fallback="crossfade")',
    fallback="crossfade",
)

# A DataSource is a table that changes: after append() or replace(), a new
# ChartSpec captures its new version, and the chart animates to it. sales()
# builds that spec: an area of sales per day on fixed axes.
scene.segment("data_source")
source = DataSource({"day": [0, 1, 2, 3], "sales": [3, 5, 4, 6]})


def sales():
    return ChartSpec(source).mark("area").encode(x="day", y="sales").axes(x=Axis.linear(0, 7), y=Axis.linear(0, 10))


first = sales()
source.append({"day": [4, 5, 6, 7], "sales": [7, 6, 8, 9]})
transition(
    first,
    'source = DataSource({"day": [0, 1, 2, 3], "sales": [3, 5, 4, 6]})\nchart = scene.viz.chart(sales())',
    sales(),
    'source.append({"day": [4, 5, 6, 7], "sales": [7, 6, 8, 9]})\nchart.animate.to(sales(), match_="index")',
    match_="index",
)

# point, line and bar charts pass to 3D when they gain a z encoding; the
# camera moves on its own, alongside.
scene.segment("to_3d")
GROUPS = Scale.category(["coast", "andes", "jungle"])
flat = (
    ChartSpec({**BEFORE, "region": ["coast", "andes", "coast", "andes", "coast", "jungle"]}, key="city")
    .mark("point")
    .encode(x="visitors", y="growth", color=Field("region", scale=GROUPS), size=Value(0.2))
    .axes(**AXES)
)
deep = flat.encode(z="growth").axes(z=Axis.linear(0, 10).ticks(2).label("z"))
before, after = label('flat = points.encode(color=Field("region"))'), label('chart.animate.to(flat.encode(z="growth"))')
chart = scene.viz.chart(flat)
scene.play([chart.layer("axes").animate.create(), before.animate.fade_in()], duration=DRAW)
scene.play([chart.layer("marks").animate.fade_in()], duration=DRAW)
scene.wait(HOLD)
scene.play([before.animate.fade_out(), after.animate.fade_in()], duration=0.4)
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.play([chart.animate.to(deep), scene.camera.animate.look_at(eye=(9, 8.5, 11), target=(0, 1.5, 0))], duration=MORPH)
scene.wait(HOLD)

# A heatmap and a surface share their grid of cells, so one becomes the other.
scene.segment("heatmap_to_surface")
scene.camera.reset()
GRID = [(x, y, math.sin(x) * math.cos(y)) for y in range(-2, 3) for x in range(-2, 3)]
CELLS = {
    "cell": [f"{x}:{y}" for x, y, _ in GRID],
    "x": [x for x, _, _ in GRID],
    "y": [y for _, y, _ in GRID],
    "value": [value for _, _, value in GRID],
}
# crossing("min") keeps the heatmap's axes on its edges, off the cells.
EDGES = {"x": Axis.linear(-2.5, 2.5).ticks(1).crossing("min"), "y": Axis.linear(-2.5, 2.5).ticks(1).crossing("min")}
cells = ChartSpec(CELLS, key="cell").axes(**EDGES)
heatmap = cells.mark("heatmap", bands=12).encode(x="x", y="y", color="value")
surface = cells.mark("surface").encode(x="x", y="y", z="value", color="value")
before = label('cells = ChartSpec(CELLS, key="cell").axes(**EDGES)\ncells.mark("heatmap", bands=12).encode(x="x", y="y", color="value")')
after = label('chart.animate.to(cells.mark("surface")\n    .encode(x="x", y="y", z="value", color="value"))')
chart = scene.viz.chart(heatmap)
scene.play([chart.layer("axes").animate.create(), before.animate.fade_in()], duration=DRAW)
scene.play([chart.layer("marks").animate.fade_in()], duration=DRAW)
scene.wait(HOLD)
scene.play([before.animate.fade_out(), after.animate.fade_in()], duration=0.4)
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000)
scene.play([chart.animate.to(surface), scene.camera.animate.look_at(eye=(13, 10, 15), target=(0, 0, 0))], duration=MORPH)
scene.wait(HOLD)

# Between two arc charts each slice sweeps to the share of its key; a city
# that leaves closes on its middle angle and one that arrives opens from it.
scene.segment("arc")
scene.camera.reset()
TRIPS = {"city": ["Lima", "Cusco", "Arequipa", "Puno"], "visitors": [82, 61, 38, 15]}
LATER = {"city": ["Lima", "Cusco", "Arequipa", "Iquitos"], "visitors": [112, 121, 70, 79]}
pie = ChartSpec(TRIPS, key="city").mark("arc", inner_radius=0.5).encode(theta="visitors", color="city", label="city")
before = label('pie = ChartSpec(TRIPS, key="city").mark("arc", inner_radius=0.5)\n    .encode(theta="visitors", color="city", label="city")')
after = label('chart.animate.to(ChartSpec(LATER, key="city").mark("arc", inner_radius=0.5)\n    .encode(theta="visitors", color="city", label="city"))')
chart = scene.viz.chart(pie)
scene.play([chart.animate.grow(), before.animate.fade_in()], duration=2 * DRAW)
scene.wait(HOLD)
scene.play([before.animate.fade_out(), after.animate.fade_in()], duration=0.4)
later = ChartSpec(LATER, key="city").mark("arc", inner_radius=0.5).encode(theta="visitors", color="city", label="city")
scene.play([chart.animate.to(later)], duration=MORPH)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment before and after its transition, and the 3D ones and the
    # arcs halfway.
    scene.snapshots(
        snapshots,
        [STEP * index + t for index in range(7) for t in (2.2, 5.0)]
        + [STEP * 5 + 3.6, STEP * 6 + 3.6]
        + [STEP * 7 + t for t in (2.2, 3.6, 5.0)],
    )
else:
    scene.render()
