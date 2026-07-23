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

=== Fade in from a direction

Use `fade_in_from` for the common Manim-style entrance: it starts invisible at
an offset and fades in while moving into its final position.

```python
from gaanim import Direction

caption = scene.text("Una idea aparece desde abajo").at(0, -180)
scene.play([caption.fade_in_from(Direction.DOWN, distance=64, duration=0.8)])
```

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

=== Named equation tags

For formulas used repeatedly in a derivation, attach short semantic names at
construction time. This keeps the Typst source clean and avoids repeating raw
substring queries in the animation code.

```python
formula = scene.equation(
    "E = m c^2",
    tags={"mass": "m", "light_speed": "c^2"},
)

formula.tag("mass").fill(GOLD).indicate(duration=0.8)
formula.tag("light_speed").color_to(BLUE, duration=0.6)
```

`write_by_term` writes each declared tag as one semantic unit, while
`reveal_fragment` supports a fade, a vector-path wipe, or an entrance from
below. `indicate_tag` is the short form for pulsing a named term.

```python
formula = scene.equation(
    "E = m c^2",
    tags={"energy": "E", "equals": "=", "mass": "m", "speed": "c^2"},
)
formula.write_by_term(duration=1.6)
formula.indicate_tag("mass", duration=0.5)
formula.reveal_fragment("c^2", style="from_below", duration=0.5)
```

`focus_equation` keeps the selected terms bright and attenuates the remainder.

```python
scene.focus_equation(formula, ["mass", "speed"], duration=0.6, dim_opacity=0.2)
```

Use `replace_term` when a tagged part changes between equation states; common
glyphs stay in place. `cancel_term` marks a named term with a diagonal strike;
the term and strike fade together when the following equation transition starts.

```python
before = scene.equation("x + 3 = 7", tags={"constant": "3", "variable": "x"})
after = scene.equation("x = 4", tags={"variable": "x"})

before.cancel_term("constant", duration=0.5)
scene.replace_term(before, after, tag="variable", duration=0.7)
```

`brace_label` places a labelled curly brace relative to a semantic term. By
default it goes below; pass `above=True` to put it above. `annotate_tag` adds a
label and a leader line whose term endpoint follows the tagged glyph as it moves.

```python
formula = scene.equation(
    "E = m c^2",
    tags={"mass": "m", "light_speed": "c^2"},
)

scene.brace_label(formula, "mass", "masa", duration=0.6)
scene.annotate_tag(
    formula,
    "light_speed",
    "velocidad de la luz",
    offset=(160, 90),
    duration=0.6,
)
```

Use `transform_equation` to animate every shared tag at once while preserving
the source equation. Provide `tags` to restrict the transition; without it,
all shared names move in parallel.

```python
source = scene.equation("E = m c^2", tags={"mass": "m"})
target = scene.equation("p = m v", tags={"mass": "m"}).at(0, -120)
scene.transform_equation(source, target, duration=0.8)
```

For an equation state where a tagged term becomes longer, use
`expand_equation`. The shared tag marks the glyph that moves; the rest of the
new expression fades in while matching glyphs glide into their new positions.

```python
compact = scene.equation("E = m c^2", tags={"mass": "m"}).at(0, 0)
expanded = scene.equation(
    "E = (m_1 + m_2) c^2",
    tags={"mass": "m"},
).at(0, 0)

scene.play(compact.write())
scene.expand_equation(compact, expanded, tag="mass", duration=0.8)
```

For a general derivation step, `step_equation` matches common glyphs
automatically and fades only the terms that change.

```python
before = scene.equation("x + 3 = 7").at(0, 0)
after = scene.equation("x = 4").at(0, 0)
scene.play(before.write())
scene.step_equation(before, after, duration=0.8)
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
