"""Editorial mechanism using anchored bars and labeled reactive dimensions."""

import os

from gaanim import BLACK, GRAY, RED, WHITE, Anchor, Scene

scene = Scene(1280, 720, background=WHITE)

# A nested frame proves that anchors use local bounds plus the full hierarchy.
outer = scene.geometry.rect(720, 190).no_fill().stroke(BLACK, 7)
inner = scene.geometry.rect(660, 130).fill(WHITE).stroke(BLACK, 4)
frame = scene.geometry.group([outer, inner]).at(0, 0)

left_support = (-420.0, 245.0)
right_support = (420.0, 245.0)
left_corner = frame.anchor_point(Anchor.TOP_LEFT)
right_corner = frame.anchor_point(Anchor.TOP_RIGHT)

left_bar = scene.mechanics.bar_between(left_support, left_corner, width=10)
right_bar = scene.mechanics.bar_between(right_support, right_corner, width=10)
left_pin = scene.geometry.dot(9).fill(WHITE).stroke(BLACK, 5).at(*left_support)
right_pin = scene.geometry.dot(9).fill(WHITE).stroke(BLACK, 5).at(*right_support)

mass = scene.geometry.rect(105, 105).fill(RED).stroke(BLACK, 5).at(-80, 0)
left_spring = (
    scene.mechanics.spring_between(
        frame.anchor_point(Anchor.LEFT, offset=(32, 0)),
        mass.anchor_point(Anchor.LEFT),
        coils=9,
        amplitude=14,
        crossing=1.0,
    )
    .no_fill()
    .stroke(BLACK, 4)
)
right_spring = (
    scene.mechanics.spring_between(
        mass.anchor_point(Anchor.RIGHT),
        frame.anchor_point(Anchor.RIGHT, offset=(-32, 0)),
        coils=9,
        amplitude=14,
        crossing=1.0,
    )
    .no_fill()
    .stroke(BLACK, 4)
)

bob = scene.geometry.circle(27).fill(BLACK).at(-145, -240)
pendulum = scene.mechanics.bar_between(
    mass.anchor_point(Anchor.CENTER),
    bob.anchor_point(Anchor.CENTER),
    width=9,
)

width_dimension = scene.mechanics.dimension_between(
    left_corner,
    right_corner,
    105,
    label="$W_f$",
    show_value=True,
    format=".0f",
    unit="$u$",
    label_gap=18,
    color=BLACK,
    line_width=3,
    extension_style="dashed",
    dash_length=12,
    gap_length=8,
)
displayed_x = scene.viz.parameter(2.5)
mass_dimension = scene.mechanics.dimension_between(
    mass.anchor_point(Anchor.TOP_LEFT),
    mass.anchor_point(Anchor.TOP_RIGHT),
    50,
    label="$x$",
    value=displayed_x,
    format=".1f",
    unit="$m$",
    label_orientation="aligned",
    color=BLACK,
)

centerline = scene.geometry.dashed_line(-470, 0, 470, 0, dash_length=14, gap_length=10).stroke(GRAY, 2)

scene.play(
    [
        frame.write().duration(0.9),
        centerline.create().duration(0.6),
        left_bar.fade_in().duration(0.4),
        right_bar.fade_in().duration(0.4),
        left_pin.grow_from_center().duration(0.35),
        right_pin.grow_from_center().duration(0.35),
        mass.create().duration(0.5),
        left_spring.fade_in().duration(0.4),
        right_spring.fade_in().duration(0.4),
        pendulum.fade_in().duration(0.35),
        bob.grow_from_center().duration(0.35),
        width_dimension.fade_in().duration(0.5),
        mass_dimension.fade_in().duration(0.4),
    ]
)
scene.wait(0.5)
scene.play(
    [
        frame.move(55, 0).duration(1.4).smooth(),
        mass.move(185, 0).duration(1.4).smooth(),
        bob.move(245, 35).duration(1.4).smooth(),
        displayed_x.animate_to(4.0, duration=1.4),
    ]
)
scene.wait(1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.8, 1.4, 2.0])
else:
    scene.render()
