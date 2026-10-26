"""From the circle to the sine: one angle drives a point on a circle and the waves it draws.

A single Parameter, ``theta``, is the only thing that animates. The point on
the circle, its radius, the sine and cosine legs, the pens on the number line,
the dashed bridge and the three readouts are all computed from it every frame,
and two traced paths draw the sine and cosine waves as the angle turns.
"""

import math
import os

from gaanim import Axis, Easing, Scene, Zones, computed, stagger

TURN = 6.0

scene = Scene()
accent = scene.colors.accent
success = scene.colors.success
foreground = scene.colors.foreground
muted = scene.colors.muted
rule = scene.colors.rule

# Readouts in a band on top; below, the circle on the left and the number
# line from 0 to 2π across the rest of the width.
band, stage = scene.layout.zones(Zones.rows(["1fr", "5fr"], gap="24px", padding="page_padding"))
radius = stage.height * 0.35
cx, cy = stage.left + radius, stage.center[1]
start = stage.left + 2 * radius + 0.6

theta = scene.viz.parameter(0.0)
sine = computed(lambda angle: math.sin(angle), inputs=[theta])
cosine = computed(lambda angle: math.cos(angle), inputs=[theta])

# The circle, its axes and the point that turns on it: polar_point is where the
# angle theta puts a point at this radius, and follow keeps the dot on it.
scene.geometry.line(cx - radius - 0.2, cy, start, cy).stroke(rule, 0.02)
scene.geometry.line(cx, cy - radius - 0.2, cx, cy + radius + 0.2).stroke(rule, 0.02)
circle = scene.geometry.circle(radius).no_fill().stroke(foreground, 0.03).move_to(cx, cy)
tip = scene.geometry.polar_point((cx, cy), radius, theta)
foot = scene.geometry.point_ref(computed(lambda c: cx + radius * c, inputs=[cosine]), cy)
spoke = scene.geometry.tracking_line((cx, cy), tip).stroke(foreground, 0.03)
cos_leg = scene.geometry.tracking_line((cx, cy), foot).stroke(success, 0.06)
sin_leg = scene.geometry.tracking_line(foot, tip).stroke(accent, 0.06)
sweep = scene.geometry.always_redraw_arc(theta, cx, cy, 0.42, 0.0).fill(muted).stroke(muted, 0.02)
dot = scene.geometry.dot(0.12).fill(accent).follow(tip)

# The number line maps theta along its length; point_ref puts each pen at the
# angle, raised by the sine or the cosine, and traced_path draws where it goes.
axis = Axis.linear(0, 2 * math.pi).ticks(math.pi / 2).numbers("pi", denominator=2).style(color=muted, number_color=muted)
line = scene.viz.number_line(axis, length=stage.right - start)
line.drawable().move_to((start + stage.right) / 2, cy)
sine_pen = scene.geometry.dot(0.1).fill(accent).follow(
    line.point_ref(theta, normal_offset=computed(lambda s: radius * s, inputs=[sine]))
)
cosine_pen = scene.geometry.dot(0.08).fill(success).follow(
    line.point_ref(theta, normal_offset=computed(lambda c: radius * c, inputs=[cosine]))
)
sine_wave = scene.geometry.traced_path(sine_pen).stroke(accent, 0.05)
cosine_wave = scene.geometry.traced_path(cosine_pen).stroke(success, 0.03)
bridge = scene.geometry.dashed_line(tip, sine_pen).stroke(muted, 0.02)

# Live readouts of the angle and of the two legs, colored like them.
readouts = [
    scene.viz.readout(theta, label="$theta$", font_size=0.8),
    scene.viz.readout(sine, label="$sin theta$", format="z.2f", font_size=0.8, color=accent),
    scene.viz.readout(cosine, label="$cos theta$", format="z.2f", font_size=0.8, color=success),
]
for readout, zone in zip(readouts, band.split(Zones.columns(3, gap="48px"))):
    readout.place(zone, anchor="center")

scene.play(stagger(
    circle.animate.create().duration(0.6),
    line.animate.create().duration(0.6),
    *(item.animate.fade_in().duration(0.3) for item in (spoke, cos_leg, sin_leg, sweep, dot, sine_pen, cosine_pen, bridge)),
    *(readout.animate.fade_in().duration(0.3) for readout in readouts),
    total=0.7,
))
scene.play([wave.animate.fade_in().duration(0.01) for wave in (sine_wave, cosine_wave)])
# Animating theta is all it takes: everything above follows it.
turn_start = scene.cursor
scene.play(theta.animate.set(2 * math.pi).duration(TURN).easing(Easing.LINEAR))
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The build-up, then every eighth of the turn, and the finished waves.
    scene.snapshots(snapshots, [0.6] + [turn_start + TURN * k / 8 for k in range(1, 9)] + [turn_start + TURN + 0.8])
else:
    scene.render()
