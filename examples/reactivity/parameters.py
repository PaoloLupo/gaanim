"""Parameters: animatable values, values computed from them, and the properties they drive.

A Parameter is an invisible number that you animate; a Variable is one that
also shows itself; ``computed`` derives a new value from others, the timeline
time included. Any absolute setter (``move_to``, ``scale_to``, ``rotate_to``,
``opacity``...) accepts these sources and follows them every frame, until a
plain number or another source replaces the binding. Each row or panel is
labelled with its code.
"""

import math
import os

from gaanim import Easing, Scene, Zones, computed

DURATION = 2.0
HOLD = 0.6

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
foreground = scene.colors.foreground
rule = scene.colors.rule


def code(text):
    """A code label, its lines aligned left; the code role keeps ``*`` and ``$`` as typed."""
    return scene.text(text, role="code", text_align="left")


def table(count):
    """``count`` rows of a label, a track and a value, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=["8.4fr", "3.8fr", "2fr"], column_gap="48px", padding="page_padding")
    )


def gauge(grid, row, label, source, value=None):
    """Label row ``row`` and slide a dot along its track as ``source`` goes from 0 to 1.

    The value column shows ``value`` when given, otherwise a readout of ``source``.
    """
    code(label).place(grid[3 * row], anchor="right")
    stage = grid[3 * row + 1]
    (left, y), (right, _) = stage.point("left"), stage.point("right")
    scene.geometry.line(left, y, right, y).stroke(rule, 0.03)
    for x in (left, right):
        scene.geometry.rect(0.03, 0.4).fill(rule).move_to(x, y)
    scene.geometry.dot(0.2).move_to(computed(lambda v: left + (right - left) * v, inputs=[source]), y)
    shown = value or scene.viz.readout(source, format="z.2f", font_size=0.7)
    shown.place(grid[3 * row + 2], anchor="left")


# A Parameter starts at its initial value and changes only when you say so:
# animated with animate.set (or keyframes), or cut with set at the cursor,
# here halfway through. A Variable is a Parameter that also draws "label = value".
parameters_start = scene.cursor
scene.segment("parameters")
grid = table(4)
glide, cut, keyed = scene.viz.parameter(0), scene.viz.parameter(0), scene.viz.parameter(0)
shown = scene.viz.variable(0, label="$v$", font_size=0.7)
gauge(grid, 0, "p.animate.set(1)", glide)
gauge(grid, 1, "p.set(1)", cut)
gauge(grid, 2, "p.animate.keyframes([0, 0.5, 1],\n    values=[None, 1, 0.4])", keyed)
gauge(grid, 3, 'v = scene.viz.variable(0, label="$v$")\nv.animate.set(1)', shown, value=shown)
scene.launch([
    glide.animate.set(1),
    keyed.animate.keyframes([0, 0.5, 1], values=[None, 1, 0.4]),
    shown.animate.set(1),
], duration=DURATION)
scene.wait(DURATION / 2)
cut.set(1)
scene.wait(DURATION / 2 + HOLD)


def swing(t):
    """A value that swings between 0 and 1 with the timeline time."""
    return 0.5 + 0.5 * math.sin(3 * t)


# computed(function, inputs=[...]) calls the function with the inputs' values,
# in order, whenever they change: any pure Python function of parameters,
# variables, other computed values or the timeline time.
computed_start = scene.cursor
scene.segment("computed")
grid = table(5)
p = scene.viz.parameter(0)
gauge(grid, 0, "p.animate.set(1)", p)
gauge(grid, 1, "computed(lambda v: v * v, inputs=[p])", computed(lambda v: v * v, inputs=[p]))
gauge(grid, 2, "computed(lambda v: 1 - v, inputs=[p])", computed(lambda v: 1 - v, inputs=[p]))
gauge(grid, 3, "computed(math.sqrt, inputs=[p])", computed(math.sqrt, inputs=[p]))
gauge(grid, 4, "computed(swing, inputs=[scene.time])", computed(swing, inputs=[scene.time]))
scene.play(p.animate.set(1).duration(DURATION))
scene.wait(HOLD)

# The setters that take a number take a source too, and follow it from the
# cursor on. Every shape here follows t; its outline marks where t = 1 leaves it.
properties_start = scene.cursor
scene.segment("properties")
panels = scene.layout.zones(Zones.grid(rows=3, columns=2, gap="48px", padding="page_padding"))
t = scene.viz.parameter(0.2)


def panel(index, label):
    """The stage of panel ``index``, under its code ``label``."""
    head, stage = panels[index].split(Zones.rows([0.5, "1fr"], gap="12px"))
    code(label).place(head, anchor="center")
    return stage


def outlined(stage, width, height, rotation=0.0):
    """A width x height rectangle in ``stage``, over an outline of its pose at t = 1."""
    scene.geometry.rect(width, height).no_fill().stroke(rule, 0.03).move_to(*stage.center).rotate_to(rotation)
    return scene.geometry.rect(width, height).move_to(*stage.center)


outlined(panel(0, "square.opacity(t)"), 1.6, 1.6).opacity(t)
outlined(panel(1, "square.scale_to(t)"), 1.6, 1.6).scale_to(t)
outlined(panel(2, "bar.rotate_to(t)"), 2.0, 0.3, rotation=1.0).rotate_to(t)
outlined(panel(3, "square.scale_to_3d(t, 1, 1)"), 1.6, 1.6).scale_to_3d(t, 1, 1)
stage = panel(4, "square.move_to(slide, y)")
(start, y), end = stage.point("left"), stage.right - 1.6
scene.geometry.square(1.6).no_fill().stroke(rule, 0.03).move_to(end + 0.8, y)
slide = computed(lambda v: start + 0.8 + (end - start) * v, inputs=[t])
scene.geometry.square(1.6).move_to(slide, y)
scene.viz.readout(t, label="$t$", font_size=1.0).place(panel(5, "scene.viz.readout(t)"), anchor="center")
scene.play(t.animate.set(1).duration(DURATION))
scene.play(t.animate.set(0.2).duration(DURATION))
scene.wait(HOLD)

# Computed values chain: area reads two parameters and side reads area, so
# the square always has the area of the rectangle.
chains_start = scene.cursor
scene.segment("chains")
stage, listing = scene.layout.zones(Zones.rows(["5fr", "6fr"], gap="24px", padding="page_padding"))
lines = listing.split(Zones.grid(rows=6, columns=["6fr", "2fr"], column_gap="48px"))
w, h = scene.viz.parameter(1.0), scene.viz.parameter(1.0)
area = computed(lambda w, h: w * h, inputs=[w, h])
side = computed(math.sqrt, inputs=[area])
halves = stage.split(Zones.columns(2, gap="48px"))
scene.geometry.rect(1.4, 1.4).move_to(*halves[0].center).scale_to_3d(w, h, 1)
scene.geometry.square(1.4).fill(success).move_to(*halves[1].center).scale_to(side)
for row, (line, value, label) in enumerate([
    ("w = scene.viz.parameter(1.0)", w, "$w$"),
    ("h = scene.viz.parameter(1.0)", h, "$h$"),
    ("area = computed(lambda w, h: w * h, inputs=[w, h])", area, "$A$"),
    ("side = computed(math.sqrt, inputs=[area])", side, "$s$"),
    ("rect.scale_to_3d(w, h, 1)", None, None),
    ("square.scale_to(side)", None, None),
]):
    code(line).place(lines[2 * row], anchor="left")
    if value is not None:
        scene.viz.readout(value, label=label, font_size=0.6).place(lines[2 * row + 1], anchor="left")
scene.play(w.animate.set(2.4).duration(1.2))
scene.play(h.animate.set(0.5).duration(1.2))
scene.play([w.animate.set(1.2), h.animate.set(1.6)], duration=1.2)
scene.wait(HOLD)

# A binding lasts until the channel is set again at a later cursor: a number
# ends it there (reversibly, so seeking back restores it) and another source
# replaces it. Once ended, the channel can be animated again. Every square
# follows slide until the cut, when it reaches the dashed mark.
unbinding_start = scene.cursor
scene.segment("unbinding")
grid = scene.layout.zones(Zones.grid(rows=5, columns=["6fr", "4fr"], column_gap="48px", padding="page_padding"))
s = scene.viz.parameter(0)
CUT = 0.5
squares = []
for row, label in enumerate([
    "square.move_to(slide, y)",
    "square.move_to(slide, y)\nsquare.move_to(x, y)  # at the cut",
    "square.move_to(slide, y)\nsquare.move_to(back, y)  # at the cut",
    "square.move_to(slide, y)\nsquare.move_to(x, y)  # at the cut\nsquare.animate.shift_by(0, 0.45)",
]):
    code(label).place(grid[2 * row], anchor="right")
    stage = grid[2 * row + 1]
    (left, y), span = stage.point("left"), stage.width - 0.8
    scene.geometry.line(left, y, stage.right, y).stroke(rule, 0.03)
    mark = left + 0.4 + span * CUT
    scene.geometry.dashed_line(mark, y - 0.55, mark, y + 0.55).stroke(foreground, 0.02)
    square = scene.geometry.square(0.6)
    square.move_to(computed(lambda v, left=left, span=span: left + 0.4 + span * v, inputs=[s]), y)
    squares.append((square, left + 0.4, span, y))
# fill_level binds its level the same way; set_fill_level(0.25) fixes it.
code("fill = fill_level(mask, accent, s)\nfill.set_fill_level(0.25)  # at the cut").place(grid[8], anchor="right")
mask = scene.geometry.rounded_rect(1.4, 1.4, 0.3).no_fill().stroke(rule, 0.03).move_to(*grid[9].center)
fill = scene.geometry.fill_level(mask, accent, s)
scene.launch(s.animate.set(1).duration(DURATION).easing(Easing.LINEAR))
scene.wait(DURATION * CUT)
_, (stopped, x0, _, y), (turned, x1, span, y1), (freed, x2, _, y2) = squares
stopped.move_to(x0 + span * CUT, y)
turned.move_to(computed(lambda v, x1=x1, span=span: x1 + span * (2 * CUT - v), inputs=[s]), y1)
freed.move_to(x2 + span * CUT, y2)
scene.launch(freed.animate.shift_by(0, 0.45).duration(0.6))
fill.set_fill_level(0.25)
scene.wait(DURATION * (1 - CUT) + HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment early, around its middle and settled.
    scene.snapshots(
        snapshots,
        [parameters_start + at for at in (0.6, 1.3, 2.4)]
        + [computed_start + at for at in (0.7, 1.6, 2.4)]
        + [properties_start + at for at in (0.3, 1.9, 3.0)]
        + [chains_start + at for at in (1.1, 2.3, 4.0)]
        + [unbinding_start + at for at in (0.9, 1.4, 2.5)],
    )
else:
    scene.render()
