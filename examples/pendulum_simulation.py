"""Deterministic pendulum simulation with fixed-step seek and export replay."""

import math
import os

from gaanim import BLACK, GOLD, RED, WHITE, Scene, Transition

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

bob.remove_updater()


scene.segment("double_pendulum", Transition.cross_fade(0.8))

double_l1 = 170.0
double_l2 = 170.0
double_g = 980.0
double_damping = 0.012
double_m1 = 1.0
double_m2 = 1.0
double_px, double_py = 0.0, 220.0
double_initial_theta1 = 0.72
double_initial_theta2 = -0.42

double_state = {
    "theta1": double_initial_theta1,
    "omega1": 0.0,
    "theta2": double_initial_theta2,
    "omega2": 0.0,
}


def double_positions():
    """Return the world positions of both masses from the current state."""
    theta1 = double_state["theta1"]
    theta2 = double_state["theta2"]
    x1 = double_px + double_l1 * math.sin(theta1)
    y1 = double_py - double_l1 * math.cos(theta1)
    x2 = x1 + double_l2 * math.sin(theta2)
    y2 = y1 - double_l2 * math.cos(theta2)
    return (x1, y1, 0.0), (x2, y2, 0.0)


def double_acceleration(theta1, omega1, theta2, omega2):
    """Compute angular accelerations for two equal point masses."""
    delta = theta1 - theta2
    denominator = 2.0 * double_m1 + double_m2 - double_m2 * math.cos(2.0 * delta)

    alpha1 = (
        -double_g * (2.0 * double_m1 + double_m2) * math.sin(theta1)
        - double_m2 * double_g * math.sin(theta1 - 2.0 * theta2)
        - 2.0
        * math.sin(delta)
        * double_m2
        * (
            omega2**2 * double_l2
            + omega1**2 * double_l1 * math.cos(delta)
        )
    ) / (double_l1 * denominator)

    alpha2 = (
        2.0
        * math.sin(delta)
        * (
            omega1**2 * double_l1 * (double_m1 + double_m2)
            + double_g * (double_m1 + double_m2) * math.cos(theta1)
            + omega2**2 * double_l2 * double_m2 * math.cos(delta)
        )
    ) / (double_l2 * denominator)

    return (
        alpha1 - double_damping * omega1,
        alpha2 - double_damping * omega2,
    )


def reset_double_pendulum():
    """Restore the complete double-pendulum state for deterministic replay."""
    double_state["theta1"] = double_initial_theta1
    double_state["omega1"] = 0.0
    double_state["theta2"] = double_initial_theta2
    double_state["omega2"] = 0.0


def double_pendulum_step(_pos, dt, _elapsed):
    """Advance both angles with a fixed-step semi-implicit Euler integrator."""
    theta1 = double_state["theta1"]
    omega1 = double_state["omega1"]
    theta2 = double_state["theta2"]
    omega2 = double_state["omega2"]

    alpha1, alpha2 = double_acceleration(theta1, omega1, theta2, omega2)
    omega1 += dt * alpha1
    omega2 += dt * alpha2
    theta1 += dt * omega1
    theta2 += dt * omega2

    double_state["theta1"] = theta1
    double_state["omega1"] = omega1
    double_state["theta2"] = theta2
    double_state["omega2"] = omega2
    return double_positions()[0]


def second_bob_position(_pos, _dt, _elapsed):
    """Follow the second mass after the first mass advances the shared state."""
    return double_positions()[1]


double_hinge = scene.dot(8).fill(WHITE).at(double_px, double_py)
double_bob1 = (
    scene.circle(21)
    .fill(GOLD)
    .stroke(WHITE, 3)
    .at(*double_positions()[0][:2])
)
double_bob2 = (
    scene.circle(27)
    .fill(RED)
    .stroke(WHITE, 3)
    .at(*double_positions()[1][:2])
)

double_rod1 = scene.tracking_line(double_hinge, double_bob1).no_fill().stroke(WHITE, 4)
double_rod2 = scene.tracking_line(double_bob1, double_bob2).no_fill().stroke(WHITE, 4)
double_trail = (
    scene.traced_path(double_bob2, dissipating_time=3.0)
    .no_fill()
    .stroke(RED, 2)
)

# Only the first bob advances the shared state. The second updater mirrors its
# resulting position so the two masses are not integrated twice per frame.
double_bob1.add_updater_fn(
    double_pendulum_step,
    reset=reset_double_pendulum,
    fixed_dt=1.0 / 240.0,
)
double_bob2.add_updater_fn(second_bob_position)

scene.play(
    [
        double_hinge.fade_in(),
        double_rod1.fade_in(),
        double_rod2.fade_in(),
        double_trail.fade_in(),
        double_bob1.grow_from_center(0.5),
        double_bob2.grow_from_center(0.5),
    ]
)
scene.wait(8.0)
double_bob1.remove_updater()
double_bob2.remove_updater()

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.0, 1.5, 3.0, 6.0, 9.0, 12.0, 16.0])

scene.render()
