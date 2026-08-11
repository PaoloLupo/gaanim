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
        scene.text("Result", role="title"),
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

The public constructors are:

```python
scene.row(children, *, gap=24, padding=0, width="hug", height="hug",
          align="center", justify="start", wrap=False, within=None)
scene.column(children, *, gap=24, padding=0, width="hug", height="hug",
             align="start", justify="start", wrap=False, within=None)
scene.grid(children, *, rows=1, columns=1, gap=0, row_gap=None,
           column_gap=None, padding=0, width="hug", height="hug",
           align="stretch", justify="start", auto_flow="row", within=None)
scene.stack(children, *, padding=0, width="hug", height="hug",
            align="center", within=None)
```

`within=None` makes a nested/intrinsic container; root layouts normally choose
`"safe"` or `"frame"`. Non-negative numeric values use canvas units. Invalid
sizes, padding, tracks, alignments, or containing blocks raise `ValueError`.

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

Explicit cells and their spans are reserved before auto-placement, independent
of child order. An explicit collision, an out-of-range span, or a grid without
enough free cells aborts layout resolution with the involved node ID.

== Ownership and reflow

After a drawable is attached, positional calls such as `at`, `next_to`,
`align_to`, `to_edge`, or manual movement animations raise
`LayoutOwnershipError`. Rotation and scale remain valid. Express intentional
displacement with `offset`.

```python
page.add(extra, at=1, animate=0.35)
page.replace(old, new, animate=0.35)
page.configure(gap=40, padding=56, animate=0.4)
page.configure(min_width=480, max_width=960, aspect_ratio=16 / 9)
page.configure_item(chart, grow=2, offset=(12, 0), animate=0.3)
page.reflow(animate=0.25)
```

Structural mutations propagate through nested layouts. Timeline operations
store versioned tree snapshots, so direct seek and sequential playback resolve
the same target geometry.

`add`, `remove`, and `replace` operate on direct children. Their `animate`
value and the `animate` values on `configure`, `configure_item`, and `reflow`
are durations in seconds. With `None`, the timeline records an instant,
deterministic transition.

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

`scene.check_layout()` returns soft-constraint diagnostics immediately after
registration; `layout.diagnostics()` filters diagnostics for one root. A
conflict message includes its label or canonical index and involved node IDs.
Expressions cannot mix drawables from different scenes.

== Responsive Text and templates

`scene.text(..., flow=TextFlow(wrap="auto"))` is the only responsive text
leaf. Free text receives the safe-frame width; managed text consumes the width
offered by its `BoxConstraints`. `wrap=False` keeps one line and a numeric
wrap value caps the typographic width without creating a second box model.

```python
from gaanim import TextFlow

copy = scene.text(
    "Layout v2 measures this copy at the width of its card.",
    flow=TextFlow(wrap="auto", align="justify", line_spacing=1.25),
)
page = scene.row([
    scene.item(copy, grow=2),
    scene.item(diagram, grow=3, fit="contain"),
], width="fill", gap=32)
```

The `CompiledTextMeasure` adapter reuses the existing intrinsic-measure pass,
eight-pass convergence, clips, diagnostics, and `ResolvedLayout`; there is no
text-specific solver. Its cache key includes structured content, resolved
style, flow, and offered constraints. Metric changes and `become`, `morph_to`,
`step_to`, or `expand_to` invalidate the shared versioned snapshot and reflow
parent layouts using the same transition duration. Transient `wiggle`, `pulse`,
and `wave` effects do not invalidate measurement.

A managed `Text` rejects `at`, `move`, `next_to`, and manual positional
animations just like any other Layout-owned child. A `TextSelection` never
becomes an independent Layout leaf, but its animation methods return normal
`Anim` values, so selections compose in `scene.play([...])` with any other
drawable animation. Cross-scene or incompatible-owner text transition targets
raise `LayoutOwnershipError`.

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

Built-in templates consume theme layout tokens instead of isolated dimensions.
Read them with `scene.canvas.layout_token(name)` and override them with
`Theme(..., layout={"page_padding": 56, "column_gap": 48})`.
