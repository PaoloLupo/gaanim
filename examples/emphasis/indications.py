"""Indications: transient animations that point at a drawable, a frame or a word.

Each cell or row plays one emphasis of ``drawable.animate`` and is labelled
with the call; the drawable is back at rest when it ends. Segments cover
pulses and shakes, blinks and passing flashes, bursts of rays and drawn
frames, ripples, flashing frames and underlines, and the emphasis of a word
inside a text.
"""

import os

from gaanim import Scene, Zones

DURATION = 1.6
HOLD = 0.5

scene = Scene()
success = scene.colors.success
foreground = scene.colors.foreground
geometry = scene.geometry


def cells(labels, rows=2, columns=3):
    """A grid of cells, each label above its stage; return the stages."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    stages = []
    for cell, label in zip(grid, labels):
        top, stage = cell.split(Zones.rows(["88px", "1fr"]))
        scene.text(label, role="code").place(top, anchor="top_left")
        stages.append(stage)
    return stages


def lines(labels, tracks=("1fr", "1fr")):
    """One row per label, the label to the left; return each row's stage."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(labels), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    for index, label in enumerate(labels):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
    return [grid[2 * index + 1] for index in range(len(labels))]


def play(entries, stages):
    """Play one emphasis per stage: ``entries`` are (label, make, emphasis).

    ``make(stage)`` builds the drawable in its stage and
    ``emphasis(drawable.animate)`` returns the animation.
    """
    moves = [emphasis(make(stage).animate).duration(DURATION) for stage, (_, make, emphasis) in zip(stages, entries)]
    scene.wait(HOLD)
    scene.play(moves)
    scene.wait(HOLD)


def circle(stage):
    return geometry.circle(0.75).move_to(*stage.center)


def square(stage):
    return geometry.square(1.3).move_to(*stage.center)


def star(stage):
    return geometry.star(5, 0.5, 0.22).move_to(*stage.center)


def card(stage):
    return geometry.rounded_rect(2.6, 1.2, 0.12).move_to(*stage.center)


def dot(stage):
    return geometry.dot(0.3).move_to(*stage.center)


def wave(stage):
    """A zigzag stroke across ``stage``."""
    (x, y), half = stage.center, stage.width / 2 - 0.3
    points = [(x - half + index * half / 4, y + (0.5 if index % 2 else -0.5)) for index in range(9)]
    return geometry.polyline(points).stroke(foreground, 0.1)


def word(stage, text="Notice", anchor="center"):
    return scene.text(text, size=0.9).place(stage, anchor=anchor)


# indicate swells the drawable for a moment and tints it: with the theme's
# accent, its foreground for a drawable already in the accent, or color.
# wiggle shakes it sideways, count times amplitude units each way.
scene.segment("pulses")
ENTRIES = [
    ("indicate()", circle, lambda a: a.indicate()),
    ("indicate(color=success,\n         scale_factor=1.5)", circle, lambda a: a.indicate(color=success, scale_factor=1.5)),
    ("wiggle()", square, lambda a: a.wiggle()),
    ("wiggle(amplitude=0.3, count=2)", square, lambda a: a.wiggle(amplitude=0.3, count=2)),
]
play(ENTRIES, cells([label for label, _, _ in ENTRIES], columns=2))

# blink switches the drawable off and on, three times by default.
# show_passing_flash sweeps a window along a stroke, which is hidden before
# and after; time_width is the window's share of the path.
scene.segment("blinks")
ENTRIES = [
    ("blink()", card, lambda a: a.blink()),
    ("blink(6)", card, lambda a: a.blink(6)),
    ("show_passing_flash()", wave, lambda a: a.show_passing_flash()),
    ("show_passing_flash(time_width=0.6)", wave, lambda a: a.show_passing_flash(time_width=0.6)),
]
play(ENTRIES, cells([label for label, _, _ in ENTRIES], columns=2))

