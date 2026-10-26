"""An angle that opens and closes while its arc, its value in degrees and a fading trail follow the moving ray.

The moving ray ends at a polar point driven by one Parameter. angle_between
measures the angle between the two rays every frame and writes its value,
a dashed drop marks the height of the tip, and the tip leaves a trail that
fades behind it.
"""

import math
import os

from gaanim import Easing, Scene, computed

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
paper = scene.colors.foreground
geometry = scene.geometry

vertex = (0.0, -2.6)
LENGTH = 5.4
theta = scene.viz.parameter(math.radians(110))
tip = geometry.polar_point(vertex, LENGTH, theta)

fixed = geometry.line(vertex[0], vertex[1], vertex[0] + LENGTH, vertex[1]).stroke(paper, 0.06)
moving = geometry.tracking_line(vertex, tip).stroke(paper, 0.06)
measure = scene.mechanics.angle_between(
    vertex, (vertex[0] + LENGTH, vertex[1]), tip, radius=1.6, label="θ", show_value=True, format=".0f",
    font_size=0.6, color=gold,
)
end = geometry.dot(0.14).fill(gold).follow(tip)
trail = geometry.traced_path(end, dissipating_time=0.8).stroke(violet, 0.06)
pivot = geometry.dot(0.12).fill(paper).move_to(*vertex)
# The angle's sine, a dashed drop from the tip to the fixed ray.
foot = geometry.point_ref(computed(lambda t: vertex[0] + LENGTH * math.cos(t), inputs=[theta]), vertex[1])
drop = geometry.dashed_line(tip, foot).stroke(lilac, 0.03)

scene.play([fixed.animate.create(), moving.animate.create(), pivot.animate.grow_from_center()], duration=0.8)
scene.play([measure.animate.fade_in(), end.animate.fade_in(), drop.animate.fade_in(), trail.animate.fade_in()],
           duration=0.6)
for degrees, easing in ((40, Easing.SMOOTH_SPRING), (150, Easing.back()), (75, Easing.SMOOTH)):
    scene.play(theta.animate.set(math.radians(degrees)), duration=1.4, easing=easing)
    scene.wait(0.3)
scene.play(measure.animate.indicate(), duration=0.8)
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawn at 110 degrees, each new angle, and the value indicated.
    scene.snapshots(snapshots, [1.4, 2.4, 3.6, 4.6, 6.9, 7.8])
else:
    scene.render()
