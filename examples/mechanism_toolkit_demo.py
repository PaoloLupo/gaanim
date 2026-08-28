"""Reactive mechanism toolkit: supports, joints, bars, angles, and forces."""

import os
from math import pi

from gaanim import BLACK, BLUE, CYAN, Direction, GOLD, GREEN, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK, margin=0.6, theme="technical")
title = scene.text("Toolkit reactivo de mecanismos", role="title").to_edge(Direction.UP)

theta = scene.viz.parameter(-0.35)
pivot = (-4.5, 0.5625)
crank_tip = scene.geometry.polar_point(pivot, 1.6875, theta)
slider = scene.geometry.rounded_rect(1.3125, 0.775, 0.125).fill(BLACK).stroke(CYAN, 0.05).move_to(1, -1.125)
slider_point = slider.anchor_point()

support = scene.mechanics.fixed_support(pivot, direction=Direction.DOWN, size=0.65)
crank = scene.mechanics.bar_between(pivot, crank_tip, width=0.1375).stroke(GOLD, 0.1375)
coupler = scene.mechanics.bar_between(crank_tip, slider_point, width=0.1125).stroke(BLUE, 0.1125)
crank_joint = scene.mechanics.joint_at(crank_tip, color=GOLD)
slider_joint = scene.mechanics.joint_at(slider_point, kind="prismatic", axis=Direction.RIGHT, color=CYAN)

angle = scene.mechanics.angle_between(
    pivot,
    Direction.RIGHT,
    crank_tip,
    radius=1.025,
    label="$theta$",
    show_value=True,
    unit="deg",
    arrowheads="end",
    show_extensions=False,
    label_gap=0.275,
    color=GOLD,
)
force = scene.mechanics.vector_between(
    slider_point,
    scene.geometry.offset_point(slider_point, 0, -1.4375),
    label="$F$",
    color=GREEN,
)
slider_label = scene.text("corredera", role="caption").follow(slider, offset=(0, 0.725))

scene.play([
    title.animate.write(), support.animate.fade_in(), crank.animate.fade_in(), coupler.animate.fade_in(),
    crank_joint.animate.grow_from_center(), slider.animate.fade_in(), slider_joint.animate.fade_in(),
    angle.animate.fade_in(), force.animate.fade_in(), slider_label.animate.write(),
])
scene.play([theta.animate.set(pi * 0.72).duration(3.0), slider.animate.shift_by(2.875, 0).duration(3.0)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.2, 3.8])
else:
    scene.render()
