"""Stress test for keyed magic move (TS-03): edge cases for visual baselines.

Segment A (moves run one after another):
  M1 0.3-1.1  six-row leaderboard where every row changes rank and score
  M2 1.1-1.7  duplicate keys pair in declaration order, the surplus fades
  M3 1.7-2.3  keys only in `before` / only in `after` with unmatched="cut"
  M4 2.3-3.1  nested keyed groups: the keyed outer card moves as a whole
Transition.magic_move into segment B (0.8 s, easing and flash overlay), with
rows and the card paired across the cut and objects shown in both segments.
Segment B:
  M5  shape/color/size/rotation morphs (circle -> star, square -> text)
  M6  no key matches at all (everything cross-fades)
  M7  identical before/after (nothing may flicker)
  M8  callable key (pairs by name prefix, where key="name" matches nothing)
  M9  key="id" between two imports of the same SVG (if the asset exists)
The errors that must be raised while building are listed in a label.
"""

import os
from pathlib import Path

from gaanim import (
    BLACK,
    BLUE,
    CORAL,
    CYAN,
    GOLD,
    GRAY,
    GREEN,
    RED,
    WHITE,
    Easing,
    Overlay,
    Scene,
    Transition,
    magic_move,
)


scene = Scene(frame=(16, 9), background=BLACK)
SVG = Path(__file__).resolve().parents[1] / "tests" / "assets" / "svg_demo.svg"
ROW_COLORS = [BLUE, GOLD, CORAL, GREEN, CYAN, "#a78bfa"]


def small(text, x, y, color=GRAY, size=0.4):
    return scene.text(text).fill(color).scale_to(size).move_to(x, y)


def leaderboard(order, scores, x0, top, step=0.55, scale=1.0):
    """One group per player named P<i>; `order` lists players by rank."""
    rows = []
    for rank, player in enumerate(order):
        y = top - step * rank
        width = scores[player] / 20 * scale
        bar = scene.geometry.rect(width, 0.4 * scale).fill(ROW_COLORS[player])
        bar = bar.move_to(x0 + width / 2, y)
        label = small(f"P{player}", x0 - 0.45, y, WHITE, 0.35 * scale)
        rows.append(scene.geometry.group([bar, label]).named(f"P{player}"))
    return scene.geometry.group(rows)


# ---------------------------------------------------------------- segment A
scene.segment("A: moves")
hud = small("stress: magic move", 5.8, 4.15, WHITE)
scene.persist(hud)  # persistent: visible in both segments, never paired
title_a = small("keyed moves", -5.0, 4.15, GOLD)  # reused into segment B

# M1: six rows, every row changes rank and score.
board_before = leaderboard([0, 1, 2, 3, 4, 5], {0: 90, 1: 80, 2: 70, 3: 60, 4: 50, 5: 40}, -6.8, 3.4)
board_after = leaderboard([3, 5, 0, 4, 1, 2], {0: 72, 1: 45, 2: 30, 3: 99, 4: 60, 5: 85}, -6.8, 3.4)

# M2: three "dot" circles, two "dot" squares: dot1->sq1, dot2->sq2, dot3 fades.
dots = [scene.geometry.circle(0.25).fill(CYAN).move_to(1.0 + 0.8 * i, 3.4).named("dot") for i in range(3)]
squares = [scene.geometry.square(0.6).fill(CORAL).move_to(4.5 + 1.2 * i, 2.6).named("dot") for i in range(2)]
dup_before, dup_after = scene.geometry.group(dots), scene.geometry.group(squares)

# M3: "keep" pairs, "gone" exists only before, "fresh" only after (cut).
cut_before = scene.geometry.group([
    scene.geometry.rect(1.2, 0.5).fill(BLUE).move_to(1.2, 1.2).named("keep"),
    scene.geometry.circle(0.35).fill(RED).move_to(3.0, 1.2).named("gone"),
])
cut_after = scene.geometry.group([
    scene.geometry.rect(2.0, 0.5).fill(GREEN).move_to(5.2, 0.6).named("keep"),
    scene.geometry.regular_polygon(3, 0.45).fill(GOLD).move_to(6.8, 1.4).named("fresh"),
])


