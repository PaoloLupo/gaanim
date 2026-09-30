"""Stress test for follow-through: ``Anim.settle`` and ``Drawable.follow(delay=)``.

Probes the edges of both features so visual baselines catch regressions:
settle after every kind of easing, on non-positional channels, after
repeat/loop, inside sequence/stagger and at the 10 s cap; delayed followers
in long chains, followers of followers, holds before the delay elapses,
local offsets on a rotating and scaling leader, and a leader animated across
a segment boundary. Expected errors are listed in a label.
"""

import math
import os

from gaanim import (
    BLUE,
    CORAL,
    CYAN,
    GOLD,
    GRAY,
    GREEN,
    ORANGE,
    PINK,
    PURPLE,
    RED,
    TEAL,
    WHITE,
    YELLOW,
    Easing,
    EasingCurve,
    Scene,
    sequence,
    stagger,
)


scene = Scene(frame=(16, 9), background="#0f1729")
scene.segment("settle")
scene.text("Follow-through stress", role="title").fill(WHITE).move_to(0, 4.0)

# --- Expected errors ---------------------------------------------------------
probe = scene.geometry.dot(0.04).fill(GRAY).move_to(-7.8, -4.4)
error_leader = scene.geometry.dot(0.04).fill(GRAY).move_to(-7.6, -4.4)
results = []


def expect(name, error, action):
    try:
        action()
    except error:
        results.append(f"{name}: {error.__name__}")
    else:
        results.append(f"{name}: NOT RAISED")


expect("overshoot<0", ValueError, lambda: probe.animate.shift_by(1, 0).settle(overshoot=-0.1))
expect("frequency=0", ValueError, lambda: probe.animate.shift_by(1, 0).settle(frequency=0.0))
expect("decay=0", ValueError, lambda: probe.animate.shift_by(1, 0).settle(decay=0.0))
expect("delay<0", ValueError, lambda: probe.follow(error_leader, delay=-0.1))
expect("delay=nan", ValueError, lambda: probe.follow(error_leader, delay=float("nan")))
expect("tuple+delay", TypeError, lambda: probe.follow((0.0, 0.0), delay=0.2))
scene.text("  ·  ".join(results), size=0.2).fill(GRAY).move_to(0, -4.25)

# --- Row 1: settle after each easing (columns rise 1.6 units) ----------------
easings = [
    ("linear", Easing.LINEAR, {}),
    ("smooth", Easing.SMOOTH, {}),  # ends at rest: the average velocity is used
    ("back", Easing.back(), {}),  # overshoots by itself, then settles on top
    ("bouncy", Easing.BOUNCY, {}),  # spring overshoot plus settle
    ("ease_in", Easing.ease_in(EasingCurve.QUADRATIC), {}),  # final velocity 2x
    ("amp 0", Easing.LINEAR, {"overshoot": 0.0}),  # no bounce, no extension
    ("40 Hz", Easing.LINEAR, {"frequency": 40.0}),  # very high frequency
]
columns = []
for index, (name, _, _) in enumerate(easings):
    x = -7.0 + index * 1.05
    scene.text(name, size=0.2).fill(GRAY).move_to(x, 0.3)
    columns.append(scene.geometry.square(0.45).fill(BLUE).move_to(x, 0.9))

# --- Row 2: settle on scale, rotation, colour and opacity --------------------
scaler = scene.geometry.square(0.6).fill(TEAL).move_to(1.2, 1.6)
spinner = scene.geometry.rect(1.0, 0.3).fill(ORANGE).move_to(2.8, 1.6)
tinted = scene.geometry.circle(0.35).fill(RED).move_to(4.4, 1.6)
faded = scene.geometry.circle(0.35).fill(PURPLE).move_to(6.0, 1.6)

# --- Low decay: hits the 10 s cap; launched so it spans both segments --------
ring = scene.geometry.circle(0.25).fill(YELLOW).move_to(7.2, 3.3)
scene.persist(ring)
scene.launch(ring.animate.shift_by(-1.0, 0).duration(0.5).easing(Easing.LINEAR).settle(0.12, 1.2, 0.3))

# --- Followers ---------------------------------------------------------------
leader = scene.geometry.circle(0.3).fill(GOLD).move_to(-6.5, -2.6)
# Long chain: 12 dots, each 0.05 s further behind.
trail = [scene.geometry.circle(0.2 - 0.012 * i).fill(CORAL) for i in range(12)]
for i, dot in enumerate(trail):
    dot.follow(leader, delay=0.05 * (i + 1))
# Followers of followers: each link reads the previous follower 0.2 s back.
links = [scene.geometry.square(0.22).fill(GREEN)]
links[0].follow(leader, delay=0.2, offset=(0.0, -0.7))
for _ in range(3):
    link = scene.geometry.square(0.22).fill(GREEN)
    link.follow(links[-1], delay=0.2)
    links.append(link)
