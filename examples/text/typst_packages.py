"""Typst Universe packages: zebraw code listings and simple-plot function plots.

A Typst document compiled with ``scene.text.typst`` can import any package
from Typst Universe with ``#import "@preview/name:version"``; Gaanim fetches
it the first time and caches it. Here zebraw numbers, highlights and
annotates code listings, and simple-plot draws function plots. The theme's
colors are passed into each package's options. Each label is the package
option that builds the document under it.
"""

import os

from gaanim import Scene, Zones

WRITE = 1.6
HOLD = 1.0
GAP = 0.3

scene = Scene()


def typst_color(role):
    """A theme color as a Typst ``rgb(...)`` expression."""
    color = scene.canvas.color(role)
    return f"rgb({color.r}, {color.g}, {color.b})"


THEME = "\n".join(
    f"#let {role} = {typst_color(role)}"
    for role in ["background", "foreground", "muted", "accent", "success", "rule", "panel"]
)

# zebraw's colors, from the theme; each listing adds its own options.
ZEBRAW = THEME + """
#import "@preview/zebraw:0.6.3": *
#let listing = zebraw.with(
  background-color: panel,
  highlight-color: accent.transparentize(75%),
  comment-color: panel.lighten(6%),
  lang-color: accent,
  lang-font-args: (fill: background),
  numbering-font-args: (fill: muted),
)
"""
CODE = """```python
def area(r):
    # the area of a circle
    return pi * r * r
```"""

# simple-plot's axes, grid, ticks and labels, from the theme.
PLOT = THEME + """
#import "@preview/simple-plot:1.0.0": plot
#let themed = (
  axis: (stroke: rule + 0.8pt, arrow: (symbol: "stealth", fill: rule, scale: 0.55)),
  grid: (major: (stroke: panel.lighten(8%) + 0.5pt), minor: (stroke: panel + 0.3pt)),
  ticks: (stroke: rule + 0.6pt, label-fill: muted, label-bg: none),
  labels: (fill: foreground, bg: none),
)
"""


def pages(variants, rows=1, write=True):
    """Show documents in a rows x 2 grid, each under its label; ``variants`` are (label, document) pairs.

    The documents are written, or faded in when ``write`` is false.
    """
    grid = scene.layout.zones(
        Zones.grid(rows=rows, columns=2, column_gap="96px", row_gap="48px", padding="page_padding")
    )
    for cell, (code, drawable) in zip(grid, variants):
        name = scene.text(code, role="code", markup=False)
        # A band as tall as the label, a gap and the document, centered in the cell.
        height = name.bounds().height + GAP + drawable.bounds().height
        band = cell.inset(((cell.height - height) / 2, 0))
        name.place(band, anchor="top_left")
        drawable.place(band, anchor="bottom_left")
    scene.play([
        (drawable.animate.write() if write else drawable.animate.fade_in()).duration(WRITE)
        for _, drawable in variants
    ])
    scene.wait(HOLD)


# zebraw numbers the lines of a raw block and tags its language; it can
# highlight lines, attach a comment to one, and add a header and a footer.
# Each label is an option of listing, the themed zebraw above.
scene.segment("zebraw")
pages(
    [
        ("lang: true", scene.text.typst(ZEBRAW + f"#listing(lang: true)[{CODE}]", width="6cm")),
        ("highlight-lines: (1, 3)",
         scene.text.typst(ZEBRAW + f"#listing(highlight-lines: (1, 3))[{CODE}]", width="6cm")),
        ("highlight-lines: ((2, [...]),)",
         scene.text.typst(ZEBRAW + f"#listing(highlight-lines: ((2, [Comments never run.]),))[{CODE}]", width="6cm")),
        ("header: [...], footer: [...]",
         scene.text.typst(ZEBRAW + f"#listing(header: [*geometry.py*], footer: [3 lines])[{CODE}]", width="6cm")),
    ],
    rows=2,
)

# simple-plot draws functions (fn:) between axis limits, with ticks, grids
# and labels; xtick-labels can name ticks with math.
scene.segment("simple-plot")
pages([
    ("plot((fn: ...), (fn: ...))", scene.text.typst(PLOT + """
#plot(
  style: themed,
  xmin: -3, xmax: 3, ymin: -0.5, ymax: 4,
  width: 6, height: 4,
  xlabel: $x$, ylabel: $y$,
  show-grid: "major",
  show-origin: false,
  (fn: x => x * x, stroke: accent + 1.2pt, label: $x^2$),
  (fn: x => calc.exp(x) / 4, stroke: success + 1.2pt, label: $e^x / 4$),
)
""")),
    ("xtick-labels: ($-pi$, $pi$)", scene.text.typst(PLOT + """
#plot(
  style: themed,
  xmin: -2 * calc.pi, xmax: 2 * calc.pi, ymin: -1.5, ymax: 1.5,
  width: 5.2, height: 3,
  xlabel: $x$, ylabel: $y$,
  show-grid: "major",
  show-origin: false,
  xtick: (-calc.pi, calc.pi),
  xtick-labels: ($-pi$, $pi$),
  (fn: x => calc.sin(x), stroke: accent + 1.2pt, samples: 200, label: $sin(x)$),
  (fn: x => calc.cos(x), stroke: success + 1.2pt, samples: 200, label: $cos(x)$),
)
""")),
], write=False)

STEP = WRITE + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Early, mid-entrance and settled in each segment.
    scene.snapshots(snapshots, [STEP * index + t for index in range(2) for t in (0.3, 0.8, 2.2)])
else:
    scene.render()