# M4: outer "card" keyed (its keyed parts are ignored), and an unnamed wrapper
# whose keyed child "badge" must still be found by descending into it.
def card(x, y, flip):
    icon = scene.geometry.circle(0.3).fill(GOLD).named("icon")
    label = scene.geometry.rect(1.2, 0.3).fill(WHITE).named("label")
    icon.move_to(x + (0.9 if flip else -0.9), y)
    label.move_to(x + (-0.4 if flip else 0.4), y)
    frame = scene.geometry.rounded_rect(2.8, 1.0, 0.2).fill("#1e293b").move_to(x, y)
    return scene.geometry.group([frame, icon, label]).named("card")


nest_before = scene.geometry.group([
    card(-4.5, -2.0, False),
    scene.geometry.group([scene.geometry.star(5, 0.35, 0.15).fill(RED).move_to(-1.5, -2.0).named("badge")]),
])
nest_after = scene.geometry.group([
    card(2.5, -2.6, True).scale_to(1.2),
    scene.geometry.group([scene.geometry.star(5, 0.5, 0.2).fill(CYAN).move_to(6.0, -1.6).named("badge")]),
])

scene.wait(0.3)
scene.play([magic_move(board_before, board_after, key="name").duration(0.8)])
scene.play([magic_move(dup_before, dup_after, unmatched="fade").duration(0.6)])
scene.play([magic_move(cut_before, cut_after, unmatched="cut").duration(0.6)])
scene.play([magic_move(nest_before, nest_after).duration(0.8)])
scene.wait(0.3)

# ---------------------------------------------------------------- segment B
# Rows P0..P5 and "card" are keyed on both sides of the cut; the hidden
# `before` groups of segment A must be skipped.
scene.segment(
    "B: shapes",
    transition=Transition.magic_move(0.8, key="name", easing=Easing.SMOOTH, overlay=Overlay.flash(WHITE, 0.2)),
)
scene.reuse(title_a)  # the same object in both segments
leaderboard([5, 4, 3, 2, 1, 0], {i: 40 + 8 * i for i in range(6)}, 4.6, 3.4, step=0.4, scale=0.7)
card(4.8, 0.2, False)

# M5: very different shapes plus color, size and rotation changes.
shapes_before = scene.geometry.group([
    scene.geometry.circle(0.5).fill(GOLD).move_to(-6.5, 2.6).named("s1"),
    scene.geometry.square(0.8).fill(BLUE).move_to(-5.0, 2.6).named("s2"),
    scene.geometry.rect(1.2, 0.3).fill(GREEN).move_to(-3.4, 2.6).named("s3"),
])
shapes_after = scene.geometry.group([
    scene.geometry.star(7, 0.9, 0.35).fill(RED).rotate_to(0.4).move_to(-6.2, 1.2).named("s1"),
    scene.text("Hi!").fill(CYAN).scale_to(0.9).move_to(-4.6, 1.2).named("s2"),
    scene.geometry.rect(1.2, 0.3).fill(CORAL).scale_to(1.8).rotate_to(1.2).move_to(-2.8, 1.2).named("s3"),
])

# M6: no key in common, so every member fades.
none_before = scene.geometry.group([
    scene.geometry.circle(0.3).fill(BLUE).move_to(-1.0 + 0.8 * i, 2.6).named(f"n_a{i}") for i in range(2)
])
none_after = scene.geometry.group([
    scene.geometry.square(0.5).fill(GOLD).move_to(-1.0 + 0.8 * i, 1.6).named(f"n_b{i}") for i in range(2)
])


# M7: identical before and after; frames during the move must not change.
def same(tag):
    return scene.geometry.group([
        scene.geometry.rect(0.9, 0.5).fill(GREEN).move_to(1.4, 2.6).named(f"{tag}1"),
        scene.geometry.circle(0.25).fill(WHITE).move_to(2.5, 2.6).named(f"{tag}2"),
    ])


