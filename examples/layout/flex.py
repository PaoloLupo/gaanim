"""Rows and columns: how a box distributes and aligns the children of one line.

``L.row`` and ``L.column`` flow their children along a main axis. ``justify``
spreads them along it, ``align`` lines them up across it, ``gap`` and
``wrap`` set the spacing and the line breaks, and each child can grow,
shrink, align itself or take margins with ``item()``. Every lane is the same
box with one rule changed, and the label is that rule.
"""

import os

from gaanim import BoxStyle, Scene, Zones, stagger

scene = Scene()
L = scene.layout
accent = scene.canvas.color("accent")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
dark = scene.canvas.color("background")
seeks = []

lane_style = BoxStyle(padding="8px", gap="8px", background=panel, border=rule)
chip = BoxStyle(padding=("4px", "32px"), min_width="160px", background=accent, color=dark, align="center",
                font_size="72px")


def chips(*names):
    """One chip per name."""
    return [L.box(name, style=chip) for name in names]


def marked(name, **rules):
    """A chip with the item rules ``rules``: the child a lane is about."""
    return L.box(name, style=chip).item(**rules)


def columns(count, split):
    """The frame cut into ``count`` rows of (label zone, example zone)."""
    grid = L.zones(Zones.grid(rows=count, columns=list(split), column_gap="48px", padding="page_padding"))
    return [(grid[2 * index], grid[2 * index + 1]) for index in range(count)]


def lane_for(split):
    """The lane style, as wide as the example column of a table with ``split``."""
    return lane_style.but(width=columns(1, split)[0][1].width)


def table(variants, split):
    """One row per (label, box): the code on the left, the box it builds on the right."""
    entrances = []
    for (label_zone, zone), (label, box) in zip(columns(len(variants), split), variants):
        code = scene.text(label, role="code").place(label_zone, anchor="right")
        box.place(zone, anchor="left")
        entrances += [code.animate.fade_in().duration(0.4), box.animate.fade_in().duration(0.4)]
    start = scene.cursor
    scene.play(stagger(*entrances, each=0.04))
    scene.wait(1.0)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


# justify spreads the children along the main axis: horizontal in a row.
scene.segment("justify")
SPLIT = ("2fr", "5fr")
lane = lane_for(SPLIT)
table([
    (f'justify="{justify}"', L.row(*chips("1", "2", "3"), style=lane, justify=justify))
    for justify in ("start", "center", "end", "between", "around", "evenly")
], SPLIT)

# align lines them up across it. Chips of three text sizes show "baseline",
# and "stretch" makes each one as tall as the line.
scene.segment("align")
table([
    (f'align="{align}"', L.row(
        L.box("Ag", style=chip, font_size="48px"),
        L.box("Ag", style=chip, font_size="80px"),
        L.box("Ag", style=chip, font_size="120px"),
        style=lane, height="176px", align=align,
    ))
    for align in ("start", "center", "end", "stretch", "baseline")
], SPLIT)

# gap separates the children; a column flows top to bottom.
scene.segment("gap")
SPLIT = ("2fr", "3fr")
lane = lane_for(SPLIT)
table([
    ('L.row(..., gap="8px")', L.row(*chips("1", "2", "3"), style=lane, gap="8px")),
    ('L.row(..., gap="64px")', L.row(*chips("1", "2", "3"), style=lane, gap="64px")),
    ('L.column(..., gap="8px")', L.column(*chips("1", "2", "3"), style=lane, gap="8px", align="start")),
], SPLIT)

# wrap=True breaks a row that does not fit into more lines; row_gap spaces
# the lines and justify places each line.
scene.segment("wrap")
WORDS = ["layout", "boxes", "rows", "grids", "zones", "gap", "wrap", "align", "justify", "grow"]
small = chip.but(font_size="64px", padding=("4px", "16px"), min_width=0)
table([
    ("wrap=True", L.row(*[L.box(word, style=small) for word in WORDS], style=lane, wrap=True)),
    ('wrap=True, row_gap="32px"', L.row(*[L.box(word, style=small) for word in WORDS], style=lane, wrap=True,
                                        row_gap="32px")),
    ('wrap=True, justify="center"', L.row(*[L.box(word, style=small) for word in WORDS], style=lane, wrap=True,
                                          justify="center")),
], SPLIT)

# grow shares the free space by weight; width="fill" is grow=1.
scene.segment("grow")
SPLIT = ("4fr", "5fr")
lane = lane_for(SPLIT)
table([
    ("item(grow=1)", L.row(*chips("hug"), marked("grow=1", grow=1), *chips("hug"), style=lane)),
    ("item(grow=1), item(grow=2)", L.row(marked("grow=1", grow=1), marked("grow=2", grow=2), style=lane)),
    ('width="fill"', L.row(*chips("hug"), L.box("fill", style=chip, width="fill"), style=lane)),
], SPLIT)

# basis is the size before growing or shrinking: a length or a share of
# the line. When the line is too full, children shrink, except those with
# shrink=0.
scene.segment("basis")
table([
    ('item(basis="320px")', L.row(marked("320px", basis="320px"), *chips("hug", "hug"), style=lane)),
    ('item(basis="40%")', L.row(marked("40%", basis="40%"), *chips("hug", "hug"), style=lane)),
    ('item(basis="400px")', L.row(*[marked("400px", basis="400px") for _ in range(3)], style=lane)),
    ('item(basis="400px", shrink=0)', L.row(marked("shrink=0", basis="400px", shrink=0),
                                            *[marked("400px", basis="400px") for _ in range(2)], style=lane)),
], SPLIT)

# A child can align itself, take margins ("auto" takes the free space), or
# leave the flow with absolute=True and sit by an anchor.
scene.segment("items")
SPLIT = ("9fr", "7fr")
lane = lane_for(SPLIT)
table([
    ('item(align_self="end")', L.row(*chips("1"), marked("end", align_self="end"), *chips("3"),
                                     style=lane, height="184px", align="start")),
    ('item(margin="32px")', L.row(*chips("1"), marked("32px", margin="32px"), *chips("3"), style=lane)),
    ('item(margin=(0, 0, 0, "auto"))', L.row(*chips("1", "2"), marked("auto", margin=(0, 0, 0, "auto")), style=lane)),
    ('item(margin=(0, "auto"))', L.row(*chips("1"), marked("auto", margin=(0, "auto")), *chips("3"), style=lane)),
    ('item(absolute=True, anchor="right")', L.row(*chips("1", "2", "3"), marked("badge", absolute=True, anchor="right"),
                                                  style=lane)),
], SPLIT)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
