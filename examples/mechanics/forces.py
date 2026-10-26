"""Forces: vectors with readouts, forces from a magnitude or from components, moments, frames and a free body.

``vector_between`` draws an arrow between two endpoints with an optional
symbol and a readout of its length; ``force_at`` and
``force_from_components`` take physical magnitudes in newtons and a
``visual_scale`` that turns them into scene units, so the readout reports the
force, not the drawn length. Every input may be a reactive value, and every
part may be styled. Each label is the code that builds the variant.
"""

import math
import os

from gaanim import Anchor, Direction, Easing, Scene, Zones, computed

GROW = 2.0
HOLD = 0.6

scene = Scene()
M = scene.mechanics
fg = scene.colors.foreground
accent = scene.colors.accent
success = scene.colors.success
panel = scene.colors.panel


def rows(count):
    """A padded grid of ``count`` rows: a label column and a wider stage column."""
    return scene.layout.zones(Zones.grid(rows=count, columns=["5fr", "3fr"], column_gap="64px", padding="page_padding"))


def label(zone, text, anchor="right"):
    """A code label in ``zone``; the lines of a long one align to the left."""
    scene.text(text, role="code", text_align="left").place(zone, anchor=anchor)


def table(entries):
    """One row per (label, build): ``build(tail, tip)`` draws an arrow from tail to a tip that grows."""
    grid = rows(len(entries))
    reach = scene.viz.parameter(0.4)
    arrows = []
    for index, (text, build) in enumerate(entries):
        label(grid[2 * index], text)
        stage = grid[2 * index + 1]
        tail = (stage.left, stage.center[1] - 0.3)
        tip_x = computed(lambda f, x=stage.left, w=stage.width: x + f * w, inputs=[reach])
        arrows.append(build(tail, scene.geometry.point_ref(tip_x, tail[1])))
    scene.play([arrow.animate.fade_in() for arrow in arrows], duration=0.4)
    scene.play([reach.animate.set(1.0).duration(GROW).easing(Easing.SMOOTH)])
    scene.wait(HOLD)


def cells(count, label_height):
    """A padded row of ``count`` cells, each split into a label strip and a stage."""
    grid = scene.layout.zones(Zones.columns(count, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows([label_height, "1fr"], gap="16px")) for zone in grid]


# vector_between(a, b) draws a shaft and a solid head; label is a symbol (math
# between $ signs) and show_value reads the length, times scale. A readout
# sits label_gap from the shaft, measured to the middle of the text.
scene.segment("vectors")
table([
    ("vector_between(a, b)", lambda a, b: M.vector_between(a, b)),
    ('vector_between(a, b, label="$F$")', lambda a, b: M.vector_between(a, b, label="$F$")),
    (
        "vector_between(a, b, show_value=True,\n    label_gap=0.35)",
        lambda a, b: M.vector_between(a, b, show_value=True, label_gap=0.35),
    ),
    (
        'vector_between(a, b, label="$F$",\n    show_value=True, unit="N",\n    label_gap=0.35)',
        lambda a, b: M.vector_between(a, b, label="$F$", show_value=True, unit="N", label_gap=0.35),
    ),
    (
        'vector_between(a, b, show_value=True,\n    scale=100, format=".0f", unit="cm",\n    label_gap=0.35)',
        lambda a, b: M.vector_between(a, b, show_value=True, scale=100, format=".0f", unit="cm", label_gap=0.35),
    ),
])

# The readout's gap, size and color are options, and the shaft, head,
# label, number and unit of a ForceVector are drawables of their own.
scene.segment("styles")


def readout(style):
    """A vector with a symbol and a reading in newtons, restyled by ``style``."""

    def build(a, b):
        vector = M.vector_between(a, b, label="$F$", show_value=True, unit="N", label_gap=0.35)
        style(vector)
        return vector

    return build


