"""Stress test for deterministic particles (scene.fx.particles / confetti).

Twelve panels on a 4x3 grid probe the edges of the particle API so visual
baselines catch regressions: every emitter and particle shape, drag = 0 vs a
tiny drag (series-expansion path) vs a large one, zero / strong / sideways
gravity, scalar vs range arguments, size_end, fade 0 and 1, spin and flutter,
spread 0 and tau, single / list / gradient colors, an emitter anchored to a
drawable moving fast along a curve, bursts at a segment boundary, repeated
bursts, confetti with count=0 then burst(), a few thousand live particles,
and the expected validation errors listed in a text label.

Timeline: segment "one" from 0 to 3.5 s, segment "two" from 3.5 s (hard cut)
to 8.5 s. Continuous emitters stop at 4.0 s; everything is dead by 8.4 s.
"""

import math
import os

from gaanim import (
    BLUE, CORAL, CYAN, GOLD, GRAY, GREEN, ORANGE, PINK, PURPLE, RED, TEAL, WHITE,
    YELLOW, Brush, Easing, Emitter, Scene,
)

EMIT_FOR = 4.0  # seconds of continuous emission for every rate-based panel

scene = Scene(frame=(16, 9), background="#0b1020")
scene.segment("one")

# Panel centres: 4 columns x 3 rows, each about 4 x 3 units.
COLS = (-6.0, -2.0, 2.0, 6.0)
ROWS = (3.0, 0.0, -3.0)


def panel(col, row, title):
    """Faint frame + caption so a baseline shows which panel is which."""
    x, y = COLS[col], ROWS[row]
    frame = scene.geometry.rect(3.9, 2.9).no_fill().stroke(GRAY, 0.01).move_to(x, y).opacity(0.4)
    label = scene.text(title, role="caption", markup=False).scale_to(0.16).move_to(x, y + 1.3)
    return (x, y), [frame, label]


keep = []  # everything persisted across the segment boundary

# P1: point emitter, spread 0 (a single ray), drag 0, no gravity, scalar
# lifetime/size/speed, single color: a straight dotted line to the right.
(x, y), deco = panel(0, 0, "P1 point · spread 0 · drag 0 · scalars")
keep += deco
p1 = scene.fx.particles(
    Emitter.point().at((x - 1.8, y)), rate=10, duration=EMIT_FOR, lifetime=1.5,
    speed=2.0, direction=0.0, spread=0.0, size=0.08, fade=0.0, color=CYAN, seed=1,
)

# P2: circle rim emitter, squares, tiny drag (k*t < 1e-4: series path),
# strong gravity, spin range.
(x, y), deco = panel(1, 0, "P2 circle edge · square · drag 1e-7 · g=-8")
keep += deco
p2 = scene.fx.particles(
    Emitter.circle(0.5, edge=True).at((x, y + 0.6)), rate=40, duration=EMIT_FOR,
    lifetime=(0.4, 0.8), speed=(1.0, 3.0), spread=math.tau, gravity=(0, -8),
    drag=1e-7, size=(0.05, 0.1), spin=(-8.0, 8.0), shape="square", color=GOLD, seed=2,
)

# P3: filled circle emitter, triangles, large drag, sideways gravity, fast
# spin given as one number (spins both ways).
(x, y), deco = panel(2, 0, "P3 circle · triangle · drag 12 · g sideways")
keep += deco
p3 = scene.fx.particles(
    Emitter.circle(0.4).at((x - 1.2, y)), rate=50, duration=EMIT_FOR,
    lifetime=(0.8, 1.4), speed=(3.0, 6.0), spread=math.tau, gravity=(4.0, 0.0),
    drag=12.0, size=(0.06, 0.12), spin=6.0, shape="triangle", color=[RED, ORANGE, YELLOW],
    seed=3,
)

# P4: rect emitter at the panel top raining downward strips, fade=1 (fades
# over the whole life), size_end=0 (shrinks away), flutter.
(x, y), deco = panel(3, 0, "P4 rect · rect strips · fade 1 · size_end 0 · flutter")
keep += deco
p4 = scene.fx.particles(
    Emitter.rect(3.4, 0.1).at((x, y + 1.1)), rate=30, duration=EMIT_FOR,
    lifetime=(1.2, 1.6), speed=(0.5, 1.0), direction=-math.pi / 2, spread=0.4,
    gravity=(0, -1), size=(0.1, 0.16), size_end=0.0, fade=1.0, flutter=0.3,
    spin=(-3.0, 3.0), shape="rect", color=[PINK, PURPLE], seed=4,
)

# P5: tilted line emitter, streaks along the velocity, gradient Brush colors,
# fade 0 (particles pop out at death), speed/lifetime ranges.
(x, y), deco = panel(0, 1, "P5 line 0.6 rad · streak · gradient · fade 0")
keep += deco
p5 = scene.fx.particles(
    Emitter.line(2.5, angle=0.6).at((x, y - 0.6)), rate=45, duration=EMIT_FOR,
    lifetime=(0.5, 1.0), speed=(2.0, 5.0), spread=0.5, gravity=(0, -3), drag=0.8,
    size=(0.03, 0.06), fade=0.0, shape="streak",
    color=Brush.linear([BLUE, CYAN, WHITE], start=(0, 0), end=(1, 0)), seed=5,
)

