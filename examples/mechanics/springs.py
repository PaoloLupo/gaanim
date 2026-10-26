"""Springs: helical springs between moving endpoints, with their coils, radius, crossings and straight ends.

``scene.mechanics.spring_between`` regenerates a helix every frame between
two endpoints, keeping its radius while its pitch stretches with the
distance. Endpoints may be points, drawables or anchor points of moving and
rotating bodies. Each label is the code that builds the variant.
"""

import os

from gaanim import Anchor, Direction, Easing, Scene, Zones

STRETCH = 1.2
HOLD = 0.6

scene = Scene()
M = scene.mechanics
fg = scene.colors.foreground
panel = scene.colors.panel


def block(size=0.8):
    """A mass: a square block with the theme's panel fill."""
    return scene.geometry.rounded_rect(size, size, 0.08).fill(panel).stroke(fg, 0.04)


# Every row hangs a spring from a wall to the left face of a block that
# moves out, in and back. amplitude is the helix radius, crossing (0 to 1)
# folds each turn back into a loop, and the straight lengths sit at the ends.
scene.segment("options")
OPTIONS = [
    ("spring_between(wall, face)", {}),
    ("spring_between(wall, face, coils=4)", {"coils": 4}),
    ("spring_between(wall, face, coils=14)", {"coils": 14}),
    ("spring_between(wall, face, amplitude=0.3)", {"amplitude": 0.3}),
    ("spring_between(wall, face, crossing=0.5)", {"crossing": 0.5}),
    ("spring_between(wall, face, crossing=1.0)", {"crossing": 1.0}),
    ("spring_between(wall, face,\n    start_straight=0.8, end_straight=0.4)", {"start_straight": 0.8, "end_straight": 0.4}),
]
grid = scene.layout.zones(Zones.grid(rows=len(OPTIONS), columns=[9.0, "1fr"], column_gap="64px", padding="page_padding"))
parts, blocks = [], []
for index, (text, options) in enumerate(OPTIONS):
    scene.text(text, role="code", text_align="left").place(grid[2 * index], anchor="right")
    stage = grid[2 * index + 1]
    wall = (stage.left + 0.3, stage.center[1])
    mass = block(0.7).move_to(stage.left + 3.0, stage.center[1])
    parts += [
        M.fixed_support(wall, direction=Direction.RIGHT, size=0.7, ground_length=0.9),
        mass,
        M.spring_between(wall, mass.anchor_point(Anchor.LEFT), **options),
    ]
    blocks.append(mass)
scene.play([part.animate.fade_in() for part in parts], duration=0.4)
for shift in (1.4, -2.2, 0.8):
    scene.play([mass.animate.shift_by(shift, 0).duration(STRETCH).easing(Easing.SMOOTH) for mass in blocks])
scene.wait(HOLD)

# Springs in systems: a block that drops and bounces from a ceiling, two
# blocks between walls (w) that move apart, and a spring that holds the tip
# of a lever as it turns. Anchor points keep each end on its body.
scene.segment("systems")
cells = scene.layout.zones(Zones.columns(3, gap="48px", padding="page_padding"))
labels = [
    "spring_between(\n  ceiling,\n  m.anchor_point(\n    Anchor.TOP))",
    "spring_between(w, a)\nspring_between(a, b)\nspring_between(b, w)",
    "spring_between(\n  ground,\n  lever.anchor_point(\n    Anchor.RIGHT))",
]
stages = []
for zone, text in zip(cells, labels):
    label_zone, stage = zone.split(Zones.rows([2.0, "1fr"], gap="24px"))
    scene.text(text, role="code", text_align="left").place(label_zone, anchor="bottom")
    stages.append(stage)

# A block hanging from a ceiling, dropped with a springy easing.
stage = stages[0]
cx = stage.center[0]
ceiling = (cx, stage.top - 0.2)
hanging = block(1.0).move_to(cx, stage.top - 2.6)
hang = [
    M.fixed_support(ceiling, direction=Direction.DOWN, size=0.9, ground_length=2.4),
    hanging,
    M.spring_between(ceiling, hanging.anchor_point(Anchor.TOP), coils=10, amplitude=0.3),
]

# Two blocks between walls, joined in series.
stage = stages[1]
y = stage.center[1] + 0.6
left, right = (stage.left + 0.2, y), (stage.right - 0.2, y)
a = block(0.7).move_to(stage.left + 1.6, y)
b = block(0.7).move_to(stage.right - 1.6, y)
series = [
    M.fixed_support(left, direction=Direction.RIGHT, size=0.7, ground_length=1.0),
    M.fixed_support(right, direction=Direction.LEFT, size=0.7, ground_length=1.0),
    a,
    b,
    M.spring_between(left, a.anchor_point(Anchor.LEFT), coils=5, amplitude=0.2),
    M.spring_between(a.anchor_point(Anchor.RIGHT), b.anchor_point(Anchor.LEFT), coils=5, amplitude=0.2),
    M.spring_between(b.anchor_point(Anchor.RIGHT), right, coils=5, amplitude=0.2),
]

# A lever on a pin, its tip held by a spring to the ground below.
stage = stages[2]
pivot = (stage.left + 0.6, stage.center[1] + 0.6)
lever = scene.geometry.rounded_rect(3.2, 0.24, 0.12).fill(panel).stroke(fg, 0.04)
lever.move_to(pivot[0] + 1.5, pivot[1]).pivot(*pivot)
ground = (pivot[0] + 3.0, stage.bottom + 0.6)
lever_parts = [
    M.pin_support(pivot, size=0.8, ground_length=1.2),
    M.fixed_support(ground, size=0.8, ground_length=1.6),
    lever,
    M.joint_at(pivot, size=0.5),
    M.spring_between(ground, lever.anchor_point(Anchor.RIGHT), coils=8, amplitude=0.25, crossing=1.0),
]

scene.play([part.animate.fade_in() for part in hang + series + lever_parts], duration=0.4)
scene.play([
    hanging.animate.shift_by(0, -1.4).duration(2 * STRETCH).easing(Easing.spring(bounce=0.6)),
    a.animate.shift_by(-0.4, 0).duration(STRETCH).easing(Easing.THERE_AND_BACK),
    b.animate.shift_by(0.4, 0).duration(STRETCH).easing(Easing.THERE_AND_BACK),
    lever.animate.rotate_by(-0.5).duration(STRETCH).easing(Easing.SMOOTH),
])
scene.play([lever.animate.rotate_by(0.8).duration(STRETCH).easing(Easing.SMOOTH)])
scene.wait(HOLD)

# The blocks out, in and back, then the systems mid-swing, at the turn
# of the lever, and settled.
options_end = 0.4 + 3 * STRETCH + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [0.4 + STRETCH, 0.4 + 2 * STRETCH, options_end - 0.1]
        + [options_end + 0.4 + t for t in (STRETCH / 2, STRETCH, 2 * STRETCH + HOLD - 0.1)],
    )
else:
    scene.render()
