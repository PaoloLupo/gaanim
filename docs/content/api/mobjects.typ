#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Mobjects",
  description: "Drawable objects available from Scene",
  route: "/api/mobjects/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Mobjects

Every factory on `Scene` returns a `Drawable`. A drawable can be configured
fluently, animated, grouped, and used as an endpoint for reactive helpers.

== Styling and layout

```python
from gaanim import Anchor, BLUE, Direction, GOLD

circle = (
    scene.circle(80)
    .fill(BLUE)
    .stroke(GOLD, 4.0)
    .opacity(0.8)
    .z_index(10)
    .at(100, 200)
    .scaled(1.2)
    .rotated(0.2)
    .to_edge(Direction.UP, 20)
)
```

For relative layout, use `next_to`, `align_to`, `to_edge`, and `to_corner` with
`Direction` and `Anchor` values.

== Primitive factories

```python
circle = scene.circle(80)
rect = scene.rect(200, 120)
rounded = scene.rounded_rect(200, 120, 16)
square = scene.square(100)
dot = scene.dot(12)
ellipse = scene.ellipse(100, 60)
line = scene.line(-200, 0, 200, 0)
arrow = scene.arrow(-100, 0, 100, 0)
```

== Text and math

```python
title = scene.title("A title")
subtitle = scene.subtitle("A subtitle")
text = scene.text("Custom text")
equation = scene.equation("E = m c^2")
```

Equations are compiled through Typst. Use Typst math syntax in the string.

=== Color equation fragments

`color_by` works on `text`, `title`, `subtitle`, `paragraph`, and `equation`.
It changes all matching glyph fragments while preserving vector paths, including
during `write` and transform animations. Matching is case-insensitive and
ignores math spacing and sub/superscript markers. If calls overlap, the last
color wins.

```python
from gaanim import BLUE, GOLD

energy = (
    scene.equation("E = m c^2")
    .color_by("m", GOLD)
    .color_by("c", BLUE)
)
label = scene.text("Energy depends on mass").color_by("mass", GOLD)
```

For a step-by-step explanation, `select` acts on every occurrence by default.
Pass `occurrence=0` (zero-based) to isolate one repeated fragment. Its actions
are queued in the normal scene timeline.

```python
formula = scene.equation("x + x = 2x")
formula.select("x", occurrence=1).fill(BLUE).indicate(duration=0.8)
formula.select("2x").color_to(GOLD, duration=0.6)
```

`transform_to` morphs one selected fragment into another, pairing their glyphs
in order. It works best when both selections contain the same number of glyphs.

```python
source = scene.equation("E = m c^2")
target = scene.equation("p = m v").at(0, -120)
source.select("m").transform_to(target.select("m"), duration=0.8)
```

== Groups

```python
group = scene.group([circle, rect, title])
scene.play([group.move(0, 100).duration(1.0)])
```

== Reactive objects

```python
from gaanim import CYAN, ORANGE, Updater

dot = scene.dot(12).fill(ORANGE).at(180, 0)
dot.add_updater(Updater.orbit(0, 0, 180, 1.5))
trail = scene.traced_path(dot).stroke(CYAN, 3).no_fill()
line = scene.tracking_line((0, 0), dot)

scene.wait(3.0)
dot.remove_updater()
```
