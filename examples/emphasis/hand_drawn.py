"""Hand-drawn notes: rough boxes, circles, underlines, strikes and brackets that draw themselves.

``drawable.annotate`` returns a mark drawn as if by hand around the drawable
or a text selection; ``mark.animate.create()`` draws it on, one pass after
another, and the mark follows its target from then on. Segments cover every
kind of mark, the sides of a bracket, roughness, passes, seeds and style,
marks on moving targets, and the chalk look of a whole drawable.
"""

import os

from gaanim import Scene, Zones

DRAW = 1.2
HOLD = 0.6

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
geometry = scene.geometry


def cells(labels, rows=2, columns=3):
    """A grid of cells, each label above its stage; return the stages."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    stages = []
    for cell, label in zip(grid, labels):
        top, stage = cell.split(Zones.rows(["48px", "1fr"]))
        scene.text(label, role="code").place(top, anchor="top_left")
        stages.append(stage)
    return stages


def word(stage, text="Notice", size=0.9):
    return scene.text(text, size=size).move_to(*stage.center)


def draw(marks):
    """Draw the marks on together, then hold."""
    scene.wait(0.3)
    scene.play([mark.animate.create() for mark in marks], duration=DRAW)
    scene.wait(HOLD)


# Six kinds of mark. Without a color they take the theme's stroke; each
# label abbreviates word.annotate.<mark>.
scene.segment("kinds")
KINDS = [
    ("underline()", lambda notes: notes.underline()),
    ("box()", lambda notes: notes.box()),
    ("circle()", lambda notes: notes.circle()),
    ("strike_through()", lambda notes: notes.strike_through()),
    ("crossed_off()", lambda notes: notes.crossed_off()),
    ("bracket()", lambda notes: notes.bracket()),
]
draw([mark(word(stage).annotate) for stage, (_, mark) in zip(cells([label for label, _ in KINDS]), KINDS)])

# bracket draws square brackets on one side or several: "left", "right",
# "top" and "bottom" (left and right by default).
scene.segment("brackets")
SIDES = ['bracket("left")', 'bracket("right")', 'bracket("top")', 'bracket("bottom")', 'bracket(["top", "bottom"])',
         'bracket(["left", "top"])']
SIDE_ARGS = ["left", "right", "top", "bottom", ["top", "bottom"], ["left", "top"]]
draw([
    word(stage, "Two lines\nof notes", 0.7).annotate.bracket(sides, color=accent)
    for stage, sides in zip(cells(SIDES, rows=3, columns=2), SIDE_ARGS)
])


def table(rows):
    """Mark a word in every cell: a row per option, a column per kind of mark.

    ``rows`` are (label, options) pairs; each row draws a box, a circle and
    an underline with ``options``, so the cell under box(...) in the row
    padding=0.3 is word.annotate.box(padding=0.3).
    """
    kinds = ["box", "circle", "underline"]
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + 1, columns=["5fr", "2fr", "2fr", "2fr"], column_gap="48px", padding="page_padding")
    )
    for index, name in enumerate(kinds, start=1):
        scene.text(f"{name}(...)", role="code").place(grid[index])
    marks = []
    for row_index, (label, options) in enumerate(rows, start=1):
        first = row_index * 4
        scene.text(label, role="code").place(grid[first], anchor="right")
        for index, name in enumerate(kinds, start=1):
            target = scene.text("Notice", size=0.6).move_to(*grid[first + index].center)
            marks.append(getattr(target.annotate, name)(**options))
    draw(marks)


# roughness scales the wobble: 0 draws clean geometry, 1 (the default) the
# usual hand-drawn look. The same seed always draws the same scribble.
scene.segment("roughness")
table([
    ("roughness=0", {"roughness": 0}),
    ("roughness=0.5", {"roughness": 0.5}),
    ("roughness=1.0", {"roughness": 1.0}),
    ("roughness=2.0", {"roughness": 2.0}),
    ("roughness=3.0", {"roughness": 3.0}),
])

# passes draws the shape over itself 1 to 8 times, each with fresh
# randomness (2 by default); another seed draws another scribble.
scene.segment("passes")
table([
    ("passes=1", {"passes": 1}),
    ("passes=2", {"passes": 2}),
    ("passes=4", {"passes": 4}),
    ("passes=8", {"passes": 8}),
    ("seed=1, roughness=2.0", {"seed": 1, "roughness": 2.0}),
    ("seed=7, roughness=2.0", {"seed": 7, "roughness": 2.0}),
])

# color and width restyle the stroke (a width needs a color); padding grows
# the target's bounds first, by one value, (vertical, horizontal) or
# (top, right, bottom, left).
scene.segment("style")
table([
    ("color=accent", {"color": accent}),
    ("color=success, width=0.08", {"color": success, "width": 0.08}),
    ("padding=0.3", {"padding": 0.3}),
    ("padding=(0, 0.5)", {"padding": (0, 0.5)}),
    ("padding=(0.3, 0, 0, 0)", {"padding": (0.3, 0, 0, 0)}),
])


# A mark follows its target's bounds every frame and keeps its scribble
# while the target moves or grows; marks go on text selections too, and
# fade or restyle like any drawable.
def rows(labels):
    """One row per label, the label to the left; return each row's stage."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(labels), columns=["4fr", "3fr"], column_gap="48px", padding="page_padding")
    )
    for index, label in enumerate(labels):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
    return [grid[2 * index + 1] for index in range(len(labels))]


