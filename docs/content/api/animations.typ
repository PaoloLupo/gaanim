#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Animations",
  description: "Every Anim on Drawable — move, fade, write, transform, and timing",
  route: "/api/animations/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Animations

Every method on `Drawable` returns an `Anim`. Pass them to `Scene.play([...])` — calls are sequential, lists run in parallel.

#html.div(style: "font-family: var(--font-code); font-size: 0.65rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; background: var(--text-main); color: var(--bg-main); padding: 4px 8px; display: inline-block; margin-bottom: 16px;", [— 22 ANIMS · ALL ON Drawable —])

== Motion

#api-entry(
  name: "Drawable.move",
  kind: "method",
  signature: ".move(dx: float, dy: float) -> Anim",
  params: ((name: "dx", type: "float", default: none, desc: [Delta x.]), (name: "dy", type: "float", default: none, desc: [Delta y.]),),
  returns: (type: "Anim", desc: [Relative move.]),
  desc: [Translates by delta. For absolute, use `move_to`.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
circle = scene.circle(40).fill(BLUE).at(-80, 0)
scene.play([circle.move(160, 0).duration(1.0).spring()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.move_to",
  kind: "method",
  signature: ".move_to(x: float, y: float) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Target x.]), (name: "y", type: "float", default: none, desc: [Target y.]),),
  returns: (type: "Anim", desc: [Move to absolute position.]),
  desc: [Centers drawable at (x,y).],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
rect = scene.rect(100, 60).fill(BLUE).stroke(WHITE, 2).at(-120, 0)
scene.play([rect.move_to(80, 0).duration(0.9).smooth()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.glide_to",
  kind: "method",
  signature: ".glide_to(x: float, y: float) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Target x.]), (name: "y", type: "float", default: none, desc: [Target y.]),),
  returns: (type: "Anim", desc: [Glide to position.]),
  desc: [Smoother arrival than `move_to`. Good for camera-like drifts.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
label = scene.text("Glide").at(-100, 0)
scene.play([label.glide_to(80, 0).duration(1.1)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.scale",
  kind: "method",
  signature: ".scale(factor: float) -> Anim",
  params: ((name: "factor", type: "float", default: none, desc: [factor greater than 1 enlarges, less than 1 shrinks.]),),
  returns: (type: "Anim", desc: [Scale anim.]),
  desc: [Uniform scale around current pivot. Set pivot with `with_pivot(x,y)`.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
icon = scene.circle(36).fill(BLUE).stroke(WHITE, 2).at(0, 0)
scene.play([icon.scale(1.8).duration(0.7).spring()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.rotate",
  kind: "method",
  signature: ".rotate(radians: float) -> Anim",
  params: ((name: "radians", type: "float", default: none, desc: [Angle in radians.]),),
  returns: (type: "Anim", desc: [Rotation anim.]),
  desc: [Clockwise positive in screen coords. Use `with_pivot` for hinge.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
from math import pi
scene = Scene(480, 270, background="#0f172a")
arm = scene.rect(80, 14).fill(BLUE).at(40, 0).with_pivot(0, 0)
scene.play([arm.rotate(pi/2).duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

== Fades

#api-entry(
  name: "Drawable.fade_in / fade_out / fade_to",
  kind: "method",
  signature: ".fade_in(duration?) .fade_out(duration?) .fade_to(alpha: 0..1)",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds, uses default if None.]),),
  returns: (type: "Anim", desc: [Opacity anim.]),
  desc: [`fade_to` animates to target alpha. `fade_in_from` below is directional.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
title = scene.text("Fade").fill(WHITE).at(0, 40)
box = scene.rect(120, 50).fill(BLUE).at(0, -40)
scene.play([title.fade_in().duration(0.5)])
scene.play([box.fade_to(0.35).duration(0.6)])
scene.play([title.fade_out().duration(0.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.fade_in_from",
  kind: "method",
  signature: ".fade_in_from(direction: Direction, distance=48, duration?) -> Anim",
  params: ((name: "direction", type: "Direction", default: none, desc: [UP/DOWN/LEFT/RIGHT]), (name: "distance", type: "float", default: "48.0", desc: [Offset before entrance.]),),
  returns: (type: "Anim", desc: [Entrance from offset.]),
  desc: [Manim-style entrance: starts invisible at offset, fades + moves in.],
)[
```python
# show-code: true
from gaanim import Direction, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
caption = scene.text("Enters from below").fill(WHITE).at(0, 0)
scene.play([caption.fade_in_from(Direction.DOWN, distance=48).duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

== Write & Create

#api-entry(
  name: "Drawable.write / unwrite",
  kind: "method",
  signature: ".write(duration?) .unwrite(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Glyph-by-glyph write.]),
  desc: [For text/equation. Respects vector paths, not just opacity. `unwrite` reverses.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
formula = scene.equation("E = m c^2").fill(GOLD).at(0, 0)
scene.play([formula.write().duration(1.0)])
scene.play([formula.unwrite().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.write_by_term",
  kind: "method",
  signature: ".write_by_term(*, tags?, duration=1.0) -> Drawable",
  params: ((name: "tags", type: "list[str]", default: "None (all)", desc: [Subset of declared tags to sequence.]), (name: "duration", type: "float", default: "1.0", desc: [Total duration.] ),),
  returns: (type: "Drawable", desc: [Self — queues term-by-term write.]),
  desc: [Writes each `tags` declaration as one semantic unit. Needs `equation(..., tags={...})`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(480, 270, background="#0f172a")
eq = scene.equation("E = m c^2", tags={"e":"E","m":"m","c2":"c^2"}).at(0, 0)
eq.write_by_term(duration=1.4)
scene.play([eq.create().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.create / uncreate",
  kind: "method",
  signature: ".create(duration?) .uncreate(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Stroke-drawing.]),
  desc: [Draws outline progressively. `uncreate` erases. Different from `write` (which follows glyphs).],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
circle = scene.circle(50).no_fill().stroke(BLUE, 4).at(0, 0)
scene.play([circle.create().duration(1.0).smooth()])
scene.play([circle.uncreate().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

== Emphasis

#api-entry(
  name: "Drawable.grow_from_center / shrink_to_center",
  kind: "method",
  signature: ".grow_from_center(duration?) .shrink_to_center(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Scale from/to center.]),
  desc: [Pop in/out. Great for charts, badges.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
chart = scene.bar_chart([18, 42, 31], labels=["Q1","Q2","Q3"], color=BLUE).at(0, -10)
scene.play([chart.grow_from_center().duration(0.7).spring()])
scene.play([chart.shrink_to_center().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.spin_in_from_nothing",
  kind: "method",
  signature: ".spin_in_from_nothing(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Spin + scale from nothing.]),
  desc: [Playful entrance for stars, icons.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
star = scene.star(5, 55, 26).fill(GOLD).at(0, 0)
scene.play([star.spin_in_from_nothing().duration(0.9).spring()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.draw_border_then_fill",
  kind: "method",
  signature: ".draw_border_then_fill(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds, border then fill.] ),),
  returns: (type: "Anim", desc: [Two-phase: stroke then fill.]),
  desc: [Elegant for filled shapes — draws edge first, then floods.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
rect = scene.rect(140, 80).fill(BLUE).stroke(WHITE, 3).at(0, 0)
scene.play([rect.draw_border_then_fill().duration(1.3)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.indicate / wiggle",
  kind: "method",
  signature: ".indicate(duration?) .wiggle(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Attention anims.]),
  desc: [`indicate` pulses scale+glow, `wiggle` shakes. Use for wrong answer / highlight.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
key = scene.circle(32).fill(BLUE).stroke(WHITE, 2).at(-50, 0)
wrong = scene.cross(28).stroke(WHITE, 3).at(60, 0)
scene.play([key.indicate().duration(0.7)])
scene.play([wrong.wiggle().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

== Transforms

#api-entry(
  name: "Drawable.transform / fade_transform / replacement_transform",
  kind: "method",
  signature: ".transform(target) .fade_transform(target) .replacement_transform(target)",
  params: ((name: "target", type: "Drawable", default: none, desc: [Target shape to morph into.]),),
  returns: (type: "Anim", desc: [Morph anim.]),
  desc: [`transform` morphs in place, `fade_transform` cross-fades, `replacement_transform` replaces source with target. All pair geometry.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
circle = scene.circle(42).fill(BLUE).at(-90, 0)
target = scene.rect(90, 60).fill(GOLD).at(80, 0)
scene.play([circle.create().duration(0.6)])
scene.play([circle.transform(target).duration(1.0).spring()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.transform_matching_shapes / transform_matching_tex",
  kind: "method",
  signature: "scene.transform_matching_shapes(source, target, duration=1.0) / scene.transform_matching_tex(source, target, duration=1.0)",
  params: (
    (name: "source", type: "Drawable", default: none, desc: [Source object/group containing elements to match.]),
    (name: "target", type: "Drawable", default: none, desc: [Target object/group containing elements to match.]),
    (name: "duration", type: "float", default: "1.0", desc: [Duration of the transition in seconds.]),
  ),
  returns: (type: "none", desc: [Schedules the transform matching animation.]),
  desc: [`transform_matching_shapes` auto-matches sub-elements by geometry, position and color using Hungarian assignment + shape hashing. `transform_matching_tex` uses order-preserving LCS character matching for text/equations. Unmatched elements automatically fade in/out.],
)[
```python
from gaanim import BLACK, BLUE, GOLD, GREEN, Scene
scene = Scene(1920, 1080, background=BLACK)
e1 = scene.equation("E = m c").fill(WHITE).at(0, 80).scaled(1.3)
e2 = scene.equation("p = m v").fill(GOLD).at(0, 80).scaled(1.3)
scene.transform_matching_tex(e1, e2, duration=1.6)
```
]

== Timing & Easing

Configure any `Anim` fluently before passing to `play`:

```python
scene.play([
    circle.move(240, 0).duration(1.0).linear(),
    label.fade_to(0.5).duration(1.0).smooth(),
    icon.rotate(1.5).duration(0.8).spring(),
])

# chainable
anim = circle.create().duration(1.2).delay(0.3).ease("cubic").lag_ratio(0.2)

# stagger a list
scene.play([a.fade_in(), b.fade_in(), c.fade_in()], lag=0.1)
```

#api-entry(
  name: "Anim timing",
  kind: "method",
  signature: ".duration(s) .delay(s) .steps(n) .lag_ratio(0..1)",
  params: ((name: "value", type: "float|int", default: none, desc: [Timing value.]),),
  returns: (type: "Anim", desc: [Self.]),
  desc: [`duration` total time, `delay` wait before start, `steps` discrete steps, `lag_ratio` staggers sub-paths inside one drawable (for groups/text).],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
g = scene.group([scene.circle(18).fill(BLUE).at(-50,0), scene.circle(18).fill(BLUE).at(0,0), scene.circle(18).fill(BLUE).at(50,0)])
scene.play([g.create().duration(1.0).lag_ratio(0.25)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Anim easing",
  kind: "method",
  signature: ".linear() .smooth() .spring() .ease(name) .rate(name)",
  params: ((name: "name", type: "str", default: "—", desc: ["Easing name for ease/rate if needed."]),),
  returns: (type: "Anim", desc: [Self.]),
  desc: [Built-ins cover most cases. `smooth` is cubic in/out, `spring` overshoots.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
dot = scene.dot(10).fill(BLUE).at(-110, 0)
scene.play([dot.move(220, 0).duration(0.9).spring()])
scene.play([dot.move(-220, 0).duration(0.9).smooth()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Anim.stroke_width / with_pen_tip",
  kind: "method",
  signature: ".stroke_width(w: float) .with_pen_tip() -> Anim",
  params: ((name: "w", type: "float", default: none, desc: [Target stroke width.]),),
  returns: (type: "Anim", desc: [Self with tip effect.]),
  desc: [Rare, for handwriting emphasis with pen tip.],
)[
```python
# show-code: true
from gaanim import WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
path = scene.path([(-120, 0), (0, 40), (120, 0)]).no_fill().stroke(WHITE, 3)
scene.play([path.write().with_pen_tip().duration(1.4)])
scene.export("preview.webp", fps=30)
```
]
