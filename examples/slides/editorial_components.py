"""Editorial components: badges, chips, cards, bullet lists and callouts from scene.slides.

Each component measures its own text and returns one group that is placed and
animated like any drawable. They share five semantic variants and three
appearances, taken from the theme. Each label is the code that builds the
component beside it.
"""

import os

from gaanim import Easing, Scene, TextStyle, Zones, stagger

scene = Scene()
S = scene.slides
accent = scene.colors.accent
chart = scene.colors.chart
dark = scene.colors.background
foreground = scene.colors.foreground
muted = scene.colors.muted
rule = scene.colors.rule
seeks = []

# Badges and chips are shown at ZOOM times their size, and cards at
# CARD_ZOOM times.
ZOOM = 1.6
CARD_ZOOM = 1.2
VARIANTS = ["neutral", "accent", "success", "warning", "danger"]
APPEARANCES = ["soft", "solid", "outline"]


def code(label):
    return scene.text(label, role="code")


def show(*objects, hold=1.0):
    """Fade the objects in one after another, hold, and note the seeks."""
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in objects], each=0.04))
    scene.wait(hold)
    # Mid-entrance and settled.
    seeks.extend([start + 0.3, scene.cursor - 0.1])


def matrix(corner, rows, columns, build):
    """One component per (row, column) cell, under a header of column labels.

    ``rows`` and ``columns`` are (label, value) pairs; ``corner`` names the
    keyword the column labels are values of. ``build(row, column)`` makes
    the component of each cell.
    """
    tracks = ["4fr"] + ["3fr"] * len(columns)
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + 1, columns=tracks, column_gap="48px", padding="page_padding")
    )
    shown = [code(corner).place(grid[0], anchor="right")]
    shown += [code(label).place(grid[index]) for index, (label, _) in enumerate(columns, start=1)]
    for row_index, (label, row) in enumerate(rows, start=1):
        first = row_index * len(tracks)
        shown.append(code(label).place(grid[first], anchor="right"))
        for index, (_, column) in enumerate(columns, start=1):
            shown.append(build(row, column).scale_by(ZOOM).place(grid[first + index]))
    show(*shown)


