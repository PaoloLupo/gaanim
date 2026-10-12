"""Procedural motion: updaters that oscillate, wiggle, orbit and spin, seeded randomness and noise.

An ``Updater`` adds motion that runs by itself, as a pure function of the
timeline, so seeks and exports match playback. ``scene.random`` draws
reproducible values for placing objects, and ``scene.noise`` is a smooth
signal over time to drive any reactive property. Every row or panel is
labelled with its code.
"""

import os

from gaanim import Easing, Scene, StrokeStyle, Updater, Zones, computed, stagger

RUN = 4.0
CROSSING = 4.0

scene = Scene()
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


def rows(count, label_share):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[label_share, "2fr"], column_gap="48px", padding="page_padding")
    )


# oscillate layers a periodic value between low and high on one channel: x, y
# and rotation add to the animated value, scale and opacity multiply it. Every
# waveform starts at low; phase shifts it by a fraction of a cycle.
oscillate_start = scene.cursor
scene.segment("oscillate")
WAVEFORMS = ["sine", "square", "triangle", "saw"]
CHANNELS = [
    ('"x", low=-0.4, high=0.4', ("x",), {"low": -0.4, "high": 0.4}),
    ('"y", high=0.3', ("y",), {"high": 0.3}),
    ('"y", high=0.3, phase=0.5', ("y",), {"high": 0.3, "phase": 0.5}),
    ('"rotation", high=0.8', ("rotation",), {"high": 0.8}),
    ('"scale", low=0.5', ("scale",), {"low": 0.5}),
    ('"opacity", low=0.15', ("opacity",), {"low": 0.15}),
]
grid = scene.layout.zones(
    Zones.grid(rows=len(CHANNELS) + 1, columns=["5fr"] + ["2fr"] * 4, column_gap="48px", padding="page_padding")
)
scene.text("Updater.oscillate(...)", role="code").place(grid[0], anchor="right")
for column, waveform in enumerate(WAVEFORMS, start=1):
    scene.text(f'"{waveform}"', role="code").place(grid[column], anchor="center")
for row, (label, args, options) in enumerate(CHANNELS, start=1):
    scene.text(label, role="code").place(grid[5 * row], anchor="right")
    for column, waveform in enumerate(WAVEFORMS, start=1):
        cell = grid[5 * row + column]
        scene.geometry.square(0.4).no_fill().stroke(rule, 0.02).move_to(*cell.center)
        square = scene.geometry.square(0.4).move_to(*cell.center)
        square.add_updater(Updater.oscillate(*args, waveform=waveform, frequency=0.5, **options))
scene.wait(RUN)

# wiggle layers seeded noise on position (scene units), rotation (radians) and
# scale (a fraction); frequency sets its pace and octaves its detail. Any
# amplitude can be a Parameter: animating it to 0 calms the jitter. The
# outline marks where each star rests.
wiggle_start = scene.cursor
scene.segment("wiggle")
shake = scene.viz.parameter(0.3)
WIGGLES = [
    ("Updater.wiggle()", {}),
    ("Updater.wiggle(position=0.3)", {"position": 0.3}),
    ("Updater.wiggle(position=0.3, seed=7)", {"position": 0.3, "seed": 7}),
    ("Updater.wiggle(position=0, rotation=0.5)", {"position": 0, "rotation": 0.5}),
    ("Updater.wiggle(position=0, scale=0.4)", {"position": 0, "scale": 0.4}),
    ("Updater.wiggle(position=0.3, frequency=0.5)", {"position": 0.3, "frequency": 0.5}),
    ("Updater.wiggle(position=0.3, frequency=6.0)", {"position": 0.3, "frequency": 6.0}),
    ("Updater.wiggle(position=0.3, octaves=8)", {"position": 0.3, "octaves": 8}),
    ("Updater.wiggle(position=shake)", {"position": shake}),
]
grid = rows(len(WIGGLES), "5fr")
for row, (label, options) in enumerate(WIGGLES):
    scene.text(label, role="code").place(grid[2 * row], anchor="right")
    center = grid[2 * row + 1].center
    scene.geometry.star(5, 0.3, 0.13).no_fill().stroke(rule, 0.02).move_to(*center)
    scene.geometry.star(5, 0.3, 0.13).move_to(*center).add_updater(Updater.wiggle(**options))
