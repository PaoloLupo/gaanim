"""Brushes: solid, linear, radial and sweep paints, their extend modes, and gradients that animate.

``fill()`` and ``stroke()`` take a ``Brush`` wherever they take a color. A
gradient's coordinates are local to the drawable it paints, so the paint
moves, turns and stretches with it. Each label is the code that builds the
variant; names in capitals are colors and brushes defined in the script,
such as ``RAMP``, ``[success, accent, danger]`` from the theme.
"""

import math
import os

from gaanim import Brush, Scene, Zones

GROW = 0.6
MOVE = 1.6
HOLD = 1.2

scene = Scene()
background = scene.canvas.color("background")
foreground = scene.canvas.color("foreground")
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")
chart = scene.canvas.color("chart")
RAMP = [success, accent, danger]
TWO = [accent, danger]
ORB = [foreground, accent, background]
# The accent with no opacity, to fade a gradient out.
CLEAR = (accent.r, accent.g, accent.b, 0)


def label(code):
    """A variant label: the code, in the code role."""
    return scene.text(code, role="code", markup=False)


def card():
    """The rounded rectangle most variants paint, 4.4 x 1.6 around its own origin."""
    return scene.geometry.rounded_rect(4.4, 1.6, 0.18).no_stroke()


def disc():
    """The circle the radial and sweep variants paint, of radius 0.8 around its own origin."""
    return scene.geometry.circle(0.8).no_stroke()


def gallery(variants):
    """Place each (label, drawable) pair in a 3 x 2 grid, the drawable above its label."""
    grid = scene.layout.zones(Zones.grid(rows=3, columns=2, column_gap="96px", row_gap="36px", padding="page_padding"))
    for cell, (code, drawable) in zip(grid, variants):
        art, caption = cell.split(Zones.rows(["1fr", "88px"], gap="12px"))
        label(code).place(caption, anchor="top")
        drawable.place(art)
    return [drawable for _, drawable in variants]


def reveal(drawables):
    """Grow ``drawables`` in together, then hold them."""
    scene.play([drawable.animate.grow_from_center() for drawable in drawables], duration=GROW)
    scene.wait(HOLD)


# Linear gradients run from start to end, two points in the drawable's own
# coordinates (a card spans x from -2.2 to 2.2), with the colors spread
# evenly between them. A solid brush is the same as a plain color.
scene.segment("linear")
reveal(gallery([
    ("Brush.solid(accent)", card().fill(Brush.solid(accent))),
    (
        "Brush.linear(TWO,\n  start=(-2.2, 0), end=(2.2, 0))",
        card().fill(Brush.linear(TWO, start=(-2.2, 0), end=(2.2, 0))),
    ),
    (
        "Brush.linear(RAMP,\n  start=(-2.2, 0), end=(2.2, 0))",
        card().fill(Brush.linear(RAMP, start=(-2.2, 0), end=(2.2, 0))),
    ),
    (
        "Brush.linear(RAMP,\n  start=(-2.2, -0.8), end=(2.2, 0.8))",
        card().fill(Brush.linear(RAMP, start=(-2.2, -0.8), end=(2.2, 0.8))),
    ),
    (
        "Brush.linear(RAMP,\n  start=(0, -0.8), end=(0, 0.8))",
        card().fill(Brush.linear(RAMP, start=(0, -0.8), end=(0, 0.8))),
    ),
    (
        "Brush.linear([accent, CLEAR],\n  start=(-2.2, 0), end=(2.2, 0))",
        card().fill(Brush.linear([accent, CLEAR], start=(-2.2, 0), end=(2.2, 0))),
    ),
]))

# Radial gradients spread from a center out to a radius; sweep gradients
# turn counterclockwise around a center, from start_angle to end_angle in
# degrees.
scene.segment("radial_sweep")
reveal(gallery([
    ("Brush.radial(RAMP, radius=0.8)", disc().fill(Brush.radial(RAMP, radius=0.8))),
    ("Brush.sweep(RAMP + [success])", disc().fill(Brush.sweep(RAMP + [success]))),
    (
        "Brush.radial(ORB, radius=1.2,\n  center=(-0.3, 0.3))",
        disc().fill(Brush.radial(ORB, radius=1.2, center=(-0.3, 0.3))),
    ),
    (
        "Brush.sweep(RAMP,\n  start_angle=90, end_angle=270)",
        disc().fill(Brush.sweep(RAMP, start_angle=90, end_angle=270)),
    ),
    ("Brush.radial([accent, CLEAR],\n  radius=0.8)", disc().fill(Brush.radial([accent, CLEAR], radius=0.8))),
    (
        "Brush.sweep(RAMP + [success],\n  center=(0.4, 0.3))",
        disc().fill(Brush.sweep(RAMP + [success], center=(0.4, 0.3))),
    ),
]))

# extend decides what lies past the ends of the ramp: "pad" holds the end
# colors, "repeat" starts the ramp over and "reflect" mirrors it. Every ramp
# here is short: one unit long, half a unit of radius or a quarter turn.
scene.segment("extend")
grid = scene.layout.zones(
    Zones.grid(
        rows=["48px", "1fr", "1fr", "1fr"], columns=["11fr", "8fr", "8fr", "8fr"], column_gap="48px", row_gap="36px",
        padding="page_padding",
    )
)
MODES = ["pad", "repeat", "reflect"]
for index, mode in enumerate(MODES, start=1):
    label(f'extend="{mode}"').place(grid[index])
