"""Falloffs: one value per member of a group, driving its scale, turn, opacity, position or color.

A ``Falloff`` computes a value for every member of a group from its index,
its distance to a target, noise or a value over time, and ``drive`` connects
it to a channel of all members at once. The segments show every channel,
the index ramps, the distance shapes and radii, noise and time sources, the
ways falloffs combine, and ``look_at`` turning a field of arrows toward a
moving cursor. Each label is the code of its variant.
"""

import math
import os

from gaanim import Distribution, Easing, Falloff, Scene, Zones, computed

MOVE = 2.0
HOLD = 0.8

scene = Scene()
accent = scene.canvas.color("accent")
chart = scene.canvas.color("chart")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")
foreground = scene.canvas.color("foreground")


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code", markup=False)


def rows(count, labels="16fr"):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[labels, "9fr"], column_gap="48px", row_gap="12px", padding="page_padding")
    )


def gallery(count, rows=3):
    """``count`` panels in two columns, each split into its art and the caption under it."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=2, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows(["1fr", 0.45])) for zone in list(grid)[:count]]


def line_of(shape, zone, count):
    """``count`` copies of ``shape`` spread across ``zone`` in one row."""
    spacing = zone.width / count
    return scene.geometry.duplicate(shape, Distribution.grid(count, 1, spacing, center=zone.center))


def field(zone, shape, columns=12, rows=3):
    """A grid of copies of ``shape(spacing)`` filling ``zone``."""
    spacing = min(zone.width / columns, zone.height / rows)
    return scene.geometry.duplicate(shape(spacing), Distribution.grid(columns, rows, spacing, center=zone.center))


def cursor(x, y):
    """A ring that marks where a distance falloff is measured from."""
    return scene.geometry.circle(0.16).no_fill().stroke(foreground, 0.05).move_to(x, y).z_index(1)


# drive(channel, falloff) applies a falloff to one channel of every member:
# scale and opacity multiply, rotation adds radians, x and y add scene units,
# and fill takes the colors of falloff.gradient(...).
channels_start = scene.cursor
scene.segment("channels")
grid = rows(7)
label("f = Falloff.index()").place(grid[0], anchor="right")
CHANNELS = [
    ('row.drive("scale", f.remap(0.3, 1.0))', lambda row, f: row.drive("scale", f.remap(0.3, 1.0))),
    ('row.drive("rotation", f.remap(0, 1.6))', lambda row, f: row.drive("rotation", f.remap(0, 1.6))),
    ('row.drive("opacity", f.remap(0.1, 1.0))', lambda row, f: row.drive("opacity", f.remap(0.1, 1.0))),
    ('row.drive("x", f.remap(0, -1.2))', lambda row, f: row.drive("x", f.remap(0, -1.2))),
    ('row.drive("y", f.remap(-0.3, 0.3))', lambda row, f: row.drive("y", f.remap(-0.3, 0.3))),
    ('row.drive("fill", f.gradient(chart, accent))', lambda row, f: row.drive("fill", f.gradient(chart, accent))),
]
for index, (code, drive) in enumerate(CHANNELS, start=1):
    label(code).place(grid[2 * index], anchor="right")
    stage = grid[2 * index + 1]
    drive(line_of(scene.geometry.square(stage.height * 0.45), stage, 7), Falloff.index())
scene.wait(HOLD)

# Falloff.index gives 0 to the first member and 1 to the last, shaped by an
# easing; constant gives every member the same value.
ramps_start = scene.cursor
scene.segment("index")
RAMPS = [
    ("Falloff.index()", Falloff.index()),
    ("Falloff.index(easing=Easing.SMOOTH)", Falloff.index(easing=Easing.SMOOTH)),
    ("Falloff.index(easing=Easing.THERE_AND_BACK)", Falloff.index(easing=Easing.THERE_AND_BACK)),
    ("Falloff.index(easing=Easing.steps(4))", Falloff.index(easing=Easing.steps(4))),
    ("Falloff.index(reverse=True)", Falloff.index(reverse=True)),
    ("Falloff.constant(0.5)", Falloff.constant(0.5)),
]
grid = rows(len(RAMPS) + 1, labels="18fr")
label('dots.drive("scale", f.remap(0.15, 1.0))').place(grid[0], anchor="right")
for index, (code, falloff) in enumerate(RAMPS, start=1):
    label(code).place(grid[2 * index], anchor="right")
    stage = grid[2 * index + 1]
    radius = min(stage.height * 0.45, stage.width / 10 * 0.48)
    line_of(scene.geometry.circle(radius), stage, 10).drive("scale", falloff.remap(0.15, 1.0))
scene.wait(HOLD)

# Falloff.distance is 1 at a target and 0 at radius and beyond; the target is
# a point or a drawable it follows. falloff shapes the drop between them.
distance_start = scene.cursor
scene.segment("distance")
SHAPES = [
    ("Falloff.distance(ring)", {}),
    ('Falloff.distance(ring, falloff="linear")', {"falloff": "linear"}),
    ('Falloff.distance(ring, falloff="sharp")', {"falloff": "sharp"}),
    ('Falloff.distance(ring, falloff="round")', {"falloff": "round"}),
    ("Falloff.distance(ring, radius=0.8)", {"radius": 0.8}),
    ("Falloff.distance(ring, radius=3.0)", {"radius": 3.0}),
]
grid = rows(len(SHAPES) + 1, labels="18fr")
label('dots.drive("scale", f.remap(1.0, 2.5))').place(grid[0], anchor="right")
sweeps = []
for index, (code, options) in enumerate(SHAPES, start=1):
    label(code).place(grid[2 * index], anchor="right")
    stage = grid[2 * index + 1]
    (left, y), (right, _) = stage.point("left"), stage.point("right")
    ring = cursor(left, y)
    dots = line_of(scene.geometry.circle(stage.height * 0.13), stage, 12)
    dots.drive("scale", Falloff.distance(ring, **options).remap(1.0, 2.5))
    sweeps.append(ring.animate.move_to(right, y))
scene.wait(0.3)
scene.play(sweeps, duration=MOVE)
scene.wait(HOLD)

# Noise gives neighbors similar values and drifts with time: scale is cycles
# per scene unit, frequency how fast it drifts, octaves add detail. source
# brings a value over time, the same for every member; linear ramps from a
# start to an end, here one that moves. Each panel drives rotation and fill.
fields_start = scene.cursor
scene.segment("fields")
wave = computed(lambda t: 0.5 + 0.5 * math.sin(3 * t), inputs=[scene.time])
moving = []


def sweep(art):
    """A ramp from the left of ``art`` to a ring that crosses it."""
    (left, y), (right, _) = art.point("left"), art.point("right")
    ring = cursor(left, y)
    moving.append(ring.animate.move_to(right, y))
    return Falloff.linear((left, y), ring)


FIELDS = [
    ("Falloff.noise()", lambda art: Falloff.noise()),
    ("Falloff.noise(scale=1.5)", lambda art: Falloff.noise(scale=1.5)),
    ("Falloff.noise(octaves=4)", lambda art: Falloff.noise(octaves=4)),
    ("Falloff.noise(frequency=2.0)", lambda art: Falloff.noise(frequency=2.0)),
    ("Falloff.source(wave)", lambda art: Falloff.source(wave)),
    ("Falloff.linear(left, ring)", sweep),
]
for (art, caption), (code, make) in zip(gallery(len(FIELDS)), FIELDS):
    label(code).place(caption)
    squares = field(art, lambda spacing: scene.geometry.square(spacing * 0.62))
    falloff = make(art)
    squares.drive("rotation", falloff.remap(-0.8, 0.8))
    squares.drive("fill", falloff.gradient(chart, accent))
scene.wait(0.3)
scene.play(moving, duration=MOVE)
scene.wait(HOLD)

# Falloffs combine with +, - and * (with numbers or other falloffs), maximum
# and minimum. In each panel a = Falloff.distance(ring_a, radius=1.5) follows
# a ring crossing to the right and b one crossing to the left; the result
# drives scale and fill.
combine_start = scene.cursor
scene.segment("combine")
COMBINED = [
    ("a.maximum(b)", lambda a, b: a.maximum(b)),
    ("a.minimum(b)", lambda a, b: a.minimum(b)),
    ("a + b", lambda a, b: a + b),
    ("a * Falloff.index()", lambda a, b: a * Falloff.index()),
    ("a.invert()", lambda a, b: a.invert()),
    ("a.maximum(0.3) - 0.3 * b", lambda a, b: a.maximum(0.3) - 0.3 * b),
]
crossings = []
for (art, caption), (code, combine) in zip(gallery(len(COMBINED)), COMBINED):
    label(code).place(caption)
    dots = field(art, lambda spacing: scene.geometry.circle(spacing * 0.2), columns=14)
    (left, y), (right, _) = art.point("left"), art.point("right")
    ring_a, ring_b = cursor(left, y), cursor(right, y)
    falloff = combine(Falloff.distance(ring_a, radius=1.5), Falloff.distance(ring_b, radius=1.5))
    dots.drive("scale", falloff.remap(1.0, 2.2))
    dots.drive("fill", falloff.gradient(chart, accent))
    crossings += [ring_a.animate.move_to(right, y), ring_b.animate.move_to(left, y)]
scene.wait(0.3)
scene.play(crossings, duration=MOVE)
scene.wait(HOLD)

# look_at turns each member to point its x axis at a drawable it follows, or
# at a point; offset adds radians. clear_drive ends every drive and look_at
# at the cursor, and the members return to how they were authored.
aim_start = scene.cursor
scene.segment("look_at")
panels = scene.layout.zones(Zones.grid(rows=1, columns=2, gap="48px", padding="page_padding"))
orbits = []
fields = []
for zone, (code, offset) in zip(panels, [
    ("arrows.look_at(ring)\narrows.clear_drive()", 0.0),
    ("arrows.look_at(ring, offset=math.pi)\narrows.clear_drive()", math.pi),
]):
    art, caption = zone.split(Zones.rows(["1fr", 0.9]))
    label(code).place(caption)
    arrows = field(
        art.inset(0.3),
        lambda spacing: scene.geometry.polygon([(-0.3, -0.2), (0.35, 0.0), (-0.3, 0.2)]).scale_to(spacing * 0.9),
        columns=8,
        rows=8,
    )
    radius = min(art.width, art.height) * 0.3
    path = scene.geometry.circle(radius).no_fill().stroke(rule, 0.02).move_to(*art.center)
    ring = cursor(art.center[0] + radius, art.center[1])
    arrows.look_at(ring, offset=offset)
    arrows.drive("fill", Falloff.distance(ring, radius=2.0).gradient(chart, accent))
    orbits.append(ring.animate.move_along(path).easing(Easing.LINEAR))
    fields.append(arrows)
scene.wait(0.3)
scene.play(orbits, duration=2 * MOVE)
for arrows in fields:
    arrows.clear_drive()
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The static ramps, then each moving segment early, mid and late; last, the arrows after clear_drive.
    scene.snapshots(
        snapshots,
        [channels_start + 0.4, ramps_start + 0.4]
        + [distance_start + 0.3 + MOVE * t for t in (0.2, 0.5, 0.8)]
        + [fields_start + 0.3 + MOVE * t for t in (0.1, 0.5, 0.9)]
        + [combine_start + 0.3 + MOVE * t for t in (0.2, 0.5, 0.8)]
        + [aim_start + 0.3 + 2 * MOVE * t for t in (0.0, 0.3, 0.7)]
        + [aim_start + 0.3 + 2 * MOVE + 0.5],
    )
else:
    scene.render()
