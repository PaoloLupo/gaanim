"""Reusable box styles: BoxStyle, classes, inherited typography and components.

A BoxStyle groups box properties like a CSS rule; classes name them for the
whole scene, and a box's typography reaches every string it holds. A
component wraps a whole box tree in a function with checked arguments. Each
label is the code that builds the box beside it.
"""

import os

from gaanim import BoxStyle, Scene, Zones, component, stagger

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
header = scene.canvas.color("header")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")
dark = scene.canvas.color("background")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")
danger = scene.canvas.color("danger")
seeks = []


def columns(count, split):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1]) for index in range(count)]


def example_width(split):
    """The width of the example column of a table with ``split``, in scene units."""
    return columns(1, split)[0][1].width


def table(variants, split):
    """One row per (label, box): the code on the left, the box it builds on the right."""
    entrances = []
    for (label_zone, zone), (label, box) in zip(columns(len(variants), split), variants):
        code = scene.text(label, role="code", markup=False).place(label_zone, anchor="right")
        box.place(zone, anchor="left")
        entrances += [code.animate.fade_in().duration(0.4), box.animate.fade_in().duration(0.4)]
    start = scene.cursor
    scene.play(stagger(*entrances, each=0.04))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


# style= applies a BoxStyle, and .but() derives a copy with some properties
# changed.
scene.segment("style")
tag = BoxStyle(padding=("12px", "48px"), radius="12px", background=header, font_size="72px")
table(
    [
        ("style=tag", L.box("Draft", style=tag)),
        ("style=tag.but(border=accent)", L.box("Draft", style=tag.but(border=accent))),
        ('style=tag.but(radius="full")', L.box("Draft", style=tag.but(radius="full"))),
    ],
    ("1fr", "1fr"),
)

# classes() names styles for class_=. Classes apply first (the last name
# wins), then style=, then the properties written in the call.
scene.segment("classes")
L.classes(
    pill=BoxStyle(padding=("12px", "48px"), radius="full", background=header, font_size="72px"),
    ok=BoxStyle(background=success, color=dark),
)
warn = BoxStyle(background=warning, color=dark)
table(
    [
        ('class_="pill"', L.box("Draft", class_="pill")),
        ('class_="pill ok"', L.box("Ready", class_="pill ok")),
        ('class_="pill ok", style=warn', L.box("Review", class_="pill ok", style=warn)),
        ('class_="pill", style=warn, background=danger', L.box("Blocked", class_="pill", style=warn,
                                                                background=danger)),
    ],
    ("2fr", "1fr"),
)

# Typography set on a box reaches the strings it holds, which become text.
scene.segment("typography")
SPLIT = ("1fr", "1fr")
SENTENCE = "Boxes set type"
body = BoxStyle(font_size="64px")
table(
    [
        ('font_size="96px"', L.box(SENTENCE, style=body, font_size="96px")),
        ("weight=700, color=accent", L.box(SENTENCE, style=body, weight=700, color=accent)),
        ('italic=True, letter_spacing="8px"', L.box(SENTENCE, style=body, italic=True, letter_spacing="8px")),
        ('role="code"', L.box(SENTENCE, style=body, role="code")),
        ("markup=True", L.box("A *strong* word and $x^2$", style=body, markup=True)),
    ],
    SPLIT,
)

# A box narrower than its text wraps it; text_align and line_spacing shape
# the lines.
scene.segment("wrapping")
para = BoxStyle(width=example_width(SPLIT), padding=("8px", "16px"), border=rule, font_size="56px")
PARAGRAPH = "A box wraps the text it holds to its own width, line by line"
table(
    [
        ('text_align="left"', L.box(PARAGRAPH, style=para, text_align="left")),
        ('text_align="center"', L.box(PARAGRAPH, style=para, text_align="center")),
        ('text_align="right"', L.box(PARAGRAPH, style=para, text_align="right")),
        ('text_align="justify"', L.box(PARAGRAPH, style=para, text_align="justify")),
        ("line_spacing=1.5", L.box(PARAGRAPH, style=para, line_spacing=1.5)),
    ],
    SPLIT,
)

# text_box chooses the box a text is measured and placed by: whole lines,
# from the capital height to the baseline, or the ink of its glyphs.
scene.segment("text_box")
sample = BoxStyle(background=header, font_size="176px")
table(
    [
        ('text_box="line"', L.box("Typography", style=sample, text_box="line")),
        ('text_box="cap"', L.box("Typography", style=sample, text_box="cap")),
        ('text_box="ink"', L.box("Typography", style=sample, text_box="ink")),
    ],
    ("2fr", "3fr"),
)


# A component is a function that returns a box; @component checks its
# keyword slots and every call builds new objects.
@component
def kpi(scene, *, label: str, value: str, up: bool = True):
    return scene.layout.column(
        scene.layout.box(label, color=muted, font_size="48px"),
        scene.layout.box(value, font_size="104px", weight=700, color=success if up else danger),
        padding=("16px", "40px"), radius="20px", background=header, width="480px",
    )


scene.segment("components")
table(
    [
        ('kpi(scene, label="Visits", value="48.2k")', kpi(scene, label="Visits", value="48.2k")),
        ('kpi(scene, label="Sign-ups", value="1 284")', kpi(scene, label="Sign-ups", value="1 284")),
        ('kpi(scene, label="Bounce", value="31%", up=False)', kpi(scene, label="Bounce", value="31%", up=False)),
    ],
    ("5fr", "2fr"),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
