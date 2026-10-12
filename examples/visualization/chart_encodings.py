"""Chart encodings: color, size, opacity and label channels, scales, guides and explicit axes.

``ChartSpec.encode`` maps columns to channels. A channel takes a column
name, ``Field(column, scale=...)`` when the scale matters, or ``Value(...)``
for a constant. Guides (legends and colorbars) explain the color, size and
opacity channels, and ``axes`` replaces the inferred axes. Each segment
draws one chart at its full size, labelled with the spec that builds it.
"""

import os

from gaanim import Axis, ChartSpec, Field, Guide, Scale, Scene, Value, Zones

DRAW = 0.8
HOLD = 1.0
# A segment draws the axes, then the marks and guides, then holds.
STEP = 2 * DRAW + HOLD

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
warning = scene.canvas.color("warning")
background = scene.canvas.color("background")

# Twelve observations: a position, a group, a weight and a confidence.
POINTS = {
    "x": [1.0, 1.6, 2.2, 2.9, 3.4, 4.1, 4.8, 5.3, 6.0, 6.6, 7.2, 7.9],
    "y": [2.1, 3.4, 2.7, 4.6, 3.9, 5.2, 4.4, 6.1, 5.5, 6.8, 6.2, 7.4],
    "group": ["north", "south", "east", "north", "south", "east", "north", "south", "east", "north", "south", "east"],
    "weight": [1, 4, 2, 8, 3, 6, 9, 2, 5, 7, 4, 10],
    "confidence": [0.2, 0.9, 0.5, 0.7, 0.3, 1.0, 0.6, 0.4, 0.8, 0.5, 0.9, 0.3],
}
# Monthly values of three cities, one row per city and month.
MONTHS = list(range(1, 9))
LEVELS = {"Lima": [3, 4, 4, 6, 7, 7, 8, 9], "Cusco": [1, 2, 4, 3, 5, 6, 6, 7], "Puno": [2, 2, 3, 3, 3, 4, 5, 5]}
CITIES = {
    "month": [month for _ in LEVELS for month in MONTHS],
    "value": [value for values in LEVELS.values() for value in values],
    "city": [city for city in LEVELS for _ in MONTHS],
}
# Yearly changes, positive and negative.
CHANGES = {
    "year": ["2019", "2020", "2021", "2022", "2023"],
    "change": [12, -18, 7, 15, -4],
    "kind": ["gain", "loss", "gain", "gain", "loss"],
}
# Populations over four orders of magnitude.
TOWNS = {"population": [40, 220, 900, 3100, 12000, 34000, 81000], "share": [0.12, 0.25, 0.31, 0.46, 0.58, 0.71, 0.86]}

points = ChartSpec(POINTS).mark("point", radius=0.12)


def show(label, spec, guides=True):
    """Draw ``spec`` a little below its default place, its label above it.

    ``guides`` says whether the spec has a legend or colorbar to bring in.
    """
    top, _ = scene.layout.zones(Zones.rows(["1fr", "5fr"], padding="page_padding"))
    text = scene.text(label, role="code", markup=False).place(top, anchor="bottom_left")
    chart = scene.viz.chart(spec).move_to(0, -0.3)
    scene.play([chart.layer("axes").animate.create(), text.animate.fade_in()], duration=DRAW)
    layers = ["marks", "guides"] if guides else ["marks"]
    scene.play([chart.layer(name).animate.fade_in() for name in layers], duration=DRAW)
    scene.wait(HOLD)


# A text column on color is categorical: each group gets a color of the
# categorical palette, listed by a legend in order of appearance.
scene.segment("color_groups")
show(
    'points.encode(x="x", y="y", color="group")\n    .guides(color=Guide.legend(title="group"))',
    points.encode(x="x", y="y", color="group").guides(color=Guide.legend(title="group")),
)

# Scale.category fixes the order of the groups and colors() their colors.
scene.segment("color_scale")
palette = Scale.category(["north", "south", "east"]).colors([accent, success, danger])
show(
    'palette = Scale.category(["north", "south", "east"])\n'
    "    .colors([accent, success, danger])\n"
    'points.encode(x="x", y="y", color=Field("group", scale=palette))',
    points.encode(x="x", y="y", color=Field("group", scale=palette)).guides(color=Guide.legend()),
)

# A numeric column on color maps its domain through a continuous scale; a
# colorbar is the guide of a continuous channel.
scene.segment("color_numeric")
show(
    'points.encode(x="x", y="y",\n'
    '    color=Field("weight", scale=Scale.linear((0, 10))))\n'
    '    .guides(color=Guide.colorbar(title="weight"))',
    points.encode(x="x", y="y", color=Field("weight", scale=Scale.linear((0, 10)))).guides(
        color=Guide.colorbar(title="weight")
    ),
)

# size takes the point radius from a column (0.03 to 0.12 scene units, by
# area) and opacity the transparency; Value() makes any channel constant.
scene.segment("size_opacity")
show(
    'ChartSpec(POINTS).mark("point").encode(x="x", y="y",\n'
    '    size="weight", opacity="confidence", color=Value(accent))',
    ChartSpec(POINTS).mark("point").encode(x="x", y="y", size="weight", opacity="confidence", color=Value(accent)),
    guides=False,
)

# On a line, step or area mark a categorical color draws one series per
# category.
scene.segment("series")
show(
    'ChartSpec(CITIES).mark("line")\n'
    '    .encode(x="month", y="value", color="city")\n'
    '    .guides(color=Guide.legend(title="city"))',
    ChartSpec(CITIES).mark("line").encode(x="month", y="value", color="city").guides(color=Guide.legend(title="city")),
)

# Bar labels sit past the bar's end by default, or inside it in their own
# color. Bars grow from zero both ways; crossing("min") keeps the year axis
# at the bottom, clear of the bars that go down.
scene.segment("bar_labels")
gains = Scale.category(["gain", "loss"]).colors([success, danger])
changes = (
    ChartSpec(CHANGES)
    .encode(x="year", y="change", label="change", color=Field("kind", scale=gains))
    .axes(y=Axis.linear(-20, 20).ticks(10).crossing("min"))
)
show(
    'changes.mark("bar", label_position="inside", label_color=background)',
    changes.mark("bar", label_position="inside", label_color=background),
    guides=False,
)

# axes() replaces the inferred axes: here a logarithmic x and a y in
# percent. An explicit domain is kept as given.
scene.segment("axes")
show(
    'ChartSpec(TOWNS).mark("line").encode(x="population", y="share")\n'
    '    .axes(x=Axis.log(10, 100000).label("population", position="center"),\n'
    '          y=Axis.linear(0, 1).numbers("percent", precision=0))',
    ChartSpec(TOWNS)
    .mark("line")
    .encode(x="population", y="share")
    .axes(
        x=Axis.log(10, 100000).label("population", position="center"),
        y=Axis.linear(0, 1).numbers("percent", precision=0),
    ),
    guides=False,
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Every chart once its marks and guides are in.
    scene.snapshots(snapshots, [STEP * index + 2.4 for index in range(7)])
else:
    scene.render()