table([
    ('vector_between(a, b, label="$F$",\n    label_gap=0.7)', lambda a, b: M.vector_between(a, b, label="$F$", label_gap=0.7)),
    (
        'vector_between(a, b, label="$F$",\n    font_size=0.8, label_gap=0.45)',
        lambda a, b: M.vector_between(a, b, label="$F$", font_size=0.8, label_gap=0.45),
    ),
    ('vector_between(a, b, label="$F$",\n    color=success)', lambda a, b: M.vector_between(a, b, label="$F$", color=success)),
    ("v.shaft.stroke(accent, 0.1)", readout(lambda v: v.shaft.stroke(accent, 0.1))),
    ("v.head.fill(accent)", readout(lambda v: v.head.fill(accent))),
    ("v.number.fill(accent)", readout(lambda v: v.number.fill(accent))),
])

# force_at takes a magnitude and a direction in radians; force_from_components
# takes x and y components. visual_scale is scene units per newton, so 40 N
# at 0.05 is 2 units long, and any input may be a Parameter. READOUT holds
# the shared keywords.
scene.segment("forces")
READOUT = {"visual_scale": 0.05, "show_value": True, "label_gap": 0.45}
magnitude = scene.viz.parameter(20.0)
angle = scene.viz.parameter(0.0)
fx, fy = scene.viz.parameter(50.0), scene.viz.parameter(-10.0)
FORCES = [
    ("force_at(o, 40, **READOUT)", 0.06, lambda o: M.force_at(o, 40, **READOUT)),
    ("force_at(o, magnitude, **READOUT)", 0.06, lambda o: M.force_at(o, magnitude, **READOUT)),
    (
        'force_at(o, 30, direction=angle,\n    visual_scale=0.05, label="$F$")',
        0.3,
        lambda o: M.force_at(o, 30, direction=angle, visual_scale=0.05, label="$F$", label_gap=0.3),
    ),
    ("force_from_components(o, fx, fy, **READOUT)", 0.06, lambda o: M.force_from_components(o, fx, fy, **READOUT)),
]
grid = scene.layout.zones(Zones.grid(rows=len(FORCES), columns=[9.4, "1fr"], column_gap="64px", padding="page_padding"))
parts = []
for index, (text, place, build) in enumerate(FORCES):
    label(grid[2 * index], text)
    stage = grid[2 * index + 1]
    body = scene.geometry.circle(0.2).fill(panel).stroke(fg, 0.04)
    body.move_to(stage.left + place * stage.width, stage.center[1] - 0.5)
    parts += [body, build(body)]
scene.play([part.animate.fade_in() for part in parts], duration=0.4)
scene.play([
    magnitude.animate.set(80.0).duration(GROW),
    angle.animate.set(1.3).duration(GROW),
    fx.animate.set(40.0).duration(GROW),
    fy.animate.set(25.0).duration(GROW),
])
scene.wait(HOLD)

# moment_about draws a curved arrow around a centre, counterclockwise unless
# direction="cw". It follows its centre: here every body moves right.
scene.segment("moments")
MOMENTS = [
    ("moment_about(c, 1.2)", lambda c: M.moment_about(c, 1.2)),
    ('moment_about(c, 1.2,\n  direction="cw")', lambda c: M.moment_about(c, 1.2, direction="cw")),
    ('moment_about(c, 1.2,\n  label="$M$",\n  color=accent)', lambda c: M.moment_about(c, 1.2, label="$M$", color=accent)),
]


def follow_bodies(variants):
    """Label each cell, put a body in it with its symbol, then move every body right."""
    parts, moves = [], []
    for (label_zone, stage), (text, build) in zip(cells(len(variants), 1.4), variants):
        label(label_zone, text, anchor="bottom")
        body = scene.geometry.circle(0.15).fill(fg).move_to(stage.center[0] - 0.6, stage.center[1])
        parts += [body, build(body)]
        moves.append(body.animate.shift_by(1.2, 0).duration(GROW).easing(Easing.SMOOTH))
    scene.play([part.animate.fade_in() for part in parts], duration=0.4)
    scene.play(moves)
    scene.wait(HOLD)


