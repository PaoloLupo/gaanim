"""Echo: trails of delayed copies, their count, delay and decay, what they replay, and onion skins.

``echo`` draws copies of a drawable as it was a moment earlier, beneath it.
The first segment varies the count, delay and decay on balls crossing the same
track. The second shows that copies replay every animation of the drawable:
turns, scales, color changes, writing and changes of shape. The third stops
and starts a puck to compare copies that catch up, copies that hold their
poses, and a trail limited to a time window. Each label is the code of its
variant.
"""

import math
import os

from gaanim import Easing, Scene, Zones

MOVE = 1.2
HOLD = 0.8

scene = Scene()
rule = scene.canvas.color("rule")
success = scene.canvas.color("success")


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code", markup=False)


def rows(count, labels="15fr"):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[labels, "9fr"], column_gap="48px", row_gap="24px", padding="page_padding")
    )


def track(zone, ticks=()):
    """Draw a track across ``zone`` with ticks at the fractions ``ticks``; return its ends and height."""
    (start, y), (end, _) = zone.point("left"), zone.point("right")
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    for fraction in ticks:
        scene.geometry.rect(0.02, 0.4).fill(rule).move_to(start + (end - start) * fraction, y)
    return start, end, y


# echo(count=5, delay=0.04, decay=0.6): copy k shows the drawable k * delay
# seconds ago with decay ** k of its opacity. When the drawable stops, the
# copies catch up with it.
options_start = scene.cursor
scene.segment("options")
VARIANTS = [
    ("ball.echo()", {}),
    ("ball.echo(12)", {"count": 12}),
    ("ball.echo(5, delay=0.1)", {"count": 5, "delay": 0.1}),
    ("ball.echo(8, decay=0.9)", {"count": 8, "decay": 0.9}),
    ("ball.echo(3, delay=0.15, decay=0.4)", {"count": 3, "delay": 0.15, "decay": 0.4}),
    ("ball.echo(6, delay=0.08, decay=1.0)", {"count": 6, "delay": 0.08, "decay": 1.0}),
]
grid = rows(len(VARIANTS), labels="12fr")
moves = []
for index, (code, options) in enumerate(VARIANTS):
    label(code).place(grid[2 * index], anchor="right")
    start, end, y = track(grid[2 * index + 1])
    ball = scene.geometry.circle(0.4).move_to(start + 0.4, y).echo(**options)
    moves.append(ball.animate.move_to(end - 0.4, y).easing(Easing.SMOOTH))
scene.wait(0.3)
scene.play(moves, duration=MOVE)
scene.wait(HOLD)

# Copies replay the drawable's own animations with their delay: transforms,
# colors, writing and new outlines, at any seek and in every export.
replays_start = scene.cursor
scene.segment("replays")
grid = rows(5)


def stage(index, code):
    """Label row ``index`` with ``code`` and return its stage."""
    label(code).place(grid[2 * index], anchor="right")
    return grid[2 * index + 1]


zone = stage(0, "bar.echo(6, delay=0.05)\nbar.animate.rotate_by(math.pi)")
bar = scene.geometry.rounded_rect(zone.height * 1.6, 0.2, 0.1).move_to(*zone.center).echo(6, delay=0.05)
zone = stage(1, "ring.echo(5, delay=0.06)\nring.animate.scale_to(2.0)")
ring = scene.geometry.circle(zone.height * 0.25).no_fill().stroke(scene.canvas.color("accent"), 0.06)
ring.move_to(*zone.center).echo(5, delay=0.06)
zone = stage(2, "dot.echo(6, delay=0.05)\ndot.animate.move_to(x, y).fill(success)")
(left, y), (right, _) = zone.point("left"), zone.point("right")
dot = scene.geometry.circle(0.4).move_to(left + 0.4, y).echo(6, delay=0.05)
zone = stage(3, "word.echo(3, delay=0.1)\nword.animate.write()")
word = scene.text("Echo", size=zone.height * 0.8).place(zone).echo(3, delay=0.1)
zone = stage(4, "shape.echo(3, delay=0.3, decay=1.0)\nshape.animate.points(octagon)")
# A square and an octagon with the same eight vertices, so points() can tween them.
size = zone.height * 0.4
(x0, y0), (x1, _) = zone.point("left"), zone.point("right")
ANGLES = [math.pi / 4 * k for k in range(8)]
square = [(size * math.cos(a) / max(abs(math.cos(a)), abs(math.sin(a))),
           size * math.sin(a) / max(abs(math.cos(a)), abs(math.sin(a)))) for a in ANGLES]
octagon = [(1.2 * size * math.cos(a), 1.2 * size * math.sin(a)) for a in ANGLES]
shape = scene.geometry.polygon(square).move_to(x0 + size + 0.2, y0).echo(3, delay=0.3, decay=1.0)
scene.wait(0.3)
scene.play(
    [
        bar.animate.rotate_by(math.pi),
        ring.animate.scale_to(2.0),
        dot.animate.move_to(right - 0.4, y).fill(success),
        word.animate.write(),
        shape.animate.points(octagon).move_to(x1 - size - 0.2, y0).fill(success),
    ],
    duration=MOVE,
)
scene.wait(HOLD)

# A puck hops three times with rests in between. Plain copies catch up at each
# rest; with hold=True they freeze where they were, an onion skin of its last
# poses, and move on with it. start and end (scene seconds) limit what the
# copies record, so only the middle hop leaves a trail.
HOP = 0.6
REST = 0.6
hold_start = scene.cursor
scene.segment("hold")
grid = rows(3, labels="12fr")
hops = []
window = (hold_start + 0.3 + HOP + REST, hold_start + 0.3 + 2 * HOP + REST)
CODES = [
    ("puck.echo(4, delay=0.1)", {}),
    ("puck.echo(4, delay=0.1, hold=True)", {"hold": True}),
    ("puck.echo(8, delay=0.04,\n    start=t1, end=t2)", {"start": window[0], "end": window[1]}),
]
for index, (code, options) in enumerate(CODES):
    label(code).place(grid[2 * index], anchor="right")
    start, end, y = track(grid[2 * index + 1].inset((0, 0.5)), ticks=(0, 1 / 3, 2 / 3, 1))
    count, delay = (8, 0.04) if "start" in options else (4, 0.1)
    puck = scene.geometry.circle(0.5).move_to(start, y).echo(count, delay=delay, **options)
    hops.append((puck, start, end, y))
scene.wait(0.3)
for step in (1, 2, 3):
    scene.play(
        [puck.animate.move_to(start + (end - start) * step / 3, y).easing(Easing.SMOOTH) for puck, start, end, y in hops],
        duration=HOP,
    )
    scene.wait(REST)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Options and replays mid-move and settled, then the hops at each rest and in the middle one.
    scene.snapshots(
        snapshots,
        [options_start + t for t in (0.3 + MOVE * 0.35, 0.3 + MOVE * 0.7, 0.3 + MOVE + 0.5)]
        + [replays_start + t for t in (0.3 + MOVE * 0.4, 0.3 + MOVE * 0.8, 0.3 + MOVE + 0.5)]
        + [hold_start + 0.3 + t for t in (HOP + 0.4, HOP + REST + HOP / 2, 2 * HOP + REST + 0.4, 3 * HOP + 2 * REST + 0.4)],
    )
else:
    scene.render()
