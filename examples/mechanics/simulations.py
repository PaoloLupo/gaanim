"""Simulations: a damped pendulum, a double pendulum and a mass on a spring, stepped at a fixed rate.

``add_updater_fn(step, reset=reset, fixed_dt=...)`` integrates state that
cannot be written as a function of time. Gaanim calls ``reset`` and replays
the fixed steps whenever it seeks or exports, so every frame is the same in
the editor, in snapshots and in a video. Rods, springs, angles, forces and
traces built on the simulated bodies follow them in the same frame. The code
on the left of each segment is how it is built.
"""

import math
import os

from gaanim import Anchor, Direction, Scene, Zones

STEP = 1 / 240
HOLD = 0.4

scene = Scene()
M = scene.mechanics
fg = scene.colors.foreground
accent = scene.colors.accent
muted = scene.colors.muted
panel = scene.colors.panel


def page(lines):
    """Set the code ``lines`` in a left column and return the stage to its right."""
    zones = scene.layout.zones(Zones.columns([7.2, "1fr"], gap="48px", padding="page_padding"))
    scene.text("\n".join(lines), role="code", text_align="left", line_spacing=1.6).place(zones[0], anchor="left")
    return zones[1]


# A damped pendulum: step() advances the angle and its speed by one fixed
# step and returns the bob's position; reset() restores the start.
scene.segment("pendulum")
stage = page([
    "bob.add_updater_fn(step,",
    "    reset=reset, fixed_dt=1 / 240)",
    "bar_between(hinge, bob)",
    "angle_between(hinge,",
    "    Direction.DOWN, bob)",
    "traced_path(bob,",
    "    dissipating_time=1.5)",
])
LENGTH, GRAVITY, DAMPING, START = 3.2, 9.81, 0.25, 1.1
hinge = (stage.center[0], stage.top - 1.2)
pendulum = {"angle": START, "speed": 0.0}


def bob_at(angle):
    """The bob's position for a rod at ``angle`` from the vertical."""
    return (hinge[0] + LENGTH * math.sin(angle), hinge[1] - LENGTH * math.cos(angle), 0.0)


def reset_pendulum():
    """Restore the starting angle and speed."""
    pendulum.update(angle=START, speed=0.0)


def step_pendulum(_position, dt, _elapsed):
    """One semi-implicit Euler step of a damped pendulum."""
    pendulum["speed"] += dt * (-GRAVITY / LENGTH * math.sin(pendulum["angle"]) - DAMPING * pendulum["speed"])
    pendulum["angle"] += dt * pendulum["speed"]
    return bob_at(pendulum["angle"])


bob = scene.geometry.circle(0.3).fill(accent).move_to(*bob_at(START)[:2])
parts = [
    M.pin_support(hinge, direction=Direction.DOWN, size=0.9, ground_length=1.8),
    M.bar_between(hinge, bob).stroke(fg, 0.08),
    M.angle_between(hinge, Direction.DOWN, bob, radius=1.2, label="$theta$", label_gap=0.25),
    scene.geometry.traced_path(bob, dissipating_time=1.5).no_fill().stroke(accent, 0.04),
]
bob.add_updater_fn(step_pendulum, reset=reset_pendulum, fixed_dt=STEP)
scene.play([part.animate.fade_in() for part in parts] + [bob.animate.fade_in()], duration=0.3)
scene.wait(5.0)
bob.remove_updater()
scene.wait(HOLD)

# A double pendulum: only the upper bob steps the shared state; the lower
# one reads it in the same frame, so the system is integrated once.
scene.segment("double_pendulum")
stage = page([
    "upper.add_updater_fn(step,",
    "    reset=reset, fixed_dt=1 / 240)",
    "lower.add_updater_fn(lower_bob)",
    "bar_between(hinge, upper)",
    "bar_between(upper, lower)",
    "traced_path(lower,",
    "    dissipating_time=3.0)",
])
ARM, DOUBLE_START = 1.65, (2.0, 2.7)
pivot = (stage.center[0], stage.center[1] + 0.3)
double = {}


def reset_double():
    """Both arms at rest at their starting angles."""
    double.update(a1=DOUBLE_START[0], w1=0.0, a2=DOUBLE_START[1], w2=0.0)


def joints():
    """The positions of both bobs from the shared state."""
    x1 = pivot[0] + ARM * math.sin(double["a1"])
    y1 = pivot[1] - ARM * math.cos(double["a1"])
    return (x1, y1, 0.0), (x1 + ARM * math.sin(double["a2"]), y1 - ARM * math.cos(double["a2"]), 0.0)


def step_double(_position, dt, _elapsed):
    """One semi-implicit Euler step of two equal masses on equal arms."""
    a1, w1, a2, w2 = double["a1"], double["w1"], double["a2"], double["w2"]
    delta = a1 - a2
    den = 3.0 - math.cos(2.0 * delta)
    alpha1 = (-3.0 * GRAVITY * math.sin(a1) - GRAVITY * math.sin(a1 - 2.0 * a2)
              - 2.0 * math.sin(delta) * (w2 * w2 * ARM + w1 * w1 * ARM * math.cos(delta))) / (ARM * den)
    alpha2 = (2.0 * math.sin(delta) * (2.0 * w1 * w1 * ARM + 2.0 * GRAVITY * math.cos(a1)
              + w2 * w2 * ARM * math.cos(delta))) / (ARM * den)
    double["w1"] = w1 + dt * (alpha1 - 0.02 * w1)
    double["w2"] = w2 + dt * (alpha2 - 0.02 * w2)
    double["a1"] = a1 + dt * double["w1"]
    double["a2"] = a2 + dt * double["w2"]
    return joints()[0]