follow_bodies(MOMENTS)

# coordinate_frame_at draws an x axis toward x_direction and a y axis a
# quarter turn from it, of the given length.
scene.segment("frames")
follow_bodies([
    ("coordinate_frame_at(c,\n  Direction.RIGHT)", lambda c: M.coordinate_frame_at(c, Direction.RIGHT)),
    ("coordinate_frame_at(c,\n  Direction.UP_RIGHT,\n  length=1.6)", lambda c: M.coordinate_frame_at(c, Direction.UP_RIGHT, length=1.6)),
    ("coordinate_frame_at(c,\n  Direction.UP,\n  color=success)", lambda c: M.coordinate_frame_at(c, Direction.UP, length=1.6, color=success)),
])

# A free body: a pull that varies on its own (Parameter.add_updater_fn) drags
# the block, and the normal force and friction are computed from it. The
# normal force pushes up on the block from a point a reactive distance below.
scene.segment("free_body")
page = scene.layout.zones(Zones.columns([7.8, "1fr"], gap="48px", padding="page_padding"))
scene.text(
    "\n".join([
        "pull = scene.viz.parameter(40)",
        "pull.add_updater_fn(swing)",
        "n = computed(normal, inputs=[pull])",
        "f = computed(friction, inputs=[n])",
        "force_at(corner, pull, direction=A)",
        "force_from_components(below, 0, n)",
    ]),
    role="code",
    text_align="left",
    line_spacing=1.6,
).place(page[0], anchor="left")
stage = page[1]
W, A, MU, SCALE = 60.0, 0.35, 0.4, 0.04
block = scene.geometry.rounded_rect(2.6, 1.8, 0.1).fill(panel).stroke(fg, 0.05)
block.move_to(stage.left + 2.3, stage.center[1] + 0.8)
pull = scene.viz.parameter(40.0)


def swing(_value, _dt, elapsed):
    """A pull that rises and falls around 40 N."""
    return 40.0 + 15.0 * math.sin(2.5 * elapsed)


def normal(p):
    """The floor carries the weight less the lift of the pull."""
    return W - p * math.sin(A)


def friction(n):
    """Sliding friction, proportional to the normal force."""
    return MU * n


n = computed(normal, inputs=[pull])
f = computed(friction, inputs=[n])
contact = block.anchor_point(Anchor.BOTTOM, offset=(-0.7, 0))
below = scene.geometry.offset_point(contact, 0, computed(lambda value: -SCALE * value, inputs=[n]))
corner = block.anchor_point(Anchor.TOP_RIGHT)
loads = [
    M.force_at(corner, pull, direction=A, visual_scale=SCALE, label="$P$", show_value=True, label_gap=0.4, color=accent),
    M.force_at(block.anchor_point(Anchor.BOTTOM_LEFT), f, direction=math.pi, visual_scale=SCALE, label="$f$", label_gap=0.3),
    M.force_from_components(block, 0, -W, visual_scale=SCALE, label="$W$", label_gap=0.3),
    M.force_from_components(below, 0, n, visual_scale=SCALE, label="$N$", label_gap=0.3),
]
scene.play([block.animate.fade_in()] + [load.animate.fade_in() for load in loads], duration=0.5)
pull.add_updater_fn(swing)
scene.play([block.animate.shift_by(0.8, 0).duration(2 * GROW).easing(Easing.SMOOTH)])
pull.remove_updater()
scene.wait(HOLD)

# Each segment growing or moving, and settled; then the free body at
# three moments of its pull.
step = 0.4 + GROW + HOLD
body = 5 * step
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [step * index + t for index in range(5) for t in (0.4 + GROW / 2, step - 0.1)]
        + [body + 0.5 + 2 * GROW * f for f in (0.2, 0.5, 0.9)],
    )
else:
    scene.render()
