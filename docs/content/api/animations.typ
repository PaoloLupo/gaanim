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

Los handles usan un solo vocabulario. Una llamada directa aplica un corte en el
cursor actual sin avanzar el tiempo; la misma llamada bajo la propiedad
`animate` describe un `Anim` puro que solo entra al timeline mediante
`Scene.play([...])`.

```python
dot.move_to(100, 80).fill(BLUE)
scene.play([dot.animate.move_to(400, 80).fill(RED)])
```

“Inmediato” no significa modificar globalmente un objeto ya compilado: registra
un corte reversible en el cursor. Los seeks anteriores conservan el estado
anterior. Construir, configurar o abandonar un `Anim` no cambia visibilidad,
estado autoral, operaciones ni cursor.

== Animaciones de propiedades compuestas

`Drawable.animate -> Anim` inicia una animación tipada de propiedades. Encadena
varios objetivos en el `Anim` devuelto: comparten duración, curva y retraso, y
se muestrean simultáneamente desde la línea temporal:

```python
scene.play([
  circle.animate
    .move_to(160, 40)
    .scale_by(1.4)
    .fill(BLUE)
    .stroke(WHITE, 5)
    .opacity(0.8)
    .duration(1.5)
    .smooth()
])
```

El proxy admite movimiento 2D y 3D, escala 3D absoluta, rotación relativa o
absoluta, opacidad, relleno sólido y trazo vectorial. `color(c)` recolorea solo
la pintura vectorial ya visible. Llamar `fill(c)` tras `no_fill()`, o
`stroke(c, width)` tras `no_stroke()`, revela suavemente la pintura desde la
transparencia; un trazo nuevo también crece desde ancho cero.

En objetos `Text` y Typst, los canales de relleno y trazo se propagan a cada
glifo visible. Cada glifo interpola desde su pintura actual, incluso si un
fragmento tiene color propio. Por eso `fill(c)` o `color(c)` sobre el texto
completo converge esos colores al objetivo, mientras `stroke(c, width)` cambia
el contorno sin reemplazar sus rellenos distintos.

En una `Primitive3D` nativa, `fill(c)` y `color(c)` cambian el color base PBR y
conservan rugosidad, metalicidad y emisión. Usa
`primitive.animate.material(Material3D(...))` para interpolar el material PBR
completo junto con transformaciones y opacidad. Los métodos de trazo vectorial
sobre una primitiva 3D producen `TypeError`.