def lower_bob(_position, _dt, _elapsed):
    """Read the lower bob from the state the upper bob just stepped."""
    return joints()[1]


reset_double()
start_upper, start_lower = joints()
upper = scene.geometry.circle(0.22).fill(fg).move_to(*start_upper[:2])
lower = scene.geometry.circle(0.28).fill(accent).move_to(*start_lower[:2])
parts = [
    M.pin_support(pivot, size=0.8, ground_length=1.4),
    M.bar_between(pivot, upper).stroke(fg, 0.07),
    M.bar_between(upper, lower).stroke(fg, 0.07),
    scene.geometry.traced_path(lower, dissipating_time=3.0).no_fill().stroke(accent, 0.035),
]
upper.add_updater_fn(step_double, reset=reset_double, fixed_dt=STEP)
lower.add_updater_fn(lower_bob)
scene.play([part.animate.fade_in() for part in parts] + [upper.animate.fade_in(), lower.animate.fade_in()], duration=0.3)
scene.wait(6.0)
upper.remove_updater()
lower.remove_updater()
scene.wait(HOLD)

# A mass on a spring: the spring force is a Parameter stepped from the same
# state, drawn as a force, and a pen plots the displacement over time.
scene.segment("mass_on_spring")
stage = page([
    "block.add_updater_fn(step,",
    "    reset=reset, fixed_dt=1 / 240)",
    "fx = scene.viz.parameter(0)",
    "fx.add_updater_fn(spring_force)",
    "force_from_components(top, fx, 0)",
    "pen.add_updater_fn(plot)",
    "traced_path(pen)",
])
STIFFNESS, FRICTION, PULL = 6.0, 0.35, 1.3
wall = (stage.left + 0.3, stage.center[1] + 1.6)
rest = wall[0] + 3.4
oscillator = {}


def reset_oscillator():
    """The block pulled out and let go."""
    oscillator.update(x=PULL, v=0.0)


def step_oscillator(position, dt, _elapsed):
    """One semi-implicit Euler step of a damped mass on a spring."""
    oscillator["v"] += dt * (-STIFFNESS * oscillator["x"] - FRICTION * oscillator["v"])
    oscillator["x"] += dt * oscillator["v"]
    return (rest + oscillator["x"], position[1], position[2])


def spring_force(_value, _dt, _elapsed):
    """Hooke's law on the displacement the block has just reached."""
    return -STIFFNESS * oscillator["x"]


origin = (stage.left + 0.6, stage.bottom + 1.4)


def plot(_position, _dt, elapsed):
    """Time to the right, displacement up."""
    return (origin[0] + 1.1 * elapsed, origin[1] + 0.9 * oscillator["x"], 0.0)


reset_oscillator()
block = scene.geometry.rounded_rect(1.0, 1.0, 0.08).fill(panel).stroke(fg, 0.05).move_to(rest + PULL, wall[1])
fx = scene.viz.parameter(-STIFFNESS * PULL)
# The force is drawn above the block, from a point that moves with it.
top = block.anchor_point(Anchor.TOP, offset=(0, 0.9))
pen = scene.geometry.dot(0.08).fill(accent).move_to(origin[0], origin[1] + 0.9 * PULL)
parts = [
    M.fixed_support(wall, direction=Direction.RIGHT, size=0.9, ground_length=1.4),
    M.fixed_support((rest, wall[1] - 0.5), size=0.8, ground_length=5.2),
    M.spring_between(wall, block.anchor_point(Anchor.LEFT), coils=9, amplitude=0.25),
    M.force_from_components(top, fx, 0, visual_scale=0.2, label="$F$", show_value=True, label_gap=0.45, color=accent),
    scene.geometry.line(origin[0], origin[1], stage.right, origin[1]).stroke(muted, 0.02),
    scene.geometry.line(origin[0], origin[1] - 1.3, origin[0], origin[1] + 1.3).stroke(muted, 0.02),
    scene.geometry.traced_path(pen).no_fill().stroke(accent, 0.04),
]
block.add_updater_fn(step_oscillator, reset=reset_oscillator, fixed_dt=STEP)
fx.add_updater_fn(spring_force)
pen.add_updater_fn(plot)
scene.play([part.animate.fade_in() for part in parts] + [block.animate.fade_in(), pen.animate.fade_in()], duration=0.3)
scene.wait(5.6)
for driven in (block, fx, pen):
    driven.remove_updater()
scene.wait(HOLD)

# Three moments of each simulation, the last one near its end.
pendulum_end = 0.3 + 5.0 + HOLD
double_end = pendulum_end + 0.3 + 6.0 + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(
        snapshots,
        [0.3 + t for t in (0.7, 2.2, 4.8)]
        + [pendulum_end + 0.3 + t for t in (1.0, 3.0, 5.8)]
        + [double_end + 0.3 + t for t in (0.6, 2.0, 5.4)],
    )
else:
    scene.render()
