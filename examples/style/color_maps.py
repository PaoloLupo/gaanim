"""Color maps: built-in continuous and categorical maps, sampling, custom maps and data colored by value.

A ``ColorMap`` turns a position from 0 to 1 into a color. Each strip samples
its map at 64 evenly spaced positions with ``sample(t)``, and each row of
swatches is the list ``colors(n)`` returns. Labels are the code that builds
the map; the column on the right reads a property of it back.
"""

import math
import os

from gaanim import ColorMap, Direction, Scene, Zones

GROW = 0.8
HOLD = 1.2
STEPS = 64

scene = Scene()
background = scene.canvas.color("background")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
accent = scene.canvas.color("accent")
success = scene.canvas.color("success")
warning = scene.canvas.color("warning")
danger = scene.canvas.color("danger")


def label(code):
    """A variant label: the code, in the code role."""
    return scene.text(code, role="code", markup=False)


def row_of(zone, colors, width):
    """One rectangle per color across ``zone``, left to right, each ``width`` times its share of it."""
    share = zone.width / len(colors)
    y = zone.center[1]
    return scene.geometry.group([
        scene.geometry.rect(share * width, zone.height).fill(color).move_to(zone.left + share * (index + 0.5), y)
        for index, color in enumerate(colors)
    ])


def strip(zone, colors, overlap=True):
    """A continuous bar of slim cells; they overlap a little so no seam shows.

    Translucent cells must not overlap, or the overlaps would show darker.
    """
    return row_of(zone, colors, 1.02 if overlap else 1.0)


def swatches(zone, colors):
    """One swatch per color, with a gap between swatches."""
    return row_of(zone, colors, 0.86)


def sampled(colormap):
    """``colormap`` sampled at STEPS evenly spaced positions from 0 to 1."""
    return [colormap.sample(index / (STEPS - 1)) for index in range(STEPS)]


def table(rows, header=None):
    """One row per (label, build, readout): build(zone) draws the row's colors next to its label.

    ``header`` labels the readout column. The rows grow in from the left together.
    """
    grid = scene.layout.zones(
        Zones.grid(
            rows=len(rows) + 1, columns=[7.4, "1fr", 2.6], column_gap=0.4, row_gap="20px", padding="page_padding"
        )
    )
    if header:
        label(header).fill(muted).place(grid[2], anchor="left")
    bars = []
    for index, (code, build, readout) in enumerate(rows, start=1):
        label(code).place(grid[3 * index], anchor="right")
        bars.append(build(grid[3 * index + 1]))
        if readout is not None:
            label(readout).fill(muted).place(grid[3 * index + 2], anchor="left")
    scene.play([bar.animate.grow_from_edge(Direction.LEFT) for bar in bars], duration=GROW)
    scene.wait(HOLD)


def continuous(name, factory="ColorMap"):
    """A row for the built-in map ``name``, built by ``factory``: its sampled strip and its catalog."""
    colormap = ColorMap.named(name) if factory == "ColorMap.named" else ColorMap(name)
    return (f'{factory}("{name}")', lambda zone: strip(zone, sampled(colormap)), colormap.category)


# Built-in maps come from two catalogs: Matplotlib's and the Scientific Colour
# Maps. Each is sequential, diverging (two hues meeting at a light or dark
# middle) or cyclic (both ends match). ColorMap.names(category) lists them
# and ColorMap.named(name) is the same as ColorMap(name).
scene.segment("built_in")
table(
    [
        continuous("viridis"),
        continuous("magma"),
        continuous("coolwarm"),
        continuous("twilight"),
        continuous("batlow"),
        continuous("vik"),
        continuous("romaO"),
        continuous("lajolla", factory="ColorMap.named"),
    ],
    header=".category",
)

# Categorical maps hold distinct steps: sample(t) jumps from one to the next
# and colors(n) returns the first n in order. colors(n) on a continuous map
# spreads n colors evenly from end to end.
scene.segment("categorical")
tab10 = ColorMap("tab10")
viridis = ColorMap("viridis")
table(
    [
        ('ColorMap("tab10").colors(10)', lambda zone: swatches(zone, tab10.colors(10)), "True"),
        ('ColorMap("tab20").colors(20)', lambda zone: swatches(zone, ColorMap("tab20").colors(20)), "True"),
        ('ColorMap("tab20b").colors(20)', lambda zone: swatches(zone, ColorMap("tab20b").colors(20)), "True"),
        ('ColorMap("tab20c").colors(20)', lambda zone: swatches(zone, ColorMap("tab20c").colors(20)), "True"),
        ('ColorMap("tab10").sample(t)', lambda zone: strip(zone, sampled(tab10)), "True"),
        ('ColorMap("viridis").colors(5)', lambda zone: swatches(zone, viridis.colors(5)), "False"),
        ('ColorMap("viridis").colors(12)', lambda zone: swatches(zone, viridis.colors(12)), "False"),
    ],
    header=".categorical",
)



