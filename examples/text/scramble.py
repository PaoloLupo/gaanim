"""Scramble: glyphs cycle through a charset and settle left to right, into the text or a new one.

``scramble`` decodes a text: every grapheme shows random glyphs of
``charset``, changing ``speed`` times a second, and after ``reveal_delay``
the graphemes settle from left to right. The final width is reserved from
the first frame, so nothing jumps. ``scramble_to`` decodes into new text.
Each label is the call that decodes the sample next to it.
"""

import os

from gaanim import Scene, Zones

DURATION = 1.6
HOLD = 0.6
CODE = "ACCESS GRANTED"

scene = Scene()


def sample(content=CODE):
    return scene.text(content, role="code", size=0.5)


def table(rows, columns=("1fr", "1fr"), wait=0.0):
    """Decode one sample per row, with the call that decodes it as the label.

    ``rows`` are (label, text, decode) triples: ``decode(text)`` returns the
    sample's animation. ``columns`` are the two column tracks, and ``wait``
    holds the samples still before they play.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(columns), column_gap="48px", padding="page_padding")
    )
    for index, (code, text, _) in enumerate(rows):
        scene.text(code, role="code").place(grid[2 * index], anchor="right")
        text.place(grid[2 * index + 1], anchor="left")
    if wait:
        scene.wait(wait)
    scene.play([decode(text).duration(DURATION) for _, text, decode in rows])
    scene.wait(HOLD)


# A named charset, or any literal string of glyphs to cycle through.
scene.segment("charsets")
table([
    (f'scramble(charset="{charset}")', sample(), lambda text, charset=charset: text.animate.scramble(charset=charset))
    for charset in ["upper", "lower", "digits", "hex", "symbols", "01"]
])

# reveal_delay is the time before the first grapheme settles; speed is how
# many times a second the random glyphs change; seed picks them.
scene.segment("timing")
table([
    ("scramble(reveal_delay=0)", sample(), lambda text: text.animate.scramble(reveal_delay=0)),
    ("scramble(reveal_delay=1)", sample(), lambda text: text.animate.scramble(reveal_delay=1)),
    ("scramble(speed=4)", sample(), lambda text: text.animate.scramble(speed=4)),
    ("scramble(speed=40)", sample(), lambda text: text.animate.scramble(speed=40)),
    ("scramble(seed=7)", sample(), lambda text: text.animate.scramble(seed=7)),
])

# scramble_to decodes a text already on screen into new text.
scene.segment("scramble_to")
table(
    [
        ('scramble_to("ACCESS DENIED")', sample(), lambda text: text.animate.scramble_to("ACCESS DENIED")),
        ('scramble_to("LOCKED", charset="01")', sample(),
         lambda text: text.animate.scramble_to("LOCKED", charset="01")),
        ('scramble_to("WAIT 30", charset="digits")', sample(),
         lambda text: text.animate.scramble_to("WAIT 30", charset="digits")),
    ],
    columns=("3fr", "2fr"),
    wait=0.4,
)

STEP = DURATION + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Scrambling, settling and decoded in each segment.
    scene.snapshots(snapshots, [
        *[STEP * index + t for index in range(2) for t in (0.2, 0.9, 1.9)],
        *[2 * STEP + t for t in (0.2, 0.8, 1.5, 2.3)],
    ])
else:
    scene.render()