def card(stage):
    return geometry.rounded_rect(1.8, 0.7, 0.1).fill(panel).stroke(rule, 0.02).place(stage.inset((0, "48px")), anchor="left")


scene.segment("follow")
stages = rows([
    "card.annotate.box(color=accent)\ncard.animate.shift_by(2.5, 0)",
    "card.annotate.circle(color=accent)\ncard.animate.scale_to(1.3)",
    'line["gap"].annotate\n    .underline(color=accent)',
    "line.words[0].annotate\n    .crossed_off(color=danger)",
    "mark.animate.fade_out()",
])
moving, growing = card(stages[0]), card(stages[1])
lines = [scene.text("Mind the gap", size=0.72).place(stage.inset((0, "48px")), anchor="left") for stage in stages[2:]]
marks = [
    moving.annotate.box(color=accent),
    growing.annotate.circle(color=accent),
    lines[0]["gap"].annotate.underline(color=accent),
    lines[1].words[0].annotate.crossed_off(color=danger),
    lines[2].annotate.box(),
]
draw(marks)
scene.play([moving.animate.shift_by(2.5, 0), growing.animate.scale_to(1.3), marks[4].animate.fade_out()], duration=DRAW)
scene.wait(HOLD)

# chalk draws a whole drawable as chalk on a blackboard: a trembling outline
# up to roughness units and a grain, both following seed.
scene.segment("chalk")
CHALK = [
    ("star.chalk()", {}),
    ("star.chalk(seed=4)", {"seed": 4}),
    ("star.chalk(roughness=0.05)", {"roughness": 0.05}),
    ("star.chalk(seed=4, roughness=0.1)", {"seed": 4, "roughness": 0.1}),
]
stars = [
    geometry.star(5, 1.35, 0.6).move_to(*stage.center).chalk(**options)
    for stage, (_, options) in zip(cells([label for label, _ in CHALK], columns=2, rows=2), CHALK)
]
draw(stars)

MARKS = 0.3 + DRAW + HOLD
FOLLOW = 5 * MARKS
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Every set of marks midway and drawn; the moving targets midway and
    # settled; the chalk stars midway and drawn.
    scene.snapshots(
        snapshots,
        [MARKS * index + t for index in range(5) for t in (0.3 + DRAW / 2, MARKS - 0.1)]
        + [FOLLOW + MARKS - 0.1, FOLLOW + MARKS + DRAW / 2, FOLLOW + MARKS + DRAW + HOLD - 0.1]
        + [FOLLOW + MARKS + DRAW + HOLD + t for t in (0.3 + DRAW / 2, MARKS - 0.1)],
    )
else:
    scene.render()
