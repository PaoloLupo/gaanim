"""Typography: text roles, style, spacing, parts, inline markup, paragraphs, anchors and selections.

Each segment covers one part of ``scene.text``: the theme's text roles, the
style keywords, spacing, locally styled parts, a root style, inline markup
and math, paragraph alignment, wrapping and line limits, anchors, and
selections that restyle part of a text. Each label is the code that styles
the sample next to it.
"""

import os

from gaanim import Anchor, Scene, TextAnchor, TextFlow, TextStyle, Zones, part

WRITE = 0.8
HOLD = 1.2
GAP = 0.2
SAMPLE = "Sphinx of black quartz"
PARAGRAPH = "Typography arranges type so that written language is legible, readable and appealing."

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
rule = scene.colors.rule
background = scene.colors.background


def label(code):
    """A variant label: the code, in the code role, which reads literally."""
    return scene.text(code, role="code")


def table(rows, columns=("1fr", "1fr"), then=()):
    """Write one sample per row with its label to the left, then play ``then``.

    ``rows`` are (label, text) pairs and ``columns`` the two column tracks.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(columns), column_gap="48px", padding="page_padding")
    )
    for index, (code, text) in enumerate(rows):
        label(code).place(grid[2 * index], anchor="right")
        text.place(grid[2 * index + 1], anchor="left")
    scene.play([text.animate.write(by="word").duration(WRITE) for _, text in rows])
    if then:
        scene.play(list(then))
    scene.wait(HOLD)


def paragraphs(variants):
    """Write a 2 x 2 grid of paragraphs, each under its label; ``variants`` are (label, text) pairs."""
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, column_gap="96px", padding="page_padding"))
    for cell, (code, text) in zip(grid, variants):
        name = label(code)
        # A band as tall as the label, a gap and the paragraph, centered in the cell.
        height = name.bounds().height + GAP + text.bounds().height
        band = cell.inset(((cell.height - height) / 2, 0))
        name.place(band, anchor="top_left")
        text.place(band, anchor="bottom_left")
    scene.play([text.animate.write(by="line").duration(WRITE) for _, text in variants])
    scene.wait(HOLD)


# Each role sets a size, a face and a color from the theme.
scene.segment("roles")
ROLES = ["title", "subtitle", "kicker", "heading", "body", "caption", "label"]
table(
    [(f'role="{role}"', scene.text(SAMPLE, role=role)) for role in ROLES]
    + [
        ('role="code"', scene.text("judge(vow, sphinx=True)", role="code")),
        ('role="math"', scene.text("$a^2 + b^2 = c^2$", role="math")),
    ],
    columns=("4fr", "7fr"),
)

# Keywords override the role. The bundled faces are New Computer Modern,
# JetBrains Mono (every weight from 100 to 800) and Libertinus Serif.
scene.segment("style")
table([
    ("size=0.56", scene.text(SAMPLE, size=0.56)),
    ("weight=700", scene.text(SAMPLE, weight=700)),
    ("italic=True", scene.text(SAMPLE, italic=True)),
    ("color=accent", scene.text(SAMPLE, color=accent)),
    ("opacity=0.4", scene.text(SAMPLE, opacity=0.4)),
    ('font="Libertinus Serif"', scene.text(SAMPLE, font="Libertinus Serif")),
    ('font="JetBrains Mono", weight=200', scene.text("judge(vow)", font="JetBrains Mono", weight=200)),
    ('font="JetBrains Mono", weight=800', scene.text("judge(vow)", font="JetBrains Mono", weight=800)),
])

# Spacing is in scene units. tracking() spreads the laid-out glyphs without a
# new layout, and a TextStyle bundles keywords to reuse.
scene.segment("spacing")
WIDE = TextStyle(italic=True, letter_spacing=0.04, word_spacing=0.2)
table([
    ("letter_spacing=0.08", scene.text(SAMPLE, letter_spacing=0.08)),
    ("word_spacing=0.3", scene.text(SAMPLE, word_spacing=0.3)),
    (".tracking(0.12)", scene.text(SAMPLE).tracking(0.12)),
    ("style=WIDE", scene.text(SAMPLE, style=WIDE)),
])

# part() names a piece of the content and can style it locally; a baseline
# raises or lowers it within its line.
scene.segment("parts")
UNDERLINE = TextStyle(decorations=["underline"])
STRIKE = TextStyle(decorations=["strike"])
table(
    [
        ('part("v", "vow", color=accent)', scene.text("Judge my ", part("v", "vow", color=accent))),
        ('part("v", "vow", weight=700)', scene.text("Judge my ", part("v", "vow", weight=700))),
        ('part("v", "vow", italic=True)', scene.text("Judge my ", part("v", "vow", italic=True))),
        ('part("v", "vow", size=0.26)', scene.text("Judge my ", part("v", "vow", size=0.26))),
        ('part("v", "vow", style=UNDERLINE)', scene.text("Judge my ", part("v", "vow", style=UNDERLINE))),
        ('part("v", "vow", style=STRIKE)', scene.text("Judge my ", part("v", "vow", style=STRIKE))),
        ('part("n", "2", size=0.26, baseline=0.14)', scene.text("x", part("n", "2", size=0.26, baseline=0.14))),
        ('part("n", "2", size=0.26, baseline=-0.08)', scene.text("x", part("n", "2", size=0.26, baseline=-0.08))),
    ],
    columns=("3fr", "2fr"),
)

# The same TextStyle as the root style covers the whole text. A stroke
# outlines the glyphs, 0.02 wide unless stroke_width says otherwise: filled
# with the background, bold glyphs read as hollow.
scene.segment("root_style")
OUTLINE = TextStyle(size=0.64, weight=700, color=background, stroke=accent)
table(
    [
        ("style=UNDERLINE", scene.text(SAMPLE, style=UNDERLINE)),
        ("style=STRIKE", scene.text(SAMPLE, style=STRIKE)),
        ("style=OUTLINE", scene.text(SAMPLE, style=OUTLINE)),
    ],
    columns=("1fr", "2fr"),
)

# *strong* and _emphasis_ are markup and $...$ is math; markup=False keeps
# * and _ literal, \$ is a literal dollar, and the code role shows every
# character as typed.
scene.segment("markup")
table([
    ('"_emphasis_ and *strong*"', scene.text("_emphasis_ and *strong*")),
    ('"*_both at once_*"', scene.text("*_both at once_*")),
    (r'r"literal \* and \_"', scene.text(r"literal \* and \_")),
    ('"x_1 * y_2", markup=False', scene.text("x_1 * y_2", markup=False)),
    ('"the area $pi r^2$ of a disc"', scene.text("the area $pi r^2$ of a disc")),
    ('"$ gaanim init", role="code"', scene.text("$ gaanim init", role="code")),
    ('"stagger(*grows)", role="code"', scene.text("stagger(*grows)", role="code")),
])

# A paragraph wraps at a width in scene units and aligns its lines.
scene.segment("alignment")
paragraphs([
    (f'wrap=5, text_align="{align}"', scene.text(PARAGRAPH, wrap=5, text_align=align))
    for align in ["left", "center", "right", "justify"]
])

# Narrow paragraphs can hyphenate; line_spacing is in multiples of the size.
# A TextFlow bundles these keywords to reuse.
LOOSE = TextFlow(wrap=6.5, line_spacing=1.5)
scene.segment("wrapping")
paragraphs([
    ("wrap=3", scene.text(PARAGRAPH, wrap=3)),
    ("wrap=3, hyphenate=True", scene.text(PARAGRAPH, wrap=3, hyphenate=True)),
    ("wrap=6.5", scene.text(PARAGRAPH, wrap=6.5)),
    ("flow=LOOSE", scene.text(PARAGRAPH, flow=LOOSE)),
])

# max_lines limits a paragraph: overflow="clip" (the default) keeps what
# fits, "ellipsis" ends it with an ellipsis and "visible" shows every line.
scene.segment("line_limits")
paragraphs([
    ("wrap=5, max_lines=2", scene.text(PARAGRAPH, wrap=5, max_lines=2)),
    ('wrap=5, max_lines=2,\noverflow="ellipsis"', scene.text(PARAGRAPH, wrap=5, max_lines=2, overflow="ellipsis")),
    ('wrap=5, max_lines=1,\noverflow="ellipsis"', scene.text(PARAGRAPH, wrap=5, max_lines=1, overflow="ellipsis")),
    ('wrap=5, max_lines=2,\noverflow="visible"', scene.text(PARAGRAPH, wrap=5, max_lines=2, overflow="visible")),
])

# move_to(x, y, anchor) puts the anchor on the dot. A TextAnchor sits on the
# first baseline, so mixed sizes and math share one; a geometric Anchor uses
# the bounds, and a left or right one also aligns the lines of a multiline text.
scene.segment("anchors")
ANCHORS = [
    ("TextAnchor.BASELINE_LEFT", TextAnchor.BASELINE_LEFT),
    ("TextAnchor.BASELINE_CENTER", TextAnchor.BASELINE_CENTER),
    ("TextAnchor.BASELINE_RIGHT", TextAnchor.BASELINE_RIGHT),
    ("Anchor.LEFT", Anchor.LEFT),
    ("Anchor.CENTER", Anchor.CENTER),
    ("Anchor.RIGHT", Anchor.RIGHT),
]
grid = scene.layout.zones(
    Zones.grid(rows=len(ANCHORS), columns=["2fr", "3fr"], column_gap="48px", padding="page_padding")
)
anchored = []
for index, (code, anchor) in enumerate(ANCHORS):
    label(code).place(grid[2 * index], anchor="right")
    zone = grid[2 * index + 1]
    x, y = zone.center
    if isinstance(anchor, TextAnchor):
        # The baseline, across the row.
        scene.geometry.line(zone.left, y, zone.right, y).stroke(rule, 0.015)
        text = scene.text("Large, ", part("s", "small", size=0.24), " and $x^2/2$", size=0.56)
    else:
        # The anchor's vertical, through the lines.
        scene.geometry.line(x, zone.bottom, x, zone.top).stroke(rule, 0.015)
        text = scene.text("Two lines\nof uneven length")
    scene.geometry.dot(0.07).move_to(x, y)
    anchored.append(text.move_to(x, y, anchor))
scene.play([text.animate.write(by="word").duration(WRITE) for text in anchored])
scene.wait(HOLD)

# Parts, words, graphemes, lines and literal text select glyphs to restyle;
# selection.animate changes their fill or opacity over time.
scene.segment("selections")
by_part = scene.text("Energy is ", part("mass", "mass"), " times light squared")
by_part["mass"].fill(accent)
nested = scene.text("Newton: ", part("law", "$F = m ", part("a", "a"), "$"))
nested["law"]["a"].fill(accent)
by_words = scene.text(SAMPLE + ", judge my vow")
by_words.words[2:4].fill(accent)
by_grapheme = scene.text(SAMPLE)
by_grapheme.graphemes[0].fill(accent)
by_line = scene.text("Sphinx of\nblack quartz")
by_line.lines[1].fill(accent)
literal = scene.text(SAMPLE)
literal["quartz"].fill(accent)
recolored = scene.text(SAMPLE + ", judge my vow")
faded = scene.text(SAMPLE + ", judge my vow")
table(
    [
        ('text["mass"].fill(accent)', by_part),
        ('text["law"]["a"].fill(accent)', nested),
        ("text.words[2:4].fill(accent)", by_words),
        ("text.graphemes[0].fill(accent)", by_grapheme),
        ("text.lines[1].fill(accent)", by_line),
        ('text["quartz"].fill(accent)', literal),
        ('text["vow"].animate.fill(success)', recolored),
        ('text["black"].animate.opacity(0.2)', faded),
    ],
    then=[
        recolored["vow"].animate.fill(success).duration(0.6),
        faded["black"].animate.opacity(0.2).duration(0.6),
    ],
)

STEP = WRITE + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-write in the first segment, then each segment settled.
    scene.snapshots(snapshots, [0.4] + [STEP * index + 1.6 for index in range(10)] + [10 * STEP + 2.2])
else:
    scene.render()
