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
dot.move_to(1.25, 1).fill(BLUE)
scene.play([dot.animate.move_to(5, 1).fill(RED)])
```

“Inmediato” no significa modificar globalmente un objeto ya compilado: registra
un corte reversible en el cursor. Los seeks anteriores conservan el estado
anterior. Construir, configurar o abandonar un `Anim` no cambia visibilidad,
estado autoral, operaciones ni cursor.

Cada `Anim` contiene un solo efecto (`fade_in`, `write`, `indicate`…) o una
combinación de destinos de propiedad (`move_to`, `fill`, `opacity`…). Encadenar
un efecto después de un destino de propiedad o de otro efecto, como
`dot.animate.move_to(1, 0).fade_in()`, o un destino de propiedad después de un
efecto, como `dot.animate.fade_in().move_to(1, 0)`, lanza `ValueError`; combina
animaciones separadas con `parallel()`. Los modificadores de tiempo y de trazo
(`duration`, `easing`, `stroke_width`…) siguen configurando el efecto. `pulse`, `wave`, `highlight`, `focus`, `cancel`,
`reveal`, `brace` y `annotate` solo existen en selecciones de texto (`text["part"].animate.pulse()`); sobre el
`animate` de un `Drawable` lanzan `TypeError`.

== Animaciones de propiedades compuestas

`Drawable.animate -> Anim` inicia una animación tipada de propiedades. Encadena
varios objetivos en el `Anim` devuelto: comparten duración, curva y retraso, y
se muestrean simultáneamente desde la línea temporal:

```python
scene.play([
  circle.animate
    .move_to(2, 0.5)
    .scale_by(1.4)
    .fill(BLUE)
    .stroke(WHITE, 0.06)
    .opacity(0.8)
    .duration(1.5)
    .easing(Easing.SMOOTH)
])
```

El proxy admite movimiento 2D y 3D, escala 3D absoluta, rotación relativa o
absoluta, opacidad, pinturas de relleno y trazo vectorial. Llamar `fill(c)` tras `no_fill()`, o
`stroke(c, width)` tras `no_stroke()`, revela suavemente la pintura desde la
transparencia; un trazo nuevo también crece desde ancho cero.

En objetos `Text` y Typst, los canales de relleno y trazo se propagan a cada
glifo visible. Cada glifo interpola desde su pintura actual, incluso si un
fragmento tiene color propio. Por eso `fill(c)` sobre el texto
completo converge esos colores al objetivo, mientras `stroke(c, width)` cambia
el contorno sin reemplazar sus rellenos distintos.

En una `Primitive3D` nativa, `fill(c)` cambia el color base PBR y
conservan rugosidad, metalicidad y emisión. Usa
`primitive.animate.material(Material3D(...))` para interpolar el material PBR
completo junto con transformaciones y opacidad. Los métodos de trazo vectorial
sobre una primitiva 3D producen `TypeError`.

El proxy es una propiedad de solo lectura, no una función. Cada `Anim` es de un
solo uso y los objetivos de posición conservan la restricción normal de
propiedad del layout.

== Fuentes reactivas y destinos

Los setters absolutos de posición, rotación, escala y opacidad aceptan
`ScalarSource`: un número, `Parameter`, `Variable`, `Computed` o `scene.time`.
Esto incluye sus variantes 3D y permite mezclar números y fuentes por eje.

```python
phase = scene.viz.parameter(0.0)
height = computed(lambda x: x*x, inputs=[phase])
dot.move_to(phase, height)
scene.play(phase.animate.set(2).duration(2))
dot.move_to(2, 4)
scene.play(dot.animate.move_to(other).opacity(0.5))
```

En el setter directo, una fuente mantiene el canal enlazado desde el cursor.
Otra fuente lo reemplaza; un valor fijo lo termina con un corte reversible.
Al retroceder se recupera el enlace anterior. Posición, rotación y escala
ocupan cada una su canal completo. Mientras esté enlazado, anima el parámetro
o fija primero el canal: las animaciones directas y operaciones relativas
conflictivas producen un error. Los otros canales siguen disponibles.

Bajo `.animate`, las fuentes destino se evalúan al inicio efectivo del clip,
incluidos retrasos y secuencias, y quedan congeladas durante la interpolación.
`animate.move_to` también acepta un `Drawable` o `AnchorPoint` de la misma
escena, resuelto al inicio; no registra seguimiento persistente.

== Animaciones personalizadas

```python
animation = dot.animate.custom(callback, channels=("position", "opacity"))
```

`callback(alpha)` recibe el progreso después de aplicar easing y devuelve un
diccionario con exactamente los canales declarados. Usa valores absolutos:
`position` es una pareja o terna local en unidades de escena; `rotation` es un
ángulo Z en radianes; `scale` es un factor uniforme o terna XYZ; `opacity` está
entre cero y uno; `fill` y `stroke` son pinturas; `stroke_width` es un ancho
no negativo. Todos los números deben ser finitos. Los tipos públicos
`AnimationChannel` y `CustomAnimationValues` describen este contrato.

```python
from gaanim import Easing, parallel