# Hold: a delay longer than the segment has run so far.
late = scene.geometry.circle(0.18).fill(PINK).follow(leader, delay=3.0, offset=(0.0, 0.7))
# delay=0 sits exactly on the leader (an outline around it).
exact = scene.geometry.circle(0.36).no_fill().stroke(WHITE, 0.04).follow(leader, delay=0.0)

# Local offset on a leader that rotates and scales: the satellite orbits late.
pivot = scene.geometry.rect(0.8, 0.4).fill(CYAN).move_to(-1.5, -1.2)
satellite = scene.geometry.dot(0.1).fill(WHITE).follow(pivot, offset=(1.0, 0.0), offset_space="local", delay=0.2)

followers = [*trail, *links, late, exact]
scene.persist(leader, *followers)
scene.play([f.animate.fade_in().duration(0.01) for f in [*followers, satellite]])

# Phase 1 (t = 0.01 to about 1.71): every settle variant at once; the leader
# itself settles, so its trail bounces too.
scene.play(
    [
        *[
            column.animate.shift_by(0, 1.6).duration(0.5).easing(easing).settle(**kwargs)
            for column, (_, easing, kwargs) in zip(columns, easings)
        ],
        scaler.animate.scale_to(1.6).duration(0.5).settle(0.15),
        spinner.animate.rotate_by(math.pi / 2).duration(0.5).easing(Easing.LINEAR).settle(0.1, 2.0, 5.0),
        tinted.animate.fill(GREEN).duration(0.5).easing(Easing.LINEAR).settle(0.2),
        faded.animate.opacity(0.3).duration(0.5).easing(Easing.LINEAR).settle(0.2),
        leader.animate.move_to(-2.5, -2.6).duration(0.6).easing(Easing.SMOOTH).settle(0.15, 2.5, 5.0),
        pivot.animate.rotate_by(math.pi).duration(1.2).easing(Easing.LINEAR),
        pivot.animate.scale_to(1.5).duration(1.2).easing(Easing.LINEAR),
    ]
)

# Phase 2 (about 1.71 to 3.54): settle after repeat/yoyo/loop, inside stagger
# and sequence, and a settle followed at once by another animation of the
# same channel.
scene.play(
    [
        # Ping-pong ends moving backwards, so it bounces the other way.
        columns[0].animate.shift_by(0, -0.8).duration(0.3).easing(Easing.LINEAR).repeat(2, yoyo=True).settle(),
        columns[1].animate.shift_by(0, -0.3).duration(0.3).loop("offset", until=0.9).settle(0.08),
        stagger(*[c.animate.shift_by(0, -1.2).duration(0.4).settle(0.1) for c in columns[2:]], each=0.08),
        sequence(
            spinner.animate.rotate_by(-math.pi / 2).duration(0.3).settle(0.1, 5.0, 8.0),
            spinner.animate.rotate_by(math.pi / 4).duration(0.3),
        ),
        sequence(
            scaler.animate.scale_to(0.8).duration(0.3).easing(Easing.LINEAR).settle(0.1),
            scaler.animate.scale_to(1.0).duration(0.3),
        ),
        leader.animate.move_to(3.0, -1.6).duration(0.8).easing(Easing.LINEAR).settle(0.06),
    ]
)

# Phase 3 (about 3.54 to 5.74): the leader keeps moving across a segment
# boundary (about 4.54). In the new segment every follower holds the leader's
# segment-start position until its delay has elapsed there.
scene.launch(leader.animate.move_to(-4.0, 1.0).duration(2.0).easing(Easing.LINEAR))
scene.wait(1.0)
scene.segment("trail")
scene.text("Across the segment boundary", size=0.3).fill(GRAY).move_to(0, 3.3)
scene.wait(1.2)

# Phase 4 (about 5.74 to 10.83): an overshooting easing plus settle on the
# leader, then rest until the capped low-decay ring (ends about 10.5) is done.
scene.play(leader.animate.move_to(4.0, 2.0).duration(0.7).easing(Easing.back()).settle(0.1, 4.0, 5.0))
scene.wait(3.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [
            0.30,  # mid-motion; long-delay followers still hold the start
            0.51,  # end of the 0.5 s motions: bounces start
            0.60,  # linear bounce near its first peak, 40 Hz mid-oscillation
            0.90,  # leader settling; colour/opacity/rotation swinging back
            2.20,  # phase 2: yoyo, loop, stagger and sequence under way
            3.00,  # stagger bounces, leader trail settling at its target
            3.30,  # the delay=3 follower has just been released
            4.50,  # just before the segment boundary
            4.58,  # just after it: followers hold the boundary position
            5.20,  # small delays caught up; delay=3 follower still holding
            6.60,  # back easing + settle on the leader, trail bouncing
            10.40,  # low-decay ring still oscillating near the 10 s cap
        ],
    )
else:
    scene.render()
