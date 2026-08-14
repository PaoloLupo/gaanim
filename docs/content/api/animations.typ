#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Animaciones",
  description: "Animaciones de Drawable: movimiento, fundido, escritura, transformación y tiempo",
  route: "/api/animations/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Animaciones

Every method on `Drawable` returns an `Anim`. Pass them to `Scene.play([...])` — calls are sequential, lists run in parallel.

== Animaciones de propiedades compuestas

`Drawable.animate() -> Anim` starts a typed property animation. Chain several
targets on the returned `Anim`; they share duration, easing, and delay and are
sampled concurrently from the timeline:

```python
scene.play([
  circle.animate()
    .move_to(160, 40)
    .scale(1.4)
    .fill(BLUE)
    .stroke(WHITE, 5)
    .opacity(0.8)
    .duration(1.5)
    .smooth()
])
```

The proxy supports 2D and 3D movement, absolute 3D scale, relative or absolute
rotation, opacity, solid fill, and vector stroke. `color(c)` recolors only
vector paints that are already visible. Calling `fill(c)` from `no_fill()` or
`stroke(c, width)` from `no_stroke()` reveals the paint smoothly from
transparent; a new stroke also grows from zero width.

For `Text` and Typst drawables, fill and stroke channels propagate to every
visible glyph. Each glyph interpolates from its own current paint, including
fragment-specific colors. A whole-text `fill(c)` or `color(c)` therefore
converges those fragment colors to the requested target, while `stroke(c,
width)` changes the outline without replacing their distinct fills.

For a native `Primitive3D`, `fill(c)` and `color(c)` target the PBR base color
while preserving roughness, metallic, and emission. Use
`primitive.animate().material(Material3D(...))` to interpolate the complete PBR
material together with transforms and opacity. Vector stroke methods on a 3D
primitive raise `TypeError`.

Only the first property activates the deferred animation queue, so an unused
`animate()` proxy does not advance scene time. Position targets retain the
usual layout-ownership restriction.

== Acciones glTF

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

== Transformaciones 3D

```python
part.move_3d(dx, dy, dz) -> Anim
part.move_to_3d(x, y, z) -> Anim
part.rotate_by_3d(axis, radians) -> Anim
part.rotate_to_3d(x, y, z) -> Anim
part.scale_to_3d(x, y, z) -> Anim
```

Euler triples use XYZ order and radians. `rotate_by_3d` accepts only `"x"`,
`"y"`, or `"z"`; other axes raise `ValueError`.

Native `Primitive3D` meshes also provide
`primitive.material_to(material: Material3D) -> Anim`. Color, emissive color,
roughness, metallic, and emission strength interpolate deterministically;
exact endpoints are restored when seeking in either direction. On a mesh,
`create()` means grow from center plus fade. `write()` remains vector-only and
raises `TypeError` with guidance to use `create()`.

#html.div(style: "font-family: var(--font-code); font-size: 0.65rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; background: var(--text-main); color: var(--bg-main); padding: 4px 8px; display: inline-block; margin-bottom: 16px;", [— 22 ANIMS · ALL ON Drawable —])