scene.wait(1.0)
scene.play(shake.animate.set(0).duration(2.0))
scene.wait(1.0)

# The other presets: an orbit, a vertical bob, a pulsing scale, a steady spin
# (a Parameter speed eases it up and down), a constant drift to the right,
# and dashes flowing along their path.
presets_start = scene.cursor
scene.segment("presets")
grid = rows(7, "3fr")
speed = scene.viz.parameter(0.0)
stages = [grid[2 * row + 1] for row in range(7)]
(ox, oy) = stages[0].center
scene.geometry.circle(0.35).no_fill().stroke(rule, 0.02).move_to(ox, oy)
(left, y), (right, _) = stages[6].point("left"), stages[6].point("right")
pipe = scene.geometry.line(left, y, right, y)
pipe.stroke_style(StrokeStyle(foreground, 0.08, cap="butt", dashes=[0.25, 0.15]))
presets = [
    (f"Updater.orbit({ox:.2f}, {oy:.2f}, 0.35, 2.0)", scene.geometry.dot(0.1), Updater.orbit(ox, oy, 0.35, 2.0)),
    ("Updater.bob(0.25, 1.0)", scene.geometry.square(0.3).move_to(*stages[1].center), Updater.bob(0.25, 1.0)),
    ("Updater.pulse(0.6, 1.3, 1.0)", scene.geometry.square(0.3).move_to(*stages[2].center),
     Updater.pulse(0.6, 1.3, 1.0)),
    ("Updater.rotate(2.0)", scene.geometry.rect(0.7, 0.12).move_to(*stages[3].center), Updater.rotate(2.0)),
    ("Updater.rotate(speed)", scene.geometry.rect(0.7, 0.12).move_to(*stages[4].center), Updater.rotate(speed)),
    ("Updater.advance_x(0.8)", scene.geometry.square(0.3).move_to(*stages[5].point("left")), Updater.advance_x(0.8)),
    ("Updater.dash_flow(0.8)", pipe, Updater.dash_flow(0.8)),
]
for row, (label, drawable, updater) in enumerate(presets):
    scene.text(label, role="code").place(grid[2 * row], anchor="right")
    drawable.add_updater(updater)
scene.play(speed.animate.set(8.0).duration(2.0).easing(Easing.SMOOTH))
scene.play(speed.animate.set(0.0).duration(2.0).easing(Easing.SMOOTH))

# scene.random(seed) draws the same values on every platform: uniform and
# gauss floats, integers, a choice from a list, and shuffles; a second stream
# with the same seed repeats the first panel exactly.
random_start = scene.cursor
scene.segment("random")
panels = scene.layout.zones(Zones.grid(rows=2, columns=3, gap="48px", padding="page_padding"))
palette = [scene.canvas.color(role) for role in ("accent", "success", "warning", "danger", "chart")]


def panel(index, label):
    """The stage of panel ``index``, under its ``label``."""
    head, stage = panels[index].split(Zones.rows([0.5, "1fr"], gap="12px"))
    scene.text(label, role="code").place(head, anchor="center")
    return stage.inset(0.2)


def scatter(stream, stage):
    """Sixty dots at places drawn uniformly from ``stream`` inside ``stage``."""
    dots = []
    for _ in range(60):
        x, y = stream.uniform(stage.left, stage.right), stream.uniform(stage.bottom, stage.top)
        dots.append(scene.geometry.dot(0.07).move_to(x, y))
    return dots


