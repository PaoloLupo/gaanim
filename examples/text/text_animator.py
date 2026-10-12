"""The text range animator: selector shapes, unit orders, animated channels and sweeps.

``text.animator(by, shape, order, seed)`` selects units of a text,
``set(...)`` defines the state a unit reaches at full influence, and
``animate.sweep()`` moves the selector across the units. Reveal shapes bring
each unit from that state to rest; wave shapes pass through and settle back.
Each label is the call that differs between the animators of a segment.
"""

import os

from gaanim import Scene, Zones, part

DURATION = 1.6
HOLD = 0.5
SAMPLE = "Sphinx of black quartz"
SIZE = 0.5

scene = Scene()
accent = scene.canvas.color("accent")


def sample():
    return scene.text(SAMPLE, size=SIZE)


def table(rows, columns=("1fr", "1fr")):
    """Sweep one animator per row, with the code that builds it as the label.

    ``rows`` are (label, text, build) triples: ``build(text)`` returns the
    sample's animation. ``columns`` are the two column tracks.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=list(columns), column_gap="48px", padding="page_padding")
    )
    for index, (code, text, _) in enumerate(rows):
        scene.text(code, role="code").place(grid[2 * index], anchor="right")
        text.place(grid[2 * index + 1], anchor="left")
    scene.play([build(text).duration(DURATION) for _, text, build in rows])
    scene.wait(HOLD)


# Reveal shapes ease each unit in from the out state, here .set(**DROP);
# wave shapes rise to it and settle back, here .set(**LIFT).
scene.segment("shapes")
DROP = {"offset": (0, -0.4), "opacity": 0.0}
LIFT = {"offset": (0, 0.3), "color": accent}
table([
    *[
        (f'animator(shape="{shape}")', sample(),
         lambda text, shape=shape: text.animator(shape=shape).set(**DROP).animate.sweep())
        for shape in ["square", "ramp", "smooth", "ease_in", "ease_out"]
    ],
    *[
        (f'animator(shape="{shape}")', sample(),
         lambda text, shape=shape: text.animator(shape=shape).set(**LIFT).animate.sweep())
        for shape in ["triangle", "round"]
    ],
])

# order sets which unit the range reaches first; "random" is a permutation
# fixed by seed. Every row sets DROP and sweeps once.
scene.segment("order")
table([
    *[
        (f'animator(order="{order}")', sample(),
         lambda text, order=order: text.animator(order=order).set(**DROP).animate.sweep())
        for order in ["forward", "reverse", "center", "random"]
    ],
    ('animator(order="random", seed=5)', sample(),
     lambda text: text.animator(order="random", seed=5).set(**DROP).animate.sweep()),
])

# Every channel set() can animate, one per row, on text.animator().
scene.segment("channels")
CHANNELS = [
    ("offset=(0.4, 0.4)", {"offset": (0.4, 0.4)}),
    ("opacity=0.0", {"opacity": 0.0}),
    ("scale=0.2", {"scale": 0.2}),
    ("rotation=1.2", {"rotation": 1.2}),
    ("blur=0.15", {"blur": 0.15}),
    ("tracking=0.12", {"tracking": 0.12}),
    ("color=accent", {"color": accent}),
]
table(
    [
        (f"set({code})", sample(), lambda text, state=state: text.animator().set(**state).animate.sweep())
        for code, state in CHANNELS
    ],
    columns=("4fr", "5fr"),
)

# The selector walks graphemes, words, explicit lines or parts. Every row
# sets DROP and sweeps once.
scene.segment("units")
table([
    ('animator(by="grapheme")', sample(), lambda text: text.animator(by="grapheme").set(**DROP).animate.sweep()),
    ('animator(by="word")', sample(), lambda text: text.animator(by="word").set(**DROP).animate.sweep()),
    ('animator(by="line")', scene.text("Sphinx of black\nquartz, judge my vow", size=SIZE),
     lambda text: text.animator(by="line").set(**DROP).animate.sweep()),
    ('animator(by="part")', scene.text(part("who", "Sphinx"), " of ", part("what", "black quartz"), size=SIZE),
     lambda text: text.animator(by="part").set(**DROP).animate.sweep()),
])

# sweep(start, end) moves the range between 0 (before the first unit) and 1
# (after the last): sweep(1, 0) plays backward, turning a reveal into an
# exit, and a partial range stops midway. stagger is the delay in seconds between units. Every row sweeps
# text.animator().set(**DROP).
scene.segment("sweeps")
table([
    ("sweep()", sample(), lambda text: text.animator().set(**DROP).animate.sweep()),
    ("sweep(1, 0)", sample(), lambda text: text.animator().set(**DROP).animate.sweep(1, 0)),
    ("sweep(0, 0.5)", sample(), lambda text: text.animator().set(**DROP).animate.sweep(0, 0.5)),
    ("sweep(stagger=0.02)", sample(), lambda text: text.animator().set(**DROP).animate.sweep(stagger=0.02)),
    ("sweep(stagger=0.1)", sample(), lambda text: text.animator().set(**DROP).animate.sweep(stagger=0.1)),
])

STEP = DURATION + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Early, mid-sweep and settled in each of the five segments.
    scene.snapshots(snapshots, [STEP * index + t for index in range(5) for t in (0.4, 0.9, 1.9)])
else:
    scene.render()
