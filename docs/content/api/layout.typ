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

== Layout possibility atlas

This section is a compact catalogue of the complete public Layout v2 design
space. The features are orthogonal: a grid can be nested in a column, a stack
can occupy one fractional track, responsive text can grow inside either, and
the resulting tree can still be constrained and animated.

#table(
  columns: (1.05fr, 1.15fr, 2.4fr),
  inset: 7pt,
  [*Goal*], [*API*], [*What it controls*],
  [Horizontal flow], [`scene.row(...)`], [Main axis is left to right; optionally wraps into new rows.],
  [Vertical flow], [`scene.column(...)`], [Main axis is top to bottom; optionally wraps into new columns.],
  [Track layout], [`scene.grid(...)`], [Fixed, intrinsic `auto`, and weighted `fr` rows and columns, spans, and auto-placement.],
  [Overlays], [`scene.stack(...)`], [Shared containing box for backgrounds, media, captions, badges, and absolute children.],
  [Outer size], [`width` / `height`], [A fixed canvas-unit number, intrinsic `"hug"`, or available-space `"fill"`.],
  [Flexible children], [`scene.item(...)`], [`grow`, `shrink`, per-item alignment, grid coordinates, spans, offsets, anchors, and fitting.],
  [Spacing], [`padding`, `gap`], [One inset, vertical/horizontal insets, four side insets, and independent grid row/column gaps.],
  [Alignment], [`align`, `justify`], [Cross-axis placement and main-axis distribution.],
  [Root bounds], [`within="safe"` / `"frame"`], [Whether a root consumes the margin-aware safe frame or the complete viewport.],
  [Responsive content], [`TextFlow(wrap="auto")`], [Remeasures text using the width offered by its final nested Layout box.],
  [Media fitting], [`fit=...`], [`none`, `contain`, `cover`, `stretch`, or `scale_down`; `cover` also clips.],
  [Relations], [`scene.constrain(...)`], [Linear equations and inequalities between geometry in different branches.],
  [Live structure], [`add` / `remove` / `replace` / `configure`], [Creates an instant or animated deterministic reflow snapshot.],
  [Reusable pages], [`scene.template(...)` / `segment.bind(...)`], [Typed slots, built-in presentation patterns, and theme-driven spacing tokens.],
)

=== Complete scene without coordinates

The following scene uses a nested `column -> row -> stack -> column` tree. No
child calls `at()`: the outer tree decides every translation, and the text is
remeasured at the width of the card that contains it.

```python
from gaanim import BLUE, GOLD, WHITE, Scene, TextFlow

scene = Scene(1280, 720, background="#0b1020", margin=48)

copy = scene.text(
    "The same tree can drive a slide, a dashboard, or a vertical video.",
    role="body",
    color=WHITE,
    flow=TextFlow(wrap="auto", line_spacing=1.2),
)

card = scene.stack(
    [
        scene.item(scene.rounded_rect(360, 220, 18).fill(BLUE), fit="stretch"),
        scene.column(
            [
                scene.text("Measured content", role="heading", color=GOLD),
                copy,
            ],
            width="fill",
            height="fill",
            padding=28,
            gap=18,
            align="stretch",
            justify="center",
        ),
    ],
    width=360,
    height=220,
    align="stretch",
)

body = scene.row(
    [
        scene.item(card, grow=2),
        scene.item(scene.circle(96).fill(GOLD), grow=1, align="center"),
    ],
    width="fill",
    gap=40,
    align="center",
)

page = scene.column(
    [
        scene.text("Layout v2 atlas", role="title", color=GOLD),
        scene.item(body, grow=1, align="stretch"),
        scene.text("No manual coordinates", role="caption", color=WHITE),
    ],
    within="safe",
    width="fill",
    height="fill",
    padding=(24, 40),
    gap=32,
    align="stretch",
    justify="between",
)

scene.play([page.fade_in().duration(0.6)])
scene.render()
```

=== Sizing, padding, and flexible space

