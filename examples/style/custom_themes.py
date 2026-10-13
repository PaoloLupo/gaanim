"""Custom themes: a Theme derived from a built-in one, with its own colors, fonts, sizes, rules and tokens.

``FIELD_NOTES`` below derives the ``nord`` theme and replaces only what it
names: color roles and tokens of its own, font families, text sizes and
styles, rules for families, factories, chart parts and classes of objects,
chart palettes and layout tokens. The scene is built with it, so everything
here, labels included, is drawn in its look. Each segment shows one part of
it, labelled with the code that uses it.
"""

import math
import os

from gaanim import AxesStyle, Axis, ChartSpec, Scene, StrokeStyle, Style, TextStyle, Theme, Zones, colors

GROW = 0.8
HOLD = 1.4

palette = colors.tailwind
FIELD_NOTES = Theme(
    "nord",
    name="field-notes",
    # Roles replace the base's colors; any other name is a token for rules.
    colors={
        "title": palette.amber[300],
        "accent": palette.orange[400],
        "brand": palette.sky[400],
        "ink": palette.slate[200],
        "calm": palette.emerald[400],
    },
    # Families by role: "text" names every prose role. Both families here
    # ship with Gaanim; font_dir or font_files embed font files of your own.
    fonts={"text": "Libertinus Serif", "code": "DejaVu Sans Mono"},
    sizes={"title": 0.8, "heading": 0.6, "body": 0.44},
    text={
        "label": TextStyle(weight=700, letter_spacing=0.04),
        "caption": TextStyle(italic=True),
    },
    # Selectors: a family ("shape", "line"), a factory ("circle", and
    # "expression_plot" for function plots), a part ("axes" and its axis,
    # grid, ticks, numbers and labels) or a class.
    styles={
        "shape": Style(fill="brand"),
        "circle": Style(fill="accent", stroke=StrokeStyle("ink", 0.05)),
        "line": Style(stroke=StrokeStyle("ink", 0.06, cap="butt", dashes=[0.24, 0.12])),
        "expression_plot": Style(stroke=StrokeStyle("accent", 0.07)),
        "axes": AxesStyle(
            axis=StrokeStyle("ink", 0.04),
            grid=StrokeStyle("rule", 0.012),
            ticks=StrokeStyle("ink", 0.03),
            numbers=TextStyle(size=0.26, color=palette.slate[400]),
            labels=TextStyle(size=0.32, weight=700, color=palette.amber[300]),
        ),
        ".warning": Style(fill="warning"),
        ".calm": Style(fill="calm"),
        ".ghost": Style(opacity=0.35),
    },
    # The first series color paints chart marks; heatmap cells pick theirs
    # along the heatmap ramp.
    series=[palette.sky[400], palette.amber[300], palette.rose[400]],
    heatmap=[palette.slate[800], palette.sky[700], palette.sky[300], palette.amber[200]],
    # Tokens are scene units, used wherever a length is: gap="gutter".
    layout={"page_padding": 0.6, "gutter": 0.5},
    # With markup off, * and _ are literal unless a text asks for markup.
    text_markup=False,
)

scene = Scene(theme=FIELD_NOTES)
muted = scene.canvas.color("muted")
success = scene.canvas.color("success")


def label(code):
    """A variant label: the code, in the code role."""
    return scene.text(code, role="code")


def readout(text):
    """A value read back from the theme, in the muted color."""
    return scene.text(text, role="code").fill(muted)


