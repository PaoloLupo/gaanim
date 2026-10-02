"""Speed ramps and time remapping of whole compositions."""

import os

from gaanim import BLUE, CORAL, GOLD, TEAL, WHITE, Easing, EasingCurve, Scene, parallel, sequence, stagger


scene = Scene(frame=(16, 9))
scene.text("speed_ramp · time_remap", role="title").fill(WHITE).move_to(0, 3.3)

# Three steps that slow to 0.15x in the middle of the composition and
# speed back up: the middle step plays in slow motion.
dots = [scene.geometry.circle(0.3).fill(color).move_to(-6.0, 2.0 - 1.2 * i) for i, color in enumerate((GOLD, CORAL, TEAL))]
ramped = sequence(
    *(dot.animate.move_to(-1.0, dot_y).duration(0.8) for dot, dot_y in zip(dots, (2.0, 0.8, -0.4)))
).speed_ramp({0.0: 1.0, 0.4: 0.15, 0.6: 0.15, 1.0: 1.0})

# A stagger whose time eases in and out: it starts and ends slowly, its span unchanged.
bars = [scene.geometry.rect(0.3, 0.3).fill(BLUE).move_to(1.0 + 0.8 * i, -2.5) for i in range(7)]
remapped = stagger(*(bar.animate.shift_by(0, 4.5).duration(0.6) for bar in bars), each=0.25).time_remap(
    Easing.ease_in_out(EasingCurve.CUBIC)
)

scene.play(parallel(ramped, remapped))
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Fast start, the slow middle (the coral step), the speed-up, at rest.
    scene.snapshots(snapshots, [0.3, 1.0, 2.0, 3.5, 5.0, 6.5, 7.6])
else:
    scene.render()
