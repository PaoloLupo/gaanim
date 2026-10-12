"""Writing text: write() by unit and order, its stagger, and the chalk brush.

``write`` traces each glyph's outline and then fills it. ``by`` groups the
glyphs that start together, ``order`` picks which group goes first,
``stagger`` sets the lag between groups, and ``brush="chalk"`` (or
``Drawable.chalk``) draws a trembling, grainy chalk line. Each label is the
call that writes the sample next to it.
"""

import os

from gaanim import Scene, Zones, part

DURATION = 1.6
HOLD = 0.6
SAMPLE = "Sphinx of black quartz"
SIZE = 0.5

scene = Scene()


def sample():
    return scene.text(SAMPLE, size=SIZE)


def table(rows, columns=("1fr", "1fr")):
    """Write one sample per row, with the call that writes it as the label.

    ``rows`` are (label, text, write) triples: ``write(text)`` returns the
    sample's animation. ``columns`` are the two column tracks.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(columns), column_gap="48px", padding="page_padding")
    )
    for index, (code, text, _) in enumerate(rows):
        scene.text(code, role="code", markup=False).place(grid[2 * index], anchor="right")
        text.place(grid[2 * index + 1], anchor="left")
    scene.play([write(text).duration(DURATION) for _, text, write in rows])
    scene.wait(HOLD)


# Glyphs of one grapheme, word, explicit line or part start together.
scene.segment("units")
table([
    ('write(by="grapheme")', sample(), lambda text: text.animate.write(by="grapheme")),
    ('write(by="word")', sample(), lambda text: text.animate.write(by="word")),
    ('write(by="line")', scene.text("Sphinx of black\nquartz, judge my vow", size=SIZE),
     lambda text: text.animate.write(by="line")),
    ('write(by="part")', scene.text(part("who", "Sphinx"), " of ", part("what", "black quartz"), size=SIZE),
     lambda text: text.animate.write(by="part")),
])

# order picks the first group: forward, reverse, from the center outward,
# or a fixed random permutation.
scene.segment("order")
table(
    [
        (f'write(by="word", order="{order}")', sample(),
         lambda text, order=order: text.animate.write(by="word", order=order))
        for order in ["forward", "reverse", "center", "random"]
    ],
    columns=("4fr", "3fr"),
)

# stagger is the lag ratio between groups: None adapts it, 0 starts every
# group at once and 1 starts each one when the previous one ends.
scene.segment("stagger")
table([
    (f'write(by="word", stagger={stagger})', sample(),
     lambda text, stagger=stagger: text.animate.write(by="word", stagger=stagger))
    for stagger in [None, 0.0, 0.3, 1.0]
])

# brush="chalk" writes with a chalk line whose tremble and grain follow seed;
# Drawable.chalk(seed, roughness) sets the same look before any write.
scene.segment("chalk")
EQUATION = "e^(i pi) + 1 = 0"
BIG = 0.8
table(
    [
        ('write(brush="pen")', scene.text.equation(EQUATION, size=BIG), lambda text: text.animate.write(brush="pen")),
        ('write(brush="chalk", seed=3)', scene.text.equation(EQUATION, size=BIG),
         lambda text: text.animate.write(brush="chalk", seed=3)),
        ('write(brush="chalk", seed=8)', scene.text.equation(EQUATION, size=BIG),
         lambda text: text.animate.write(brush="chalk", seed=8)),
        (".chalk(seed=3, roughness=0)", scene.text("black quartz", size=BIG).chalk(seed=3, roughness=0),
         lambda text: text.animate.write()),
        (".chalk(seed=3)", scene.text("black quartz", size=BIG).chalk(seed=3), lambda text: text.animate.write()),
        (".chalk(seed=3, roughness=0.05)", scene.text("black quartz", size=BIG).chalk(seed=3, roughness=0.05),
         lambda text: text.animate.write()),
    ],
    columns=("1fr", "1fr"),
)

STEP = DURATION + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Early, mid-write and settled in each of the four segments.
    scene.snapshots(snapshots, [STEP * index + t for index in range(4) for t in (0.4, 0.9, 2.0)])
else:
    scene.render()
