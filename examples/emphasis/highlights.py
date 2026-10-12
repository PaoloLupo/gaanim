"""Highlights: frames, color-cycling borders, highlighter bands and spotlights that stay on a target.

``scene.geometry.surrounding_rect`` draws a live frame that follows its
targets and can move to others; ``animated_boundary`` draws a border whose
color cycles; ``marker`` sweeps a highlighter band behind words; and
``scene.fx.spotlight`` dims everything but its target. Each label is the
code that builds or animates its row or card.
"""

import os

from gaanim import Easing, Scene, Zones, part

DRAW = 1.2
HOLD = 0.6
LIGHT = 1.6

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")
danger = scene.canvas.color("danger")
foreground = scene.canvas.color("foreground")
background = scene.canvas.color("background")
panel = scene.canvas.color("panel")
rule = scene.canvas.color("rule")
geometry = scene.geometry


def rows(labels, tracks=("3fr", "2fr")):
    """One row per label, the label to the left; return each row's stage."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(labels), columns=list(tracks), column_gap="48px", padding="page_padding")
    )
    for index, label in enumerate(labels):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
    return [grid[2 * index + 1] for index in range(len(labels))]


def card(stage, width=3.0, height=0.9):
    """A panel card on the left of ``stage``, a little in from its edge."""
    return geometry.rounded_rect(width, height, 0.1).fill(panel).stroke(rule, 0.02).place(stage.inset((0, "32px")), anchor="left")


def line(stage, text="Mind the gap", size=0.8):
    """A line of text on the left of ``stage``, a little in from its edge."""
    return scene.text(text, size=size).place(stage.inset((0, "32px")), anchor="left")


# surrounding_rect outlines its targets' bounds, padding away (0.12 by
# default), in the theme's foreground; several targets share one frame and
# a text selection frames just its glyphs. Restyle it like any drawable: a
# translucent fill behind the text makes a background highlight. Each label
# abbreviates scene.geometry.surrounding_rect(...).
scene.segment("frames")
stages = rows([
    "surrounding_rect(box)",
    "surrounding_rect(box, padding=0.3,\n                 corner_radius=0.3)",
    "surrounding_rect([box, dot])",
    'surrounding_rect(line["gap"])',
    'surrounding_rect(line["gap"])\n    .stroke(accent, 0.06)',
    "surrounding_rect(line)\n    .fill(accent).opacity(0.35).z_index(-1)",
], ("2fr", "1fr"))
boxed = card(stages[0])
padded = card(stages[1])
pair = card(stages[2], 2.0)
spot = geometry.dot(0.22).fill(accent).move_to(pair.bounds().right + 1.2, stages[2].center[1])
selected = line(stages[3])
styled = line(stages[4])
shaded = line(stages[5])
frames = [
    geometry.surrounding_rect(boxed),
    geometry.surrounding_rect(padded, padding=0.3, corner_radius=0.3),
    geometry.surrounding_rect([pair, spot]),
    geometry.surrounding_rect(selected["gap"]),
    geometry.surrounding_rect(styled["gap"]).stroke(accent, 0.06),
    geometry.surrounding_rect(shaded).fill(accent).opacity(0.35).z_index(-1),
]
scene.play([frame.animate.create() for frame in frames], duration=DRAW)
scene.wait(HOLD)


# A frame follows its targets wherever they go, and retarget tweens its four
# edges to new targets, which it follows from then on.
def energy(stage):
    """E = m c^2, with the parts mass and light, on the left of ``stage``."""
    return scene.text.equation("E = ", part("mass", "m"), part("light", "c^2"), size=1.1).place(
        stage.inset((0, "32px")), anchor="left"
    )


scene.segment("retarget")
stages = rows([
    'frame.retarget(eq["light"])',
    'frame.retarget([eq["mass"], eq["light"]])',
    "eq.animate.shift_by(1.5, 0)",
])
equations = [energy(stage) for stage in stages]
frames = [geometry.surrounding_rect(eq["mass"]).stroke(accent, 0.05) for eq in equations]
scene.wait(HOLD)
scene.play(
    [
        frames[0].retarget(equations[0]["light"]),
        frames[1].retarget([equations[1]["mass"], equations[1]["light"]]),
        equations[2].animate.shift_by(1.5, 0),
    ],
    duration=DRAW,
)
scene.wait(HOLD)


# animated_boundary frames a drawable with a stroke that cycles through
# colors, cycle_rate turns of the list per second (the last color blends
# back into the first). It is visible at once: fade it in like any
# drawable. warm is [accent, danger] and three is [accent, success, warning].
scene.segment("boundaries")
warm, three = [accent, danger], [accent, success, warning]
stages = rows([
    "box.animated_boundary(warm)",
    "box.animated_boundary(three, cycle_rate=2.0)",
    "box.animated_boundary(warm, width=0.1,\n    padding=0.3, corner_radius=0.3)",
    "box.animated_boundary(three)\n    .animate.fade_in()",
], ("2fr", "1fr"))
boxes = [card(stage, 3.0, 1.1) for stage in stages]
boxes[0].animated_boundary(warm)
boxes[1].animated_boundary(three, cycle_rate=2.0)
boxes[2].animated_boundary(warm, width=0.1, padding=0.3, corner_radius=0.3)
faded = boxes[3].animated_boundary(three)
scene.play([faded.animate.fade_in()], duration=DRAW)
scene.wait(2 * HOLD)


# marker sweeps a highlighter band behind the selected glyphs, one band per
# line, and leaves it there; the static form draws it at once. color,
# opacity, skew (radians) and padding style the band; blend="multiply" inks
# it into a light card under the text. gap is line["gap"].
scene.segment("marker")
stages = rows([
    "gap.animate.marker()",
    "gap.animate.marker(accent)",
    "gap.animate.marker(accent, skew=0.2)",
    "gap.animate.marker(accent, opacity=0.9,\n                   padding=0.15)",
    "gap.marker(success)",
    'gap.animate.marker(accent,\n                   blend="multiply")',
    "lines.words[1:5].animate.marker(accent)",
])
texts = [line(stage, size=0.72) for stage in stages[:5]]
paper = geometry.rounded_rect(4.8, 0.95, 0.1).fill(foreground).place(stages[5].inset((0, "32px")), anchor="left")
inked = scene.text("Mind the gap", size=0.72, color=background).move_to(paper)
wrapped = line(stages[6], "Mind the gap\nbetween lines", size=0.5)
texts[4]["gap"].marker(success)
scene.wait(HOLD)
scene.play(
    [
        texts[0]["gap"].animate.marker(),
        texts[1]["gap"].animate.marker(accent),
        texts[2]["gap"].animate.marker(accent, skew=0.2),
        texts[3]["gap"].animate.marker(accent, opacity=0.9, padding=0.15),
        inked["gap"].animate.marker(accent, blend="multiply"),
        wrapped.words[1:5].animate.marker(accent),
    ],
    duration=DRAW,
)
scene.wait(HOLD)


# scene.fx.spotlight dims everything outside its target, to dim at most,
# and lights it all again: padding and corner_radius shape the hole, which
# can frame a text selection or the joint bounds of several targets. With
# Easing.SMOOTH the dim stays at the end. Each card is labelled with the
# call that lights it, abbreviating scene.fx.spotlight(...).
scene.segment("spotlight")
grid = scene.layout.zones(Zones.grid(rows=3, columns=2, gap="64px", padding="page_padding"))
CALLS = [
    "spotlight(card)",
    "spotlight(card, dim=0.95)",
    "spotlight(card, padding=0.5,\n          corner_radius=0.5)",
    'spotlight(line["gap"])',
    "spotlight([card, cards[5]])",
    "spotlight(card)\n    .easing(Easing.SMOOTH)",
]
cards = []
for zone, call in zip(grid, CALLS):
    box = zone.inset("24px")
    cards.append(geometry.rounded_rect(box.width, box.height, 0.12).fill(panel).stroke(rule, 0.02).place(box))
    scene.text(call, role="code").place(box.inset("24px"), anchor="top_left")
gap_line = scene.text("Mind the gap", size=0.7).place(grid[3].inset("48px"), anchor="bottom_right")
fx = scene.fx
for light in [
    fx.spotlight(cards[0]),
    fx.spotlight(cards[1], dim=0.95),
    fx.spotlight(cards[2], padding=0.5, corner_radius=0.5),
    fx.spotlight(gap_line["gap"]),
    fx.spotlight([cards[4], cards[5]]),
    fx.spotlight(cards[5]).easing(Easing.SMOOTH),
]:
    scene.play([light], duration=LIGHT)
scene.wait(HOLD)

RETARGET = DRAW + HOLD
BOUNDARIES = RETARGET + HOLD + DRAW + HOLD
MARKER = BOUNDARIES + DRAW + 2 * HOLD
SPOTLIGHT = MARKER + HOLD + DRAW + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Frames drawn; retargets midway and done; boundaries at three moments;
    # bands midway and done; every spotlight at its darkest, and the end.
    scene.snapshots(
        snapshots,
        [DRAW / 2, RETARGET - 0.1]
        + [RETARGET + HOLD + DRAW / 2, BOUNDARIES - 0.1]
        + [BOUNDARIES + t for t in (0.3, 0.9, 2.0)]
        + [MARKER + HOLD + DRAW / 2, SPOTLIGHT - 0.1]
        + [SPOTLIGHT + index * LIGHT + LIGHT / 2 for index in range(6)] + [SPOTLIGHT + 6 * LIGHT + HOLD - 0.1],
    )
else:
    scene.render()