El proxy es una propiedad de solo lectura, no una función. Cada `Anim` es de un
solo uso y los objetivos de posición conservan la restricción normal de
propiedad del layout.

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
part.animate.shift_by_3d(dx, dy, dz) -> Anim
part.animate.move_to_3d(x, y, z) -> Anim
part.animate.rotate_by_3d(axis, radians) -> Anim
part.animate.rotate_to_3d(x, y, z) -> Anim
part.animate.scale_to_3d(x, y, z) -> Anim
```

Euler triples use XYZ order and radians. `rotate_by_3d` accepts only `"x"`,
`"y"`, or `"z"`; other axes raise `ValueError`.

Native `Primitive3D` meshes also provide
`primitive.animate.material(material: Material3D) -> Anim`. Color, emissive color,
roughness, metallic, and emission strength interpolate deterministically;
exact endpoints are restored when seeking in either direction. On a mesh,
`create()` means grow from center plus fade. `write()` remains vector-only and
raises `TypeError` with guidance to use `create()`.

#html.div(style: "font-family: var(--font-code); font-size: 0.65rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; background: var(--text-main); color: var(--bg-main); padding: 4px 8px; display: inline-block; margin-bottom: 16px;", [— 22 ANIMS · ALL ON Drawable —])

== Movimiento

#api-entry(
  name: "Drawable.move",
  kind: "method",
  signature: ".animate.shift_by(dx: float, dy: float) -> Anim",
  params: ((name: "dx", type: "float", default: none, desc: [Delta x.]), (name: "dy", type: "float", default: none, desc: [Delta y.]),),
  returns: (type: "Anim", desc: [Relative move.]),
  desc: [Translates by delta. For absolute, use `move_to`.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
circle = scene.geometry.circle(40).fill(BLUE).move_to(-80, 0)
scene.play([circle.animate.shift_by(160, 0).duration(1.0).spring()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.move_to",
  kind: "method",
  signature: ".animate.move_to(x: float, y: float, anchor: Anchor = Anchor.CENTER) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Target x.]), (name: "y", type: "float", default: none, desc: [Target y.]), (name: "anchor", type: "Anchor", default: "Anchor.CENTER", desc: [Drawable anchor that arrives at the target point.])),
  returns: (type: "Anim", desc: [Move to absolute position.]),
  desc: [Moves the selected anchor to `(x, y)`. The default remains the drawable center.],
)[
```python
# show-code: true
from gaanim import Anchor, BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
rect = scene.geometry.rect(100, 60).fill(BLUE).stroke(WHITE, 2).move_to(-120, 0)
scene.play([rect.animate.move_to(80, 0, anchor=Anchor.TOP_RIGHT).duration(0.9).smooth()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SurroundingRect.retarget",
  kind: "animation",
  signature: ".retarget(targets, *, duration=None) -> Anim",
  params: ((name: "targets", type: "Drawable | TextSelection | Sequence", default: none, desc: [New live bounds, including semantic text or equation parts.]), (name: "duration", type: "float | None", default: "None", desc: [Positive finite seconds; `None` uses the animation default.])),
  returns: (type: "Anim", desc: [Edge-interpolation animation supporting normal easing.]),
  desc: [Interpolates left, right, top, and bottom while both source and destination may continue moving. At completion the frame remains bound to the destination. Timeline seeks and rewinds reproduce the same geometry.],
)[
```python
scene.play([frame.retarget(equation["result"], duration=0.9).spring()])
```
]

#api-entry(
  name: "Drawable.animate.move_to",
  kind: "method",
  signature: ".animate.move_to(x: float, y: float) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Target x.]), (name: "y", type: "float", default: none, desc: [Target y.]),),
  returns: (type: "Anim", desc: [Glide to position.]),
  desc: [Smoother arrival than `move_to`. Good for camera-like drifts.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
label = scene.text("Glide").move_to(-100, 0)
scene.play([label.animate.move_to(80, 0).duration(1.1)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.scale",
  kind: "method",
  signature: ".animate.scale_by(factor: float) -> Anim",
  params: ((name: "factor", type: "float", default: none, desc: [factor greater than 1 enlarges, less than 1 shrinks.]),),
  returns: (type: "Anim", desc: [Scale anim.]),
  desc: [Uniform scale around current pivot. Set pivot with `with_pivot(x,y)`.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
icon = scene.geometry.circle(36).fill(BLUE).stroke(WHITE, 2).move_to(0, 0)
scene.play([icon.animate.scale_by(1.8).duration(0.7).spring()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.rotate",
  kind: "method",
  signature: ".animate.rotate_by(radians: float) -> Anim",
  params: ((name: "radians", type: "float", default: none, desc: [Angle in radians.]),),
  returns: (type: "Anim", desc: [Rotation anim.]),
  desc: [Clockwise positive in screen coords. Use `with_pivot` for hinge or chain `.pivot(x,y)` / `.about_point(x,y)` on the `Anim` for orbital motion (e.g. `dot.pivot(200,0).animate.rotate_by(TAU)`).],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
from math import pi
scene = Scene(480, 270, background="#0f172a")
arm = scene.geometry.rect(80, 14).fill(BLUE).move_to(40, 0).with_pivot(0, 0)
scene.play([arm.animate.rotate_by(pi/2).duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.move_along_path",
  kind: "method",
  signature: ".animate.move_along(target: Drawable) -> Anim",
  params: ((name: "target", type: "Drawable", default: none, desc: [Path drawable to follow — circle, rect, curve, polyline, etc. Its world geometry (after `at`, groups) is sampled.]),),
  returns: (type: "Anim", desc: [Follow-path translation.]),
  desc: [Samples the target's Bézier outline by true arc-length and sets the caller's translation to the point at eased `t` (`get_point_at_alpha`). Combine with `.linear()` for uniform speed, or `.smooth()` for ease. Rotation/scale unaffected.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, WHITE, Scene
scene = Scene(480, 270, background=BLACK)
circle = scene.geometry.circle(60).stroke(BLUE, 3).no_fill().move_to(0, 0)
dot = scene.geometry.dot(8).fill(WHITE).move_to(60, 0)
scene.play([dot.animate.move_along(circle).duration(2.0).linear()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Anim.pivot / about_point",
  kind: "method",
  signature: ".pivot(x: float, y: float) -> Anim / .about_point(x: float, y: float) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Pivot x in scene pixels.]), (name: "y", type: "float", default: none, desc: [Pivot y in scene pixels.]),),
  returns: (type: "Anim", desc: [Same Anim with orbital pivot.]),
  desc: [Only valid on `RotateBy` anims (`Drawable.rotate`). Replaces hinge with scene-space point; the engine builds an orbital `Arc` for translation plus a slerped `Rotation` (splits `>π`).],
)[
```python
import math
from gaanim import BLACK, WHITE, Scene
scene = Scene(480, 270, background=BLACK)
dot = scene.geometry.dot(10).fill(WHITE).move_to(60, 0)
scene.play([dot.pivot(0, 0).animate.rotate_by(math.tau).duration(1.5).linear()])
# output: preview.webp
scene.render()
```
]

== Fundidos

#api-entry(
  name: "Drawable.fade_in / fade_out / fade_to",
  kind: "method",
  signature: ".animate.fade_in(duration?) .animate.fade_out(duration?) .animate.opacity(alpha: 0..1)",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds, uses default if None.]),),
  returns: (type: "Anim", desc: [Opacity anim.]),
  desc: [`fade_to` animates to target alpha. `fade_in_from` below is directional.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
title = scene.text("Fade").fill(WHITE).move_to(0, 40)
box = scene.geometry.rect(120, 50).fill(BLUE).move_to(0, -40)
scene.play([title.animate.fade_in().duration(0.5)])
scene.play([box.animate.opacity(0.35).duration(0.6)])
scene.play([title.animate.fade_out().duration(0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.fade_in_from",
  kind: "method",
  signature: ".animate.fade_in_from(direction: Direction, distance=48, duration?) -> Anim",
  params: ((name: "direction", type: "Direction", default: none, desc: [UP/DOWN/LEFT/RIGHT]), (name: "distance", type: "float", default: "48.0", desc: [Offset before entrance.]),),
  returns: (type: "Anim", desc: [Entrance from offset.]),
  desc: [Starts invisible at the requested offset, then fades and moves into place.],
)[
```python
# show-code: true
from gaanim import Direction, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
caption = scene.text("Enters from below").fill(WHITE).move_to(0, 0)
scene.play([caption.animate.fade_in_from(Direction.DOWN, distance=48).duration(0.8)])
# output: preview.webp
scene.render()
```
]

== Escritura y creación

#api-entry(
  name: "Drawable.write / unwrite",
  kind: "method",
  signature: ".animate.write(duration?) .animate.unwrite(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Glyph-by-glyph write.]),
  desc: [For text/equation. Respects vector paths, not just opacity. Generated and reactive descendants remain hidden before the scheduled animation and retain the current reveal progress while updating. `unwrite` reverses.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
formula = scene.text("$E = m c^2$").fill(GOLD).move_to(0, 0)
scene.play([formula.animate.write().duration(1.0)])
scene.play([formula.animate.unwrite().duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Text.write grouping",
  kind: "method",
  signature: ".animate.write(duration=None, *, by=\"grapheme\", order=\"forward\", stagger=0.0) -> Anim",
  params: ((name: "duration", type: "float | None", default: "None", desc: [Optional total duration, accepted as the first positional argument.]), (name: "by", type: "str", default: "\"grapheme\"", desc: [Grouping: grapheme, word, line, or semantic part.]),),
  returns: (type: "Anim", desc: [Animation descriptor accepted by #raw("scene.play()") .]),
  desc: [Writes graphemes, words, rendered lines, or semantic parts in deterministic order. The duration is positional, so #raw("text.animate.write(0.8, by=\"word\")") is valid.],
)[
```python
# show-code: true
from gaanim import Scene, part
scene = Scene(480, 270, background="#0f172a")
# Grouping is resolved by the specialized Text API.
eq = scene.text("$", part("energy", "E"), " = ", part("mass", "m"), " ", part("speed", "c^2"), "$").move_to(0, 0)
scene.play([eq.animate.write(1.4, by="part", stagger=0.08)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.create / uncreate",
  kind: "method",
  signature: ".animate.create(duration?) .animate.uncreate(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Stroke-drawing.]),
  desc: [Draws outline progressively. Generated and reactive descendants remain hidden before the scheduled animation and retain the current reveal progress while updating. `uncreate` erases. Different from `write` (which follows glyphs).],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
circle = scene.geometry.circle(50).no_fill().stroke(BLUE, 4).move_to(0, 0)
scene.play([circle.animate.create().duration(1.0).smooth()])
scene.play([circle.animate.uncreate().duration(0.6)])
# output: preview.webp
scene.render()
```
]

== Énfasis

#api-entry(
  name: "Drawable.grow_from_center / shrink_to_center",
  kind: "method",
  signature: ".animate.grow_from_center(duration?) .animate.shrink_to_center(duration?) -> Anim",
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
chart = scene.viz.chart(spec)
scene.play([chart.layer("marks").animate.grow_from_center().duration(0.7).spring()])
scene.play([chart.layer("marks").animate.shrink_to_center().duration(0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.spin_in_from_nothing",
  kind: "method",
  signature: ".animate.spin_in_from_nothing(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Spin + scale from nothing.]),
  desc: [Playful entrance for stars, icons.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
star = scene.geometry.star(5, 55, 26).fill(GOLD).move_to(0, 0)
scene.play([star.animate.spin_in_from_nothing().duration(0.9).spring()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.draw_border_then_fill",
  kind: "method",
  signature: ".animate.draw_border_then_fill(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds, border then fill.] ),),
  returns: (type: "Anim", desc: [Two-phase: stroke then fill.]),
  desc: [Elegant for filled shapes — draws edge first, then floods.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
rect = scene.geometry.rect(140, 80).fill(BLUE).stroke(WHITE, 3).move_to(0, 0)
scene.play([rect.animate.draw_border_then_fill().duration(1.3)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.indicate / wiggle",
  kind: "method",
  signature: ".animate.indicate(duration?) .animate.wiggle(duration?) -> Anim",
  params: ((name: "duration", type: "float", default: "None", desc: [Seconds.]),),
  returns: (type: "Anim", desc: [Attention anims.]),
  desc: [`indicate` makes a subtle upward hop from the visual center and highlights the target; `wiggle` shakes. Use for wrong answer / highlight.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
key = scene.geometry.circle(32).fill(BLUE).stroke(WHITE, 2).move_to(-50, 0)
wrong = scene.geometry.cross(28).stroke(WHITE, 3).move_to(60, 0)
scene.play([key.animate.indicate().duration(0.7)])
scene.play([wrong.animate.wiggle().duration(0.5)])
# output: preview.webp
scene.render()
```
]

== Transiciones de texto estructurado

The complete text-specific surface, including selections, annotations,
ownership, and error behavior, is documented in
#link("/api/text/", "Text — unified authoring and animation").

#api-entry(
  name: "Text.animate.transform_to",
  kind: "method",
  signature: "source.animate.transform_to(target).duration(seconds) -> Anim",
  params: (
    (name: "source", type: "Text", default: none, desc: [Current structured text or formula.]),
    (name: "target", type: "Text", default: none, desc: [Next structured text version.]),
    (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.]),
  ),
  returns: (type: "Anim", desc: [A structured transition accepted by `scene.play()`; Layout v2 reflow shares its duration.]),
  desc: [The descriptor remains pure until `scene.play()` commits it. Cross-scene or incompatible Layout owners raise `LayoutOwnershipError`.],
)[
```python
# show-code: true
from gaanim import BLACK, GOLD, Scene, part
scene = Scene(480, 270, background=BLACK)
# Fluent scaling retains semantic selection support.
before = scene.text("$", part("x", "x"), " + 3 = ", part("result", "7"), "$").scale_to(1.6)
after = scene.text("$", part("x", "x"), " = ", part("result", "4"), "$").scale_to(1.6)
before["result"].fill(GOLD)
after["result"].fill(GOLD)
scene.play([before.animate.write().duration(0.7)])
scene.play([before.animate.transform_to(after).duration(0.9)])
scene.play([after["result"].animate.indicate(duration=0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "TextSelection.animate.cancel",
  kind: "method",
  signature: "selection.animate.cancel(duration=None) -> Anim",
  params: ((name: "duration", type: "float | None", default: "None", desc: [Positive finite seconds; the animation default is used when omitted.]),),
  returns: (type: "Anim", desc: [Deferred cancellation animation accepted by `scene.play()`.]),
  desc: [Draws a diagonal mark and dims the selected glyphs. The mark remains associated with its owning `Text` until a replacing transition retires it.],
)[
```python
from gaanim import Scene, part
scene = Scene(480, 270)
before = scene.text("$x + ", part("obsolete", "3"), " = 7$")
after = scene.text("$x = 4$")
scene.play([before["obsolete"].animate.cancel(duration=0.6)])
scene.play([before.animate.transform_to(after).duration(0.8)])
```
]

#api-entry(
  name: "TextSelection.animate.morph_to",
  kind: "method",
  signature: "selection.animate.morph_to(target_selection).duration(seconds) -> Anim",
  params: ((name: "target_selection", type: "TextSelection", default: none, desc: [Destination for a local selection morph.]), (name: "duration", type: "float", default: "1.0", desc: [Positive finite seconds.])),
  returns: (type: "Anim", desc: [A pure descriptor accepted by `scene.play()`.]),
  desc: [Explicitly pairs two local selections while the surrounding text remains one Layout leaf.],
)[
```python
from gaanim import Scene, part
scene = Scene(480, 270)
compact = scene.text("$E = ", part("mass", "m"), " c^2$")
expanded = scene.text("$E = ", part("mass", "(m_1 + m_2)"), " c^2$")
scene.play([compact.animate.transform_to(expanded).duration(0.9)])
```
]

#api-entry(
  name: "TextSelection.animate.copy_to",
  kind: "method",
  signature: "source_selection.animate.copy_to(target_selection).duration(seconds) -> Anim",
  params: ((name: "target_selection", type: "TextSelection", default: none, desc: [Destination semantic or query selection.]), (name: "duration", type: "float | None", default: "None", desc: [Positive finite seconds; the animation default is used when omitted.])),
  returns: (type: "Anim", desc: [A deferred animation accepted by `scene.play()` and composable with other animations.]),
  desc: [Keeps the source selection visible and moves a semantic copy into the destination selection.],
)[
```python
scene.play([energy["mass"].animate.copy_to(momentum["mass"]).duration(0.8)])
```
]

== Transformaciones

#api-entry(
  name: "Drawable.transform / fade_transform / replacement_transform",
  kind: "method",
  signature: ".animate.transform_to(target) .animate.fade_transform_to(target) .animate.replacement_transform_to(target)",
  params: ((name: "target", type: "Drawable", default: none, desc: [Target shape to morph into.]),),
  returns: (type: "Anim", desc: [Morph anim.]),
  desc: [`transform` morphs in place, `fade_transform` cross-fades, `replacement_transform` replaces source with target. All pair geometry.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
circle = scene.geometry.circle(42).fill(BLUE).move_to(-90, 0)
target = scene.geometry.rect(90, 60).fill(GOLD).move_to(80, 0)
scene.play([circle.animate.create().duration(0.6)])
scene.play([circle.animate.transform_to(target).duration(1.0).spring()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.transform_matching_shapes / Text.animate.transform_to",
  kind: "method",
  signature: "scene.geometry.transform_matching_shapes(source, target, duration=1.0) / source_text.animate.transform_to(target_text).duration(seconds)",
  params: (
    (name: "source", type: "Drawable", default: none, desc: [Source object/group containing elements to match.]),
    (name: "target", type: "Drawable", default: none, desc: [Target object/group containing elements to match.]),
    (name: "duration", type: "float", default: "1.0", desc: [Duration of the transition in seconds.]),
  ),
  returns: (type: "None | Anim", desc: [`Geometry.transform_matching_shapes` queues directly; the text proxy returns an animation accepted by `scene.play()`.]),
  desc: [`transform_matching_shapes` matches arbitrary sub-elements by geometry, position, and color. The text form uses the universal pure `animate.transform_to` vocabulary.],
)[
```python
from gaanim import BLACK, GOLD, WHITE, Scene
scene = Scene(1920, 1080, background=BLACK)
e1 = scene.text("$E = m c$").fill(WHITE).move_to(0, 80).scale_to(1.3)
e2 = scene.text("$p = m v$").fill(GOLD).move_to(0, 80).scale_to(1.3)
scene.play([e1.animate.transform_to(e2).duration(1.6)])
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
ball = scene.geometry.dot(12).fill(GOLD).move_to(0, 90)
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
# output: simulation.webp
scene.render()
```
]

For a coupled example with a tracking rod, dimension and trail, see
`examples/pendulum_simulation.py`.

Reactive `tracking_line` drawables support both `create()` and `write()`.
Their endpoints may continue moving during the reveal because regeneration
updates the full path source and reapplies the current draw progress:

```python
rod = scene.geometry.tracking_line(anchor, mass).no_fill().stroke(WHITE, 4)
scene.play([rod.animate.create(0.8), mass.animate.shift_by(120, 0).duration(0.8)])
scene.play([rod.animate.write(0.8), mass.animate.shift_by(-80, 40).duration(0.8)])
```

== Series muestreadas nativas

#api-entry(
  name: "Drawable.drive_from_samples",
  kind: "method",
  signature: "drive_from_samples(times, values, property=\"x\", *, interpolation=\"linear\", scale=1.0, offset=0.0) -> Drawable",
  params: (
    (name: "times, values", type: "sequence[float]", default: none, desc: [Matching series; times must be finite and non-decreasing.]),
    (name: "property", type: "\"x\" | \"y\" | \"z\" | \"rotation\" | \"scale\" | \"opacity\" | \"signal\"", default: "\"x\"", desc: [Driven channel.]),
    (name: "interpolation", type: "\"linear\" | \"step\"", default: "\"linear\"", desc: [Interpolation between consecutive samples.]),
    (name: "scale, offset", type: "float", default: "1.0, 0.0", desc: [Output transform applied to each sample.]),
  ),
  returns: (type: "Drawable", desc: [The same drawable for fluent chaining.]),
  desc: [Drives the property as a pure function of timeline time, evaluated in Rust — no per-frame Python callbacks. Translation axes and `rotation` are relative to the authored pose (`base + offset + scale * sample`); `scale`, `opacity`, and `signal` are absolute. Samples outside the series clamp to its first/last value. Seeks and paused scrubbing are exact because the driver keeps no accumulated state. Detach with `remove_updater()`.],
)[
```python
from gaanim import CYAN, Scene

scene = Scene()
times = [i * 0.02 for i in range(len(accel))]
building = scene.geometry.rounded_rect(160, 360, 10).fill(CYAN).move_to(-200, -120)
# El edificio oscila con el registro medido; el seek es determinista.
building.drive_from_samples(times, accel, "x", scale=520.0)
scene.play([building.animate.grow_from_center()])
scene.wait(4.0)
```

`Parameter.drive_from_samples(times, values, *, ...)` drives a parameter's
float signal the same way, so computed values, readouts, and reactive plots
that reference the parameter follow the measured series for free.
]

== Composición de animaciones

#api-entry(
  name: "parallel / sequence / stagger",
  kind: "function",
  signature: "parallel(*items) | sequence(*items, gap=0.0) | stagger(*items, each=0.1) -> Composition",
  params: (
    (name: "items", type: "Anim | Audio | Video | Lottie | Composition", default: none, desc: [One or more pure leaves or nested compositions.]),
    (name: "gap", type: "float", default: "0.0", desc: [Seconds between sequence steps; a bounded negative value overlaps adjacent steps.]),
    (name: "each", type: "float", default: "0.1", desc: [Non-negative start offset between staggered children.]),
  ),
  returns: (type: "Composition", desc: [Immutable tree accepted directly by `scene.play`.]),
  desc: [The tree remains structured until `Scene.play` resolves defaults, spans, overlaps, channel conflicts, and relative targets atomically. Use `defaults`, `delay`, `stretch`, and `schedule` to configure or inspect a subtree.],
)[
```python
from gaanim import Scene, parallel, sequence, stagger

scene.play(
    sequence(
        title.animate.write(0.8),
        parallel(
            box.animate.create(),
            stagger(label.animate.fade_in(), badge.animate.fade_in(), each=0.15),
        ),
        gap=-0.1,
    )
)
```

`plan.schedule()` returns a read-only local schedule without changing the
cursor or consuming any leaf. `plan.stretch(seconds)` accepts animation-only
trees; media are rejected because their playback speed is not silently changed.
]

== Tiempo y easing

Configure any `Anim` fluently before passing to `play`:

```python
scene.play([
    circle.animate.shift_by(240, 0).duration(1.0).linear(),
    label.animate.opacity(0.5).duration(1.0).smooth(),
    icon.animate.rotate_by(1.5).duration(0.8).spring(),
])

# chainable
anim = circle.animate.create().duration(1.2).delay(0.3).ease("cubic").lag_ratio(0.2)

# stagger a list
scene.play(stagger(a.animate.fade_in(), b.animate.fade_in(), c.animate.fade_in(), each=0.1))
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
g = scene.geometry.group([scene.geometry.circle(18).fill(BLUE).move_to(-50,0), scene.geometry.circle(18).fill(BLUE).move_to(0,0), scene.geometry.circle(18).fill(BLUE).move_to(50,0)])
scene.play([g.animate.create().duration(1.0).lag_ratio(0.25)])
# output: preview.webp
scene.render()
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
dot = scene.geometry.dot(10).fill(BLUE).move_to(-110, 0)
scene.play([dot.animate.shift_by(220, 0).duration(0.9).spring()])
scene.play([dot.animate.shift_by(-220, 0).duration(0.9).smooth()])
# output: preview.webp
scene.render()
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
path = scene.geometry.path([(-120, 0), (0, 40), (120, 0)]).no_fill().stroke(WHITE, 3)
scene.play([path.animate.write().with_pen_tip().duration(1.4)])
# output: preview.webp
scene.render()
```
]