def table(variants, split=("1fr", "1fr"), zoom=1.0):
    """One row per (label, component): the code on the left, the component on the right.

    The component is shown at ``zoom`` times its size.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=list(split), column_gap="48px", padding="page_padding")
    )
    shown = []
    for index, (label, component) in enumerate(variants):
        shown += [code(label).place(grid[2 * index], anchor="right"),
                  component.scale_by(zoom).place(grid[2 * index + 1], anchor="left")]
    show(*shown)
    return grid


# Every component takes variant= (a semantic tone of the theme) and
# appearance= (a soft fill, a solid fill or only a border).
scene.segment("badges")
WORDS = {"neutral": "DRAFT", "accent": "NEW", "success": "PASSED", "warning": "REVIEW", "danger": "FAILED"}
matrix(
    "appearance=",
    [(f'variant="{variant}"', variant) for variant in VARIANTS],
    [(f'"{appearance}"', appearance) for appearance in APPEARANCES],
    lambda variant, appearance: S.badge(WORDS[variant], variant=variant, appearance=appearance),
)

# A chip is a compact badge with a dot of its variant's color; dot=False
# leaves it out.
scene.segment("chips")
STATES = {"neutral": "Idle", "accent": "Beta", "success": "Online", "warning": "Pending", "danger": "Offline"}
matrix(
    "appearance=",
    [(f'variant="{variant}"', variant) for variant in VARIANTS],
    [(f'"{appearance}"', {"appearance": appearance}) for appearance in APPEARANCES]
    + [("dot=False", {"dot": False})],
    lambda variant, options: S.chip(STATES[variant], variant=variant, **options),
)

# Badges fit their text: radius=None makes a pill, padding and min_width
# size the panel, and explicit colors replace the variant's.
scene.segment("badge_shapes")
table([
    ('badge("v2.4")', S.badge("v2.4")),
    ('badge("v2.4", radius=0.04)', S.badge("v2.4", radius=0.04)),
    ('badge("v2.4", padding=(0.5, 0.2))', S.badge("v2.4", padding=(0.5, 0.2))),
    ('badge("v2.4", min_width=2.5)', S.badge("v2.4", min_width=2.5)),
    ('badge("v2.4", color=dark, background=chart)', S.badge("v2.4", color=dark, background=chart)),
    ('badge("v2.4", border=foreground)', S.badge("v2.4", border=foreground)),
], split=("2fr", "1fr"), zoom=ZOOM)

# The label takes the theme's label role; font_size, weight, style and font
# change it, and markup=False keeps * and _ literal. Chips take the same
# options.
scene.segment("badge_text")
table([
    ('badge("v2.4", font_size=0.5)', S.badge("v2.4", font_size=0.5)),
    ('badge("v2.4", weight=800)', S.badge("v2.4", weight=800)),
    ('badge("v2.4", style=TextStyle(italic=True))', S.badge("v2.4", style=TextStyle(italic=True))),
    ('badge("v2.4", font="JetBrains Mono")', S.badge("v2.4", font="JetBrains Mono")),
    ('badge("_max_ speed")', S.badge("_max_ speed")),
    ('badge("_max_ speed", markup=False)', S.badge("_max_ speed", markup=False)),
], split=("2fr", "1fr"), zoom=ZOOM)

# A card stacks a title, a body that wraps to width= and an optional
# footer, each in its own text role; its height follows the text.
TITLE, BODY, FOOTER = "Result", "Converged in twelve steps.", "12 ms per step"
scene.segment("cards")
table([
    ("card(TITLE, BODY)", S.card(TITLE, BODY)),
    ('card(TITLE, BODY, FOOTER, variant="accent")', S.card(TITLE, BODY, FOOTER, variant="accent")),
    ("card(TITLE, BODY, width=3.2, radius=0)", S.card(TITLE, BODY, width=3.2, radius=0)),
], split=("9fr", "5.3fr"), zoom=CARD_ZOOM)

# A stat card shows a value with its label and an optional delta, both in
# the variant's tone.
scene.segment("stat_cards")
table([
    ('stat_card("98%", "Accuracy")', S.stat_card("98%", "Accuracy")),
    ('stat_card("98%", "Accuracy", delta="+4.2%")', S.stat_card("98%", "Accuracy", delta="+4.2%")),
    ('stat_card(..., variant="success", appearance="solid")',
     S.stat_card("98%", "Accuracy", delta="+4.2%", variant="success", appearance="solid")),
], split=("11.1fr", "3.5fr"), zoom=CARD_ZOOM)

# A quote card wraps the quotation in typographic quotes and sets the
# attribution apart on the right.
QUOTE = "Simplicity is prerequisite for reliability."
scene.segment("quote_cards")
table([
    ("quote_card(QUOTE)", S.quote_card(QUOTE)),
    ('quote_card(QUOTE, "Edsger W. Dijkstra")', S.quote_card(QUOTE, "Edsger W. Dijkstra")),
    ('quote_card(..., variant="warning")', S.quote_card(QUOTE, "Edsger W. Dijkstra", variant="warning")),
], split=("8fr", "6.4fr"))

# A bullet list is one object: gap spaces the items, and bullet_radius,
# bullet_color and color restyle the marks and the text.
ITEMS = ["Measure", "Model", "Verify"]
scene.segment("bullets")
table([
    ("bullets(ITEMS, width=4)", S.bullets(ITEMS, width=4)),
    ("bullets(ITEMS, width=4, gap=0.45)", S.bullets(ITEMS, width=4, gap=0.45)),
    ("bullets(ITEMS, width=4, bullet_radius=0.14)", S.bullets(ITEMS, width=4, bullet_radius=0.14)),
    ("bullets(..., bullet_color=chart, color=muted)",
     S.bullets(ITEMS, width=4, bullet_color=chart, color=muted)),
], split=("13fr", "7fr"))

# A callout is a card joined to its target by a line; it keeps its offset
# from the target as the target moves, and starts hidden until it fades in.
scene.segment("callouts")
CALLOUTS = [
    ('callout("Peak", dot)', {}),
    ('callout("Peak", dot, offset=(2.2, 0))', {"offset": (2.2, 0)}),
    ('callout("Peak", dot, width=1.6, height=0.9)', {"width": 1.6, "height": 0.9}),
    ("callout(..., background=accent, color=dark)", {"background": accent, "color": dark}),
]
grid = scene.layout.zones(
    Zones.grid(rows=len(CALLOUTS), columns=["2fr", "1fr"], column_gap="48px", padding="page_padding")
)
shown, moves, notes = [], [], []
for index, (label, options) in enumerate(CALLOUTS):
    lane = grid[2 * index + 1]
    # The dot runs low in its row, leaving room for a card above it.
    (start, y), (end, _) = lane.point("left"), lane.point("right")
    y -= 0.5
    track = scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    dot = scene.geometry.dot(0.12).move_to(start, y)
    notes.append(S.callout("Peak", dot, **options))
    shown += [code(label).place(grid[2 * index], anchor="right"), track, dot]
    moves.append(dot.animate.move_to(start + 1.6, y).duration(1.4).easing(Easing.SMOOTH))
start = scene.cursor
scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in shown], each=0.04))
scene.play([*moves, *[note.animate.fade_in().duration(0.4) for note in notes]])
scene.wait(0.6)
# Before the move, mid-move and settled.
seeks.extend([start + 0.6, scene.cursor - 1.2, scene.cursor - 0.1])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