KINDS = [
    (
        "Brush.linear(TWO,\n  start=(-0.5, 0),\n  end=(0.5, 0))",
        lambda mode: scene.geometry.rounded_rect(3.4, 1.7, 0.16).no_stroke().fill(
            Brush.linear(TWO, start=(-0.5, 0), end=(0.5, 0), extend=mode)
        ),
    ),
    (
        "Brush.radial(TWO,\n  radius=0.5)",
        lambda mode: scene.geometry.circle(0.85).no_stroke().fill(Brush.radial(TWO, radius=0.5, extend=mode)),
    ),
    (
        "Brush.sweep(TWO,\n  end_angle=90)",
        lambda mode: scene.geometry.circle(0.85).no_stroke().fill(Brush.sweep(TWO, end_angle=90, extend=mode)),
    ),
]
shapes = []
for row, (code, build) in enumerate(KINDS, start=1):
    label(code).place(grid[4 * row], anchor="left")
    for column, mode in enumerate(MODES, start=1):
        shapes.append(build(mode).place(grid[4 * row + column]))
reveal(shapes)

# A brush paints strokes and text as well as fills. Its coordinates belong
# to the drawable, so the gradient turns and stretches along with it. A
# text's own coordinates start at its left end, so its ramp runs from x = 0.
scene.segment("paint_follows")
CARD = Brush.linear(RAMP, start=(-2.2, 0), end=(2.2, 0))
DISC = Brush.linear(RAMP, start=(-0.8, 0), end=(0.8, 0))
TEXT = Brush.linear(RAMP, start=(0, 0), end=(4.4, 0))
SWEEP = Brush.sweep(RAMP + [success])
turning = disc().fill(DISC)
stretching = disc().fill(DISC)
spinning = disc().no_fill().stroke(SWEEP, 0.2)
drawables = gallery([
    ('scene.text("Brush", size=1.5)\n  .fill(TEXT)', scene.text("Brush", size=1.5).fill(TEXT)),
    (".no_fill().stroke(CARD, 0.2)", card().no_fill().stroke(CARD, 0.2)),
    (".fill(DISC)\n.animate.rotate_by(math.pi / 2)", turning),
    (".fill(DISC)\n.animate.scale_to_3d(2.5, 1, 1)", stretching),
    (".no_fill().stroke(SWEEP, 0.2)\n.animate.rotate_by(math.pi)", spinning),
    (".fill(DISC)", disc().fill(DISC)),
])
scene.play([drawable.animate.grow_from_center() for drawable in drawables], duration=GROW)
scene.play(
    [
        turning.animate.rotate_by(math.pi / 2),
        stretching.animate.scale_to_3d(2.5, 1, 1),
        spinning.animate.rotate_by(math.pi),
    ],
    duration=MOVE,
)
scene.wait(HOLD)

# animate.fill and animate.stroke blend one paint into another: gradients of
# one kind blend their geometry and colors (stops are matched by position,
# so two can become three), and a solid color turns into a gradient or back.
# Blending different kinds, such as linear into radial, raises ValueError.
scene.segment("animated")
WARM = Brush.linear([accent, danger], start=(-2.2, 0), end=(2.2, 0))
COOL = Brush.linear([success, chart], start=(-2.2, 0), end=(2.2, 0))
RISING = Brush.linear([success, chart], start=(0, -0.8), end=(0, 0.8))
GLOW = Brush.radial(ORB, center=(-0.3, 0.3), radius=1.2)
SHIFTED = Brush.radial([foreground, danger, background], center=(0.35, -0.3), radius=1.6)
VARIANTS = [
    (".fill(WARM)\n.animate.fill(COOL)", card().fill(WARM), lambda shape: shape.animate.fill(COOL)),
    (".fill(WARM)\n.animate.fill(RISING)", card().fill(WARM), lambda shape: shape.animate.fill(RISING)),
    (".fill(WARM)\n.animate.fill(CARD)", card().fill(WARM), lambda shape: shape.animate.fill(CARD)),
    (".fill(accent)\n.animate.fill(COOL)", card().fill(accent), lambda shape: shape.animate.fill(COOL)),
    (".fill(GLOW)\n.animate.fill(SHIFTED)", disc().fill(GLOW), lambda shape: shape.animate.fill(SHIFTED)),
    (
        ".no_fill().stroke(accent, 0.06)\n.animate.stroke(CARD, 0.24)",
        card().no_fill().stroke(accent, 0.06),
        lambda shape: shape.animate.stroke(CARD, 0.24),
    ),
]
shapes = gallery([(code, shape) for code, shape, _ in VARIANTS])
scene.play([shape.animate.grow_from_center() for shape in shapes], duration=GROW)
scene.play([change(shape) for shape, (_, _, change) in zip(shapes, VARIANTS)], duration=MOVE)
scene.wait(HOLD)

STILL = GROW + HOLD
MOVING = GROW + MOVE + HOLD
SEEKS = [STILL * index + 1.2 for index in range(3)] + [
    3 * STILL + MOVING * index + t for index in range(2) for t in (0.6, 1.4, 3.0)
]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The three still segments settled; the two animated ones before,
    # during and after their changes.
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
