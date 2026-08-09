"""Deterministic pendulum simulation with fixed-step seek and export replay."""

import math
import os

from gaanim import BLACK, GOLD, RED, WHITE, Scene

scene = Scene(1280, 720, background=BLACK)

# The reactive objects are declared after an earlier timeline interval. Their
# updater and trail must still begin here, not at t=0.
scene.wait(1.0)

L = 220.0
g = 980.0
damping = 0.03
px, py = 0.0, 120.0
initial_theta = 0.65
initial_omega = 0.0

state = {
    "theta": initial_theta,
    "omega": initial_omega,
}

hinge = scene.dot(8).fill(WHITE).at(px, py)
bob = (
    scene.circle(20)
    .fill(GOLD)
    .stroke(WHITE, 3)
    .at(
        px + L * math.sin(initial_theta),
        py - L * math.cos(initial_theta),
    )
)

rod = scene.tracking_line(hinge, bob).no_fill().stroke(WHITE, 4)
length = scene.dimension_between(hinge, bob, 25).stroke(RED, 2)
trail = (
    scene.traced_path(bob, dissipating_time=2.0)
    .no_fill()
    .stroke(RED, 2)
)


def acceleration(theta, omega):
    return -(g / L) * math.sin(theta) - damping * omega


def reset_pendulum():
    """Restore every piece of Python state captured by ``pendulum_step``."""
    state["theta"] = initial_theta
    state["omega"] = initial_omega


def pendulum_step(_pos, dt, _elapsed):
    """Semi-implicit Euler step; Gaanim supplies a constant ``dt``."""
    theta = state["theta"]
    omega = state["omega"]

    omega += dt * acceleration(theta, omega)
    theta += dt * omega

    state["theta"] = theta
    state["omega"] = omega
    return (
        px + L * math.sin(theta),
        py - L * math.cos(theta),
        0.0,
    )


# The reset + fixed_dt pair makes editor playback, random seeks, snapshots and
# exported frames reconstruct the same physical state.
bob.add_updater_fn(
    pendulum_step,
    reset=reset_pendulum,
    fixed_dt=1.0 / 240.0,
)

# A traced path is hidden until its entrance is explicitly authored.
scene.play(
    [
        hinge.fade_in(),
        rod.fade_in(),
        length.fade_in(),
        trail.fade_in(),
        bob.grow_from_center(0.5),
    ]
)
scene.wait(7.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 1.5, 3.0, 6.0, 9.0])

export_path = os.environ.get("GAANIM_EXPORT")
if export_path:
    scene.export(export_path, fps=30)
else:
    scene.render()
