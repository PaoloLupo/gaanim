"""Equations: Typst math notation, inline and display math, quantities, sizes and aligned lines.

``scene.text.equation`` typesets Typst math given without its dollars, and
``scene.text`` mixes ``$...$`` math into prose. Each segment covers one part
of the typesetting API, and each label is the code that builds the equation
beside it.
"""

import os

from gaanim import Scene, Zones, quantity

WRITE = 1.2
HOLD = 0.8
SEGMENTS = 10
# Equations are set larger than the theme's math size (0.44) so that they
# read at a glance.
SIZE = 0.8

scene = Scene()
accent = scene.colors.accent
equation = scene.text.equation


def quoted(source):
    """The Python literal of a Typst source, as the code that passes it."""
    quote = "'" if '"' in source else '"'
    return quote + source.replace("\\", "\\\\") + quote


def typeset(source):
    return equation(source, size=SIZE)


def table(rows, columns=None, build=typeset, tracks=("1fr", "1fr")):
    """Write one equation per cell, with each row's label to the left of its equations.

    ``rows`` are (label, value) pairs. Without ``columns``, ``build(value)``
    makes each row's equation. Otherwise ``columns`` are (label, value) pairs
    too, labelled in a header row, and ``build(row_value, column_value)``
    makes each cell's equation.
    """
    header = [] if columns is None else columns
    tracks = [tracks[0]] + [tracks[1]] * max(len(header), 1)
    grid = scene.layout.zones(
        Zones.grid(rows=len(rows) + bool(header), columns=tracks, column_gap="48px", padding="page_padding")
    )
    for index, (label, _) in enumerate(header, start=1):
        scene.text(label, role="code").place(grid[index], anchor="left")
    equations = []
    for row_index, (label, row) in enumerate(rows, start=bool(header)):
        first = row_index * len(tracks)
        scene.text(label, role="code").place(grid[first], anchor="right")
        cells = [build(row)] if columns is None else [build(row, column) for _, column in columns]
        for index, cell in enumerate(cells, start=1):
            equations.append(cell.place(grid[first + index], anchor="left"))
    scene.play([shown.animate.write().duration(WRITE) for shown in equations])
    scene.wait(HOLD)


def sources(*typst):
    """Rows that typeset each Typst source, labelled with the source."""
    return [(quoted(source), source) for source in typst]


# Typst math: scripts, fractions and roots...
scene.segment("notation")
table(sources(
    "E = m c^2",
    "x_1 + x_2 + dots.c + x_n",
    "(a + b) / 2 = 1/2 (a + b)",
    "sqrt(x) + root(3, y)",
))

# ...symbols by name, shorthands, accents and delimiters.
scene.segment("symbols")
table(sources(
    "alpha + beta != pi",
    "x <= y => f(x) <= f(y)",
    "hat(x) + arrow(v) + overline(z)",
    "abs(x) + norm(v) + floor(y)",
))

# Big operators take their limits above and below...
scene.segment("operators")
table(sources(
    "sum_(k=1)^n k = (n(n + 1)) / 2",
    "integral_0^1 x^2 dif x = 1/3",
    "lim_(x -> 0) (sin x) / x = 1",
))

# ...and arrays line their entries up.
scene.segment("arrays")
table(
    sources(
        "mat(1, 2; 3, 4) vec(x, y)",
        'cases(x "if" x >= 0, -x "if" x < 0)',
        "binom(n, k) = (n!) / (k! (n - k)!)",
    ),
    tracks=("4fr", "3fr"),
)

# scene.text sets $...$ inline, beside prose; spaces inside the dollars set a
# display equation, as scene.text.equation does, with its limits above and below.
scene.segment("inline")
table(
    [
        ('scene.text("$sum_(k=1)^n 1/k$", size=0.8)', lambda: scene.text("$sum_(k=1)^n 1/k$", size=0.8)),
        ('scene.text("$ sum_(k=1)^n 1/k $", size=0.8)', lambda: scene.text("$ sum_(k=1)^n 1/k $", size=0.8)),
        ('scene.text("$H_n$ grows", size=0.8)', lambda: scene.text("$H_n$ grows", size=0.8)),
    ],
    build=lambda make: make(),
    tracks=("2fr", "1fr"),
)

# quantity() writes a number and its unit as math for scene.text.equation:
# a thin space, upright units, exponents and a slash...
scene.segment("quantities")
table(
    [
        ('quantity(9.81, "m/s^2")', quantity(9.81, "m/s^2")),
        ('quantity(1, "m^3")', quantity(1, "m^3")),
        ('quantity(1000, "L")', quantity(1000, "L")),
        ('quantity(6, "kg m/s^2")', quantity(6, "kg m/s^2")),
    ],
)

# ...with negative exponents, degrees, fixed decimals and a decimal comma.
scene.segment("number_formats")
table(
    [
        ('quantity(50, "s^-1")', quantity(50, "s^-1")),
        ('quantity(21.5, "°C")', quantity(21.5, "°C")),
        ("quantity(3.14159, decimals=2)", quantity(3.14159, decimals=2)),
        ('quantity(2.5, "kg", decimal_separator=",")', quantity(2.5, "kg", decimal_separator=",")),
    ],
    tracks=("2fr", "1fr"),
)

# Size comes from the role, the size keyword or a scale...
scene.segment("sizes")
EULER = "e^(i pi) + 1 = 0"
table(
    [
        ("equation(EULER)", lambda: equation(EULER)),
        ("equation(EULER, size=0.8)", lambda: equation(EULER, size=0.8)),
        ("equation(EULER, size=1.2)", lambda: equation(EULER, size=1.2)),
        ("equation(EULER).scale_to(2)", lambda: equation(EULER).scale_to(2)),
    ],
    build=lambda make: make(),
)

# ...and color from the role, the color keyword or an opacity.
scene.segment("colors")
table(
    [
        ('equation(EULER, size=0.8, role="caption")', lambda: equation(EULER, size=0.8, role="caption")),
        ("equation(EULER, size=0.8, color=accent)", lambda: equation(EULER, size=0.8, color=accent)),
        ("equation(EULER, size=0.8, opacity=0.5)", lambda: equation(EULER, size=0.8, opacity=0.5)),
    ],
    build=lambda make: make(),
    tracks=("2fr", "1fr"),
)

# A backslash breaks the line and & marks the column the lines align on;
# line_spacing widens the gap between the lines, Typst's own at 1.
scene.segment("aligned")
table(
    sources(
        "f &= x^2 \\ f' &= 2 x",
        "x &= 2 + 3 \\ &= 5",
    ),
    [("line_spacing=1.0", 1.0), ("line_spacing=1.6", 1.6)],
    lambda source, spacing: equation(source, size=SIZE, line_spacing=spacing),
    tracks=("5fr", "4fr"),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-write and settled, in each segment.
    scene.snapshots(snapshots, [(WRITE + HOLD) * index + t for index in range(SEGMENTS) for t in (0.6, 1.9)])
else:
    scene.render()