def table(rows, columns=("10fr", "4fr", "3fr"), anchor="center"):
    """One row per (label, drawable, readout): the label, the drawable at ``anchor`` and an optional readout.

    Without a third column the readouts are left out. The drawables fade in
    together, then hold.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(columns), column_gap="48px", row_gap="24px", padding="page_padding")
    )
    width = len(columns)
    drawables = []
    for index, (code, drawable, value) in enumerate(rows):
        label(code).place(grid[width * index], anchor="right")
        drawables.append(drawable.place(grid[width * index + 1], anchor=anchor))
        if value is not None and width > 2:
            readout(value).place(grid[width * index + 2], anchor="left")
    scene.play([drawable.animate.fade_in() for drawable in drawables], duration=GROW)
    scene.wait(HOLD)


# Text roles take the theme's fonts, sizes and styles: Libertinus Serif for
# prose and DejaVu Sans Mono for code, the labels included.
scene.segment("typography")
SAMPLES = [
    ("title", "Tide tables"),
    ("subtitle", "Low water at dawn"),
    ("heading", "Field notes"),
    ("body", "The bay empties twice a day."),
    ("caption", "Measured from the north pier"),
    ("label", "HIGH WATER"),
    ("code", "tide.level(t=6.2)"),
    ("math", "$h(t) = A cos(omega t)$"),
]
table(
    [(f'role="{role}"', scene.text(sample, role=role), None) for role, sample in SAMPLES],
    columns=("2fr", "3fr"),
    anchor="left",
)

# A family rule styles every drawable of the family, a factory rule only that
# factory's drawables, and a class rule the drawables that ask for it. Each
# readout is the rule that decides the look; labels omit scene.geometry.
scene.segment("selectors")
table([
    ("square(1.1)", scene.geometry.square(1.1), '"shape"'),
    ("rounded_rect(2.4, 1.1, 0.2)", scene.geometry.rounded_rect(2.4, 1.1, 0.2), '"shape"'),
    ("circle(0.55)", scene.geometry.circle(0.55), '"circle"'),
    ("line((0, 0), (3, 0))", scene.geometry.line((0, 0), (3, 0)), '"line"'),
    ('square(1.1).style_class("warning")', scene.geometry.square(1.1).style_class("warning"), '".warning"'),
])

# The cascade: the base theme, then the family, the factory or part, the
# classes in the order they were added, and last the drawable's own setters.
scene.segment("cascade")
table([
    ('circle(0.55).style_class("warning")', scene.geometry.circle(0.55).style_class("warning"), '".warning"'),
    (
        'square(1.1).style_class("warning")\n  .style_class("calm")',
        scene.geometry.square(1.1).style_class("warning").style_class("calm"),
        '".calm"',
    ),
    (
        'square(1.1).style_class("calm")\n  .style_class("warning")',
        scene.geometry.square(1.1).style_class("calm").style_class("warning"),
        '".warning"',
    ),
    (
        'square(1.1).style_class("warning")\n  .style_class("ghost")',
        scene.geometry.square(1.1).style_class("warning").style_class("ghost"),
        '".ghost"',
    ),
    (
        'square(1.1).style_class("warning")\n  .fill(success)',
        scene.geometry.square(1.1).style_class("warning").fill(success),
        "fill()",
    ),
])

# Charts: the axes rule styles every part of a plane, the expression_plot
# rule its curves, the first series color the chart's marks and the heatmap
# ramp the cells of a heatmap.
scene.segment("charts")
left, right = scene.layout.zones(Zones.columns(["4fr", "3fr"], gap="gutter", padding="page_padding"))
plane_caption, plane_zone = left.split(Zones.rows(["48px", "1fr"], gap="24px"))
bars_caption, bars_zone, heat_caption, heat_zone = right.split(
    Zones.rows(["48px", "1fr", "48px", "1fr"], gap="16px")
)
label("cartesian_2d(x, y).plot(math.sin)").place(plane_caption, anchor="left")
label('ChartSpec(...).mark("bar")').place(bars_caption, anchor="left")
label('ChartSpec(...).mark("heatmap")').place(heat_caption, anchor="left")
plane = scene.viz.cartesian_2d(
    Axis.linear(0, 6).ticks(1).label("t"),
    Axis.linear(-1.2, 1.2).ticks(0.6).label("h"),
    width=plane_zone.width * 0.85,
    height=plane_zone.height * 0.8,
)
plane.move_to(*plane_zone.center)
wave = plane.plot(math.sin)
LEVELS = {"day": [1, 2, 3, 4, 5, 6, 7], "level": [2.1, 3.4, 2.8, 4.6, 3.9, 5.2, 4.4]}
bars = scene.viz.chart(ChartSpec(LEVELS).mark("bar", width=0.6).encode(x="day", y="level"))
GRID = [(x, y, math.sin(x / 2) * math.cos(y / 2)) for y in range(5) for x in range(10)]
CELLS = {"x": [x for x, _, _ in GRID], "y": [y for _, y, _ in GRID], "value": [v for _, _, v in GRID]}
heat = scene.viz.chart(ChartSpec(CELLS).mark("heatmap").encode(x="x", y="y", color="value"))
for chart, zone in [(bars, bars_zone), (heat, heat_zone)]:
    chart.drawable().place(zone, fit="contain")
scene.play(
    [plane.animate.create(), bars.layer("axes").animate.create(), heat.layer("axes").animate.create()],
    duration=GROW,
)
scene.play(
    [wave.animate.create(), bars.layer("marks").animate.fade_in(), heat.layer("marks").animate.fade_in()],
    duration=GROW,
)
scene.wait(HOLD)


def spaced(gap):
    """Four small squares in a row, ``gap`` apart: a layout token or a length."""
    return scene.layout.row(*[scene.geometry.square(0.5) for _ in range(4)], gap=gap)


# Layout tokens: the built-in spacing scale, the theme's page_padding (0.6
# here, so every grid of this example sits further in) and gutter, a token
# of its own. Each readout is scene.canvas.layout_token(name).
scene.segment("layout_tokens")
TOKENS = ["space_xs", "space_sm", "space_md", "space_lg", "column_gap", "gutter", "page_padding"]
table(
    [(f'row(..., gap="{name}")', spaced(name), f"{scene.canvas.layout_token(name):g}") for name in TOKENS],
    columns=("8fr", "6fr", "2fr"),
)



def stacked(rows):
    """One row per (label, drawable): the label with the drawable under it, both left-aligned.

    The drawables fade in together, then hold.
    """
    grid = scene.layout.zones(Zones.rows(len(rows), gap="24px", padding="page_padding"))
    shown = []
    for zone, (code, drawable) in zip(grid, rows):
        top, bottom = zone.split(Zones.rows(2, gap="8px"))
        label(code).place(top, anchor="bottom_left")
        shown.append(drawable.place(bottom, anchor="top_left"))
    scene.play([drawable.animate.fade_in() for drawable in shown], duration=GROW)
    scene.wait(HOLD)


# Names and the markup default: with text_markup=False, * and _ stay as
# typed, and a text that passes markup=True styles them again.
scene.segment("names_and_markup")
table(
    [
        ("FIELD_NOTES.name", readout(repr(FIELD_NOTES.name)), None),
        ("scene.canvas.theme", readout(repr(scene.canvas.theme)), None),
        ("FIELD_NOTES.text_markup", readout(repr(FIELD_NOTES.text_markup)), None),
        ('scene.text("tide_level, *stars*")', scene.text("tide_level, *stars*"), None),
        ('scene.text("*bold*, _italic_", markup=True)', scene.text("*bold*, _italic_", markup=True), None),
    ],
    columns=("11fr", "5fr"),
    anchor="left",
)

# validate() lists the colors that do not stand out enough from the
# background, for the active theme or any other; it never rejects a theme.
# DIM derives FIELD_NOTES with darker title and muted colors; every
# built-in theme passes.
scene.segment("validate")
DIM = Theme(FIELD_NOTES, name="field-notes-dim", colors={"muted": palette.slate[600], "title": palette.slate[500]})
stacked([
    ("scene.canvas.validate_theme()", readout(repr(scene.canvas.validate_theme()))),
    ("DIM.validate()", readout("\n".join(DIM.validate()))),
    (
        "[name for name in Theme.schemes() if Theme(name).validate()]",
        readout(repr([name for name in Theme.schemes() if Theme(name).validate()])),
    ),
])

STEP = GROW + HOLD
# The charts segment draws its axes, then its marks.
STARTS = [0, STEP, 2 * STEP, 3 * STEP, 4 * STEP + GROW, 5 * STEP + GROW, 6 * STEP + GROW]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment settled, and the charts while their marks draw.
    scene.snapshots(snapshots, sorted([start + 1.8 for start in STARTS] + [3 * STEP + 1.1]))
else:
    scene.render()
