"""Riemann sums that refine from four rectangles to sixty-four under a curve, their total closing in on the area.

Each sum is a riemann_sum of the plane, and each one morphs into the next
with transform_to. A Parameter holds the current total; it animates to each
new sum alongside the morph, and a readout compares it with the exact area,
which fills in once the rectangles are thin enough.
"""

import os

from gaanim import Axis, Direction, Easing, Scene, Zones

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
muted = scene.colors.muted


def f(x):
    return 0.3 * x**3 - 1.8 * x**2 + 2.6 * x + 1.6


A, B = 0.4, 4.6
EXACT = (0.075 * B**4 - 0.6 * B**3 + 1.3 * B**2 + 1.6 * B) - (0.075 * A**4 - 0.6 * A**3 + 1.3 * A**2 + 1.6 * A)


def midpoint_sum(n):
    width = (B - A) / n
    return sum(f(A + (k + 0.5) * width) for k in range(n)) * width


band, stage = scene.layout.zones(Zones.rows(["1fr", "5fr"], gap="24px", padding="page_padding"))
plane = scene.viz.cartesian_2d(
    Axis.linear(0, 5).ticks(1), Axis.linear(0, 4).ticks(1), width=stage.width * 0.8, height=stage.height * 0.85
).move_to(*stage.center)
curve = plane.plot(f).stroke(gold, 0.06)
COUNTS = [4, 8, 16, 32, 64]
sums = [plane.riemann_sum(f, (A, B), rectangles=n).fill(violet).opacity(0.75).stroke(lilac, 0.012) for n in COUNTS]
# The first sum morphs into each finer one; those only lend their shape.
for finer in sums[1:]:
    finer.hidden()
area = plane.area_under(f, (A, B)).fill(gold).opacity(0.35).hidden()

total = scene.viz.parameter(midpoint_sum(COUNTS[0]))
left, middle, right = band.split(Zones.columns(3, gap="48px"))
estimate = scene.viz.readout(total, label="$sum f(x_i) Delta x$", format=".4f", font_size=0.55, color=lilac)
exact = scene.viz.readout(EXACT, label="$integral f$", format=".4f", font_size=0.55, color=gold)
estimate.place(left, anchor="center")
exact.place(right, anchor="center")
count = scene.text(f"n = {COUNTS[0]}", role="code", color=muted).place(middle, anchor="center")

scene.play(plane.animate.create(), duration=0.8)
scene.play(curve.animate.create(), duration=1.0)
scene.play([sums[0].animate.grow_from_edge(Direction.DOWN), estimate.animate.fade_in(), exact.animate.fade_in(),
            count.animate.fade_in()], duration=0.8)
scene.wait(0.4)
rectangles, count_label = sums[0], count
for n, target in zip(COUNTS[1:], sums[1:]):
    label = scene.text(f"n = {n}", role="code", color=muted).place(middle, anchor="center").hidden()
    scene.play(
        [rectangles.animate.transform_to(target), total.animate.set(midpoint_sum(n)),
         count_label.animate.fade_out(), label.animate.fade_in()],
        duration=1.0,
        easing=Easing.SMOOTH,
    )
    count_label = label
    scene.wait(0.3)
scene.play([area.animate.fade_in(), rectangles.animate.fade_out()], duration=1.0)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Four rectangles, each refinement, and the exact area.
    scene.snapshots(snapshots, [3.0, 4.2, 5.5, 6.8, 8.1, 9.6])
else:
    scene.render()
