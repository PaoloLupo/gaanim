"""Falloffs, one per panel: index, noise, a moving distance and two combined cursors."""

import os

from gaanim import BLUE, GOLD, PINK, WHITE, Distribution, Easing, Falloff, Scene

scene = Scene(frame=(16, 9), background="#0f172a")


def panel(cx, cy, color=BLUE):
    cell = scene.geometry.rounded_rect(0.3, 0.3, 0.06).fill(color).no_stroke()
    return scene.geometry.duplicate(cell, Distribution.grid(10, 6, 0.55, center=(cx, cy)))


# 1. Index: a staggered fade and size along the group, with no time in it.
ramp = panel(-4.0, 2.0)
ramp.drive("opacity", Falloff.index(easing=Easing.SMOOTH).remap(0.15, 1.0))
ramp.drive("scale", Falloff.index(reverse=True).remap(0.6, 1.2))

# 2. Noise: neighbors turn and tint alike, and the field drifts with time.
drift = panel(4.0, 2.0)
drift.drive("rotation", Falloff.noise(frequency=0.5, seed=2).remap(-0.8, 0.8))
drift.drive("fill", Falloff.noise(frequency=0.5, scale=0.4, seed=5).gradient(BLUE, PINK))

# 3. Distance to a moving dot, reaching far: a wave of scale follows it.
wave = panel(-4.0, -2.0)
sweep = scene.geometry.dot(0.08).fill(WHITE).move_to(-6.5, -2.0)
wave.drive("scale", Falloff.distance(sweep, radius=1.6, falloff="round").remap(1.0, 1.7))

# 4. Two cursors combined with maximum; the fill follows both.
pair = panel(4.0, -2.0)
left = scene.geometry.dot(0.08).fill(WHITE).move_to(1.5, -2.0)
right = scene.geometry.dot(0.08).fill(WHITE).move_to(6.5, -2.0)
both = Falloff.distance(left, radius=1.3).maximum(Falloff.distance(right, radius=1.3))
pair.drive("scale", both.remap(1.0, 1.6))
pair.drive("fill", both.gradient(BLUE, GOLD))

scene.play([
    sweep.animate.move_to(-1.5, -2.0).duration(3.0),
    left.animate.move_to(6.5, -2.0).duration(3.0),
    right.animate.move_to(1.5, -2.0).duration(3.0),
])
# clear_drive ends the drives here: the authored look comes back.
pair.clear_drive()
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0, 2.0, 3.0, 3.4])
else:
    scene.render()
