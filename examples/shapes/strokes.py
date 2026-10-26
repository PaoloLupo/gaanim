"""Strokes: width and alignment, caps and joins, dashes, width profiles and trims.

Every row strokes the same kind of path in a different way, labelled with the
call that differs. Thin foreground lines on top of the squares, caps and joins
trace the path itself, so the part of the stroke that reaches past it shows.
"""

import math
import os

from gaanim import Scene, StrokeStyle, Updater, Zones

DRAW = 1.2
HOLD = 0.6
MARCH = 2.4

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
success = scene.colors.success

# Paths shared by the rows, in scene coordinates: a wide and a sharp corner,
# and one period of a sine wave.
CORNERS = [(0, 0), (0.7, 0.45), (1.4, 0), (1.1, 0.45)]
WAVE = [(x / 20, 0.35 * math.sin(x / 20 * math.tau / 2.6)) for x in range(53)]


def table(variants, columns=("3fr", "1fr")):
    """Place each (label, drawable) pair in a row, the label right-aligned before it."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=list(columns), gap="24px", padding="page_padding")
    )
    for index, (label, drawable) in enumerate(variants):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        drawable.place(grid[2 * index + 1])
    return [drawable for _, drawable in variants]


def draw(drawables):
    """Draw ``drawables`` together, then hold them."""
    scene.play([drawable.animate.create() for drawable in drawables], duration=DRAW)
    scene.wait(HOLD)


def traced(shape):
    """Group ``shape`` with a thin line along its path, drawn on top."""
    return scene.geometry.group([shape, shape.copy().no_fill().stroke(foreground, 0.015)])


def square():
    """The unfilled square of the width rows."""
    return scene.geometry.square(0.8).no_fill()


def bar(style):
    """A straight line drawn with ``style``."""
    return scene.geometry.line((0, 0), (2.2, 0)).stroke_style(style)


def corners(style):
    """The two corners drawn with ``style``."""
    return scene.geometry.polyline(CORNERS).stroke_style(style)


def wave():
    """The wave of the profile rows, with a wide stroke to shape."""
    return scene.geometry.polyline(WAVE).stroke(accent, 0.24)


def ring():
    """A circle of the trim rows."""
    return scene.geometry.circle(0.5).no_fill().stroke(accent, 0.1)


def dashes():
    """A dashed line of the trim rows: one path of many pieces."""
    return scene.geometry.dashed_line((0, 0), (2.4, 0), dash_length=0.3, gap_length=0.2).stroke(accent, 0.1)


# Widths are scene units and stay put while a shape scales, unless
# scale_with_object (or scale_stroke_with_object()) lets them follow. Closed
# strokes sit inside the contour unless align moves them.
scene.segment("width")
morphing = square().stroke(accent, 0.12)
squares = table([
    (".stroke(accent, 0.12)", traced(square().stroke(accent, 0.12))),
    ('.stroke(accent, 0.12, align="center")', traced(square().stroke(accent, 0.12, align="center"))),
    ('.stroke(accent, 0.12, align="outside")', traced(square().stroke(accent, 0.12, align="outside"))),
    (".stroke(accent, 0.12, scale_with_object=True)", traced(square().stroke(accent, 0.12, scale_with_object=True))),
    (".animate.stroke(success, 0.24)", traced(morphing)),
])
scene.play([shape.animate.create() for shape in squares], duration=DRAW)
scene.play(
    [shape.animate.scale_to(1.4) for shape in squares] + [morphing.animate.stroke(success, 0.24)], duration=DRAW
)
scene.wait(HOLD)

# StrokeStyle sets the whole pen; each row passes it to stroke_style(). The
# default miter_limit of 4 bevels the sharp corner; a higher one keeps its point.
scene.segment("caps_joins")
draw(table(
    [
        ('StrokeStyle(accent, 0.24, cap="butt")', traced(bar(StrokeStyle(accent, 0.24, cap="butt")))),
        ('StrokeStyle(accent, 0.24, cap="round")', traced(bar(StrokeStyle(accent, 0.24, cap="round")))),
        ('StrokeStyle(accent, 0.24, cap="square")', traced(bar(StrokeStyle(accent, 0.24, cap="square")))),
        ('StrokeStyle(accent, 0.16, join="miter")', traced(corners(StrokeStyle(accent, 0.16, join="miter")))),
        (
            'StrokeStyle(accent, 0.16, join="miter", miter_limit=10)',
            traced(corners(StrokeStyle(accent, 0.16, join="miter", miter_limit=10))),
        ),
        ('StrokeStyle(accent, 0.16, join="round")', traced(corners(StrokeStyle(accent, 0.16, join="round")))),
        ('StrokeStyle(accent, 0.16, join="bevel")', traced(corners(StrokeStyle(accent, 0.16, join="bevel")))),
    ],
    columns=("4fr", "1fr"),
))

# Dashes alternate on and off lengths from the start of the path; a dash of
# no length with round caps makes a dot. The offset (StrokeStyle's
# dash_offset) slides the pattern: animate it with animate.dash_offset, or let
# Updater.dash_flow run it endlessly. The three borders keep their gaps with
# the round-capped style, inside their contour as strokes are by default.
scene.segment("dashes")
DASHED = StrokeStyle(accent, 0.08, dashes=[0.3, 0.15])
BUTT = StrokeStyle(accent, 0.08, cap="butt", dashes=[0.3, 0.15])
marching = scene.geometry.rounded_rect(2.2, 0.6, 0.2).no_fill().stroke_style(DASHED)
flowing = scene.geometry.rounded_rect(2.2, 0.6, 0.2).no_fill().stroke_style(DASHED)
returning = scene.geometry.rounded_rect(2.2, 0.6, 0.2).no_fill().stroke_style(DASHED)
draw(table(
    [
        ("StrokeStyle(accent, 0.08, dashes=[0.3, 0.15])", bar(DASHED)),
        ('StrokeStyle(accent, 0.08, cap="butt", dashes=[0.3, 0.15])', bar(BUTT)),
        (
            "StrokeStyle(accent, 0.08, dashes=[0.4, 0.15, 0, 0.15])",
            bar(StrokeStyle(accent, 0.08, dashes=[0.4, 0.15, 0, 0.15])),
        ),
        ("StrokeStyle(accent, 0.12, dashes=[0, 0.25])", bar(StrokeStyle(accent, 0.12, dashes=[0, 0.25]))),
        (
            "dashed_line((0, 0), (2.2, 0), dash_length=0.4)",
            scene.geometry.dashed_line((0, 0), (2.2, 0), dash_length=0.4),
        ),
        (".animate.dash_offset(1.5)", marching),
        (".add_updater(Updater.dash_flow(0.6))", flowing),
        (".add_updater(Updater.dash_flow(-0.6))", returning),
    ],
    columns=("5fr", "1fr"),
))
flowing.add_updater(Updater.dash_flow(0.6))
returning.add_updater(Updater.dash_flow(-0.6))
scene.play([marching.animate.dash_offset(1.5)], duration=MARCH)
scene.wait(HOLD)

# A profile scales the width along the path with (position, factor) pairs;
# stroke_taper(start, end) narrows the ends to points. A tapered flash keeps
# its points as it passes.
scene.segment("profiles")
flash = wave().stroke_taper(0.5, 0.5)
waves = table([
    (".stroke_taper()", wave().stroke_taper()),
    (".stroke_taper(0.6, 0)", wave().stroke_taper(0.6, 0)),
    (".stroke_profile([(0, 0.1), (0.5, 1), (1, 0.1)])", wave().stroke_profile([(0, 0.1), (0.5, 1), (1, 0.1)])),
    (".stroke_profile([(0, 1), (0.5, 0.2), (1, 1)])", wave().stroke_profile([(0, 1), (0.5, 0.2), (1, 1)])),
    (".stroke_taper(0.5, 0.5)\n.animate.show_passing_flash(time_width=0.5)", flash),
])
scene.play(
    [path.animate.create() for path in waves[:-1]] + [flash.animate.show_passing_flash(time_width=0.5)], duration=DRAW
)
scene.wait(HOLD)

# trim() shows a window of the path from start to end, shifted by offset;
# animating it draws, grows from the middle or sends a segment around. On a
# path of many pieces, "sequential" trims their total length in order.
scene.segment("trim")
TRIMS = [
    (".trim(end=0)\n.animate.trim(end=1)", ring().trim(end=0), {"end": 1}),
    (
        ".trim(start=0.5, end=0.5)\n.animate.trim(start=0, end=1)",
        ring().trim(start=0.5, end=0.5),
        {"start": 0, "end": 1},
    ),
    (".trim(end=0.2)\n.animate.trim(offset=1)", ring().trim(end=0.2), {"offset": 1}),
    (".trim(end=0)\n.animate.trim(end=1)", dashes().trim(end=0), {"end": 1}),
    ('.trim(end=0, mode="sequential")\n.animate.trim(end=1)', dashes().trim(end=0, mode="sequential"), {"end": 1}),
]
paths = table([(label, path) for label, path, _ in TRIMS])
scene.play([path.animate.trim(**target) for path, (_, _, target) in zip(paths, TRIMS)], duration=DRAW)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment while it animates and once settled; the dashes twice while
    # they march.
    scene.snapshots(snapshots, [0.6, 1.8, 2.9, 3.6, 4.7, 7.4, 8.6, 10.2, 11.3, 11.9, 13.1])
else:
    scene.render()
