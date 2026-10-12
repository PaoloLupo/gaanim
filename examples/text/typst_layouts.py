"""Typst documents: tables, grids and columns, aligned math, page widths and .typ files.

``scene.text.typst(source)`` compiles a whole Typst document into one
drawable, for layouts a ``Text`` cannot express: tables with merged cells,
grids and columns, multi-line aligned equations, wrapped paragraphs at a
page ``width``, and documents loaded from a ``.typ`` file. Its text is as
large as the body role at Typst's default 11pt. Each label is the Typst
feature or option that builds the document under it.
"""

import os
from pathlib import Path

from gaanim import Scene, Zones

WRITE = 1.6
HOLD = 1.0
GAP = 0.3

scene = Scene()


def typst_color(role):
    """A theme color as a Typst ``rgb(...)`` expression."""
    color = scene.canvas.color(role)
    return f"rgb({color.r}, {color.g}, {color.b})"


# Typst draws text in the theme's foreground; strokes and fills come from the
# document, so a preamble passes the theme's colors in.
THEME = f"""
#let accent = {typst_color("accent")}
#let rule = {typst_color("rule")}
#let panel = {typst_color("panel")}
#set table(stroke: 0.6pt + rule, inset: 6pt)
#set rect(stroke: 0.6pt + rule)
#set line(stroke: 0.6pt + rule)
"""


def document(source, **options):
    return scene.text.typst(THEME + source, **options)


def pages(variants):
    """Write two documents side by side, each under its label; ``variants`` are (label, document) pairs."""
    grid = scene.layout.zones(Zones.grid(rows=1, columns=2, column_gap="96px", padding="page_padding"))
    for cell, (code, drawable) in zip(grid, variants):
        name = scene.text(code, role="code", markup=False)
        # A band as tall as the label, a gap and the document, centered in the cell.
        height = name.bounds().height + GAP + drawable.bounds().height
        band = cell.inset(((cell.height - height) / 2, 0))
        name.place(band, anchor="top_left")
        drawable.place(band, anchor="bottom_left")
    scene.play([drawable.animate.write().duration(WRITE) for _, drawable in variants])
    scene.wait(HOLD)


# Tables: cells that span columns and rows, a header, per-cell alignment and
# fills computed from the cell position.
scene.segment("tables")
pages([
    ("table.cell(colspan: 2, rowspan: 2)", document("""
#table(
  columns: (auto, auto, auto),
  align: center + horizon,
  table.cell(colspan: 2, rowspan: 2)[*Two by two*], [top],
  [middle],
  [left], [center], [right],
)
""")),
    ("fill: (x, y) => ...", document("""
#table(
  columns: (auto, auto, auto),
  align: (left, right, right),
  fill: (x, y) => if y == 0 { panel } else if calc.even(y) { panel.lighten(4%) },
  table.header([*Method*], [*Error*], [*Time*]),
  [Baseline], [0.18], [12 s],
  [Adaptive], [0.07], [19 s],
  [GPU], [0.04], [2 s],
)
""")),
])

# Grids and columns lay out blocks side by side. Their 1fr tracks share the
# page width, so both documents pass width="6cm" (Typst units before
# scaling: at the body size, 6cm is about 6 scene units). Columns fill a
# block of fixed height, then flow into the next column.
scene.segment("grids")
pages([
    ("#grid(columns: (1fr, 2fr))", document("""
#grid(
  columns: (1fr, 2fr),
  gutter: 8pt,
  rect(width: 100%, height: 2.4cm)[*Figure*],
  rect(width: 100%, height: 2.4cm, stroke: 0.6pt + accent)[A wider column takes two shares of the width.],
)
""", width="6cm")),
    ("#columns(2)", document("""
#set par(justify: true)
#block(height: 2.4cm, columns(2, gutter: 10pt)[
  Text in two columns flows from the bottom of the first column into
  the top of the second, like a printed page: the first column fills
  before the second one starts.
])
""", width="6cm")),
])

# Math beyond one line: aligned steps with & and \, cases and matrices.
scene.segment("math")
pages([
    ("$ a &= b \\ &= c $", document("""
$ (a + b)^2 &= (a + b)(a + b) \\
            &= a^2 + a b + b a + b^2 \\
            &= a^2 + 2 a b + b^2 $
""")),
    ("cases(...) and mat(...)", document("""
$ |x| = cases(x &"if" x >= 0, -x &"if" x < 0) $
$ R(theta) = mat(cos theta, -sin theta; sin theta, cos theta) $
""")),
])

# width is the Typst page width before scaling: paragraphs wrap at it and
# #align(center) centers within it. Without width the page fits its content.
scene.segment("pages")
PARAGRAPH = """
#set par(justify: true)
#align(center)[*Page width*]
The same paragraph wraps at the page width that the width option sets,
measured before the document is scaled to the scene.
"""
pages([
    ('width="3.5cm"', document(PARAGRAPH, width="3.5cm")),
    ('width="6cm"', document(PARAGRAPH, width="6cm")),
])

# A Path loads a .typ file, resolved in assets/. #set text(size: ...)
# scales a document: 22pt is twice the body size.
scene.segment("files")
pages([
    ('Path("assets/note.typ")', scene.text.typst(Path("assets/note.typ"), width="6cm")),
    ("#set text(size: 22pt)", document("""
#set text(size: 22pt)
*Vector motion*
#set text(size: 11pt)

at the body size
""")),
])

STEP = WRITE + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Mid-write and settled in each segment.
    scene.snapshots(snapshots, [STEP * index + t for index in range(5) for t in (0.8, 2.2)])
else:
    scene.render()
