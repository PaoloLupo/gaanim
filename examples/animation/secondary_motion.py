"""Secondary motion: follow-through, trailing followers, squash and stretch, throws and flicks.

These add the motion that follows a main move: ``settle`` overshoots and swings
back, ``follow(delay=...)`` drags a tail behind a leader, ``squash_stretch``
deforms along the velocity, and ``throw`` and ``inertia`` move a drawable with
light, exact physics. Each row or panel is labelled with its code.
"""

import math
import os

from gaanim import Anchor, Easing, Scene, Zones

HOLD = 0.5

scene = Scene()
rule = scene.colors.rule


def rows(count):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=["4fr", "3fr"], column_gap="48px", padding="page_padding")
    )


def track(zone, ticks=()):
    """Draw a track across ``zone`` with ticks at the fractions ``ticks``; return its ends and height."""
    (start, y), (end, _) = zone.point("left"), zone.point("right")
    scene.geometry.line(start, y, end, y).stroke(rule, 0.02)
    for fraction in ticks:
        scene.geometry.rect(0.02, 0.24).fill(rule).move_to(start + (end - start) * fraction, y)
    return start, end, y


def table(header, entries):
    """One row per entry under a ``header`` row: ``build(stage)`` returns the row's animation."""
    grid = rows(len(entries) + 1)
    scene.text(header, role="code").place(grid[0], anchor="right")
    plays = []
    for index, (label, build) in enumerate(entries, start=1):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        plays.append(build(grid[2 * index + 1].inset((0, "48px"))))
    scene.play(plays)
    scene.wait(HOLD)


# settle adds a decaying swing past the target once the move ends:
# overshoot is seconds of swing per unit of final velocity, frequency the
# swings per second and decay how fast they fade.
def settling(**options):
    """A row whose dot moves to the middle tick and settles with ``options``."""

    def build(zone):
        start, end, y = track(zone, ticks=(0.5,))
        target = (start + end) / 2
        dot = scene.geometry.dot(0.12).move_to(start, y)
        return dot.animate.move_to(target, y).duration(0.4).easing(Easing.SMOOTH).settle(**options)
    return build


settle_start = scene.cursor
scene.segment("settle")
table("dot.animate.move_to(x, y).duration(0.4)", [
    (".settle(overshoot=0)", settling(overshoot=0)),
    (".settle()", settling()),
    (".settle(overshoot=0.25)", settling(overshoot=0.25)),
    (".settle(frequency=1.5)", settling(frequency=1.5)),
    (".settle(frequency=6.0)", settling(frequency=6.0)),
    (".settle(decay=3.0)", settling(decay=3.0)),
    (".settle(decay=12.0)", settling(decay=12.0)),
])


# follow(leader, delay=...) places a drawable where the leader was that many
# seconds ago, so followers with growing delays drag a tail behind it.
def tail(spacing):
    """A row whose leader travels a wave with six followers ``spacing`` seconds apart."""

    def build(zone):
        (x, y), half, height = zone.center, zone.width / 2, zone.height * 0.3
        points = [(x - half + 2 * half * i / 60, y + height * math.sin(1.5 * math.tau * i / 60)) for i in range(61)]
        wave = scene.geometry.polyline(points).no_fill().stroke(rule, 0.02)
        leader = scene.geometry.circle(0.16).move_to(*points[0])
        followers = [scene.geometry.circle(0.13 - 0.015 * i).follow(leader, delay=spacing * (i + 1)) for i in range(6)]
        # Followers stay hidden until an entrance shows them.
        return [*(dot.animate.fade_in().duration(0.01) for dot in followers),
                leader.animate.move_along(wave).duration(2.0).easing(Easing.SMOOTH)]
    return build


follow_start = scene.cursor
scene.segment("follow")
grid = rows(3)
plays = []
for index, spacing in enumerate((0.03, 0.08, 0.16)):
    scene.text(f"follow(leader, delay={spacing} * (i + 1))", role="code").place(grid[2 * index], anchor="right")
    plays += tail(spacing)(grid[2 * index + 1])
scene.play(plays)
scene.wait(HOLD)


# squash_stretch stretches a drawable along its velocity by 1 + amount * speed,
# capped at max_ratio, and squashes it across to keep its area.
def squashing(*args, **options):
    """A row whose ball, with ``squash_stretch(*args, **options)``, crosses the track."""

    def build(zone):
        start, end, y = track(zone)
        ball = scene.geometry.circle(0.2).move_to(start + 0.3, y).squash_stretch(*args, **options)
        return ball.animate.move_to(end - 0.3, y).duration(1.0).easing(Easing.SMOOTH)
    return build


