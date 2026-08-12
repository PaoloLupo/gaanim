"""Reactive mechanism toolkit: supports, joints, bars, angles, and forces."""

import os
from math import pi

from gaanim import BLACK, BLUE, CYAN, Direction, GOLD, GREEN, WHITE, Scene


scene = Scene(1280, 720, background=BLACK, margin=48, theme="technical")
title = scene.text("Toolkit reactivo de mecanismos", role="title").to_edge(Direction.UP)

theta = scene.parameter(-0.35)
pivot = (-360.0, 45.0)
crank_tip = scene.polar_point(pivot, 135.0, theta)
slider = scene.rounded_rect(105, 62, 10).fill(BLACK).stroke(CYAN, 4).at(80, -90)
slider_point = slider.anchor_point()

support = scene.fixed_support(pivot, direction=Direction.DOWN, size=52)
crank = scene.bar_between(pivot, crank_tip, width=11).stroke(GOLD, 11)
coupler = scene.bar_between(crank_tip, slider_point, width=9).stroke(BLUE, 9)
crank_joint = scene.joint_at(crank_tip, color=GOLD)
slider_joint = scene.joint_at(slider_point, kind="prismatic", axis=Direction.RIGHT, color=CYAN)

angle = scene.angle_between(
    pivot,
    Direction.RIGHT,
    crank_tip,
    radius=82,
    label="$theta$",
    show_value=True,
    unit="deg",
    arrowheads="end",
    show_extensions=False,
    label_gap=22,
    color=GOLD,
)
force = scene.vector_between(
    slider_point,
    scene.offset_point(slider_point, 0, -115),
    label="$F$",
    color=GREEN,
)
slider_label = scene.text("corredera", role="caption").follow(slider, offset=(0, 58))

scene.play([
    title.write(), support.fade_in(), crank.fade_in(), coupler.fade_in(),
    crank_joint.grow_from_center(), slider.fade_in(), slider_joint.fade_in(),
    angle.fade_in(), force.fade_in(), slider_label.write(),
])
scene.play([theta.animate_to(pi * 0.72, duration=3.0), slider.move(230, 0).duration(3.0)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.2, 3.8])
else:
    scene.render()