`"hug"` measures intrinsic content, a number fixes the outer box in canvas
units, and `"fill"` consumes the available constraint. `grow` distributes
remaining main-axis space between siblings; `shrink` decides which siblings
may contract when the row or column is tighter than their preferred size.

```python
badge = scene.row([icon, label], width="hug", padding=(8, 14), gap=8)

workspace = scene.row(
    [
        scene.item(sidebar, grow=0, shrink=0),
        scene.item(content, grow=3, shrink=1, align="stretch"),
        scene.item(inspector, grow=1, shrink=1),
    ],
    width="fill",
    height="fill",
    padding=(24, 40),       # vertical, horizontal
    gap=32,
    align="stretch",
)

workspace.configure(
    min_width=640,
    max_width=1180,
    min_height=360,
    aspect_ratio=16 / 9,
)
```

Padding accepts `padding=24`, `padding=(vertical, horizontal)`, or
`padding=(top, right, bottom, left)`. Rows and columns use one `gap`; grids can
override it with `row_gap` and `column_gap`.

=== Alignment and distribution

`align` controls the cross axis and accepts `start`, `center`, `end`, or
`stretch`. `justify` controls the main axis and accepts `start`, `center`,
`end`, `between`, `around`, or `evenly`. A child can override cross-axis
alignment through `scene.item(..., align=...)`.

```python
toolbar = scene.row(
    [back, scene.item(search, grow=1, align="stretch"), actions],
    width="fill",
    align="center",
    justify="between",
)

steps = scene.column(
    [intro, explanation, result],
    height="fill",
    align="stretch",
    justify="evenly",
)

chips = scene.row(tags, width=620, gap=12, wrap=True, align="center")
```

With `wrap=True`, rows start another row when the next child exceeds the
available width; columns use the corresponding vertical rule and start another
column. Absolute children never consume flow space.

=== Every per-item rule

#table(
  columns: (0.95fr, 1.15fr, 2.7fr),
  inset: 7pt,
  [*Rule*], [*Values*], [*Effect*],
  [`grow`], [non-negative number], [Weighted share of unused main-axis space.],
  [`shrink`], [non-negative number], [Relative permission to contract under pressure; `0` protects the preferred size.],
  [`align`], [`start | center | end | stretch`], [Overrides the container's cross-axis alignment for this child.],
  [`row`, `column`], [zero-based integer or `None`], [Chooses an explicit grid origin; omitted coordinates use deterministic auto-placement.],
  [`row_span`, `column_span`], [integer >= 1], [Occupies multiple grid tracks.],
  [`absolute`], [`True | False`], [Removes the child from flow measurement and places it against the containing box.],
  [`anchor`], [`Anchor.*`], [Selects center, edge, or corner placement inside a stack, grid cell, or absolute containing box.],
  [`offset`], [`(x, y)`], [Adds an intentional editorial displacement without surrendering Layout ownership.],
  [`fit`], [`none | contain | cover | stretch | scale_down`], [Maps drawable geometry into the assigned box.],
)

These rules work at construction time and can be changed later without
rebuilding the tree:

```python
page.configure_item(
    chart,
    grow=2,
    shrink=1,
    align="stretch",
    offset=(12, 0),
    fit="contain",
    animate=0.35,
)
```

=== Responsive roots and nested trees

Use `within="safe"` for presentation content that respects scene margins and
`within="frame"` for full-bleed backgrounds. Nested layouts normally leave
`within=None`; they receive their constraints from the parent. Dirty state and
responsive measurement propagate to the outermost owner automatically.

```python
background = scene.stack(
    [scene.item(photo, fit="cover")],
    within="frame",
    width="fill",
    height="fill",
    align="stretch",
)

content = scene.column(
    [title, scene.text(copy, flow=TextFlow(wrap="auto")), footer],
    within="safe",
    width="fill",
    height="fill",
    align="stretch",
    justify="between",
)

page = scene.stack([background, content], within="frame", width="fill", height="fill")
```

The same tree can target 16:9 and 9:16. Flow wrapping, fractional tracks,
`grow`, and responsive text adapt to the offered viewport; use separate trees
only when the editorial hierarchy itself changes.

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
