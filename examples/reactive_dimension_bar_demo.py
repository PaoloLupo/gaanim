"""Editorial mechanism using anchored bars and labeled reactive dimensions."""

import os

from gaanim import Easing, BLACK, GRAY, RED, WHITE, Anchor, Scene

scene = Scene(frame=(16, 9), background=WHITE)

# A nested frame proves that anchors use local bounds plus the full hierarchy.
outer = scene.geometry.rect(9, 2.375).no_fill().stroke(BLACK, 0.0875)
inner = scene.geometry.rect(8.25, 1.625).fill(WHITE).stroke(BLACK, 0.05)
frame = scene.geometry.group([outer, inner]).move_to(0, 0)

left_support = (-5.25, 3.0625)
right_support = (5.25, 3.0625)
left_corner = frame.anchor_point(Anchor.TOP_LEFT)
right_corner = frame.anchor_point(Anchor.TOP_RIGHT)

left_bar = scene.mechanics.bar_between(left_support, left_corner, width=0.125)
right_bar = scene.mechanics.bar_between(right_support, right_corner, width=0.125)
left_pin = scene.geometry.dot(0.1125).fill(WHITE).stroke(BLACK, 0.0625).move_to(*left_support)
right_pin = scene.geometry.dot(0.1125).fill(WHITE).stroke(BLACK, 0.0625).move_to(*right_support)

mass = scene.geometry.rect(1.3125, 1.3125).fill(RED).stroke(BLACK, 0.0625).move_to(-1, 0)
left_spring = (
    scene.mechanics.spring_between(
        frame.anchor_point(Anchor.LEFT, offset=(0.4, 0)),
        mass.anchor_point(Anchor.LEFT),
        coils=9,
        amplitude=0.175,
        crossing=1.0,
    )
    .no_fill()
    .stroke(BLACK, 0.05)
)
right_spring = (
    scene.mechanics.spring_between(
        mass.anchor_point(Anchor.RIGHT),
        frame.anchor_point(Anchor.RIGHT, offset=(-0.4, 0)),
        coils=9,
        amplitude=0.175,
        crossing=1.0,
    )
    .no_fill()
    .stroke(BLACK, 0.05)
)

bob = scene.geometry.circle(0.3375).fill(BLACK).move_to(-1.8125, -3)
pendulum = scene.mechanics.bar_between(
    mass.anchor_point(Anchor.CENTER),
    bob.anchor_point(Anchor.CENTER),
    width=0.1125,
)

width_dimension = scene.mechanics.dimension_between(
    left_corner,
    right_corner,
    1.3125,
    label="$W_f$",
    show_value=True,
    format=".0f",
    unit="$u$",
    label_gap=0.225,
    color=BLACK,
    line_width=0.0375,
    extension_style="dashed",
    dash_length=0.15,
    gap_length=0.1,
)
displayed_x = scene.viz.parameter(2.5)
mass_dimension = scene.mechanics.dimension_between(
    mass.anchor_point(Anchor.TOP_LEFT),
    mass.anchor_point(Anchor.TOP_RIGHT),
    0.625,
    label="$x$",
    value=displayed_x,
    format=".1f",
    unit="$m$",
    label_orientation="aligned",
    color=BLACK,
)

centerline = scene.geometry.dashed_line(-5.875, 0, 5.875, 0, dash_length=0.175, gap_length=0.125).stroke(GRAY, 0.025)

scene.play(
    [
        frame.animate.write().duration(0.9),
        centerline.animate.create().duration(0.6),
        left_bar.animate.fade_in().duration(0.4),
        right_bar.animate.fade_in().duration(0.4),
        left_pin.animate.grow_from_center().duration(0.35),
        right_pin.animate.grow_from_center().duration(0.35),
        mass.animate.create().duration(0.5),
        left_spring.animate.fade_in().duration(0.4),
        right_spring.animate.fade_in().duration(0.4),
        pendulum.animate.fade_in().duration(0.35),
        bob.animate.grow_from_center().duration(0.35),
        width_dimension.animate.fade_in().duration(0.5),
        mass_dimension.animate.fade_in().duration(0.4),
    ]
)
scene.wait(0.5)
scene.play(
    [
        frame.animate.shift_by(0.6875, 0).duration(1.4).easing(Easing.SMOOTH),
        mass.animate.shift_by(2.3125, 0).duration(1.4).easing(Easing.SMOOTH),
        bob.animate.shift_by(3.0625, 0.4375).duration(1.4).easing(Easing.SMOOTH),
        displayed_x.animate.set(4.0).duration(1.4),
    ]
)
scene.wait(1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.4, 2.0])
else:
    scene.render()
