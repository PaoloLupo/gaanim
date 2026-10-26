"""Text reveals: masked slides, fades, scale and blur, by unit, plus blur-in, tracking and exits.

``reveal`` brings a text in unit by unit (graphemes, words, explicit lines or
parts) with one of five styles, ``blur_in`` and ``tracking`` focus a title,
a selection reveals one term of a text already on screen, and ``conceal`` and
``unwrite`` take text away. Each label is the call that animates the sample
next to it.
"""

import os

from gaanim import Scene, Zones, part

DURATION = 1.2
HOLD = 0.6
SAMPLE = "Sphinx of black quartz"
SHORT = "black quartz"
SIZE = 0.5

scene = Scene()
accent = scene.colors.accent


def sample():
    return scene.text(SAMPLE, size=SIZE)


def table(rows, columns=("1fr", "1fr"), wait=0.0):
    """Animate one sample per row, with the call that animates it as the label.

    ``rows`` are (label, text, animate) triples: ``animate(text)`` returns the
    sample's animation, or a list of animations that play together.
    ``columns`` are the two column tracks, and ``wait`` holds the samples
    still before they play.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(columns), column_gap="48px", padding="page_padding")
    )
    for index, (code, text, _) in enumerate(rows):
        scene.text(code, role="code").place(grid[2 * index], anchor="right")
        text.place(grid[2 * index + 1], anchor="left")
    if wait:
        scene.wait(wait)
    anims = []
    for _, text, animate in rows:
        made = animate(text)
        anims += [anim.duration(DURATION) for anim in (made if isinstance(made, list) else [made])]
    scene.play(anims)
    scene.wait(HOLD)


# Five styles; slides rise or fall from behind a mask clipped to each row.
scene.segment("styles")
table([
    (f'reveal("{style}", by="word")', sample(), lambda text, style=style: text.animate.reveal(style, by="word"))
    for style in ["slide_up", "slide_down", "fade", "scale", "blur"]
])

# Units are graphemes, words, explicit lines or parts; mask=False slides
# a shorter way and fades in, and stagger is the delay between units.
scene.segment("units")
LINES = "Sphinx of black\nquartz, judge my vow"
PARTS = (part("who", "Sphinx"), " of ", part("what", "black quartz"))
table([
    ('reveal(by="grapheme")', sample(), lambda text: text.animate.reveal(by="grapheme", stagger=0.03)),
    ('reveal(by="word")', sample(), lambda text: text.animate.reveal(by="word")),
    ('reveal(by="line")', scene.text(LINES, size=SIZE), lambda text: text.animate.reveal(by="line", stagger=0.2)),
    ('reveal(by="part")', scene.text(*PARTS, size=SIZE), lambda text: text.animate.reveal(by="part", stagger=0.2)),
    ('reveal(by="word", mask=False)', sample(), lambda text: text.animate.reveal(by="word", mask=False)),
    ('reveal(by="word", stagger=0.25)', sample(), lambda text: text.animate.reveal(by="word", stagger=0.25)),
])

# blur_in clears a Gaussian blur unit by unit; tracking() closes up glyphs
# spread by text.tracking(...), and combines with blur_in.
scene.segment("focus")
table([
    ("blur_in()", sample(), lambda text: text.animate.blur_in()),
    ("blur_in(sigma=0.8)", sample(), lambda text: text.animate.blur_in(sigma=0.8)),
    ('blur_in(by="word", stagger=0.2)', sample(), lambda text: text.animate.blur_in(by="word", stagger=0.2)),
    ("tracking(0.0)", scene.text(SHORT, size=SIZE).tracking(0.25), lambda text: text.animate.tracking(0.0)),
    ("tracking(0.0) + blur_in()", scene.text(SHORT, size=SIZE).tracking(0.25),
     lambda text: [text.animate.tracking(0.0), text.animate.blur_in()]),
])

# On a selection, reveal brings in only those glyphs, inside a text that is
# already on screen.
scene.segment("selections")
table(
    [
        (f'text["c"].animate.reveal("{style}")', scene.text.equation("a^2 + b^2 = ", part("c", "c^2"), size=0.6),
         lambda text, style=style: text["c"].animate.reveal(style))
        for style in ["fade", "wipe", "from_below"]
    ],
    columns=("3fr", "2fr"),
    wait=0.4,
)

# conceal is the exit that matches reveal; unwrite reverses write.
scene.segment("exits")
table(
    [
        (f'conceal("{style}", by="word")', sample(), lambda text, style=style: text.animate.conceal(style, by="word"))
        for style in ["slide_up", "slide_down", "fade", "scale", "blur"]
    ]
    + [("unwrite()", sample(), lambda text: text.animate.unwrite())],
    wait=0.4,
)

STEP = DURATION + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [
        # Early, mid-reveal and settled in the first three segments ...
        *[STEP * index + t for index in range(3) for t in (0.3, 0.6, 1.5)],
        # ... and in the two that hold their text still before playing.
        *[3 * STEP + t for t in (0.2, 0.9, 1.9)],
        *[4 * STEP + 0.4 + t for t in (0.2, 1.2, 2.0)],
    ])
else:
    scene.render()