rng = scene.random(seed=42)
drawn = scatter(rng, panel(0, "rng.uniform(low, high)"))
stage = panel(1, "rng.gauss(mean, std)")
(cx, cy), spread = stage.center, stage.height / 6
drawn += [scene.geometry.dot(0.07).move_to(rng.gauss(cx, spread), rng.gauss(cy, spread)) for _ in range(60)]
stage = panel(2, "rng.integer(0, 6)")
counts = [0] * 6
for _ in range(30):
    column = rng.integer(0, 6)
    x = stage.left + (column + 0.5) * stage.width / 6
    drawn.append(scene.geometry.square(0.2).move_to(x, stage.bottom + 0.15 + 0.26 * counts[column]))
    counts[column] += 1
stage = panel(3, "rng.choice(palette)")
for row in range(4):
    for column in range(8):
        x, y = stage.left + (column + 0.5) * stage.width / 8, stage.top - (row + 0.5) * stage.height / 4
        drawn.append(scene.geometry.circle(0.16).fill(rng.choice(palette)).move_to(x, y))
stage = panel(4, "rng.shuffle(heights)")
heights = [stage.height * (index + 1) / 8 for index in range(8)]
rng.shuffle(heights)
for column, height in enumerate(heights):
    x = stage.left + (column + 0.5) * stage.width / 8
    drawn.append(scene.geometry.rect(stage.width / 12, height).move_to(x, stage.bottom + height / 2))
drawn += scatter(scene.random(seed=42), panel(5, "scene.random(seed=42)"))
scene.play(stagger(*[item.animate.grow_from_center().duration(0.3) for item in drawn], total=1.0))
scene.wait(1.0)

def crossing(left, width):
    """An x that crosses ``width`` from ``left`` in CROSSING seconds from the noise segment's start."""
    return computed(lambda t: left + width * min(max((t - noise_start) / CROSSING, 0), 1), inputs=[scene.time])


# scene.noise() is a smooth signal of timeline time within center ± amplitude.
# Each dot rides one noise signal up and down while it crosses its row, and
# its trail draws the signal.
noise_start = scene.cursor
scene.segment("noise")
NOISES = [
    ("scene.noise(amplitude=0.3)", {"amplitude": 0.3}),
    ("scene.noise(amplitude=0.3, seed=7)", {"amplitude": 0.3, "seed": 7}),
    ("scene.noise(amplitude=0.15)", {"amplitude": 0.15}),
    ("scene.noise(amplitude=0.3, frequency=3.0)", {"amplitude": 0.3, "frequency": 3.0}),
    ("scene.noise(amplitude=0.3, octaves=4)", {"amplitude": 0.3, "octaves": 4}),
    ("scene.noise(amplitude=0.15, center=0.2)", {"amplitude": 0.15, "center": 0.2}),
]
grid = rows(len(NOISES), "5fr")
trails = []
for row, (label, options) in enumerate(NOISES):
    scene.text(label, role="code").place(grid[2 * row], anchor="right")
    stage = grid[2 * row + 1]
    (left, y), width = stage.point("left"), stage.width
    scene.geometry.line(left, y, left + width, y).stroke(rule, 0.02)
    signal = scene.noise(**options)
    dot = scene.geometry.dot(0.07).move_to(crossing(left, width), computed(lambda v, y=y: y + v, inputs=[signal]))
    trails.append(scene.geometry.traced_path(dot).stroke(muted, 0.02).animate.fade_in().duration(0.1))
scene.play(trails)
scene.wait(CROSSING)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The oscillators and wiggles at several phases, the wiggle calmed down,
    # the presets spinning up, the random panels, and the noise drawn.
    scene.snapshots(
        snapshots,
        [oscillate_start + t for t in (0.4, 1.1, 1.6, 2.3)]
        + [wiggle_start + t for t in (0.5, 1.5, 3.6)]
        + [presets_start + t for t in (0.5, 1.8, 3.6)]
        + [random_start + t for t in (0.6, 1.8)]
        + [noise_start + t for t in (1.5, 4.0)],
    )
else:
    scene.render()