motion = dot.animate.custom(
    lambda alpha: {
        "position": (3*alpha, alpha*alpha),
        "opacity": 1 - 0.5*alpha,
    },
    channels=("position", "opacity"),
).duration(2).easing(Easing.SMOOTH)
scene.play(parallel(motion, dot.animate.fill(BLUE).duration(2)))
```

La función se evalúa en el tiempo exacto al reproducir, buscar o exportar;
no se convierte en muestras. Debe ser síncrona, pura y depender solamente del
progreso y constantes capturadas: no puede modificar la escena ni consultar
estado mutable para acumular movimiento. Algunas curvas producen progreso
fuera de cero y uno. El final respeta también curvas de ida y vuelta.

Los canales no declarados se conservan. No mezcles `custom` y setters en un
mismo proxy: usa `parallel`, que valida conflictos antes de programarlos.
Duración, retraso, composición y consumo único funcionan como en cualquier
`Anim`. Se valida todo el resultado antes de escribir propiedades. Ante un
fallo se restaura el estado inicial de los canales afectados y aparece un
diagnóstico; la exportación falla explícitamente.

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
from gaanim import Easing, BLUE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
circle = scene.geometry.circle(0.5).fill(BLUE).move_to(-1, 0)
scene.play([circle.animate.shift_by(2, 0).duration(1.0).easing(Easing.spring(stiffness=90, damping=12))])
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
from gaanim import Easing, Anchor, BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
rect = scene.geometry.rect(1.25, 0.75).fill(BLUE).stroke(WHITE, 0.025).move_to(-1.5, 0)
scene.play([rect.animate.move_to(1, 0, anchor=Anchor.TOP_RIGHT).duration(0.9).easing(Easing.SMOOTH)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SurroundingRect.retarget",
  kind: "animation",
  signature: ".retarget(targets).duration(seconds) -> Anim",
  params: ((name: "targets", type: "Drawable | TextSelection | Sequence", default: none, desc: [New live bounds, including semantic text or equation parts.]),),
  returns: (type: "Anim", desc: [Edge-interpolation animation supporting normal easing.]),
  desc: [Interpolates left, right, top, and bottom while both source and destination may continue moving. At completion the frame remains bound to the destination. Timeline seeks and rewinds reproduce the same geometry.],
)[
```python
scene.play([frame.retarget(equation["result"]).duration(0.9).easing(Easing.spring(stiffness=90, damping=12))])
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
scene = Scene(frame=(16, 9), background="#0f172a")
label = scene.text("Glide").move_to(-1.25, 0)
scene.play([label.animate.move_to(1, 0).duration(1.1)])
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
from gaanim import Easing, BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
icon = scene.geometry.circle(0.45).fill(BLUE).stroke(WHITE, 0.025).move_to(0, 0)
scene.play([icon.animate.scale_by(1.8).duration(0.7).easing(Easing.spring(stiffness=90, damping=12))])
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
scene = Scene(frame=(16, 9), background="#0f172a")
arm = scene.geometry.rect(1, 0.175).fill(BLUE).move_to(0.5, 0).with_pivot(0, 0)
scene.play([arm.animate.rotate_by(pi/2).duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.move_along_path",
  kind: "method",
  signature: ".animate.move_along(target, *, orient=False, rotate_offset=0, start=0, end=1) -> Anim",
  params: (
    (name: "target", type: "Drawable", default: none, desc: [Path drawable to follow — circle, rect, curve, polyline, etc. Its world geometry (after `at`, groups) is sampled.]),
    (name: "orient", type: "bool", default: "False", desc: [Turn along the path tangent.]),
    (name: "rotate_offset", type: "float", default: "0", desc: [Radians added to the tangent angle when orienting.]),
    (name: "start, end", type: "float", default: "0, 1", desc: [Travelled portion of the path as arc-length fractions.]),
  ),
  returns: (type: "Anim", desc: [Follow-path translation.]),
  desc: [Samples the target's Bézier outline by true arc-length and sets the caller's translation to the point at eased `t` (`get_point_at_alpha`). Combine with `.easing(Easing.LINEAR)` for uniform speed, or `.easing(Easing.SMOOTH)` for ease. With `orient=True` the rotation follows the tangent (plus `rotate_offset`), so a plane points where it flies; otherwise rotation and scale are unaffected. `.move_to(x, y).path_arc(angle)` instead bends an ordinary move into a circular arc that turns by `angle` radians.],
)[
```python
# show-code: true
from gaanim import Easing, BLACK, BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background=BLACK)
circle = scene.geometry.circle(0.75).stroke(BLUE, 0.04).no_fill().move_to(0, 0)
dot = scene.geometry.dot(0.1).fill(WHITE).move_to(0.75, 0)
scene.play([dot.animate.move_along(circle).duration(2.0).easing(Easing.LINEAR)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Anim.pivot / about_point",
  kind: "method",
  signature: ".pivot(x: float, y: float) -> Anim / .about_point(x: float, y: float) -> Anim",
  params: ((name: "x", type: "float", default: none, desc: [Pivot x in scene units.]), (name: "y", type: "float", default: none, desc: [Pivot y in scene units.]),),
  returns: (type: "Anim", desc: [Same Anim with orbital pivot.]),
  desc: [Only valid on `RotateBy` anims (`Drawable.rotate`). Replaces hinge with scene-space point; the engine builds an orbital `Arc` for translation plus a slerped `Rotation` (splits `>π`).],
)[
```python
import math
from gaanim import Easing, BLACK, WHITE, Scene
scene = Scene(frame=(16, 9), background=BLACK)
dot = scene.geometry.dot(0.125).fill(WHITE).move_to(0.75, 0)
scene.play([dot.pivot(0, 0).animate.rotate_by(math.tau).duration(1.5).easing(Easing.LINEAR)])
# output: preview.webp
scene.render()
```
]

== Fundidos

#api-entry(
  name: "Drawable.fade_in / fade_out / fade_to",
  kind: "method",
  signature: ".animate.fade_in() .animate.fade_out() .animate.opacity(alpha: 0..1)",
  params: (),
  returns: (type: "Anim", desc: [Opacity anim.]),
  desc: [`fade_to` animates to target alpha. A scheduled `fade_in` keeps the drawable hidden before its start, including late declarations and group members. `fade_in_from` below is directional.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Fade").fill(WHITE).move_to(0, 0.5)
box = scene.geometry.rect(1.5, 0.625).fill(BLUE).move_to(0, -0.5)
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
  signature: ".animate.fade_in_from(direction: Direction, distance=0.48) -> Anim",
  params: ((name: "direction", type: "Direction", default: none, desc: [UP/DOWN/LEFT/RIGHT]), (name: "distance", type: "float", default: "0.48", desc: [Offset before entrance.]),),
  returns: (type: "Anim", desc: [Entrance from offset.]),
  desc: [Starts invisible at the requested offset, then fades and moves into place.],
)[
```python
# show-code: true
from gaanim import Direction, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
caption = scene.text("Enters from below").fill(WHITE).move_to(0, 0)
scene.play([caption.animate.fade_in_from(Direction.DOWN, distance=0.6).duration(0.8)])
# output: preview.webp
scene.render()
```
]

== Escritura y creación

#api-entry(
  name: "Drawable.write / unwrite",
  kind: "method",
  signature: ".animate.write() .animate.unwrite() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Glyph-by-glyph write.]),
  desc: [For text/equation. Respects vector paths, not just opacity. Synthesized outlines use 0.03 logical units, stay geometrically constant during the trace, and fade away as the authored fill enters smoothly. Generated and reactive descendants remain hidden before the scheduled animation and retain the current reveal progress while updating. `unwrite` reverses.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
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
  signature: ".animate.write(*, by=\"grapheme\", order=\"forward\", stagger=None) -> Anim",
  params: (
    (name: "by", type: "str", default: "\"grapheme\"", desc: [Grouping: grapheme, word, explicit line, or innermost semantic part. Glyphs of one group start together.]),
    (name: "order", type: "str", default: "\"forward\"", desc: [Group order: forward, reverse, center (middle first, then outward), or random (a fixed permutation).]),
    (name: "stagger", type: "float | None", default: "None", desc: [Lag ratio between consecutive groups. #raw("None") adapts it to the number of groups; pass a non-negative ratio to override it.]),
  ),
  returns: (type: "Anim", desc: [Animation descriptor accepted by #raw("scene.play()") .]),
  desc: [Writes graphemes, words, explicit lines, or semantic parts in a deterministic order, with the segmentation of #raw("text.words"), #raw("text.lines"), and #raw("text.parts"). Configure time afterward with #raw("text.animate.write(by=\"word\").duration(0.8)").],
)[
```python
# show-code: true
from gaanim import Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
# Grouping is resolved by the specialized Text API.
eq = scene.text("$", part("energy", "E"), " = ", part("mass", "m"), " ", part("speed", "c^2"), "$").move_to(0, 0)
scene.play([eq.animate.write(by="part", stagger=0.08).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.create / uncreate",
  kind: "method",
  signature: ".animate.create() .animate.uncreate() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Progressive trace followed by a fill fade.]),
  desc: [Draws the outline during the first 70% and fades a closed shape's authored fill with a smooth alpha transition during the final 30%. The default easing is `DoubleSmooth`; an explicit `.easing(...)` still overrides it. Stroke width remains constant; if the object has no outline, a temporary 0.03-unit stroke is removed while the fill appears. Unfilled paths use the full duration for tracing. Generated and reactive descendants retain the current reveal progress while updating. `uncreate` erases. Different from `write`, which follows glyph order.],
)[
```python
# show-code: true
from gaanim import Easing, BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
circle = scene.geometry.circle(1).fill(BLUE).stroke(WHITE, 0.04).move_to(0, 0)
scene.play([circle.animate.create().duration(1.0).easing(Easing.SMOOTH)])
scene.play([circle.animate.uncreate().duration(0.6)])
# output: preview.webp
scene.render()
```
]

=== Máquina de escribir y decodificación

#api-entry(
  name: "Text.animate.typewriter / backspace / retype",
  kind: "method",
  signature: ".animate.typewriter(cps=18.0, cursor=\"▍\", blink=2.0, jitter=0.2, seed=0, keep_cursor=True) .animate.backspace(count=None, cps=24.0) .animate.retype(text, cps=18.0, jitter=0.2, seed=0) -> Anim",
  params: (
    (name: "cps", type: "float", default: "18.0", desc: [Keystrokes per second (`backspace`: deletions per second, default 24). Must be positive.]),
    (name: "cursor", type: "str | None", default: "\"▍\"", desc: [String drawn after the last typed grapheme; #raw("None") or #raw("\"\"") for none. Block characters (#raw("▏") to #raw("█")) are exact rectangles, independent of the font.]),
    (name: "blink", type: "float", default: "2.0", desc: [On/off cycles per second of the idle cursor; #raw("0") keeps it solid. The cursor is solid while typing.]),
    (name: "jitter", type: "float", default: "0.2", desc: [Each keystroke interval is scaled by a factor in #raw("[1 - jitter, 1 + jitter]"); must be in #raw("[0, 1)").]),
    (name: "seed", type: "int", default: "0", desc: [Seed of the keystroke rhythm. Timing is a pure function of seed, grapheme index and time.]),
    (name: "keep_cursor", type: "bool", default: "True", desc: [Keep the (blinking) cursor after typing; #raw("False") removes it at the end.]),
    (name: "count", type: "int | None", default: "None", desc: [`backspace`: graphemes to delete from the end, capped at the visible ones; #raw("None") deletes all.]),
    (name: "text", type: "str", default: none, desc: [`retype`: new plain text. The prefix shared with the visible text is kept; the rest is deleted and typed.]),
  ),
  returns: (type: "Anim", desc: [Text motion accepted by #raw("scene.play()"), with linear timing.]),
  desc: [`typewriter` clears the Text and reveals ⌊t·cps⌋ graphemes (with jitter) in their final layout, so nothing reflows; the cursor follows the pen of the last typed grapheme, across lines. `backspace` removes graphemes from the end and `retype` deletes back to the shared prefix, then types the rest of `text`, laid out with the Text's style from the same pen origin. Without an explicit duration each motion lasts until its last keystroke (about `graphemes / cps`); `.duration(...)` rescales the rhythm. Every glyph is evaluated natively from the clip time, so seeks, snapshots and export match continuous playback. Raises `ValueError` for invalid numbers or an empty `text`, and `TypeError` unless the proxy belongs to a whole Text.],
)[
```python
# show-code: true
from gaanim import CYAN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
prompt = scene.text("gaanim render", role="code").fill(CYAN).move_to(-3, 0)
scene.play([prompt.animate.typewriter(cps=18, cursor="▍")])
scene.wait(0.4)
scene.play([prompt.animate.backspace(6)])
scene.play([prompt.animate.retype("gaanim export --from clímax")])
scene.wait(0.6)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Text.animate.scramble / scramble_to",
  kind: "method",
  signature: ".animate.scramble(charset=\"upper\", reveal_delay=0.3, speed=20.0, seed=0) .animate.scramble_to(text, charset=\"upper\", reveal_delay=0.3, speed=20.0, seed=0) -> Anim",
  params: (
    (name: "charset", type: "str", default: "\"upper\"", desc: [#raw("\"upper\""), #raw("\"lower\""), #raw("\"digits\""), #raw("\"hex\""), #raw("\"symbols\"") or a literal string such as #raw("\"01\""). Its glyphs are shaped once with the Text's font.]),
    (name: "reveal_delay", type: "float", default: "0.3", desc: [Seconds every position scrambles before the first one settles. Non-negative.]),
    (name: "speed", type: "float", default: "20.0", desc: [Glyph changes per second. Positive.]),
    (name: "seed", type: "int", default: "0", desc: [Seed of the glyph choice #raw("hash(seed, index, floor(t * speed))").]),
    (name: "text", type: "str", default: none, desc: [`scramble_to`: new plain text to decode into.]),
  ),
  returns: (type: "Anim", desc: [Text motion accepted by #raw("scene.play()"), with linear timing.]),
  desc: [A decoding reveal: each non-space grapheme shows seeded charset glyphs centered in its final cell, then settles to the real glyph from left to right after `reveal_delay`. The final text's layout is reserved, so the width never jumps; `scramble_to` hides the current glyphs and decodes the new text in its own layout. The default duration is `reveal_delay` plus 0.05 s per grapheme (at least 0.6 s). Frames are exact for any seek.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
label = scene.text("LAUNCH SEQUENCE", role="title").fill(GOLD).move_to(0, 0)
scene.play([label.animate.scramble(charset="upper", reveal_delay=0.3, speed=20)])
scene.wait(0.4)
scene.play([label.animate.scramble_to("LANZAMIENTO", charset="01")])
# output: preview.webp
scene.render()
```
]

== Énfasis

#api-entry(
  name: "Drawable.grow_from_center / shrink_to_center",
  kind: "method",
  signature: ".animate.grow_from_center() .animate.shrink_to_center() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Scale from/to center.]),
  desc: [Pop in/out. Great for charts, badges.],
)[
```python
# show-code: true
# Visualization uses immutable ChartSpec batches.
from gaanim import Easing, Axis, BLUE, ChartSpec, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
spec = ChartSpec({"x": [0, 1, 2], "value": [18, 42, 31]}) \
  .mark("bar").encode(x="x", y="value") \
  .axes(x=Axis.category(["Q1", "Q2", "Q3"]), y=Axis.linear(0, 50))
chart = scene.viz.chart(spec)
scene.play([chart.layer("marks").animate.grow_from_center().duration(0.7).easing(Easing.spring(stiffness=90, damping=12))])
scene.play([chart.layer("marks").animate.shrink_to_center().duration(0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.grow_from_edge / grow_from_point",
  kind: "method",
  signature: ".animate.grow_from_edge(direction) .animate.grow_from_point(x, y) -> Anim",
  params: (
    (name: "direction", type: "Direction", desc: [Lado de la caja que queda fijo. Las diagonales fijan una esquina.]),
    (name: "x, y", type: "float", desc: [Punto de la escena que queda fijo.]),
  ),
  returns: (type: "Anim", desc: [Crecimiento desde escala cero.]),
  desc: [Como `grow_from_center`, pero con otro punto fijo. `grow_from_edge`
    usa la caja de límites en su posición final: `Direction.DOWN` fija el
    punto medio del borde inferior, así una barra sube desde su base, y
    `Direction.custom(x, y)` fija el punto correspondiente de la caja.
    `grow_from_point` fija un punto arbitrario de la escena, por ejemplo el
    origen de un callout. Ambos terminan en la posición y el tamaño
    declarados. Easing por defecto: `Smooth`.],
)[
```python
# show-code: true
from gaanim import BLUE, CYAN, GOLD, Direction, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
bars = [scene.geometry.rect(1.2, h).fill(c).move_to(x, h / 2 - 2) for x, h, c in [(-3, 2.5, BLUE), (-1.4, 4, CYAN), (0.2, 3, BLUE)]]
badge = scene.geometry.circle(0.9).fill(GOLD).move_to(4, 1.5)
scene.play([bar.animate.grow_from_edge(Direction.DOWN).duration(0.8) for bar in bars])
scene.play([badge.animate.grow_from_point(2.5, 0).duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.grow_arrow",
  kind: "method",
  signature: ".animate.grow_arrow() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Crecimiento de flecha desde la cola.]),
  desc: [Versión mejorada del `GrowArrow` de Manim. La cola queda fija y la
    punta recorre la columna de la flecha, siguiendo el arco en
    `curved_arrow` y `curved_arrow_arc`. La cabeza emerge con sus
    proporciones durante la primera longitud de cabeza y luego conserva su
    tamaño mientras el cuerpo se extiende; el grosor del trazo nunca cambia
    (Manim escala toda la flecha desde el inicio y deforma la cabeza).
    Un `scene.geometry.connector` crece a lo largo de su polilínea viva,
    pasando por cada punto `via`, mientras sus extremos siguen a sus
    referencias; la cabeza conserva su tamaño y gira en las esquinas.
    Cualquier otro drawable, o una flecha remodelada por un transform, usa
    `create()`. Easing por defecto: `Smooth`.],
)[
```python
# show-code: true
from gaanim import Anchor, CYAN, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
straight = scene.geometry.arrow(-5, 1, 0, 1, head_length=0.4, head_width=0.34, body_width=0.08).fill(CYAN).stroke(CYAN, 0.02)
curved = scene.geometry.curved_arrow(-5, -1.5, 0, -1.5, 1.2, head_length=0.4, head_width=0.34, body_width=0.08).fill(GOLD).stroke(GOLD, 0.02)
box = scene.geometry.rect(1.6, 1.0).no_fill().stroke(CYAN, 0.03).move_to(4.5, 2.2)
link = scene.geometry.connector((1.5, 0.6), box.anchor_point(Anchor.BOTTOM), via=[(4.5, 0.6)]).fill(GOLD)
scene.play([straight.animate.grow_arrow().duration(1.2), curved.animate.grow_arrow().duration(1.2), link.animate.grow_arrow().duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.spin_in_from_nothing",
  kind: "method",
  signature: ".animate.spin_in_from_nothing() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Spin + scale from nothing.]),
  desc: [Playful entrance for stars, icons.],
)[
```python
# show-code: true
from gaanim import Easing, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
star = scene.geometry.star(5, 0.69, 0.325).fill(GOLD).move_to(0, 0)
scene.play([star.animate.spin_in_from_nothing().duration(0.9).easing(Easing.spring(stiffness=90, damping=12))])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.draw_border_then_fill",
  kind: "method",
  signature: ".animate.draw_border_then_fill() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Two-phase: stroke then fill.]),
  desc: [Elegant for filled shapes — draws edge first, then floods.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
rect = scene.geometry.rect(1.75, 1).fill(BLUE).stroke(WHITE, 0.04).move_to(0, 0)
scene.play([rect.animate.draw_border_then_fill().duration(1.3)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.indicate / wiggle",
  kind: "method",
  signature: ".animate.indicate() .animate.wiggle() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Attention anims.]),
  desc: [`indicate` makes a subtle upward hop from the visual center and highlights the target; `wiggle` shakes. Use for wrong answer / highlight.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
key = scene.geometry.circle(0.4).fill(BLUE).stroke(WHITE, 0.025).move_to(-0.625, 0)
wrong = scene.geometry.cross(0.35).stroke(WHITE, 0.04).move_to(0.75, 0)
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
  desc: [The descriptor remains pure until `scene.play()` commits it. At the endpoint, the source identity adopts the target's measured typographic baseline, including equations with scripts or limits. Cross-scene or incompatible Layout owners raise `LayoutOwnershipError`.],
)[
```python
# show-code: true
from gaanim import BLACK, GOLD, Scene, part
scene = Scene(frame=(16, 9), background=BLACK)
# Fluent scaling retains semantic selection support.
before = scene.text("$", part("x", "x"), " + 3 = ", part("result", "7"), "$").scale_to(1.6)
after = scene.text("$", part("x", "x"), " = ", part("result", "4"), "$").scale_to(1.6)
before["result"].fill(GOLD)
after["result"].fill(GOLD)
scene.play([before.animate.write().duration(0.7)])
scene.play([before.animate.transform_to(after).duration(0.9)])
scene.play([after["result"].animate.indicate().duration(0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "TextSelection.animate.cancel",
  kind: "method",
  signature: "selection.animate.cancel() -> Anim",
  params: (),
  returns: (type: "Anim", desc: [Deferred cancellation animation accepted by `scene.play()`.]),
  desc: [Draws a diagonal mark and dims the selected glyphs. The mark remains associated with its owning `Text` until a replacing transition retires it.],
)[
```python
from gaanim import Scene, part
scene = Scene(frame=(16, 9))
before = scene.text("$x + ", part("obsolete", "3"), " = 7$")
after = scene.text("$x = 4$")
scene.play([before["obsolete"].animate.cancel().duration(0.6)])
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
scene = Scene(frame=(16, 9))
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
  desc: [`transform` morphs in place, `fade_transform` cross-fades, and `replacement_transform` replaces source with target. All honor composition timing, pair geometry, and preserve an absent fill instead of synthesizing one.],
)[
```python
# show-code: true
from gaanim import Easing, BLUE, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
circle = scene.geometry.circle(0.525).fill(BLUE).move_to(-1.125, 0)
target = scene.geometry.rect(1.125, 0.75).fill(GOLD).move_to(1, 0)
scene.play([circle.animate.create().duration(0.6)])
scene.play([circle.animate.transform_to(target).duration(1.0).easing(Easing.spring(stiffness=90, damping=12))])
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
scene = Scene(frame=(16, 9), background=BLACK)
e1 = scene.text("$E = m c$").fill(WHITE).move_to(0, 1).scale_to(1.3)
e2 = scene.text("$p = m v$").fill(GOLD).move_to(0, 1).scale_to(1.3)
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

scene = Scene(frame=(16, 9), background=BLACK)
ball = scene.geometry.dot(0.15).fill(GOLD).move_to(0, 1.125)
state = {"velocity": 0.0}

def reset():
    state["velocity"] = 0.0

def step(pos, dt, elapsed):
    x, y, z = pos
    state["velocity"] -= 3.0 * dt
    y += state["velocity"] * dt
    if y < -1.125:
        y = -1.125
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
rod = scene.geometry.tracking_line(anchor, mass).no_fill().stroke(WHITE, 0.05)
scene.play([rod.animate.create().duration(0.8), mass.animate.shift_by(1.5, 0).duration(0.8)])
scene.play([rod.animate.write().duration(0.8), mass.animate.shift_by(-1, 0.5).duration(0.8)])
```

== Series muestreadas nativas

#api-entry(
  name: "Drawable.drive_from_samples",
  kind: "method",
  signature: "drive_from_samples(times, values, property=\"x\", *, interpolation=\"linear\", scale=1.0, offset=0.0) -> Drawable",
  params: (
    (name: "times, values", type: "sequence[float] | sequence[(float, float)]", default: none, desc: [Matching series; times must be finite and non-decreasing. Values are `(x, y)` pairs for `"xy"`.]),
    (name: "property", type: "\"x\" | \"y\" | \"xy\" | \"z\" | \"rotation\" | \"scale\" | \"opacity\" | \"signal\"", default: "\"x\"", desc: [Driven channel. `"xy"` takes `(x, y)` pairs and drives both translation axes as the `"x"` and `"y"` channels; `scale` and `offset` apply to both.]),
    (name: "interpolation", type: "\"linear\" | \"step\"", default: "\"linear\"", desc: [Interpolation between consecutive samples.]),
    (name: "scale, offset", type: "float", default: "1.0, 0.0", desc: [Output transform applied to each sample.]),
  ),
  returns: (type: "Drawable", desc: [The same drawable for fluent chaining.]),
  desc: [Drives the property as a pure function of timeline time, evaluated in Rust — no per-frame Python callbacks. Translation axes and `rotation` are relative to the authored pose (`base + offset + scale * sample`); `scale`, `opacity`, and `signal` are absolute. Samples outside the series clamp to its first/last value. Seeks and paused scrubbing are exact because the driver keeps no accumulated state. Each property is an independent channel: driving `"x"` and then `"y"` keeps both, while driving the same property again replaces it. Detach with `remove_updater()`.],
)[
```python
from gaanim import CYAN, Scene

scene = Scene()
times = [i * 0.02 for i in range(len(accel))]
building = scene.geometry.rounded_rect(2, 4.5, 0.125).fill(CYAN).move_to(-2.5, -1.5)
# El edificio oscila con el registro medido; el seek es determinista.
building.drive_from_samples(times, accel, "x", scale=6.5)
scene.play([building.animate.grow_from_center()])
scene.wait(4.0)
```

`times` are relative to the timeline cursor where `drive_from_samples` is
called: a series declared after `scene.wait(2.0)` plays its `t = 0` sample at
two seconds, and seeks before that point hold the first sample.

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
    (name: "items", type: "Anim | Audio | Video | VideoSegment | Lottie | Composition", default: none, desc: [One or more pure leaves or nested compositions.]),
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
        title.animate.write().duration(0.8),
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

`Playable` names that union for annotations of your own helpers. Import it from
`gaanim`; it also works at runtime, including with `isinstance`.

```python
from gaanim import Playable, parallel

def entrance(*items: Playable) -> Playable:
    return parallel(*items).delay(0.2)
```
]

=== Etiquetas y posiciones relativas

#api-entry(
  name: "label / Composition.insert",
  kind: "function",
  signature: "label(name: str) -> Composition | Composition.insert(item, at: str | float) -> Composition",
  params: (
    (name: "name", type: "str", default: none, desc: [Unique label name within the composition tree. Empty names, numbers and names starting with `<`, `>`, `+`, `-` or `=` raise `ValueError`.]),
    (name: "item", type: "Anim | Audio | Video | VideoSegment | Lottie | Composition", default: none, desc: [Item placed at `at`; it may itself be a `label`, which later inserts can reference.]),
    (name: "at", type: "str | float", default: none, desc: [`"name"`, `"name+0.15"`, `"name-0.2"` or `"name+=0.15"`: a label plus an offset. `"<"` / `">"` (also `"<+0.1"`, `">-0.2"`): start / end of the previous item, i.e. the most recent non-label insert or else the last child. `"+=0.3"` / `"-=0.3"`: relative to the current end. A number: absolute local seconds.]),
  ),
  returns: (type: "Composition", desc: [A new immutable tree; the original is unchanged.]),
  desc: [A `label` is a zero-duration named instant. In a `sequence` it takes no step and no `gap`: it marks where the next step starts, or where the previous step ends when it is last. Positions resolve to absolute local times when the composition is scheduled or played; inserts go after the children and before `stretch`, `repeat` and `delay`. A malformed `at` raises `ValueError` immediately; an unknown label (the error lists the defined ones) or a position before 0 raises `ValueError` from `schedule()` or `scene.play`. An exact label name always wins, so `"part-2"` finds a label called `"part-2"`.],
)[
```python
from gaanim import label, sequence

intro = (
    sequence(
        title.animate.write().duration(0.8),
        label("golpe"),
        subtitle.animate.fade_in().duration(0.4),
    )
    .insert(logo.animate.grow_from_center(), at="golpe+0.15")
    .insert(glow.animate.flash(), at="<")        # starts with the logo
    .insert(footer.animate.fade_in(), at="-=0.2")  # overlaps the end
)
print(intro.schedule().labels)  # {'golpe': 0.8}
scene.play(intro)
```

`Schedule.labels` is a `dict` of resolved label times in local seconds, in time
order, including nested and inserted labels. A stretched composition scales
them; a repeated one reports the first repetition. For named instants on the
global timeline use `scene.marker` (see Escena).
]

== Tiempo y easing

Configure any `Anim` fluently before passing to `play`:

```python
from gaanim import Easing, EasingCurve

scene.play([
    circle.animate.shift_by(3, 0).duration(1.0).easing(Easing.LINEAR),
    label.animate.opacity(0.5).duration(1.0).easing(Easing.SMOOTH),
    icon.animate.rotate_by(1.5).duration(0.8).easing(Easing.spring(stiffness=90, damping=12)),
])

# chainable
anim = circle.animate.create().duration(1.2).delay(0.3).easing(Easing.ease_in_out(EasingCurve.CUBIC)).lag_ratio(0.2)

# stagger a list
scene.play(stagger(a.animate.fade_in(), b.animate.fade_in(), c.animate.fade_in(), each=0.1))
```

El IDE puede navegar el catálogo sin recordar strings:

- Presets: `LINEAR`, `SMOOTH`, `DOUBLE_SMOOTH`, `THERE_AND_BACK`,
  `LINGERING`, `RUNNING_START`, `EXPONENTIAL_DECAY` y `NOT_QUITE_THERE`.
- Springs con nombre: `SMOOTH_SPRING` (sin rebote), `GENTLE`, `QUICK`, `SNAPPY` y
  `BOUNCY`.
- Familias de `EasingCurve`: `QUADRATIC`, `CUBIC`, `QUARTIC`, `QUINTIC`,
  `EXPONENTIAL`, `SINE`, `CIRCULAR`, `BACK`, `ELASTIC` y `BOUNCE`.
- Fábricas: `ease_in`, `ease_out`, `ease_in_out`, `spring`, `steps`,
  `mirror`, `there_and_back`, `cubic_bezier`, `custom`, `back`, `elastic`,
  `bounce`, `slow_mo`, `rough`, `squish` y `from_svg`.

`Easing.spring(bounce=0.35)` describe el resorte por cómo se ve, no por su
física: `bounce` es el sobrepaso máximo (0 = amortiguamiento crítico, sin
rebote) y el resorte se asienta dentro de la duración de la animación, así que
`duration` controla el ritmo. La forma física `spring(stiffness, damping,
mass=1, velocity=0)` sigue disponible; `velocity` es la velocidad inicial en
distancias por duración del clip. Todos los resortes terminan exactamente en el
destino.

#table(
  columns: (1.4fr, 2fr),
  inset: 7pt,
  [*Fábrica*], [*Carácter*],
  [`back(overshoot=1.70158, mode="out")`], [Retrocede o se pasa del destino],
  [`elastic(amplitude=1, period=0.3, mode="out")`], [Oscila como una banda elástica],
  [`bounce(strength=1, mode="out")`], [Rebota al llegar; `strength=0` es un cúbico],
  [`slow_mo(linear_ratio=0.7, power=0.7)`], [Rápido, cámara lenta, rápido],
  [`rough(strength=1, points=20, seed=0)`], [Parpadeo y jitter deterministas],
  [`squish(easing, start, end)`], [Aplica `easing` solo entre `start` y `end`],
  [`from_svg("M0,0 C0.3,0 0.2,1.2 1,1")`], [Curva dibujada en cualquier editor],
  [`steps(n, jump="end")`], [Saltos discretos con los modos de CSS],
)

`mode` acepta `"in"`, `"out"` e `"in_out"`.

Las fábricas rechazan números no finitos y dominios inválidos. No se aceptan
nombres de easing ni existe un fallback silencioso a `SMOOTH`.

`Easing.custom(función, samples=256)` convierte cualquier curva de Python en un
easing. La función se llama `samples` veces, en tiempos equiespaciados de 0 a 1,
al crear el easing; el resultado es una tabla interpolada linealmente, así que
el render nunca vuelve a Python y la vista previa, los seeks y la exportación
coinciden. Los valores deben ser finitos y estar en `[-1, 2]` (se permite
sobrepasar el destino); si no, o si `samples` no está en `[2, 65536]`, se lanza
`ValueError`.

```python
salto = Easing.custom(lambda t: 1 - abs(math.cos(3 * math.pi * t)) * (1 - t) ** 2)
scene.play(ball.animate.move_to(0, -2).duration(1.2).easing(salto))
```

#api-entry(
  name: "Anim timing",
  kind: "method",
  signature: ".duration(seconds) .delay(seconds) .easing(Easing) .lag_ratio(0..1)",
  params: ((name: "value", type: "float|int", default: none, desc: [Timing value.]),),
  returns: (type: "Anim", desc: [Self.]),
  desc: [`duration` controls total time, `delay` waits before start, `easing` selects a typed interpolation, and `lag_ratio` staggers sub-paths inside one drawable. Use `Easing.steps(count)` for discrete interpolation.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
g = scene.geometry.group([scene.geometry.circle(0.225).fill(BLUE).move_to(-0.625,0), scene.geometry.circle(0.225).fill(BLUE).move_to(0,0), scene.geometry.circle(0.225).fill(BLUE).move_to(0.625,0)])
scene.play([g.animate.create().duration(1.0).lag_ratio(0.25)])
# output: preview.webp
scene.render()
```
]

=== Stagger espacial

`stagger(*items, each=0.1)` escalona por índice. Con `origin`, `grid`, `total` o
`easing`, el retardo de cada ítem crece con su distancia al origen: `"start"`,
`"end"`, `"center"`, `"edges"` (de los bordes hacia dentro), `"random"` (orden
aleatorio con `seed`) o un punto `(x, y)`. Las distancias usan las posiciones
declaradas (`grid="auto"`) o las celdas de `grid=(filas, columnas)`; `each` es
el retardo por paso de separación y `total` fija la duración de toda la onda.
`distribute(items, low, high, origin=...)` usa el mismo orden para repartir
valores (tamaños, opacidades) en lugar de tiempos.

```python
scene.play(stagger(*[d.animate.grow_from_center() for d in dots], each=0.03, origin="center"))
scene.play(stagger(*anims, total=1.2, origin="random", seed=7))
scene.play(stagger(*anims, each=0.05, origin=(0.0, -3.0)))
for dot, size in zip(dots, distribute(dots, 0.4, 1.4, origin="edges")):
    dot.scale_by(size)
```

=== Trim de trazos y efectos animables

`drawable.trim(start=None, end=None, offset=None, mode=None)` muestra solo la
ventana `[start, end]` del trazo (fracciones de longitud de arco), desplazada
por `offset`, que da la vuelta al final del camino. Los valores omitidos
conservan el actual (al principio `0`, `1` y `0`). `mode="simultaneous"` recorta
cada subtrazo y cada descendiente a la vez; `"sequential"` recorta la longitud
total, así los subtrazos aparecen uno tras otro. `animate.trim(...)` lo anima:

```python
logo.trim(end=0.0)
scene.play(logo.animate.trim(end=1.0).duration(1.2))             # dibujar
ring.trim(start=0.5, end=0.5)
scene.play(ring.animate.trim(start=0.0, end=1.0))                # desde el centro
orbit.trim(start=0.0, end=0.15)
scene.play(orbit.animate.trim(offset=1.0).duration(2))           # segmento viajero
```

`animate.glow(color, radius, intensity)`, `animate.blur(sigma)` y
`animate.shadow(color, x, y, blur)` interpolan los efectos estáticos del mismo
nombre y se combinan con otros destinos de propiedad. Un efecto ausente crece
desde cero; `glow(None)`, `blur(0)` y `shadow(None)` lo desvanecen.

```python
card.shadow(BLACK, 0, -0.05, 0.05)
scene.play(card.animate.shadow(BLACK, 0, -0.25, 0.4).scale_to(1.04))  # levantar
scene.play(orb.animate.glow(CYAN, radius=0.5, intensity=2.0).repeat(3, yoyo=True))
hero.blur(0.3)
scene.play(hero.animate.blur(0.0).duration(0.6))                       # blur-in
```

=== Repetición

`anim.repeat(count, yoyo=False, delay=0)` reproduce la animación `count` veces:
`duration` y `easing` describen un ciclo y `delay` separa los ciclos. Con
`yoyo=True` los ciclos alternan de sentido; un número par de ciclos termina
donde empezó y las animaciones siguientes continúan desde ahí.
`anim.loop(mode="cycle", until=segundos, delay=0)` repite tantos ciclos
completos como quepan en `until`: `"cycle"` reinicia, `"pingpong"` alterna y
`"offset"` continúa desde el final del ciclo anterior, así `rotate_by` o
`shift_by` se acumulan. `Composition.repeat(count, delay=0)` repite un árbol
completo de animaciones. La duración total entra en `play` y en
`Composition.schedule()`, y el timeline sigue siendo finito y exacto al hacer
seek.

```python
scene.play(spinner.animate.rotate_by(TAU).duration(1.2).repeat(3))
scene.play(badge.animate.scale_to(1.08).duration(0.4).repeat(4, yoyo=True, delay=0.1))
scene.play(arrow.animate.shift_by(0.3, 0).duration(0.5).loop("pingpong", until=4.0))
scene.play(parallel(a.animate.rotate_by(TAU), b.animate.shift_by(1, 0)).repeat(2))
```

#api-entry(
  name: "Anim easing",
  kind: "method",
  signature: ".easing(easing: Easing) -> Anim",
  params: ((name: "easing", type: "Easing", default: none, desc: [Typed preset or validated factory result.]),),
  returns: (type: "Anim", desc: [Self.]),
  desc: [`Easing` exposes IDE-discoverable presets, curve families, springs, steps, mirrored curves, there-and-back motion, and cubic Bézier controls. Unknown strings are rejected.],
)[
```python
# show-code: true
from gaanim import Easing, BLUE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
dot = scene.geometry.dot(0.125).fill(BLUE).move_to(-1.375, 0)
scene.play([dot.animate.shift_by(2.75, 0).duration(0.9).easing(Easing.spring(stiffness=90, damping=12))])
scene.play([dot.animate.shift_by(-2.75, 0).duration(0.9).easing(Easing.SMOOTH)])
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
scene = Scene(frame=(16, 9), background="#0f172a")
path = scene.geometry.path([(-1.5, 0), (0, 0.5), (1.5, 0)]).no_fill().stroke(WHITE, 0.04)
scene.play([path.animate.write().with_pen_tip().duration(1.4)])
# output: preview.webp
scene.render()
```
]


`Image.animate.crop(...)` y `Video.animate.crop(...)` animan el rectángulo
fuente dentro de un marco fijo, con las unidades descritas en la API de medios.
`VideoSegment` es una hoja finita de composición: admite `parallel`, `sequence`
y `stagger`, pero no `stretch`. Su velocidad se configura al crear el fragmento.

== Transiciones entre segmentos

`Transition` describe el paso de un segmento al siguiente en `scene.segment(...)`
o `scene.link(...)`. Todas las transiciones salvo `cut` aceptan `easing=`, con
cualquier `Easing`, incluidos los springs que sobrepasan el destino. Todas
aceptan `overlay=`, un `Overlay` dibujado encima del corte. Ninguna de las dos
opciones cambia la duración de los segmentos. Sin `easing`, `cross_fade`,
`fade_through`, `zoom_through` y `morph` avanzan de forma lineal. Los revelados
vectoriales (`wipe`, `clock_wipe`, `iris`, `blinds`, `push` y `slide`) usan
`Easing.SMOOTH`.

Los revelados vectoriales recortan ambos segmentos con caminos animados dentro
del marco visible de la cámara: el entrante se ve dentro de la región revelada
y el saliente en el resto. Si los segmentos tienen fondos distintos, el fondo
entrante se revela con la misma forma. Todo es geometría `kurbo`, sin texturas
intermedias, así que la transición es nítida a cualquier resolución y un seek
reproduce exactamente el mismo frame. El borde suave de `wipe(feather=...)` es
una rampa de alfa lineal compuesta vectorialmente (`DestIn`), también sin
texturas. Los objetos persistentes (`scene.persist`) no se recortan ni se
desplazan.

#api-entry(
  name: "Transition.wipe",
  kind: "factory",
  signature: "Transition.wipe(duration: float, direction: str = \"left\", feather: float = 0.1, *, easing: Easing | None = None, overlay: Overlay | None = None) -> Transition",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "direction", type: "str", default: "\"left\"", desc: [Sentido en que viaja el borde: `left`, `right`, `up`, `down` o una diagonal como `up_left`. Con `"left"` el revelado empieza en el lado derecho.]),
    (name: "feather", type: "float", default: "0.1", desc: [Ancho del borde suave como fracción del recorrido, en `[0, 1]`. `0` da un borde duro.]),
  ),
  returns: (type: "Transition", desc: [Barrido lineal.]),
  desc: [Un borde recto cruza el marco y descubre el segmento entrante. Una duración no positiva, una dirección desconocida o un `feather` fuera de `[0, 1]` lanzan `ValueError`.],
)[
```python
scene.segment("detalle", Transition.wipe(0.6, direction="left", feather=0.1))
```
]

#api-entry(
  name: "Transition.clock_wipe",
  kind: "factory",
  signature: "Transition.clock_wipe(duration: float, start_angle: float = 90.0, *, easing: Easing | None = None, overlay: Overlay | None = None) -> Transition",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "start_angle", type: "float", default: "90.0", desc: [Grados en sentido antihorario desde +x; `90` empieza a las doce.]),
  ),
  returns: (type: "Transition", desc: [Barrido radial.]),
  desc: [Una aguja gira en sentido horario alrededor del centro del marco y deja ver el segmento entrante en el sector recorrido.],
)[
```python
scene.segment("resumen", Transition.clock_wipe(0.8, start_angle=90))
```
]

#api-entry(
  name: "Transition.iris",
  kind: "factory",
  signature: "Transition.iris(duration: float, center: tuple[float, float] = (0.0, 0.0), shape: str | Drawable = \"circle\", *, easing: Easing | None = None, overlay: Overlay | None = None) -> Transition",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "center", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Centro del iris en unidades de escena.]),
    (name: "shape", type: "str | Drawable", default: "\"circle\"", desc: [`circle`, `diamond`, `square`, `star` o un `Drawable` cuyo contorno vectorial se usa como plantilla.]),
  ),
  returns: (type: "Transition", desc: [Iris que crece hasta cubrir el marco.]),
  desc: [La forma crece desde `center` hasta que el segmento entrante ocupa todo el marco. Un `Drawable` se centra en su caja; se sigue dibujando en su propio segmento, así que conviene ocultarlo si solo sirve de plantilla. Uno sin camino vuelve al círculo. Un nombre desconocido lanza `ValueError`.],
)[
```python
scene.segment("zoom", Transition.iris(0.7, center=(2, 1), shape="star"))
```
]

#api-entry(
  name: "Transition.blinds",
  kind: "factory",
  signature: "Transition.blinds(duration: float, count: int = 8, angle: float = 0.0, *, easing: Easing | None = None, overlay: Overlay | None = None) -> Transition",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "count", type: "int", default: "8", desc: [Número de lamas, entre 1 y 512.]),
    (name: "angle", type: "float", default: "0.0", desc: [Inclinación de las lamas en grados; `0` son lamas horizontales que se abren hacia abajo.]),
  ),
  returns: (type: "Transition", desc: [Persiana veneciana.]),
  desc: [Todas las lamas se abren a la vez y en la misma fracción.],
)[
```python
scene.segment("datos", Transition.blinds(0.6, count=8, angle=0))
```
]

#api-entry(
  name: "Transition.push",
  kind: "factory",
  signature: "Transition.push(duration: float, direction: str = \"up\", *, easing: Easing | None = None, overlay: Overlay | None = None) -> Transition",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "direction", type: "str", default: "\"up\"", desc: [Movimiento de ambos marcos: `left`, `right`, `up` o `down`.]),
  ),
  returns: (type: "Transition", desc: [Empuje.]),
  desc: [El segmento entrante empuja al saliente fuera del marco: ambos se desplazan un ancho o un alto de marco y cada uno queda recortado a su propio marco. `Transition.slide(duration, direction)` es la variante en la que el saliente queda quieto y el entrante lo cubre al deslizarse encima.],
)[
```python
scene.segment("siguiente", Transition.push(0.5, direction="up"))
scene.segment("final", Transition.slide(0.5, "left", easing=Easing.spring(bounce=0.2)))
```
]

#api-entry(
  name: "Overlay",
  kind: "class",
  signature: "Overlay.flash(color: Color | None = None, duration: float = 0.2) | Overlay.light_leak(seed: int = 0, hue: float = 0.1, duration: float = 0.8, intensity: float = 0.8)",
  params: (
    (name: "color", type: "Color | None", default: "None", desc: [Color del destello; blanco si se omite.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla de la disposición de las manchas de luz.]),
    (name: "hue", type: "float", default: "0.1", desc: [Tono base en vueltas: `0.1` naranja cálido, `0.6` azul.]),
    (name: "duration", type: "float", default: "0.2 / 0.8", desc: [Segundos, positivo. La ventana se centra en el punto medio de la transición; en `cut`, en el propio corte.]),
    (name: "intensity", type: "float", default: "0.8", desc: [Brillo máximo, en `[0, 4]`.]),
  ),
  returns: (type: "Overlay", desc: [Capa para `overlay=` de cualquier `Transition`.]),
  desc: [`flash` cubre el marco con un color que sube durante la primera mitad y se apaga en la segunda. `light_leak` dibuja manchas de luz suaves en modo pantalla (`screen`) que derivan por el marco. Las dos son funciones puras del tiempo y de la semilla, así que preview, seek y export coinciden. Valores fuera de rango lanzan `ValueError`.],
)[
```python
scene.segment("impacto", Transition.cut(overlay=Overlay.flash(WHITE, 0.15)))
scene.segment("cálido", Transition.cross_fade(0.4, overlay=Overlay.light_leak(seed=2, hue=0.1)))
```
]