squash_start = scene.cursor
scene.segment("squash")
table("ball.animate.move_to(x, y).duration(1.0)", [
    ("squash_stretch(0)", squashing(0)),
    ("squash_stretch(0.05)", squashing(0.05)),
    ("squash_stretch()", squashing()),
    ("squash_stretch(0.2)", squashing(0.2)),
    ("squash_stretch(0.2, max_ratio=1.2)", squashing(0.2, max_ratio=1.2)),
    ("squash_stretch(0.2, max_ratio=2.5)", squashing(0.2, max_ratio=2.5)),
])

# throw flies from where the drawable is under gravity and bounces on a floor,
# keeping `restitution` of its speed each time, until it rests; without a
# floor it lands back at its starting height.
throw_start = scene.cursor
scene.segment("throw")
panels = scene.layout.zones(Zones.grid(rows=3, columns=2, gap="48px", padding="page_padding"))


def launcher(index, label, from_top=False):
    """Label panel ``index``, draw its floor, and return a ball at its left and the floor's height."""
    head, stage = panels[index].split(Zones.rows([0.8, "1fr"], gap="12px"))
    scene.text(label, role="code").move_to(*head.point("top_left"), anchor=Anchor.TOP_LEFT)
    floor = stage.bottom
    scene.geometry.line(stage.left, floor, stage.right, floor).stroke(rule, 0.02)
    ball = scene.geometry.circle(0.15).move_to(stage.left + 0.3, stage.top - 0.15 if from_top else floor + 0.15)
    return ball, floor


plays = []
ball, floor = launcher(0, "throw((1.5, 4.8), floor=floor)")
plays.append(ball.animate.throw((1.5, 4.8), floor=floor))
ball, floor = launcher(1, "throw((1.5, 4.8), floor=floor,\n    restitution=0.75)")
plays.append(ball.animate.throw((1.5, 4.8), floor=floor, restitution=0.75))
ball, floor = launcher(2, "throw((1.5, 4.8), floor=floor,\n    restitution=0.3)")
plays.append(ball.animate.throw((1.5, 4.8), floor=floor, restitution=0.3))
ball, floor = launcher(3, "throw((1.5, 2.8), floor=floor,\n    gravity=4.0)")
plays.append(ball.animate.throw((1.5, 2.8), floor=floor, gravity=4.0))
ball, _ = launcher(4, "throw((1.5, 4.8))")
plays.append(ball.animate.throw((1.5, 4.8)))
ball, floor = launcher(5, "throw(2.5, floor=floor)", from_top=True)
plays.append(ball.animate.throw(2.5, floor=floor))
scene.play(plays)
scene.wait(HOLD)


# inertia glides from where the drawable is and slows like a flick: it would
# rest velocity / friction away, or at the nearest snap point instead.
def gliding(velocity, notches=(), points=(), **options):
    """A row whose coin glides with ``velocity``, snapping to ``notches`` or ``points`` if given."""

    def build(zone):
        start, _, y = track(zone, ticks=notches)
        stops = [(start + zone.width * fx, y + zone.height * fy) for fx, fy in points]
        for point in stops:
            scene.geometry.dot(0.06).fill(rule).move_to(*point)
        snap = [start + zone.width * fraction for fraction in notches] or stops or None
        coin = scene.geometry.circle(0.15).move_to(start, y)
        return coin.animate.inertia(velocity, snap=snap, **options)
    return build


inertia_start = scene.cursor
scene.segment("inertia")
NOTCHES = (0.0, 0.25, 0.5, 0.75, 1.0)
table("coin.animate.inertia(velocity, ...)", [
    ("inertia(6.0)", gliding(6.0)),
    ("inertia(6.0, friction=2.0)", gliding(6.0, friction=2.0)),
    ("inertia(6.0, snap=notches)", gliding(6.0, notches=NOTCHES)),
    ("inertia(6.0, friction=2.0, snap=notches)", gliding(6.0, notches=NOTCHES, friction=2.0)),
    ("inertia((6.0, 0.9))", gliding((6.0, 0.9))),
    ("inertia((6.0, 0.9), snap=points)", gliding((6.0, 0.9), points=((0.45, -0.25), (0.6, 0.3)))),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment mid-move and settled.
    scene.snapshots(
        snapshots,
        [settle_start + t for t in (0.46, 0.52, 0.62)] + [follow_start - 0.1]
        + [follow_start + t for t in (0.6, 1.3, 2.3)]
        + [squash_start + t for t in (0.3, 0.5, 1.3)]
        + [throw_start + t for t in (0.4, 1.2, 4.0)]
        + [inertia_start + t for t in (0.3, 0.8, 3.6)],
    )
else:
    scene.render()
