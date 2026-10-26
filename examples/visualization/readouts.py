"""Readouts: rolling counters, live numeric readouts, progress rings and countdowns.

Rolling numbers turn their digits like an odometer, readouts show a
reactive value with a label and a unit, and progress rings fill with a value
or count down seconds. Each label is the code that builds its variant.
"""

import os

from gaanim import Anchor, Easing, Scene, TextAnchor, Zones

scene = Scene()
viz = scene.viz
accent = scene.colors.accent
success = scene.colors.success

COUNT = 3.0
HOLD = 0.6


def table(labels, columns=("2fr", "1fr")):
    """One padded row per label, the code right-aligned on the left; return the cells on the right."""
    grid = list(scene.layout.zones(
        Zones.grid(rows=len(labels), columns=list(columns), column_gap="96px", padding="page_padding")
    ))
    for cell, label in zip(grid[0::2], labels):
        scene.text(label, role="code").place(cell, anchor="right")
    return grid[1::2]


# A rolling number keeps fixed-width digit cells and turns its wheels as
# the value changes: odometer wheels carry in the last unit, continuous
# ones all spin at their own speed. pad=" " reserves blank cells so the
# display keeps its width, and a negative value keeps a static minus sign.
scene.segment("counters")
cells = table([
    "rolling_number(98, min_digits=4)",
    'rolling_number(0, decimals=2,\n  prefix="$", group_separator=",")',
    'rolling_number(0, mode="continuous",\n  suffix=" km")',
    'rolling_number(0, min_digits=3,\n  pad=" ", show_plus=True)',
    'rolling_number(500, direction="down")',
])
counters = [
    viz.rolling_number(98, min_digits=4, font_size=0.8),
    viz.rolling_number(0, decimals=2, prefix="$", group_separator=",", font_size=0.8),
    viz.rolling_number(0, mode="continuous", suffix=" km", font_size=0.8),
    viz.rolling_number(0, min_digits=3, pad=" ", show_plus=True, font_size=0.8),
    viz.rolling_number(500, direction="down", font_size=0.8),
]
# Counters grow to the left, so they are aligned by their right edge.
for counter, cell in zip(counters, cells):
    counter.move_to(*cell.point("right"), anchor=Anchor.RIGHT)
scene.wait(0.3)
scene.play([
    counters[0].count_to(102, duration=COUNT).easing(Easing.LINEAR),
    counters[1].count_to(1240.5, duration=COUNT),
    counters[2].animate.set(125).duration(COUNT).easing(Easing.LINEAR),
    counters[3].count_to(42, duration=COUNT),
    counters[4].count_to(1250, duration=COUNT),
])
scene.wait(HOLD)
scene.play([
    counters[0].count_to(-12, duration=COUNT),
    counters[1].count_to(999.95, duration=COUNT),
    counters[2].count_to(0, duration=COUNT),
    counters[3].count_to(-7, duration=COUNT),
    counters[4].count_to(480, duration=COUNT),
])
scene.wait(HOLD)

# A readout shows a Parameter, a Computed, scene time or a function of
# inputs, with an optional label, "=" and unit. format is a Python format
# specification, and decimal_separator="," swaps the separators.
scene.segment("readouts")
cells = table([
    'readout(speed, label="v", unit="m/s")',
    'readout(lambda v: v * v, inputs=[speed],\n  label="v²", format=".1f")',
    'readout(share, format=".0%")',
    'readout(viz.time, label="t",\n  format=".2f", suffix=" s")',
    'readout(total, format=",.2f",\n  decimal_separator=",")',
    'readout(speed, label="v",\n  color=accent, font_size=0.8)',
])
speed = viz.parameter(0.0)
share = viz.parameter(0.0)
total = viz.parameter(0.0)
readouts = [
    viz.readout(speed, label="v", unit="m/s", font_size=0.6),
    viz.readout(lambda v: v * v, inputs=[speed], label="v²", format=".1f", font_size=0.6),
    viz.readout(share, format=".0%", font_size=0.6),
    viz.readout(viz.time, label="t", format=".2f", suffix=" s", font_size=0.6),
    viz.readout(total, format=",.2f", decimal_separator=",", font_size=0.6),
    viz.readout(speed, label="v", color=accent, font_size=0.8),
]
for readout, cell in zip(readouts, cells):
    x, y = cell.point("left")
    readout.move_to(x, y - 0.18, anchor=TextAnchor.BASELINE_LEFT)
scene.wait(0.3)
scene.play([speed.animate.set(12.5), share.animate.set(0.85), total.animate.set(1234.5)], duration=COUNT)
scene.wait(HOLD)

# A progress ring fills clockwise from 12 o'clock with a value in [0, 1]
# and rolls its percentage in the center; a countdown starts full and
# empties as its seconds run out.
scene.segment("progress")
cells = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="64px", padding="page_padding"))
LABELS = [
    "progress_ring(0)",
    "progress_ring(0,\n  decimals=1,\n  width=0.3)",
    "progress_ring(0,\n  label=False,\n  color=success)",
    "countdown(5)",
]
rings = [
    viz.progress_ring(0, radius=1.3),
    viz.progress_ring(0, radius=1.3, decimals=1, width=0.3),
    viz.progress_ring(0, radius=1.3, label=False, color=success),
    viz.countdown(5, radius=1.3, font_size=0.8),
]
for cell, ring, label in zip(cells, rings, LABELS):
    stage, caption = cell.split(Zones.columns(["1fr", "1fr"], gap="48px"))
    ring.move_to(*stage.center)
    scene.text(label, role="code").place(caption, anchor="left")
scene.wait(0.3)
scene.play([
    rings[0].animate.set(0.75).duration(COUNT),
    rings[1].animate.set(0.333).duration(COUNT),
    rings[2].animate.set(1.0).duration(COUNT),
    rings[3].count_down(),
])
scene.wait(HOLD)

COUNTERS_END = 0.3 + 2 * (COUNT + HOLD)
READOUTS_END = COUNTERS_END + 0.3 + COUNT + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The counters rising, settled, falling and settled; the readouts midway
    # and done; the rings midway, filled and the countdown at zero.
    scene.snapshots(
        snapshots,
        [0.3 + COUNT / 2, 0.3 + COUNT + 0.2, 0.3 + COUNT + HOLD + COUNT / 2, COUNTERS_END - 0.1]
        + [COUNTERS_END + 0.3 + COUNT / 2, READOUTS_END - 0.1]
        + [READOUTS_END + 0.3 + COUNT / 2, READOUTS_END + 0.3 + COUNT + 0.1, READOUTS_END + 0.3 + 5 + HOLD - 0.1],
    )
else:
    scene.render()
