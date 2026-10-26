"""A segment between two points whose run, rise and length are measured live while the points move.

Four Parameters hold the coordinates of the two points. Everything else is
drawn from them: the segment, its dashed legs, and three dimension lines
whose readouts recompute every frame, so moving a point is all it takes to
measure again.
"""

import os

from gaanim import Easing, Scene, computed

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
paper = scene.colors.foreground
muted = scene.colors.muted

geometry = scene.geometry
viz = scene.viz
ax, ay = viz.parameter(-4.5), viz.parameter(-2.0)
bx, by = viz.parameter(2.5), viz.parameter(1.8)
a = geometry.point_ref(ax, ay)
b = geometry.point_ref(bx, by)
corner = geometry.point_ref(bx, ay)

legs = [
    geometry.dashed_line(a, corner).stroke(muted, 0.025),
    geometry.dashed_line(corner, b).stroke(muted, 0.025),
]
segment = geometry.tracking_line(a, b).stroke(gold, 0.06)
dots = [geometry.dot(0.16).fill(paper).follow(point) for point in (a, b)]

# Each dimension follows its two points; show_value adds the live distance.
mechanics = scene.mechanics
run = mechanics.dimension_between(a, corner, -0.7, label="Δx", show_value=True, color=lilac, side="below")
rise = mechanics.dimension_between(corner, b, -0.7, label="Δy", show_value=True, color=lilac, side="right")
length = mechanics.dimension_between(
    a, b, 0.6, show_value=True, color=violet, label_orientation="aligned", side="above"
)
slope = viz.readout(
    computed(lambda x0, y0, x1, y1: (y1 - y0) / (x1 - x0), inputs=[ax, ay, bx, by]),
    label="Δy / Δx",
    format=".2f",
    font_size=0.6,
    color=muted,
).move_to(-4.6, 3.2)

scene.play([dot.animate.grow_from_center() for dot in dots], duration=0.5)
scene.play(segment.animate.create(), duration=0.8)
scene.play([*(leg.animate.create() for leg in legs), *(dim.animate.fade_in() for dim in (run, rise, length))], duration=0.8)
scene.play(slope.animate.fade_in(), duration=0.4)
scene.wait(0.5)
# Moving the points is all it takes: the legs, dimensions and readout follow.
scene.play([bx.animate.set(5.0), by.animate.set(-0.6)], duration=1.6, easing=Easing.SMOOTH_SPRING)
scene.play([ax.animate.set(-2.0), ay.animate.set(-3.0)], duration=1.4, easing=Easing.SMOOTH)
scene.play([bx.animate.set(1.0), by.animate.set(2.6)], duration=1.6, easing=Easing.SMOOTH_SPRING)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Measured, then after each move.
    scene.snapshots(snapshots, [1.4, 2.9, 3.8, 5.3, 7.0, 8.4])
else:
    scene.render()
