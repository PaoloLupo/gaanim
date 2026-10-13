"""A pen on the end of two turning arms that traces a rose, the second arm turning four times faster the other way.

The first arm turns about the center and the second about the first arm's
tip, both from one Parameter; each joint is a polar point of the one before.
traced_path follows the pen and keeps everything it has drawn, while a
second, fading trace shows the latest stroke.
"""

import math
import os

from gaanim import Easing, Scene, computed

scene = Scene(theme="gaanim")
violet = scene.canvas.color("accent")
gold = scene.canvas.color("title")
lilac = scene.canvas.color("chart")
paper = scene.canvas.color("foreground")
mist = scene.canvas.color("rule")
geometry = scene.geometry

center = (0.0, -0.1)
turn = scene.viz.parameter(0.0)
elbow = geometry.polar_point(center, 2.2, turn)
pen_point = geometry.polar_point(elbow, 1.4, computed(lambda t: -4 * t, inputs=[turn]))

orbit = geometry.circle(2.2).no_fill().stroke(mist, 0.02).move_to(*center)
arms = [
    geometry.tracking_line(center, elbow).stroke(violet, 0.06),
    geometry.tracking_line(elbow, pen_point).stroke(lilac, 0.05),
]
joints = [geometry.dot(0.1).fill(paper).move_to(*center), geometry.dot(0.09).fill(paper).follow(elbow)]
pen = geometry.dot(0.13).fill(gold).follow(pen_point)
rose = geometry.traced_path(pen).stroke(gold, 0.035)
stroke = geometry.traced_path(pen, dissipating_time=0.6).stroke(paper, 0.07)

scene.play([orbit.animate.create(), *(arm.animate.create() for arm in arms)], duration=0.8)
scene.play([*(joint.animate.grow_from_center() for joint in joints), pen.animate.grow_from_center()], duration=0.4)
scene.play([rose.animate.fade_in(), stroke.animate.fade_in()], duration=0.01)
scene.play(turn.animate.set(math.tau), duration=7.0, easing=Easing.LINEAR)
scene.play([arm.animate.fade_out() for arm in arms] + [orbit.animate.fade_out()], duration=0.6)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Along the turn, and the finished rose.
    scene.snapshots(snapshots, [1.4, 2.8, 4.6, 6.4, 8.2, 9.6])
else:
    scene.render()
