#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Layout v2",
  description: "Responsive trees, grid tracks, relational constraints, and animated reflow",
  route: "/api/layout/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Layout v2

Layout v2 is the single public composition model in Gaanim. A `Layout` is a
`Drawable` and owns the translation of its direct children. Build responsive
trees with `row`, `column`, `grid`, and `stack`; use `item` for per-child rules.

```python
page = scene.column(
    [
        scene.title("Result"),
        scene.row([
            scene.item(copy, grow=2),
            scene.item(diagram, grow=3, fit="contain"),
        ], gap=40, align="center"),
        footer,
    ],
    within="safe",
    width="fill",
    height="fill",
    padding=48,
    gap=32,
    align="stretch",
    justify="between",
)
```

`width` and `height` accept a fixed number, `"hug"`, or `"fill"`. Padding
accepts one value, `(vertical, horizontal)`, or `(top, right, bottom, left)`.
Alignment is `start`, `center`, `end`, or `stretch`; distribution also supports
`between`, `around`, and `evenly`.

== Grid and overlays

Grid tracks accept fixed values, `"auto"`, and fractional strings such as
`"2fr"`. Items can select a row/column and span tracks. Omitted positions use
deterministic row or column auto-placement.

```python
cards = scene.grid(
    [hero, scene.item(chart, column_span=2), notes],
    columns=[240, "1fr", "2fr"],
    rows=["auto", "1fr"],
    gap=24,
    width="fill",
)

overlay = scene.stack([
    scene.item(photo, fit="cover"),
    scene.item(caption, absolute=True, offset=(0, -180)),
], within="frame", width="fill", height="fill")
```

Image and SVG fitting supports `contain`, `cover`, `stretch`, and `scale_down`.
`cover` clips rendering to the assigned box.

== Ownership and reflow

After a drawable is attached, positional calls such as `at`, `next_to`,
`align_to`, `to_edge`, or manual movement animations raise
`LayoutOwnershipError`. Rotation and scale remain valid. Express intentional
displacement with `offset`.

```python
page.add(extra, at=1, animate=0.35)
page.replace(old, new, animate=0.35)
page.configure(gap=40, padding=56, animate=0.4)
page.configure_item(chart, grow=2, offset=(12, 0), animate=0.3)
page.reflow(animate=0.25)
```

Structural mutations propagate through nested layouts. Timeline operations
store versioned tree snapshots, so direct seek and sequential playback resolve
the same target geometry.

== Linear constraints

Every drawable exposes `left`, `right`, `top`, `bottom`, `center_x`, `center_y`,
`width`, and `height`. Expressions are linear: addition/subtraction and scalar
multiplication/division only.

```python
relations = scene.constrain(
    (label.left == chart.right + 24).strong(),
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.30).weak(),
)
```

Relations are `required` by default, with `strong`, `medium`, and `weak`
alternatives. Required conflicts fail before rendering. Stable IDs, canonical
constraint ordering, and explicit weak stays make equivalent solutions
reproducible.

== Responsive paragraphs and templates

`scene.paragraph(text, width=None)` resolves against the safe frame when free;
inside responsive compositions it is a measurable text leaf. Explicit widths
remain supported.

Templates are typed Python functions:

```python
from gaanim import comparison, layout_template

@layout_template
def two_columns(scene, *, title, left, right, footer=None):
    return scene.column([
        title,
        scene.row([scene.item(left, grow=1), scene.item(right, grow=1)]),
        footer,
    ], within="safe", width="fill", height="fill")

page = scene.template(two_columns, title=title, left=copy, right=diagram)
slide = scene.segment("Comparison", template=comparison)
page = slide.bind(title=title, left=copy, right=diagram)
```

Built-ins are `title_slide`, `lecture`, `comparison`, `vertical_short`,
`minimal`, `lower_third`, and `credits`.

