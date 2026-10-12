"""Repeaters and duplicators: copies stepped by a transform, copies placed by a distribution, and copy counts.

``scene.geometry.repeat`` makes copies of a shape, each one step further
turned, scaled, shifted and faded. ``scene.geometry.duplicate`` places copies
at the points of a ``Distribution``: a grid, a circle, a path, a random cloud
or a sunflower spiral. Both return a group whose ``count`` shows its first
copies, and ``animate.count`` builds the pattern copy by copy. Each label is
the code of its variant.
"""

import math
import os

from gaanim import Distribution, Easing, Scene, Zones

BUILD = 2.0

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
foreground = scene.canvas.color("foreground")
rule = scene.canvas.color("rule")


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


def table(variants):
    """One row per (label, build) pair: the label on the left, ``build(zone)`` on the right.

    A build may return an animation; they all play together.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=["15fr", "9fr"], column_gap="48px", row_gap="24px", padding="page_padding")
    )
    plays = []
    for index, (code, build) in enumerate(variants):
        label(code).place(grid[2 * index], anchor="right")
        play = build(grid[2 * index + 1])
        if play is not None:
            plays.append(play)
    scene.wait(0.3)
    if plays:
        scene.play(plays, duration=BUILD)
    scene.wait(1.0)


# repeat(shape, count, rotate=0, scale=1, offset=(0, 0), opacity=(1, 1),
# about=None): copy i is turned by i * rotate, scaled by scale ** i and shifted
# by i * offset, about the point ``about`` when given or else its own pivot.
def petal(zone):
    """A petal above the middle of ``zone``, and that middle."""
    (x, y), size = zone.center, zone.height * 0.5
    return scene.geometry.ellipse(size * 0.18, size * 0.5).move_to(x, y + size * 0.5), (x, y)


def flower(zone):
    """Twelve petals turned about the middle of ``zone``."""
    shape, center = petal(zone)
    scene.geometry.repeat(shape, 12, rotate=math.tau / 12, about=center, opacity=(1.0, 0.4))


def tunnel(zone):
    """Squares that turn and shrink, each inside the last."""
    square = scene.geometry.square(zone.height).no_fill().stroke(accent, 0.04).move_to(*zone.center)
    scene.geometry.repeat(square, 14, rotate=0.12, scale=0.86)


def spiral(zone):
    """Leaves turned and shrunk about the middle of ``zone``, a spiral."""
    (x, y), size = zone.center, zone.height * 0.5
    leaf = scene.geometry.ellipse(size * 0.1, size * 0.22).fill(success).move_to(x, y + size * 0.8)
    scene.geometry.repeat(leaf, 40, rotate=0.45, scale=0.95, about=(x, y))


def fan(zone):
    """Bars turned about their own middle, a fan."""
    bar = scene.geometry.rounded_rect(zone.height, 0.08, 0.04).move_to(*zone.center)
    scene.geometry.repeat(bar, 9, rotate=math.pi / 9, opacity=(1.0, 0.2))


repeat_start = scene.cursor
scene.segment("repeat_turns")
table([
    ("repeat(petal, 12, rotate=math.tau / 12,\n       about=center, opacity=(1.0, 0.4))", flower),
    ("repeat(square, 14, rotate=0.12,\n       scale=0.86)", tunnel),
    ("repeat(leaf, 40, rotate=0.45, scale=0.95,\n       about=center)", spiral),
    ("repeat(bar, 9, rotate=math.pi / 9,\n       opacity=(1.0, 0.2))", fan),
])


def row(zone):
    """Dots stepped right across ``zone``."""
    x, y = zone.point("left")
    dot = scene.geometry.circle(0.25).move_to(x + 0.3, y)
    scene.geometry.repeat(dot, 10, offset=(0.54, 0))


def fading(zone):
    """Bars stepped right, fading out."""
    x, y = zone.point("left")
    bar = scene.geometry.rounded_rect(0.2, zone.height * 0.9, 0.1).move_to(x + 0.2, y)
    scene.geometry.repeat(bar, 12, offset=(0.46, 0), opacity=(1.0, 0.1))


def stacked(zone):
    """A word stepped down and right, fading in, like a drop shadow of copies."""
    word = scene.text("Repeat", size=zone.height * 0.6, weight=800).place(zone).shift_by(-0.3, 0.25)
    scene.geometry.repeat(word, 6, offset=(0.1, -0.1), opacity=(0.15, 1.0))


def stairs(zone):
    """Squares stepped up and right, turning and shrinking."""
    x, y = zone.point("bottom_left")
    square = scene.geometry.square(zone.height * 0.5).fill(success).move_to(x + zone.height * 0.3, y + zone.height * 0.3)
    scene.geometry.repeat(square, 9, offset=(0.55, 0.1), rotate=0.2, scale=0.92)


offset_start = scene.cursor
scene.segment("repeat_offsets")
table([
    ("repeat(dot, 10, offset=(0.54, 0))", row),
    ("repeat(bar, 12, offset=(0.46, 0),\n       opacity=(1.0, 0.1))", fading),
    ("repeat(word, 6, offset=(0.1, -0.1),\n       opacity=(0.15, 1.0))", stacked),
    ("repeat(square, 9, offset=(0.55, 0.1),\n       rotate=0.2, scale=0.92)", stairs),
])


def arrow():
    """A small arrowhead pointing right."""
    return scene.geometry.polygon([(-0.13, -0.1), (0.16, 0.0), (-0.13, 0.1)])


# duplicate(shape, distribution) centers a copy on every point; oriented
# distributions also turn each copy to face outward or along its path.
def grid_of(zone):
    """A grid of dots filling ``zone``."""
    spacing = zone.height / 4
    scene.geometry.duplicate(scene.geometry.circle(spacing * 0.3), Distribution.grid(12, 4, spacing, center=zone.center))


def ring_of(zone):
    """Arrowheads on a circle, turned to face outward."""
    scene.geometry.duplicate(arrow(), Distribution.circle(16, 0.75, center=zone.center, orient=True))


def path_of(zone):
    """Arrowheads along a zigzag, turned to follow it."""
    x, y = zone.center
    w, h = zone.width * 0.45, zone.height * 0.4
    path = [(x - w, y - h), (x - w / 3, y + h), (x + w / 3, y - h), (x + w, y + h)]
    scene.geometry.polyline(path).no_fill().stroke(rule, 0.02)
    scene.geometry.duplicate(arrow().fill(success), Distribution.along(path, 24, orient=True))


def cloud_of(zone):
    """Stars scattered at random in ``zone``, the same cloud for the same seed."""
    bounds = (zone.left + 0.2, zone.bottom + 0.2, zone.right - 0.2, zone.top - 0.2)
    scene.geometry.duplicate(scene.geometry.star(5, 0.13, 0.055).fill(foreground), Distribution.random(50, bounds, seed=3))


duplicate_start = scene.cursor
scene.segment("duplicate")
table([
    ("duplicate(dot,\n    Distribution.grid(12, 4, spacing))", grid_of),
    ("duplicate(arrow, Distribution.circle(\n    16, 0.75, orient=True))", ring_of),
    ("duplicate(arrow, Distribution.along(\n    path, 24, orient=True))", path_of),
    ("duplicate(star, Distribution.random(\n    50, bounds, seed=3))", cloud_of),
])


# count(n) shows the first n copies (a fraction fades the next one in), and
# animate.count(n) builds the pattern copy by copy, from the first.
def sunflower(zone):
    """Seeds on a sunflower spiral, built from the middle out."""
    seeds = scene.geometry.duplicate(
        scene.geometry.circle(0.035), Distribution.phyllotaxis(260, 0.055, center=zone.center)
    ).count(0)
    return seeds.animate.count(260).easing(Easing.LINEAR)


def petals(zone):
    """The flower of petals, built petal by petal."""
    shape, center = petal(zone)
    built = scene.geometry.repeat(shape, 12, rotate=math.tau / 12, about=center).count(0)
    return built.animate.count(12).easing(Easing.LINEAR)


def partial(zone):
    """A ring of dots showing two and a half of its copies, then all of them."""
    ring = scene.geometry.duplicate(
        scene.geometry.circle(0.12).fill(success), Distribution.circle(12, 0.75, center=zone.center)
    ).count(2.5)
    return ring.animate.count(12)


def emptied(zone):
    """A grid of squares that hides its copies again, from the last."""
    spacing = zone.height / 4
    squares = scene.geometry.duplicate(
        scene.geometry.square(spacing * 0.7).fill(foreground), Distribution.grid(12, 4, spacing, center=zone.center)
    )
    return squares.animate.count(0)


count_start = scene.cursor
scene.segment("count")
table([
    ("seeds = duplicate(dot, Distribution\n    .phyllotaxis(260, 0.055)).count(0)\nseeds.animate.count(260)", sunflower),
    ("flower = repeat(petal, 12, ...).count(0)\nflower.animate.count(12)", petals),
    ("ring.count(2.5)\nring.animate.count(12)", partial),
    ("squares.animate.count(0)", emptied),
])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The static patterns, then the counts at the start, halfway and built.
    scene.snapshots(
        snapshots,
        [start + 0.5 for start in (repeat_start, offset_start, duplicate_start)]
        + [count_start + t for t in (0.2, 0.3 + BUILD / 2, 0.3 + BUILD + 0.5)],
    )
else:
    scene.render()
