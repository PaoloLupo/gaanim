"""Run inside the current Gaanim host: drive_from_samples(..., "xy") contract.

just dev-exec target/debug/gaanim.exe --diff --example tests/sampled_xy_api_contract.py --capture-only --no-gui
"""
import os

from gaanim import BLACK, GOLD, Scene

scene = Scene(frame=(16, 9), background=BLACK)
times = [0.0, 1.0, 2.0]

# Tuples and two-element lists are both accepted and return the drawable.
probe = scene.geometry.dot(0.2).fill(GOLD)
assert probe.drive_from_samples(times, [(0, 0), (2, 1), (4, 2)], "xy") is not None
listed = scene.geometry.dot(0.1).fill(GOLD).move_to(0, -2)
listed.drive_from_samples(times, [[0, 0], [-2, 1], [-4, 2]], "xy", interpolation="step")

for values, prop in [
    ([0.0, 1.0, 2.0], "xy"),  # scalars need a per-axis property
    ([(0, 0), (1, 1)], "xy"),  # length mismatch with times
    ([(0, 0), (1, float("nan")), (2, 2)], "xy"),  # non-finite y
    ([(0, 0, 0), (1, 1, 1), (2, 2, 2)], "xy"),  # not pairs
    ([(0, 0), (1, 1), (2, 2)], "x"),  # pairs need "xy"
]:
    target = scene.geometry.dot(0.1)
    try:
        target.drive_from_samples(times, values, prop)
    except ValueError:
        pass
    else:
        raise AssertionError((values, prop))

scene.wait(2.5)
if os.environ.get("GAANIM_SNAPSHOTS"):
    # At t=1 the probe sits at (2, 1); at t=2.5 it holds the last pair (4, 2).
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], [0.0, 1.0, 2.5])
scene.render()
