"""The typewriter: typing speed, cursors, blinking and jitter, then backspace and retype.

``typewriter`` clears a text and types it again one grapheme per keystroke,
with a cursor after the last one. ``backspace`` deletes from the end and
``retype`` keeps the prefix shared with a new text and types the rest. The
code role's monospaced face keeps the cursor and the columns aligned. Each
label is the call that types the sample next to it.
"""

import os

from gaanim import Scene, Zones

HOLD = 1.0
COMMAND = "> gaanim render --preview"

scene = Scene()


def prompt():
    return scene.text(COMMAND, role="code")


def table(rows, wait=0.0):
    """Play one typing animation per row, with its call as the label.

    ``rows`` are (label, text, type) triples: ``type(text)`` returns the
    sample's animation, which lasts until its last keystroke. ``wait`` holds
    the samples still before they play.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows), columns=["1fr", "1fr"], column_gap="48px", padding="page_padding")
    )
    for index, (code, text, _) in enumerate(rows):
        scene.text(code, role="code").place(grid[2 * index], anchor="right")
        text.place(grid[2 * index + 1], anchor="left")
    if wait:
        scene.wait(wait)
    scene.play([type_(text) for _, text, type_ in rows])
    scene.wait(HOLD)


# cps is keystrokes per second, jitter varies each interval (fixed by seed),
# and the cursor stays solid while typing, then blinks `blink` times a second.
scene.segment("typing")
table([
    ("typewriter()", prompt(), lambda text: text.animate.typewriter()),
    ("typewriter(cps=10)", prompt(), lambda text: text.animate.typewriter(cps=10)),
    ("typewriter(jitter=0)", prompt(), lambda text: text.animate.typewriter(jitter=0)),
    ("typewriter(jitter=0.8, seed=3)", prompt(), lambda text: text.animate.typewriter(jitter=0.8, seed=3)),
    ('typewriter(cursor="█")', prompt(), lambda text: text.animate.typewriter(cursor="█")),
    ('typewriter(cursor="_")', prompt(), lambda text: text.animate.typewriter(cursor="_")),
    ("typewriter(cursor=None)", prompt(), lambda text: text.animate.typewriter(cursor=None)),
    ("typewriter(blink=0)", prompt(), lambda text: text.animate.typewriter(blink=0)),
    ("typewriter(keep_cursor=False)", prompt(), lambda text: text.animate.typewriter(keep_cursor=False)),
])

# backspace deletes the last graphemes at a steady cps; retype deletes back to
# the prefix it shares with the new text, then types the rest.
scene.segment("editing")
table(
    [
        ("backspace(10)", prompt(), lambda text: text.animate.backspace(10)),
        ("backspace(10, cps=8)", prompt(), lambda text: text.animate.backspace(10, cps=8)),
        ("backspace()", prompt(), lambda text: text.animate.backspace()),
        ('retype("> gaanim export --mp4")', prompt(), lambda text: text.animate.retype("> gaanim export --mp4")),
        ('retype("> gaanim check", cps=8)', prompt(), lambda text: text.animate.retype("> gaanim check", cps=8)),
    ],
    wait=0.6,
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-typing, typed, and blinking idle; then before, during and after the edits.
    scene.snapshots(snapshots, [0.7, 1.6, 2.9, 3.6, 4.4, 5.6, 7.2])
else:
    scene.render()