def translucent(zone):
    """The half-transparent viridis strip over a light bar along its middle."""
    scene.geometry.rect(zone.width, zone.height * 0.4).fill(foreground).move_to(*zone.center)
    return strip(zone, sampled(viridis.with_alpha(0.5)), overlap=False)


# Maps of your own: from_colors spreads its colors evenly, or at positions
# from 0 to 1, and has no name or category; reversed() flips any map and
# with_alpha() makes it translucent, over a light bar here so the bar shows
# through. The long labels sit above their strips.
scene.segment("custom")
WARM = [background, accent, foreground]
CUSTOM = [
    (
        "ColorMap.from_colors([background, accent, foreground])",
        lambda zone: strip(zone, sampled(ColorMap.from_colors(WARM))),
    ),
    (
        "ColorMap.from_colors([...], positions=[0, 0.8, 1])",
        lambda zone: strip(zone, sampled(ColorMap.from_colors(WARM, positions=[0, 0.8, 1]))),
    ),
    (
        "ColorMap.from_colors([success, warning, danger])",
        lambda zone: strip(zone, sampled(ColorMap.from_colors([success, warning, danger]))),
    ),
    ('ColorMap("viridis").reversed()', lambda zone: strip(zone, sampled(viridis.reversed()))),
    ('ColorMap("viridis").with_alpha(0.5)', translucent),
    ('ColorMap("vik").reversed()', lambda zone: strip(zone, sampled(ColorMap("vik").reversed()))),
]
grid = scene.layout.zones(Zones.rows(len(CUSTOM), gap="24px", padding="page_padding"))
bars = []
for cell, (code, build) in zip(grid, CUSTOM):
    caption, stage = cell.split(Zones.rows(["48px", "1fr"], gap="8px"))
    label(code).place(caption, anchor="left")
    bars.append(build(stage))
scene.play([bar.animate.grow_from_edge(Direction.LEFT) for bar in bars], duration=GROW)
scene.wait(HOLD)


# Data: a value scaled to 0..1 picks its color. The bars take their color
# from their height, with viridis = ColorMap("viridis"), and the cells from
# a wave over the grid, with magma = ColorMap("magma").
scene.segment("data")
left, right = scene.layout.zones(Zones.columns(2, gap="96px", padding="page_padding"))
plot_left, caption_left = left.split(Zones.rows(["1fr", "60px"], gap="24px"))
plot_right, caption_right = right.split(Zones.rows(["1fr", "60px"], gap="24px"))
label("viridis.sample(height)").place(caption_left)
label("magma.sample(wave)").place(caption_right)

COUNT = 16
heights = [0.5 + 0.5 * math.sin(index / COUNT * math.tau * 1.2 - 0.6) * math.cos(index / 7) for index in range(COUNT)]
low, high = min(heights), max(heights)
width = plot_left.width / COUNT
bars = []
for index, height in enumerate(heights):
    value = (height - low) / (high - low)
    tall = 0.3 + value * (plot_left.height - 0.3)
    bar = scene.geometry.rect(width * 0.8, tall).fill(viridis.sample(value))
    bars.append(bar.move_to(plot_left.left + width * (index + 0.5), plot_left.bottom + tall / 2))

magma = ColorMap("magma")
COLUMNS, ROWS = 14, 9
size = min(plot_right.width / COLUMNS, plot_right.height / ROWS)
x0 = plot_right.center[0] - size * COLUMNS / 2
y0 = plot_right.center[1] - size * ROWS / 2
cells = []
for row in range(ROWS):
    for column in range(COLUMNS):
        wave = math.sin(column / 2.2) * math.cos(row / 2.6) * 0.5 + 0.5
        cell = scene.geometry.square(size * 0.92).fill(magma.sample(wave))
        cells.append(cell.move_to(x0 + size * (column + 0.5), y0 + size * (row + 0.5)))
scene.play(
    [bar.animate.grow_from_edge(Direction.DOWN) for bar in bars] + [cell.animate.fade_in() for cell in cells],
    duration=GROW,
)
scene.wait(HOLD)

STEP = GROW + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Growing in and settled, in each of the four segments.
    scene.snapshots(snapshots, [STEP * index + t for index in range(4) for t in (0.4, 1.6)])
else:
    scene.render()
