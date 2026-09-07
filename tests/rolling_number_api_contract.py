"""Run inside the current Gaanim host; snapshots exercise replay and scalar cuts."""
import os
import inspect

from gaanim import Anim, BLACK, Drawable, Easing, Parameter, RollingNumber, Scene, TextAnchor, WHITE

scene = Scene(frame=(16, 9), background=BLACK)
assert inspect.signature(scene.viz.rolling_number).parameters["font_family"].default is None
counter = scene.viz.rolling_number(99, min_digits=3, font_size=1.0).move_to(0, 0).fill(WHITE)
assert isinstance(counter, RollingNumber)
assert isinstance(counter, Drawable)
assert isinstance(counter.parameter, Parameter)
assert isinstance(counter.visual, Drawable)
assert isinstance(counter.count_to(100), Anim)
assert counter.current == 99
assert counter.set(99) is counter
assert counter.opacity(1) is counter
for anchor in [TextAnchor.BASELINE_LEFT, TextAnchor.BASELINE_CENTER, TextAnchor.BASELINE_RIGHT]:
    assert counter.move_to(0, 0, anchor) is counter
counter.move_to(0, 0, TextAnchor.BASELINE_CENTER)
for options in [
    {"decimals": 7}, {"decimals": -1}, {"min_digits": -1}, {"min_digits": 0}, {"decimals": 2, "min_digits": 14},
    {"font_size": 0}, {"font_family": ""}, {"digit_spacing": -1}, {"line_height": 0.5},
    {"group_separator": ".."}, {"decimal_separator": ""},
    {"group_separator": "."}, {"prefix": "bad\n"},
    {"mode": "random"}, {"direction": "left"}, {"value": float("nan")},
]:
    try:
        scene.viz.rolling_number(**options)
    except ValueError:
        pass
    else:
        raise AssertionError(options)
for action in [lambda: counter.set(float("inf")), lambda: counter.count_to(1e15),
               lambda: counter.count_to(0, duration=-1)]:
    try:
        action()
    except ValueError:
        pass
    else:
        raise AssertionError("expected ValueError")

scene.wait(0.5)
scene.play([counter.count_to(100, duration=2).easing(Easing.LINEAR)])
scene.wait(0.5)
counter.set(-12)
scene.wait(0.5)
scene.play([counter.visual.animate.move_to(3, 0).duration(1)])
scene.wait(0.5)
if directory := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(directory, [0, 1.5, 2.5, 3.25, 4.75, 1.5, 0, 3.25])
