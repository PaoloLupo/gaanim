"""Updaters: your own per-frame functions, sampled series, stacked presets and redrawn arcs.

``add_updater_fn`` runs a Python function every frame: a pure function of the
elapsed time, or a stateful simulation that ``reset`` and a fixed step make
reproducible under seeks. ``drive_from_samples`` plays a recorded series on any
channel without Python, presets stack and layer on animations, and
``always_redraw_arc`` redraws an arc from a Parameter. Each row or panel is
labelled with its code.
"""

import math
import os

from gaanim import Easing, Scene, Updater, Zones

RUN = 4.0
HOLD = 0.5

scene = Scene()
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


def code(text):
    """A code label, its lines aligned left."""
    return scene.text(text, role="code", text_align="left")


def rows(count, label_share="8.6fr", stage_share="5.6fr"):
    """``count`` rows of a label and a stage, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[label_share, stage_share], column_gap="48px", padding="page_padding")
    )


def lissajous(stage):
    """A position function drawing a Lissajous figure in ``stage`` from the elapsed time alone."""
    (cx, cy), ax, ay = stage.center, stage.width / 2 - 0.3, stage.height / 2 - 0.25

    def step(position, dt, elapsed):
        return (cx + ax * math.sin(1.5 * elapsed), cy + ay * math.sin(3 * elapsed), 0.0)

    return step


def bouncing(stage):
    """A ball simulation in ``stage``: its ``fall`` step and the ``reset`` that restarts it."""
    floor, top = stage.bottom + 0.2, stage.top - 0.2
    state = {}

    def reset():
        state.update(x=stage.left + 0.2, y=top, vy=0.0)

    def fall(position, dt, elapsed):
        state["vy"] -= 12.0 * dt
        state["y"] += state["vy"] * dt
        state["x"] = min(state["x"] + (stage.width - 0.4) / RUN * dt, stage.right - 0.2)
        if state["y"] < floor:
            state["y"], state["vy"] = floor, -0.82 * state["vy"]
        return (state["x"], state["y"], 0.0)

    reset()
    return fall, reset


def spin(value, dt, elapsed):
    """A Parameter callback: the new value from the elapsed time."""
    return 1.6 * elapsed


# add_updater_fn(callback) calls callback(position, dt, elapsed) every frame
# and moves the drawable to the (x, y, z) it returns. A pure function of the
# elapsed time seeks exactly; a simulation that accumulates state needs reset
# and fixed_dt, so seeks and exports replay it from the start in fixed steps.
# A Parameter takes callback(value, dt, elapsed) and returns its new value.
# remove_updater() stops a drawable where it is.
functions_start = scene.cursor
scene.segment("functions")
grid = rows(4)
trails = []
for row, label in enumerate([
    "square.add_updater_fn(lissajous(stage))",
    "square.add_updater_fn(lissajous(stage))\nsquare.remove_updater()  # halfway",
]):
    code(label).place(grid[2 * row], anchor="right")
    stage = grid[2 * row + 1]
    square = scene.geometry.square(0.3).move_to(*stage.center)
    square.add_updater_fn(lissajous(stage))
    trails.append(scene.geometry.traced_path(square).stroke(muted, 0.02))
stopped = square
code("ball.add_updater_fn(fall, reset=reset,\n                    fixed_dt=1 / 240)").place(grid[4], anchor="right")
stage = grid[5]
scene.geometry.line(stage.left, stage.bottom + 0.05, stage.right, stage.bottom + 0.05).stroke(rule, 0.03)
fall, reset = bouncing(stage)
ball = scene.geometry.circle(0.15).move_to(stage.left + 0.2, stage.top - 0.2)
ball.add_updater_fn(fall, reset=reset, fixed_dt=1 / 240)
trails.append(scene.geometry.traced_path(ball).stroke(muted, 0.02))
code("phase = scene.viz.parameter(0)\nphase.add_updater_fn(spin)").place(grid[6], anchor="right")
stage = grid[7]
phase = scene.viz.parameter(0)
phase.add_updater_fn(spin)
center, radius = (stage.left + stage.height / 2, stage.center[1]), stage.height / 2 - 0.1
scene.geometry.circle(radius).no_fill().stroke(rule, 0.03).move_to(*center)
hand = scene.geometry.tracking_line(center, scene.geometry.polar_point(center, radius, phase)).stroke(accent, 0.06)
scene.viz.readout(phase, label="$phi$", font_size=0.7).move_to(stage.left + stage.height + 1.4, center[1])
scene.play([*(trail.animate.fade_in() for trail in trails), hand.animate.fade_in()], duration=0.1)
scene.wait(RUN / 2)
stopped.remove_updater()
scene.wait(RUN / 2 + HOLD)

# drive_from_samples plays a recorded series natively, as a pure function of
# the timeline: times are seconds from the cursor, and positions add to the
# authored pose. "xy" takes (x, y) pairs; "step" holds each sample.
times = [0.5 * index for index in range(9)]
values = [0.0, 0.8, 0.35, 0.95, 0.5, 0.2, 0.7, 1.0, 0.6]
samples_start = scene.cursor
scene.segment("samples")
grid = rows(4)
stages = [grid[2 * row + 1] for row in range(4)]
xs = [(stages[0].width - 0.4) * v for v in values]
ys = [(stages[2].height - 0.4) * v for v in values]
pts = [((stages[3].width - 0.4) * k / 8, (stages[3].height - 0.4) * v) for k, v in enumerate(values)]
for row, label in enumerate([
    'dot.drive_from_samples(times, xs, "x")',
    'dot.drive_from_samples(times, xs, "x",\n    interpolation="step")',
    'dot.drive_from_samples(times, ys, "y")',
    'dot.drive_from_samples(times, pts, "xy")',
]):
    code(label).place(grid[2 * row], anchor="right")
for stage in stages[:2]:
    (left, y), right = stage.point("left"), stage.right
    scene.geometry.line(left, y, right, y).stroke(rule, 0.03)
    for x in xs:
        scene.geometry.rect(0.03, 0.3).fill(rule).move_to(left + 0.2 + x, y)
    dot = scene.geometry.dot(0.16).move_to(left + 0.2, y)
    dot.drive_from_samples(times, xs, "x", interpolation="step" if stage is stages[1] else "linear")
(x, bottom), top = stages[2].point("bottom"), stages[2].top
scene.geometry.line(x, bottom, x, top).stroke(rule, 0.03)
scene.geometry.dot(0.16).move_to(x, bottom + 0.2).drive_from_samples(times, ys, "y")
origin = (stages[3].left + 0.2, stages[3].bottom + 0.2)
for px, py in pts:
    scene.geometry.dot(0.05).fill(rule).move_to(origin[0] + px, origin[1] + py)
probe = scene.geometry.dot(0.14).move_to(*origin)
probe.drive_from_samples(times, pts, "xy")
probe_trail = scene.geometry.traced_path(probe).stroke(muted, 0.02)
scene.play(probe_trail.animate.fade_in(), duration=0.1)
scene.wait(times[-1] + HOLD)

# Rotation adds to the authored angle, while scale, opacity and a Parameter
# take offset + scale * sample. Anything that reads the Parameter follows it.
channels_start = scene.cursor
scene.segment("channels")
grid = rows(4)
for row, label in enumerate([
    'bar.drive_from_samples(times, values,\n    "rotation", scale=math.pi)',
    'square.drive_from_samples(times, values,\n    "scale", offset=0.4)',
    'square.drive_from_samples(times, values,\n    "opacity")',
    "level.drive_from_samples(times, values)",
]):
    code(label).place(grid[2 * row], anchor="right")
stages = [grid[2 * row + 1] for row in range(4)]
bar = scene.geometry.rect(1.4, 0.2).move_to(*stages[0].center)
bar.drive_from_samples(times, values, "rotation", scale=math.pi)
scene.geometry.square(1.0).no_fill().stroke(rule, 0.03).move_to(*stages[1].center).scale_to(1.4)
scene.geometry.square(1.0).move_to(*stages[1].center).drive_from_samples(times, values, "scale", offset=0.4)
scene.geometry.square(1.2).no_fill().stroke(rule, 0.03).move_to(*stages[2].center)
scene.geometry.square(1.2).move_to(*stages[2].center).drive_from_samples(times, values, "opacity")
level = scene.viz.parameter(0)
level.drive_from_samples(times, values)
number, gauge = stages[3].split(Zones.columns(["2fr", "3fr"], gap="24px"))
scene.viz.readout(level, font_size=0.7).place(number, anchor="center")
mask = scene.geometry.rect(gauge.width, 0.6).no_fill().stroke(rule, 0.03).move_to(*gauge.center)
scene.geometry.fill_level(mask, accent, level, direction="right")
scene.wait(times[-1] + HOLD)

# Presets stack on one drawable, and wiggle and oscillate add to its own
# animation instead of replacing it. remove_updater() ends every updater at
# once, at the cursor, keeping the pose they reached.
stacking_start = scene.cursor
scene.segment("stacking")
grid = rows(4, "9.8fr", "4.4fr")
for row, label in enumerate([
    "add_updater(Updater.rotate(1.5))\nadd_updater(Updater.pulse(0.6, 1.2, 1.0))",
    "add_updater(Updater.orbit(x, y, 0.6, 2.0))\nadd_updater(Updater.rotate(-4.0))",
    'add_updater(Updater.oscillate("y", high=0.5))\nanimate.move_to(end, y)',
    "add_updater(Updater.rotate(1.5))\nremove_updater()  # halfway",
]):
    code(label).place(grid[2 * row], anchor="right")
stages = [grid[2 * row + 1] for row in range(4)]
first = scene.geometry.square(0.9).move_to(*stages[0].center)
first.add_updater(Updater.rotate(1.5))
first.add_updater(Updater.pulse(0.6, 1.2, 1.0))
(ox, oy) = stages[1].center
scene.geometry.circle(0.6).no_fill().stroke(rule, 0.03).move_to(ox, oy)
second = scene.geometry.square(0.45).move_to(ox + 0.6, oy)
second.add_updater(Updater.orbit(ox, oy, 0.6, 2.0))
second.add_updater(Updater.rotate(-4.0))
(left, y), end = stages[2].point("left"), stages[2].right - 0.4
third = scene.geometry.square(0.6).move_to(left + 0.4, y - 0.25)
third.add_updater(Updater.oscillate("y", high=0.5))
fourth = scene.geometry.rect(2.2, 0.35).move_to(*stages[3].center)
fourth.add_updater(Updater.rotate(1.5))
scene.play(third.animate.move_to(end, y - 0.25).duration(RUN / 2))
fourth.remove_updater()
scene.wait(RUN / 2 + HOLD)

# always_redraw_arc rebuilds an arc arrow every frame from a Parameter: it
# starts at start_angle and sweeps turn * sweep_scale + sweep_offset radians.
redraw_start = scene.cursor
scene.segment("redraw")
panels = scene.layout.zones(Zones.grid(rows=2, columns=3, gap="48px", padding="page_padding"))
turn = scene.viz.parameter(0.0)
arcs = []
for index, (label, start, options) in enumerate([
    ("always_redraw_arc(\n  turn, x, y, r, 0)", 0.0, {}),
    ("always_redraw_arc(\n  turn, x, y, r,\n  math.pi)", math.pi, {}),
    ("always_redraw_arc(\n  turn, x, y, r, 0,\n  sweep_scale=-1)", 0.0, {"sweep_scale": -1.0}),
    ("always_redraw_arc(\n  turn, x, y, r, 0,\n  sweep_scale=0.5)", 0.0, {"sweep_scale": 0.5}),
    ("always_redraw_arc(\n  turn, x, y, r, 0,\n  sweep_offset=1.5)", 0.0, {"sweep_offset": 1.5}),
]):
    head, stage = panels[index].split(Zones.rows(["2fr", "3fr"], gap="12px"))
    code(label).place(head, anchor="center")
    (x, y), r = stage.center, stage.height / 2 - 0.1
    scene.geometry.circle(r).no_fill().stroke(rule, 0.03).move_to(x, y)
    scene.geometry.line(x, y, x + r * math.cos(start), y + r * math.sin(start)).stroke(rule, 0.03)
    arc = scene.geometry.always_redraw_arc(turn, x, y, r, start, **options).fill(accent).stroke(accent, 0.06)
    arcs.append(arc.animate.fade_in())
head, stage = panels[5].split(Zones.rows(["2fr", "3fr"], gap="12px"))
code("turn.animate.set(4.5)").place(head, anchor="center")
scene.viz.readout(turn, label="turn", font_size=0.8).place(stage, anchor="center")
scene.play(arcs, duration=0.2)
scene.play(turn.animate.set(4.5).duration(RUN).easing(Easing.SMOOTH))
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment early, midway and settled.
    scene.snapshots(
        snapshots,
        [functions_start + at for at in (1.0, 2.6, 4.4)]
        + [samples_start + at for at in (1.1, 2.4, 4.4)]
        + [channels_start + at for at in (0.6, 1.9, 3.4)]
        + [stacking_start + at for at in (0.7, 2.3, 4.2)]
        + [redraw_start + at for at in (1.2, 2.6, 4.6)],
    )
else:
    scene.render()
