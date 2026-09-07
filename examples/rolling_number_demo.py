"""Rolling counters: carries, money, reverse motion and a shared scalar."""
import os

from gaanim import Easing, Scene, WHITE, CYAN, GOLD

scene = Scene(frame=(16, 9), background="#101827")
scene.text("ROLLING NUMBERS", size=0.45).fill(WHITE).move_to(0, 3.5)

odometer = scene.viz.rolling_number(98, min_digits=4, font_size=1.2, color=CYAN).move_to(0, 1.8)
money = scene.viz.rolling_number(
    1234.50, decimals=2, min_digits=4, prefix="$ ", group_separator=",",
    font_size=0.85, color=WHITE,
).move_to(0, -0.1)
continuous = scene.viz.rolling_number(
    0, min_digits=3, mode="continuous", direction="down", suffix=" km",
    font_size=0.85, color=GOLD,
).move_to(0, -2)

scene.wait(0.5)
scene.play([
    odometer.count_to(102, duration=4).easing(Easing.LINEAR),
    money.count_to(1240, duration=4).easing(Easing.SMOOTH),
    continuous.animate.set(125).duration(4).easing(Easing.LINEAR),
])
scene.wait(0.5)
scene.play([
    odometer.count_to(-12, duration=3),
    money.count_to(999.95, duration=3),
    continuous.count_to(0, duration=3),
])
scene.wait(0.5)
# The public parameter also works with computed(..., inputs=[odometer.parameter]),
# scene.viz.readout(...), or parameter.drive_from_samples(...).
odometer.set(0)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0, 2, 2.5, 4.5, 6, 8.25, 8.75, 2])
else:
    scene.render()