# flash bursts rays out of the drawable: lines of them, length long, gap
# beyond it. circumscribe draws a frame around it and lets it go.
scene.segment("flashes")
ENTRIES = [
    ("flash()", star, lambda a: a.flash()),
    ("flash(lines=6, length=0.6,\n      gap=0.4, color=success)", star, lambda a: a.flash(lines=6, length=0.6, gap=0.4, color=success)),
    ("circumscribe()", word, lambda a: a.circumscribe()),
    ("circumscribe(padding=0.3,\n             corner_radius=0.3)", word, lambda a: a.circumscribe(padding=0.3, corner_radius=0.3)),
]
play(ENTRIES, cells([label for label, _, _ in ENTRIES], columns=2))

# broadcast sends copies of the drawable out as ripples (stars, from a
# star): count of them, growing to max_scale times its size, each lag of a
# ripple after the last.
scene.segment("ripples")
ENTRIES = [
    ("broadcast()", dot, lambda a: a.broadcast()),
    ("broadcast(count=2, max_scale=5.0,\n          lag=0.0)", dot, lambda a: a.broadcast(count=2, max_scale=5.0, lag=0.0)),
    ("broadcast(count=8, lag=0.6)", dot, lambda a: a.broadcast(count=8, lag=0.6)),
    ("broadcast(max_scale=2.5)", star, lambda a: a.broadcast(max_scale=2.5)),
]
play(ENTRIES, cells([label for label, _, _ in ENTRIES], columns=2))

# flash_around and flash_under sweep a bright window around a new frame or
# along a new underline, both invisible outside the flash. color and width
# restyle them, padding, corner_radius, gap and overhang shape them, and
# time_width sets the window's length.
scene.segment("frames")
ENTRIES = [
    ("flash_around()", word, lambda a: a.flash_around()),
    ("flash_around(color=success, width=0.08)", word, lambda a: a.flash_around(color=success, width=0.08)),
    ("flash_around(padding=0.3,\n             corner_radius=0.3)", word, lambda a: a.flash_around(padding=0.3, corner_radius=0.3)),
    ("flash_around(time_width=1.0)", word, lambda a: a.flash_around(time_width=1.0)),
    ("flash_under()", word, lambda a: a.flash_under()),
    ("flash_under(gap=0.2, overhang=0.5,\n            time_width=1.0)", word, lambda a: a.flash_under(gap=0.2, overhang=0.5, time_width=1.0)),
]
play(ENTRIES, lines([label for label, _, _ in ENTRIES], ("3fr", "2fr")))

# A selection of a text takes indications of its own: indicate swells and
# tints its glyphs, pulse swells them twice, highlight tints them, wiggle
# shakes them, wave lifts them one after another, and focus dims the rest of
# the text.
scene.segment("words")
ENTRIES = [
    ('line["gap"].animate.indicate()', lambda a: a["gap"].animate.indicate()),
    ('line["gap"].animate.pulse()', lambda a: a["gap"].animate.pulse()),
    ('line["gap"].animate.highlight()', lambda a: a["gap"].animate.highlight()),
    ('line["gap"].animate.wiggle()', lambda a: a["gap"].animate.wiggle()),
    ('line["gap"].animate.wave()', lambda a: a["gap"].animate.wave()),
    ('line["gap"].animate.focus()', lambda a: a["gap"].animate.focus()),
    ("line.words[0:2].animate.indicate()", lambda a: a.words[0:2].animate.indicate()),
]
stages = lines([label for label, _ in ENTRIES])
texts = [scene.text("Mind the gap", size=0.8).place(stage, anchor="left") for stage in stages]
scene.wait(HOLD)
scene.play([emphasis(line).duration(DURATION) for (_, emphasis), line in zip(ENTRIES, texts)])
scene.wait(HOLD)

SEGMENT = 2 * HOLD + DURATION
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Every segment early, at its peak and late in the emphasis.
    scene.snapshots(snapshots, [SEGMENT * index + HOLD + t for index in range(6) for t in (0.4, 0.8, 1.2)])
else:
    scene.render()
