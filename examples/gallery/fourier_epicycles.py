"""Five turning circles stacked on each other, the odd harmonics of a square wave, whose tip draws the wave as they spin.

Circle k has radius 4 / (k pi) and turns k times as fast as the first; each
one is centered on the tip of the one before, a polar point of a single
Parameter, the angle. The tip's height is carried to the right by a dashed
line, where a pen moving with the angle draws the sum of the five sines:
a square wave with the ripples of a truncated Fourier series.
"""

import math
import os

from gaanim import Easing, Scene, Zones, computed

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
muted = scene.colors.muted
mist = scene.colors.rule
geometry = scene.geometry

band, stage = scene.layout.zones(Zones.rows(["1fr", "5fr"], gap="24px", padding="page_padding"))
SCALE = stage.height * 0.36
center = (stage.left + 1.3 * SCALE, stage.center[1])
wave_left, wave_right = center[0] + 1.6 * SCALE, stage.right
TURNS = 2
HARMONICS = [1, 3, 5, 7, 9]

angle = scene.viz.parameter(0.0)
joint = center
circles, arms = [], []
for k in HARMONICS:
    radius = SCALE * 4 / (k * math.pi)
    circles.append(geometry.circle(radius).no_fill().stroke(mist, 0.02).follow(joint))
    tip = geometry.polar_point(joint, radius, computed(lambda a, k=k: k * a, inputs=[angle]))
    arms.append(geometry.tracking_line(joint, tip).stroke(violet if k == 1 else lilac, 0.035))
    joint = tip


def height(a):
    return center[1] + SCALE * sum(4 / (k * math.pi) * math.sin(k * a) for k in HARMONICS)


pen = geometry.dot(0.1).fill(gold).follow(geometry.point_ref(
    computed(lambda a: wave_left + (wave_right - wave_left) * a / (TURNS * math.tau), inputs=[angle]),
    computed(height, inputs=[angle]),
))
end = geometry.dot(0.08).fill(gold).follow(joint)
bridge = geometry.dashed_line(joint, pen).stroke(muted, 0.02)
wave = geometry.traced_path(pen).stroke(gold, 0.045)
axis = geometry.line(wave_left, center[1], wave_right, center[1]).stroke(mist, 0.02)
series = scene.text.equation(
    "f(t) = 4 / pi (sin t + (sin 3t) / 3 + (sin 5t) / 5 + (sin 7t) / 7 + (sin 9t) / 9)", size=0.6, color=muted
).place(band, anchor="center")

scene.play([circle.animate.create() for circle in circles] + [arm.animate.create() for arm in arms], duration=1.2)
scene.play([axis.animate.create(), end.animate.grow_from_center(), pen.animate.grow_from_center(),
            bridge.animate.fade_in(), series.animate.write()], duration=1.0)
scene.play(wave.animate.fade_in(), duration=0.01)
scene.play(angle.animate.set(TURNS * math.tau), duration=9.0, easing=Easing.LINEAR)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The circles, and the wave drawing over two periods.
    scene.snapshots(snapshots, [1.2, 3.4, 5.6, 7.8, 10.0, 12.2])
else:
    scene.render()
