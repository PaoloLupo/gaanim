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

== glTF Actions

```python
model.animation(
  "Walk",
  duration=None,
  speed=1.0,
  loop=False,
  reverse=False,
  transition=0.0,
  start_time=0.0,
) -> Anim
```

Without an explicit `duration`, the Action uses its authored duration divided
by `speed`. `loop=True` repeats within the timeline clip; otherwise the final
pose is retained. `reverse` samples backwards, `start_time` resumes at an
authored offset, and `transition` cross-fades from the preceding Action. Only
the current and outgoing Actions are weighted during that transition.

Animation players remain paused and are sampled from the timeline's absolute
time. Forward seeks, backwards scrubbing, export frames, and snapshots therefore
resolve the same Action pose.

== 3D transforms

```python
part.move_3d(dx, dy, dz) -> Anim
part.move_to_3d(x, y, z) -> Anim
part.rotate_by_3d(axis, radians) -> Anim
part.rotate_to_3d(x, y, z) -> Anim
part.scale_to_3d(x, y, z) -> Anim
```

Euler triples use XYZ order and radians. `rotate_by_3d` accepts only `"x"`,
`"y"`, or `"z"`; other axes raise `ValueError`.

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
  desc: [Clockwise positive in screen coords. Use `with_pivot` for hinge or chain `.pivot(x,y)` / `.about_point(x,y)` on the `Anim` for orbital motion (e.g. `dot.pivot(200,0).rotate(TAU)`).],
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

#api-entry(
  name: "Drawable.move_along_path",
  kind: "method",
  signature: ".move_along_path(target: Drawable) -> Anim",
  params: ((name: "target", type: "Drawable", default: none, desc: [Path drawable to follow — circle, rect, curve, polyline, etc. Its world geometry (after `at`, groups) is sampled.]),),
  returns: (type: "Anim", desc: [Follow-path translation.]),
  desc: [Samples the target's Bézier outline by true arc-length and sets the caller's translation to the point at eased `t` (`get_point_at_alpha`). Equivalent to Manim's `MoveAlongPath`. Combine with `.linear()` for uniform speed, or `.smooth()` for ease. Rotation/scale unaffected.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, WHITE, Scene
scene = Scene(480, 270, background=BLACK)
circle = scene.circle(60).stroke(BLUE, 3).no_fill().at(0, 0)
dot = scene.dot(8).fill(WHITE).at(60, 0)
scene.play([dot.move_along_path(circle).duration(2.0).linear()])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Anim.pivot / about_point",
  kind: "method",
  signature: ".pivot(x: float, y: float) -> Anim / .about_point(x: float, y: float) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Pivot x in scene pixels.]), (name: "y", type: "float", default: none, desc: [Pivot y in scene pixels.]),),
  returns: (type: "Anim", desc: [Same Anim with orbital pivot.]),
  desc: [Only valid on `RotateBy` anims (`Drawable.rotate`). Replaces hinge with scene-space point; the engine builds an orbital `Arc` for translation plus a slerped `Rotation` (splits `>π`). Alias `about_point` mirrors Manim.],
)[
```python
import math
from gaanim import BLACK, WHITE, Scene
scene = Scene(480, 270, background=BLACK)
dot = scene.dot(10).fill(WHITE).at(60, 0)
scene.play([dot.pivot(0, 0).rotate(math.tau).duration(1.5).linear()])
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
from gaanim import Axis, BLUE, DataSource, Scene
scene = Scene(480, 270, background="#0f172a")
data = DataSource({"x": [0, 1, 2], "value": [18, 42, 31]})
space = scene.axes(Axis.category(["Q1", "Q2", "Q3"]), Axis.linear(0, 50))
chart = space.bars(data, "x", "value").fill(BLUE)
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
  desc: [`indicate` makes a subtle upward hop from the visual center and highlights the target; `wiggle` shakes. Use for wrong answer / highlight.],
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

== Semantic equation transitions

#api-entry(
  name: "Scene.step_equation",
  kind: "method",
  signature: "step_equation(source, target, *, matches=None, duration=1.0) -> Drawable",
  params: (
    (name: "source", type: "Drawable", default: none, desc: [Current equation.]),
    (name: "target", type: "Drawable", default: none, desc: [Next equation.]),
    (name: "matches", type: "list[str] | dict[str,str]", default: "None", desc: [Same-name tags or source → target tag names. `None` uses every shared tag.]),
    (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.]),
  ),
  returns: (type: "Drawable", desc: [The target equation, ready for another step.]),
  desc: [Semantic tags are paired first; remaining identical glyphs are matched automatically. Paired terms move and morph without fading. Removed terms shrink into their own visual centers and new terms grow outward from their centers. Unknown explicit tags and invalid durations raise `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLACK, GOLD, Scene
scene = Scene(480, 270, background=BLACK)
before = scene.equation("x + 3 = 7", tags={"x":"x", "result":"7"}).scaled(1.6)
after = scene.equation("x = 4", tags={"x":"x", "result":"4"}).scaled(1.6)
before.tag("result").fill(GOLD)
after.tag("result").fill(GOLD)
scene.play([before.write().duration(0.7)])
current = scene.step_equation(before, after, duration=0.9)
current.tag("result").indicate(duration=0.4)
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.expand_equation / replace_term",
  kind: "method",
  signature: "expand_equation(source, target, *, tag, duration=1.0) -> Drawable / replace_term(source, target, *, tag, target_tag=None, duration=1.0) -> Drawable",
  params: ((name: "tag", type: "str", default: none, desc: [Source semantic term. `expand_equation` requires the same name on the target.]), (name: "target_tag", type: "str | None", default: "None", desc: [Different destination name for `replace_term`.]), (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.]),),
  returns: (type: "Drawable", desc: [The target equation.]),
  desc: [`expand_equation` uses the tagged term as the one-to-many origin. `replace_term` forces the selected source and destination terms to correspond while unchanged glyphs continue moving automatically.],
)[
```python
from gaanim import Scene
scene = Scene(480, 270)
compact = scene.equation("E = m c^2", tags={"mass":"m"})
expanded = scene.equation("E = (m_1 + m_2) c^2", tags={"mass":"(m_1 + m_2)"})
scene.expand_equation(compact, expanded, tag="mass", duration=0.9)
```
]