# P6: anchor trail. A dot runs fast along an S-curve (yoyo, so the trail
# also reverses); particles leave from where the dot was at their birth.
(x, y), deco = panel(1, 1, "P6 anchor on a fast curve (trail)")
keep += deco
route = scene.geometry.bezier((x - 1.6, y - 0.9), [(x - 0.8, y + 2.0), (x + 0.8, y - 2.0)], (x + 1.6, y + 0.9))
route.no_fill().stroke(GRAY, 0.01).opacity(0.5)
runner = scene.geometry.dot(0.07).fill(WHITE).move_to(x - 1.6, y - 0.9)
keep += [route, runner]
p6 = scene.fx.particles(
    Emitter.circle(0.05).at(runner), rate=90, duration=EMIT_FOR, lifetime=(0.6, 0.9),
    speed=(0.05, 0.25), spread=math.tau, size=(0.03, 0.05), size_end=0.2,
    color=[GREEN, TEAL], seed=6,
)

# P7: burst-only emitter (rate 0) fired by a repeated animate.burst: three
# bursts at 0.0, 0.5 and 1.0 s if .repeat() expands into cycles.
(x, y), deco = panel(2, 1, "P7 rate 0 · animate.burst(40).repeat(3)")
keep += deco
p7 = scene.fx.particles(
    (x, y), rate=0, lifetime=(0.6, 1.0), speed=(1.0, 2.5), spread=math.tau,
    gravity=(0, -2), size=(0.04, 0.07), color=[CORAL, GOLD, WHITE], seed=7,
)

# P8: near the practical limit for a software renderer: about 3300 live
# particles (rate 3000/s x ~1.1 s life), tiny squares in a box.
(x, y), deco = panel(3, 1, "P8 ~3300 live particles")
keep += deco
p8 = scene.fx.particles(
    Emitter.rect(3.4, 2.2).at((x, y - 0.1)), rate=3000, duration=EMIT_FOR,
    lifetime=(1.0, 1.2), speed=(0.0, 0.2), spread=math.tau, size=0.02, fade=0.3,
    shape="square", color=[BLUE, CYAN], seed=8,
)

# P9: confetti with count=0 throws nothing until burst() in segment two.
(x, y), deco = panel(0, 2, "P9 confetti(count=0) → animate.burst at 3.5 s")
keep += deco
p9 = scene.fx.confetti(origin=(x, y - 1.4), count=0, seed=9, speed=(3.0, 5.0), spread=1.2)

# P10: burst fired with Drawable.burst exactly at the segment boundary.
(x, y), deco = panel(1, 2, "P10 Drawable.burst(50) at the boundary")
keep += deco
p10 = scene.fx.particles(
    (x, y), rate=0, lifetime=(0.8, 1.5), speed=(0.8, 2.0), spread=math.tau,
    size=(0.05, 0.09), shape="circle", color=PURPLE, seed=10,
)

# P11: "smoke": ranges everywhere, growth (size_end 3), long fade, narrow
# upward cone, gray palette.
(x, y), deco = panel(2, 2, "P11 smoke · size_end 3 · fade 0.6")
keep += deco
p11 = scene.fx.particles(
    Emitter.circle(0.25).at((x, y - 1.1)), rate=25, duration=EMIT_FOR,
    lifetime=(1.5, 2.2), speed=(0.4, 0.9), spread=0.6, size=(0.08, 0.16),
    size_end=3.0, fade=0.6, color=[GRAY, WHITE], seed=11,
)
p11.opacity(0.5)

# P12: validation errors. Each case must raise ValueError; the label lists
# every case with OK (raised as expected) or FAIL (accepted).
(x, y), deco = panel(3, 2, "P12 expected errors")
keep += deco
cases = {
    "fade=1.5": lambda: scene.fx.particles((0, 0), fade=1.5),
    "17 colors": lambda: scene.fx.particles((0, 0), color=[RED] * 17),
    "rate*life>1e5": lambda: scene.fx.particles((0, 0), rate=100000, lifetime=2.0),
    "burst(0)": lambda: p10.burst(0),
    "lifetime low>high": lambda: scene.fx.particles((0, 0), lifetime=(2.0, 1.0)),
}
lines = []
for name, attempt in cases.items():
    try:
        attempt()
    except ValueError:
        lines.append(f"OK   {name}")
    else:
        lines.append(f"FAIL {name}")
for index, line in enumerate(lines):
    keep.append(scene.text(line, role="caption", markup=False).scale_to(0.15).move_to(x, y + 0.7 - index * 0.35))

emitters = [p1, p2, p3, p4, p5, p6, p7, p8, p9, p10, p11]
scene.persist(*keep, *emitters)

# Segment one: the runner loops the curve (4 x 0.75 s, yoyo) while P7 bursts
# three times.
scene.play([
    runner.animate.move_along(route).duration(0.75).easing(Easing.LINEAR).repeat(4, yoyo=True),
    p7.animate.burst(40).duration(0.5).repeat(3),
])
scene.wait(0.5)  # cursor at 3.5 s

# Segment two starts with a hard cut at 3.5 s: both bursts happen exactly
# at the boundary.
scene.segment("two")
p10.burst(50)
scene.play([p9.animate.burst(60).duration(1.5)])  # cursor at 5.0 s
scene.wait(3.5)  # ends at 8.5 s; confetti (<= 3.6 s life) is dead by 7.1 s

SEEKS = [
    0.0,     # first births, every particle at age 0
    0.0999,  # just before P1's second birth at 0.1 s
    0.5,     # exactly at P7's second burst
    0.75,    # mid-life everywhere; runner at the end of its first pass
    1.0,     # exactly at P7's third burst
    2.2,     # steady state, runner on its way back (yoyo)
    3.5,     # exactly at the segment boundary: P9 and P10 burst now
    3.6,     # just after the boundary bursts
    4.3,     # continuous emission stopped at 4.0, last particles in flight
    5.5,     # confetti mid-air, P10 fading
    8.4,     # everything dead: no particles should remain
]

if snapshot_dir := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshot_dir, SEEKS)
else:
    scene.render()
