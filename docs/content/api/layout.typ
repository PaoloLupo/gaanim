#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Layouts",
  description: "Regions, grids, paragraphs, stacks, flows, and video presets",
  route: "/api/layout/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Layouts

Gaanim layouts are composition tools for video scenes. A `FrameLayout` splits
the safe frame into `header`, `content`, and `footer` regions. Regions place
regular drawables using the existing `Anchor` values, can be inset, and can be
subdivided into grids.

```python
from gaanim import Anchor, Scene

scene = Scene(1920, 1080, margin=72)
layout = scene.frame_layout(header=180, footer=72, gap=32)

title = layout.header.place(scene.title("Fourier transform"), Anchor.TOP_LEFT)
footer = layout.footer.place(scene.text("Chapter 1"), Anchor.BOTTOM_RIGHT)
```

== Video presets

Presets derive their bands from the safe frame, so the same script adapts to
different viewport sizes.

```python
lecture = scene.layout_preset("lecture")
comparison = scene.layout_preset("comparison")
vertical = scene.layout_preset("vertical_short")
minimal = scene.layout_preset("minimal")
```

Use presets for the common editorial structure, then refine individual regions
with `inset` or a grid. `frame_layout(...)` remains available when exact header,
footer, and gap dimensions are needed.

== Regions and anchors

`LayoutRegion.place` pins the same anchor on a drawable and its target region.
This means text, images, groups, and shapes all follow the same placement API.

```python
safe = layout.content.inset(24)
badge = safe.place(scene.text("New").z_index(100), Anchor.TOP_RIGHT)
center = safe.point(Anchor.CENTER)
```

`inset(value)` applies the same inset on every edge. The expanded form is
`inset(top, right, bottom, left)`.

== Equal grids and spans

Rows are numbered from top to bottom; columns from left to right. `cell` picks
one cell and `area` joins a rectangular span.

```python
grid = layout.content.grid(rows=2, columns=12, row_gap=24, column_gap=24)

copy = grid.area(0, 0, row_span=2, column_span=5).inset(16)
visual = grid.area(0, 5, row_span=2, column_span=7).inset(16)

paragraph = copy.place(scene.paragraph("Explanation", width=copy.width), Anchor.TOP_LEFT)
diagram = visual.place(scene.circle(140), Anchor.CENTER)
```

== Fixed and fractional tracks

For asymmetric layouts, `grid_tracks` accepts fixed numbers and fractional
strings. A number consumes a fixed amount of scene space; `fr` tracks divide
the remaining space by weight.

```python
grid = layout.content.grid_tracks(
    rows=["1fr"],
    columns=[260, "1fr", "2fr"],
    column_gap=24,
)

fixed = grid.cell(0, 0)
middle = grid.cell(0, 1)
wide = grid.cell(0, 2)
```

Here the first column is 260 units wide while the remaining width is split 1:2.
Intrinsic `auto` tracks are planned for a later measurement phase.

== Paragraphs

`paragraph` uses Typst to wrap and compose multi-line vector text. The result
is still a regular `Drawable`, so it can be placed, styled, and animated.

```python
body = scene.paragraph(
    "A longer explanation that wraps inside the available column.",
    width=copy.width,
    align="justify",  # left | center | right | justify
    line_spacing=1.25,
    font_size=34,
    font_family="New Computer Modern",
    max_lines=4,
    overflow="clip",
)
copy.place(body, Anchor.TOP_LEFT)
```

`max_lines` reserves a deterministic text-box height. With the default
`overflow="clip"`, additional lines do not invade the next layout region. Use
`overflow="visible"` when the text is intentionally allowed to extend beyond
that box.

== Stacks and flow

`scene.layout(...)` is the persistent presentation container. It can contain
drawables or other layouts. Calling `add(..., animate=...)` inserts an item,
reflows every affected container, and animates the displaced elements.

```python
agenda = scene.layout("column", gap=20)
agenda.add(scene.text("Prepare"))
agenda.add(scene.text("Explain"))
agenda.add(scene.text("New section"), at=1, animate=0.45)

side_by_side = scene.layout("row", gap=48)
cards = scene.layout("grid", columns=2, gap=24)
cards.add(scene.circle(32))
side_by_side.add(agenda)
side_by_side.add(cards)
```

`row`, `column`, and `grid` are the only layout kinds. The same `add` and
`reflow` operations work at every level of the tree.

Groups can arrange their direct children with `vstack` or `hstack` before the
group itself is placed in a region.

```python
items = [scene.text("Prepare"), scene.text("Explain"), scene.text("Close")]
stack = scene.group(items).vstack(gap=20)
copy.place(stack, Anchor.TOP_LEFT)
```

When items are produced sequentially, `Flow` removes the explicit group:

```python
flow = scene.flow(direction="vertical", gap=20, align=Anchor.LEFT)
flow.add(scene.text("Steps").scaled(1.25))
flow.add(scene.text("1. Add content"))
flow.add(scene.text("2. Build the flow"))

content = layout.content.place(flow.build(), Anchor.TOP_LEFT)
```

After `build`, a flow is immutable and repeated calls return the same grouped
drawable. This avoids accidentally duplicating content in the timeline.

== Floating labels

An overlay can be placed in the full frame with a high z-index. For labels
that should follow an animated object, combine the layout API with `follow_to`.

```python
overlay = layout.frame.inset(32).place(
    scene.text("Chapter 2").z_index(100),
    Anchor.TOP_RIGHT,
)

label = scene.text("maximum").z_index(100)
label.follow_to(point, offset=(0, 36))
```