#api-entry(
  name: "Scene.copy_equation_terms / transform_equation",
  kind: "method",
  signature: "copy_equation_terms(source, target, *, tags=None, duration=1.0) -> Drawable",
  params: ((name: "tags", type: "list[str] | None", default: "None", desc: [Shared semantic names to copy; `None` uses all shared names.]), (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.]),),
  returns: (type: "Drawable", desc: [The destination equation.]),
  desc: [Keeps the source visible and moves visual copies of the selected terms into the destination. `transform_equation` remains as a compatibility alias with identical behavior.],
)[
```python
scene.copy_equation_terms(energy, momentum, tags=["mass"], duration=0.8)
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
  name: "Scene.transform_matching_shapes / transform_matching_tex / transform_matching_text",
  kind: "method",
  signature: "scene.transform_matching_shapes(source, target, duration=1.0) / scene.transform_matching_tex(source, target, duration=1.0) / scene.transform_matching_text(source, target, duration=1.0)",
  params: (
    (name: "source", type: "Drawable", default: none, desc: [Source object/group containing elements to match.]),
    (name: "target", type: "Drawable", default: none, desc: [Target object/group containing elements to match.]),
    (name: "duration", type: "float", default: "1.0", desc: [Duration of the transition in seconds.]),
  ),
  returns: (type: "Drawable | none", desc: [`transform_matching_tex/text` return the destination; shape matching retains its existing `None` return.]),
  desc: [`transform_matching_shapes` auto-matches sub-elements by geometry, position and color using Hungarian assignment + shape hashing. `transform_matching_tex` (alias `transform_matching_text`) uses semantic equation tags first and order-preserving LCS for remaining characters, then performs the same clean handoff as `step_equation`. Generic `scene.transform_matching(source, target, mode="shapes"|"tex", duration=1.0)` dispatches by mode.],
)[
```python
from gaanim import BLACK, GOLD, WHITE, Scene
scene = Scene(1920, 1080, background=BLACK)
e1 = scene.equation("E = m c").fill(WHITE).at(0, 80).scaled(1.3)
e2 = scene.equation("p = m v").fill(GOLD).at(0, 80).scaled(1.3)
scene.transform_matching_tex(e1, e2, duration=1.6)
```
]

== Reactive simulation

#api-entry(
  name: "Drawable.add_updater_fn",
  kind: "method",
  signature: "add_updater_fn(callback, *, reset=None, fixed_dt=None) -> Drawable",
  params: (
    (name: "callback", type: "callable", default: none, desc: [`callback((x, y, z), dt, elapsed)` returns the new local position.]),
    (name: "reset", type: "callable | None", default: "None", desc: [Restores all Python state captured by a stateful simulation.]),
    (name: "fixed_dt", type: "float | None", default: "None", desc: [Positive simulation step in seconds.]),
  ),
  returns: (type: "Drawable", desc: [The same drawable for fluent chaining.]),
  desc: [Pass `reset` and `fixed_dt` together for physics or any incremental state. The updater starts at the timeline cursor where `add_updater_fn` is authored; it does not evolve during earlier segments. Gaanim restores the drawable's initial local position, calls `reset()`, and replays constant substeps after random seeks and during export. A callback without that pair is intended for lightweight frame or absolute-time behavior. Invalid coordinates or callback exceptions stop the updater.],
)[
```python
# show-code: true
from gaanim import BLACK, GOLD, Scene

scene = Scene(480, 270, background=BLACK)
ball = scene.dot(12).fill(GOLD).at(0, 90)
state = {"velocity": 0.0}

def reset():
    state["velocity"] = 0.0

def step(pos, dt, elapsed):
    x, y, z = pos
    state["velocity"] -= 240.0 * dt
    y += state["velocity"] * dt
    if y < -90:
        y = -90
        state["velocity"] *= -0.8
    return (x, y, z)

ball.add_updater_fn(step, reset=reset, fixed_dt=1 / 240)
scene.wait(3.0)
scene.export("simulation.webp", fps=30)
```
]

For a coupled example with a tracking rod, dimension and trail, see
`examples/pendulum_simulation.py`.

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