== Movimiento

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
  signature: ".move_to(x: float, y: float, anchor: Anchor = Anchor.CENTER) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Target x.]), (name: "y", type: "float", default: none, desc: [Target y.]), (name: "anchor", type: "Anchor", default: "Anchor.CENTER", desc: [Drawable anchor that arrives at the target point.])),
  returns: (type: "Anim", desc: [Move to absolute position.]),
  desc: [Moves the selected anchor to `(x, y)`. The default remains the drawable center.],
)[
```python
# show-code: true
from gaanim import Anchor, BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
rect = scene.rect(100, 60).fill(BLUE).stroke(WHITE, 2).at(-120, 0)
scene.play([rect.move_to(80, 0, anchor=Anchor.TOP_RIGHT).duration(0.9).smooth()])
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

== Fundidos

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

== Escritura y creación

#api-entry(
  name: "Drawable.write / unwrite",
  kind: "method",
  signature: ".write(duration?) .unwrite(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Glyph-by-glyph write.]),
  desc: [For text/equation. Respects vector paths, not just opacity. Generated and reactive descendants remain hidden before the scheduled animation and retain the current reveal progress while updating. `unwrite` reverses.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
formula = scene.text("$E = m c^2$").fill(GOLD).at(0, 0)
scene.play([formula.write().duration(1.0)])
scene.play([formula.unwrite().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Text.write grouping",
  kind: "method",
  signature: ".write(duration=None, *, by=\"grapheme\", order=\"forward\", stagger=0.0) -> Anim",
  params: ((name: "duration", type: "float | None", default: "None", desc: [Optional total duration, accepted as the first positional argument.]), (name: "by", type: "str", default: "\"grapheme\"", desc: [Grouping: grapheme, word, line, or semantic part.]),),
  returns: (type: "Anim", desc: [Animation descriptor accepted by #raw("scene.play()") .]),
  desc: [Writes graphemes, words, rendered lines, or semantic parts in deterministic order. The duration is positional, so #raw("text.write(0.8, by=\"word\")") is valid.],
)[
```python
# show-code: true
from gaanim import Scene, part
scene = Scene(480, 270, background="#0f172a")
# Grouping is resolved by the specialized Text API.
eq = scene.text("$", part("energy", "E"), " = ", part("mass", "m"), " ", part("speed", "c^2"), "$").at(0, 0)
scene.play([eq.write(1.4, by="part", stagger=0.08)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.create / uncreate",
  kind: "method",
  signature: ".create(duration?) .uncreate(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Stroke-drawing.]),
  desc: [Draws outline progressively. Generated and reactive descendants remain hidden before the scheduled animation and retain the current reveal progress while updating. `uncreate` erases. Different from `write` (which follows glyphs).],
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

== Énfasis

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
# Visualization uses immutable ChartSpec batches.
from gaanim import Axis, BLUE, ChartSpec, Scene
scene = Scene(480, 270, background="#0f172a")
spec = ChartSpec({"x": [0, 1, 2], "value": [18, 42, 31]}) \
  .mark("bar").encode(x="x", y="value") \
  .axes(x=Axis.category(["Q1", "Q2", "Q3"]), y=Axis.linear(0, 50))
chart = scene.chart(spec)
scene.play([chart.layer("marks").grow_from_center().duration(0.7).spring()])
scene.play([chart.layer("marks").shrink_to_center().duration(0.5)])
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

== Transiciones de texto estructurado

The complete text-specific surface, including selections, annotations,
ownership, and error behavior, is documented in
#link("/api/text/", "Text — unified authoring and animation").

#api-entry(
  name: "Text.step_to",
  kind: "method",
  signature: "source.step_to(target, *, matches=None, duration=1.0) -> Anim",
  params: (
    (name: "source", type: "Text", default: none, desc: [Current structured text or formula.]),
    (name: "target", type: "Text", default: none, desc: [Next structured text version.]),
    (name: "matches", type: "sequence[tuple[str,str]] | mapping[str,str]", default: "None", desc: [Explicit source → target semantic paths. `None` uses shared paths.]),
    (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.]),
  ),
  returns: (type: "Anim", desc: [A structured transition accepted by `scene.play()`; Layout v2 reflow shares its duration.]),
  desc: [Semantic paths are paired first; remaining identical graphemes are matched automatically. Paired content moves and morphs without fading. Removed glyphs shrink into their own visual centers and new glyphs grow outward. Cross-scene or incompatible Layout owners raise `LayoutOwnershipError`.],
)[
```python
# show-code: true
from gaanim import BLACK, GOLD, Scene, part
scene = Scene(480, 270, background=BLACK)
# Fluent scaling retains semantic selection support.
before = scene.text("$", part("x", "x"), " + 3 = ", part("result", "7"), "$").scaled(1.6)
after = scene.text("$", part("x", "x"), " = ", part("result", "4"), "$").scaled(1.6)
before["result"].fill(GOLD)
after["result"].fill(GOLD)
scene.play([before.write().duration(0.7)])
scene.play([before.step_to(after, duration=0.9)])
scene.play([after["result"].indicate(duration=0.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "TextSelection.cancel",
  kind: "method",
  signature: "selection.cancel(duration=None) -> Anim",
  params: ((name: "duration", type: "float | None", default: "None", desc: [Positive finite seconds; the animation default is used when omitted.]),),
  returns: (type: "Anim", desc: [Deferred cancellation animation accepted by `scene.play()`.]),
  desc: [Draws a diagonal mark and dims the selected glyphs. The mark remains associated with its owning `Text`; the next replacing `morph_to`, `step_to`, or `expand_to` fades both the mark and canceled glyphs as the source leaves.],
)[
```python
from gaanim import Scene, part
scene = Scene(480, 270)
before = scene.text("$x + ", part("obsolete", "3"), " = 7$")
after = scene.text("$x = 4$")
scene.play([before["obsolete"].cancel(duration=0.6)])
scene.play([before.step_to(after, duration=0.8)])
```
]

#api-entry(
  name: "Text.expand_to / TextSelection.morph_to",
  kind: "method",
  signature: "source.expand_to(target, *, anchor=\"part\", duration=1.0) / selection.morph_to(target_selection) -> Anim",
  params: ((name: "target", type: "Text", default: none, desc: [Destination structured text.]), (name: "anchor", type: "str", default: "\"part\"", desc: [Shared semantic path; `part` selects the first shared path automatically.]), (name: "target_selection", type: "TextSelection", default: none, desc: [Destination for a local selection morph.]), (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.])),
  returns: (type: "Anim", desc: [Both transitions return animations accepted by `scene.play()`.]),
  desc: [`expand_to` uses a shared semantic part as the one-to-many origin. `TextSelection.morph_to` explicitly pairs two local selections while the surrounding text remains one Layout leaf.],
)[
```python
from gaanim import Scene, part
scene = Scene(480, 270)
compact = scene.text("$E = ", part("mass", "m"), " c^2$")
expanded = scene.text("$E = ", part("mass", "(m_1 + m_2)"), " c^2$")
scene.play([compact.expand_to(expanded, anchor="mass", duration=0.9)])
```
]

#api-entry(
  name: "TextSelection.copy_to",
  kind: "method",
  signature: "source_selection.copy_to(target_selection, *, duration=None) -> Anim",
  params: ((name: "target_selection", type: "TextSelection", default: none, desc: [Destination semantic or query selection.]), (name: "duration", type: "float | None", default: "None", desc: [Positive finite seconds; the animation default is used when omitted.])),
  returns: (type: "Anim", desc: [A deferred animation accepted by `scene.play()` and composable with other animations.]),
  desc: [Keeps the source selection visible and moves a semantic copy into the destination selection.],
)[
```python
scene.play([energy["mass"].copy_to(momentum["mass"], duration=0.8)])
```
]

== Transformaciones

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
  name: "Scene.transform_matching_shapes / Text.morph_to",
  kind: "method",
  signature: "scene.transform_matching_shapes(source, target, duration=1.0) / source_text.morph_to(target_text, match=\"auto\", duration=1.0)",
  params: (
    (name: "source", type: "Drawable", default: none, desc: [Source object/group containing elements to match.]),
    (name: "target", type: "Drawable", default: none, desc: [Target object/group containing elements to match.]),
    (name: "duration", type: "float", default: "1.0", desc: [Duration of the transition in seconds.]),
  ),
  returns: (type: "None | Anim", desc: [`Scene.transform_matching_shapes` queues directly; `Text.morph_to` returns an animation accepted by `scene.play()`.]),
  desc: [`transform_matching_shapes` matches arbitrary sub-elements by geometry, position, and color. `Text.morph_to` first matches semantic part paths, then ordered equal graphemes and shape similarity, with deterministic entry/exit for unmatched glyphs.],
)[
```python
from gaanim import BLACK, GOLD, WHITE, Scene
scene = Scene(1920, 1080, background=BLACK)
e1 = scene.text("$E = m c$").fill(WHITE).at(0, 80).scaled(1.3)
e2 = scene.text("$p = m v$").fill(GOLD).at(0, 80).scaled(1.3)
scene.play([e1.morph_to(e2, duration=1.6)])
```
]

== Simulación reactiva

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

Reactive `tracking_line` drawables support both `create()` and `write()`.
Their endpoints may continue moving during the reveal because regeneration
updates the full path source and reapplies the current draw progress:

```python
rod = scene.tracking_line(anchor, mass).no_fill().stroke(WHITE, 4)
scene.play([rod.create(0.8), mass.move(120, 0).duration(0.8)])
scene.play([rod.write(0.8), mass.move(-80, 40).duration(0.8)])
```

== Tiempo y easing

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