same_before, same_after = same("i"), same("i")

# M8: key="name" finds nothing ("red:alpha" vs "red:gamma"); the callable
# pairs by the prefix before ':' and returns None for unnamed drawables.
call_before = scene.geometry.group([
    scene.geometry.circle(0.3).fill(RED).move_to(-6.5, -0.6).named("red:alpha"),
    scene.geometry.circle(0.3).fill(BLUE).move_to(-5.7, -0.6).named("blue:beta"),
])
call_after = scene.geometry.group([
    scene.geometry.square(0.5).fill(RED).move_to(-3.0, -1.2).named("red:gamma"),
    scene.geometry.square(0.5).fill(BLUE).move_to(-4.0, -0.2).named("blue:delta"),
])


def by_team(drawable):
    return drawable.name.split(":")[0] if drawable.name else None


# M9: two imports of the same SVG; the top-level ids pair with key="id".
svg_pair = None
if SVG.exists():
    svg_before = scene.media.svg(str(SVG)).scale_to(0.8).move_to(-4.5, -3.0)
    svg_after = scene.media.svg(str(SVG)).scale_to(1.1).rotate_to(-0.15).move_to(0.5, -2.7)
    svg_pair = (svg_before, svg_after)

# Build-time errors that must be raised (ValueError); listed on screen.
results = []


def expect_error(label, build):
    try:
        build()
    except ValueError:
        results.append(f"{label} ok")
    else:
        results.append(f"{label} NOT RAISED")


expect_error("key='color'", lambda: magic_move(none_before, none_after, key="color"))
expect_error("unmatched='scale'", lambda: magic_move(none_before, none_after, unmatched="scale"))
expect_error("named('')", lambda: none_before.named(""))
expect_error("same drawable", lambda: magic_move(none_before, none_before))
expect_error("duration=0", lambda: magic_move(none_before, none_after, duration=0.0))
expect_error("Transition key='x'", lambda: Transition.magic_move(0.5, key="x"))
small("errors: " + "; ".join(results), 0.0, -4.2, GOLD, 0.32)

scene.wait(0.2)
scene.play([magic_move(shapes_before, shapes_after).duration(0.8)])
scene.play([magic_move(none_before, none_after).duration(0.6)])
scene.play([magic_move(same_before, same_after).duration(0.6)])
scene.play([magic_move(call_before, call_after, key=by_team).duration(0.6)])
if svg_pair:
    scene.play([magic_move(*svg_pair, key="id").duration(0.8)])
else:
    scene.wait(0.8)  # keep the timeline (and the seeks) identical without the asset
scene.wait(0.4)

# Measured with scene.cursor: segment A 0-3.4; segment B starts at 3.4 and its
# 0.8 s magic-move transition overlaps B's first moves (flash peaks 3.7-3.9).
# M5 3.6-4.4, M6 4.4-5.0, M7 5.0-5.6, M8 5.6-6.2, M9 6.2-7.0, end 7.4.
SEEKS = [
    0.3,  # M1 exact start
    0.7,  # M1 midway: all six rows crossing
    1.1,  # M1 exact end / M2 start
    1.4,  # M2 mid: duplicates pairing, surplus dot fading
    2.0,  # M3 mid: cut mode (gone already hidden, fresh not yet shown)
    2.7,  # M4 mid: nested card as a whole, badge found inside wrapper
    3.1,  # M4 exact end
    3.45,  # just after the segment transition starts
    3.6,  # transition under way, before the flash; M5 starts
    4.05,  # transition past the flash: keyed rows and card travelling; M5 mid
    4.25,  # right after the transition ends
    4.7,  # M6 mid: nothing matches, everything fades
    5.3,  # M7 mid: identical states, must equal the end frame
    5.9,  # M8 mid: callable-key pairing
    6.6,  # M9 mid: SVG key="id"
    7.3,  # settled end
]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
