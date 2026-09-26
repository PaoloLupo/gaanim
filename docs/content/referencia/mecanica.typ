#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Mecánica",
  description: "Dibujo técnico y mecanismos con scene.mechanics: cotas, barras, muelles, fuerzas, apoyos y engranajes reactivos",
  route: "/referencia/mecanica/",
  nav: "Mecánica",
)

= Mecánica

`scene.mechanics` dibuja la notación de la mecánica y el dibujo técnico: cotas,
ángulos, barras, muelles, vectores de fuerza, apoyos y engranajes. Casi todo es
reactivo: los extremos aceptan objetos, `AnchorPoint` y `PointRef` (ver
#link("/referencia/geometria/")[Geometría]), y la geometría se regenera en el
mismo fotograma cuando se mueven. Las magnitudes aceptan números o valores
reactivos (`Parameter`, `Variable`, `Computed`).

```python
# show-code: true
from gaanim import Anchor, BLACK, Direction, WHITE, Scene

scene = Scene(frame=(16, 9), background=WHITE)
wall = scene.mechanics.fixed_support((-4, 0), direction=Direction.RIGHT, size=1.2, ground_length=2.4)
block = scene.geometry.rect(2, 1.6).fill("#cbd5e1").stroke(BLACK, 0.04).move_to(0, 0)
spring = scene.mechanics.spring_between((-4, 0), block.anchor_point(Anchor.LEFT), coils=7, amplitude=0.3).no_fill().stroke(BLACK, 0.05)
gap = scene.mechanics.dimension_between((-4, -0.8), block.anchor_point(Anchor.BOTTOM_LEFT), 0.6, side="below", show_value=True, unit="m", color=BLACK)
scene.play([spring.animate.fade_in(), gap.animate.fade_in()], duration=0.3)
scene.play([block.animate.shift_by(2, 0).duration(1.2)])
# output: preview.webp
scene.render()
```

Los textos de las anotaciones (etiqueta, valor y unidad) miden 0.48 unidades
por defecto, legibles en 1080p; `font_size` lo cambia. `color` pinta la
geometría y la anotación completa, incluido el número cuando cambia o al hacer
seek.

== Cotas y ángulos

Medidas técnicas estáticas o que siguen a sus extremos.

#api-entry(
  name: "Mechanics.dimension",
  kind: "factory",
  desc: [Cota estática entre `(x1, y1)` y `(x2, y2)`, desplazada `offset` en perpendicular, con líneas de extensión de 0.02 unidades y las puntas de `double_arrow`. Para una cota que siga a objetos móviles, usa `dimension_between`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
dim = scene.mechanics.dimension(-3, 0, 3, 0, 0.6).fill(WHITE)
```
]

#api-entry(
  name: "Mechanics.dimension_between",
  kind: "factory",
  params: ((name: "from_ / to", type: "Endpoint", default: none, desc: [Extremos medidos.]), (name: "offset", type: "float", default: none, desc: [Desplazamiento perpendicular. Sin `side`, positivo es a la izquierda de `from_` → `to`; con `side`, solo cuenta su magnitud.]), (name: "side", type: "str | None", default: "None", desc: [`left`, `right`, `above` o `below`: mantiene la cota en ese lado de la escena aunque los extremos se crucen.]), (name: "label", type: "str | None", default: "None", desc: [Texto simbólico o matemática en línea.]), (name: "show_value", type: "bool", default: "False", desc: [Muestra la distancia actual multiplicada por `scale`.]), (name: "value", type: "float | Parameter | Variable | Computed | None", default: "None", desc: [Lectura numérica propia; implica `show_value` y sustituye la distancia medida y `scale`.]), (name: "format / unit / scale", type: "str / str | None / float", default: "\".2f\" / None / 1", desc: [Formato, unidad y factor de escena a unidades mostradas.]), (name: "label_gap / label_orientation", type: "float / str", default: "0.1 / \"upright\"", desc: [Separación de la anotación y orientación `upright` o `aligned` (nunca boca abajo).]), (name: "line_width", type: "float", default: "0.03", desc: [Ancho de la línea; las puntas miden seis anchos, limitadas en cotas cortas.]), (name: "extension_style / dash_length / gap_length", type: "str / float / float", default: "\"solid\" / 0.12 / 0.08", desc: [Líneas de extensión `solid` o `dashed`.]), (name: "font / weight / label_style", type: "str / int / TextStyle | None", default: "None", desc: [Tipografía de la anotación; `font`, `weight` y `font_size` sustituyen a `label_style`.])),
  desc: [Cota reactiva con líneas de extensión, puntas sólidas y anotación sincronizadas con los extremos. `value` solo controla el número: cambiarlo nunca cambia la longitud. Tipos, métricas, estilos u orientaciones inválidos lanzan `TypeError` o `ValueError`; un resultado reactivo no finito muestra el marcador de valor inválido.],
)[
```python
# show-code: true
from gaanim import Anchor, BLACK, WHITE, Scene
scene = Scene(frame=(16, 9), background=WHITE)
frame = scene.geometry.rect(4.5, 2).move_to(0, 0)
physical_width = scene.viz.parameter(2.5)
dim = scene.mechanics.dimension_between(
  frame.anchor_point(Anchor.TOP_LEFT),
  frame.anchor_point(Anchor.TOP_RIGHT),
  0.6, label="$W_f$", value=physical_width, unit="m", color=BLACK,
  extension_style="dashed", line_width=0.03, dash_length=0.12, gap_length=0.08,
)
height = scene.mechanics.dimension_between(
  frame.anchor_point(Anchor.TOP_RIGHT),
  frame.anchor_point(Anchor.BOTTOM_RIGHT),
  0.6, side="right", show_value=True, unit="m", color=BLACK,
  font="DejaVu Sans Mono", weight=700,
)
scene.play([
  dim.animate.fade_in().duration(0.3),
  height.animate.fade_in().duration(0.3),
  physical_width.animate.set(4.0).duration(0.9),
])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Dimension.line / extensions / label / number / unit",
  kind: "property",
  signature: "line · extensions · label · number · unit",
  desc: [Tipo que devuelve `dimension_between`. Sus partes son `Drawable` que se estilizan por separado; `label`, `number` y `unit` valen `None` si no se pidieron.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
dim = scene.mechanics.dimension_between((-2, 0), (2, 0), 0.5, label="$L$", show_value=True)
dim.extensions.opacity(0.4)
dim.number.fill(GOLD)
```
]

#api-entry(
  name: "Mechanics.angle_between",
  kind: "factory",
  params: ((name: "vertex", type: "Endpoint", default: none, desc: [Vértice del ángulo.]), (name: "from_ / to", type: "Direction | Endpoint", default: none, desc: [Rayos como direcciones fijas o extremos.]), (name: "radius", type: "float", default: "0.64", desc: [Radio del arco.]), (name: "sweep", type: "str", default: "\"minor\"", desc: [`minor`, `major`, `cw` o `ccw`.]), (name: "arrowheads", type: "str", default: "\"both\"", desc: [`none`, `start`, `end` o `both`.]), (name: "unit", type: "str", default: "\"deg\"", desc: [`deg` o `rad` para el valor mostrado.]), (name: "show_extensions", type: "bool", default: "True", desc: [Dibuja los rayos.])),
  desc: [Cota angular reactiva. Un rayo de longitud cero oculta la geometría afectada en lugar de emitir caminos inválidos.],
)[
```python
# show-code: true
from gaanim import Direction, GOLD, Scene
scene = Scene(frame=(16, 9))
bob = scene.geometry.dot(0.125).move_to(1, -1.125)
theta = scene.mechanics.angle_between((0,0), Direction.DOWN, bob, label="$theta$", show_value=True, color=GOLD)
scene.play([theta.animate.fade_in(), bob.animate.shift_by(1.125, 0.44).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "AngleDimension.arc / arrows / extensions / label / number / unit",
  kind: "property",
  signature: "arc · arrows · extensions · label · number · unit",
  desc: [Tipo que devuelve `angle_between`, con partes que se estilizan por separado.],
  none,
)

== Barras y muelles

Elementos que unen dos extremos y cambian de longitud con ellos.

#api-entry(
  name: "Mechanics.bar_between",
  kind: "factory",
  desc: [Barra de extremos redondeados con longitud y ángulo reactivos; `width` en unidades de escena. Dibuja las articulaciones aparte si las necesitas.],
)[
```python
# show-code: true
from gaanim import Anchor, BLACK, Scene
scene = Scene(frame=(16, 9))
body = scene.geometry.rect(1.875, 0.875).move_to(0.5, -0.25)
bar = scene.mechanics.bar_between((-1.875, 1.25), body.anchor_point(Anchor.TOP_LEFT), width=0.11).stroke(BLACK, 0.11)
scene.play([bar.animate.fade_in(), body.animate.shift_by(1, 0).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.spring_between",
  kind: "factory",
  params: ((name: "from_ / to", type: "Endpoint", default: none, desc: [Extremos.]), (name: "coils", type: "int", default: "8", desc: [Número de vueltas.]), (name: "amplitude", type: "float", default: "0.12", desc: [Radio de la hélice, perpendicular al eje.]), (name: "crossing", type: "float", default: "0", desc: [De 0 a 1: pliega cada vuelta para crear cruces en forma de «e».]), (name: "start_straight / end_straight", type: "float", default: "0.12 / 0.12", desc: [Tramos rectos no negativos en cada extremo; se acortan si los extremos están muy cerca.])),
  desc: [Muelle helicoidal oculto que conserva su radio mientras su paso se deforma con la distancia. Revélalo con una animación de entrada. Tramos negativos o no finitos lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
mass = scene.geometry.dot(0.125).fill(GOLD).move_to(0.875, 0)
spring = scene.mechanics.spring_between(( -0.875, 0), mass, coils=6, amplitude=0.175, crossing=1.0, start_straight=0.225, end_straight=0.225).no_fill().stroke(WHITE, 0.04)
scene.play([spring.animate.fade_in().duration(0.3), mass.animate.shift_by(0.5, 0).duration(1.0)])
# output: preview.webp
scene.render()
```
]

== Fuerzas y momentos

Vectores con punta sólida y lectura opcional, en unidades físicas.

#api-entry(
  name: "Mechanics.vector_between",
  kind: "factory",
  desc: [Vector reactivo entre dos extremos con asta, punta sólida y lectura opcional de su magnitud (`show_value`, `format`, `unit`, `scale`).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
force = scene.mechanics.vector_between((-1.25, 0), (0.875, 0.56), label="$F$", color=GOLD)
```
]

#api-entry(
  name: "Mechanics.force_at",
  kind: "factory",
  params: ((name: "origin", type: "Endpoint", default: none, desc: [Punto de aplicación.]), (name: "magnitude", type: "float | fuente reactiva", default: none, desc: [Magnitud física.]), (name: "direction", type: "float | fuente reactiva", default: "0.0", desc: [Dirección en radianes.]), (name: "visual_scale", type: "float", default: "1.0", desc: [Unidades de escena por unidad física; positiva.]), (name: "unit", type: "str", default: "\"N\"", desc: [Unidad de la lectura.])),
  desc: [Fuerza a partir de magnitud y dirección. La lectura opcional informa la magnitud física, no la longitud dibujada.],
)[
```python
# show-code: true
from gaanim import GREEN, Scene
scene = Scene(frame=(16, 9))
body = scene.geometry.circle(0.3)
magnitude = scene.viz.parameter(30)
force = scene.mechanics.force_at(
  body.anchor_point(), magnitude, direction=0.5, visual_scale=0.025,
  label="$F$", show_value=True, unit="N", color=GREEN,
)
scene.play([body.animate.fade_in(), force.animate.fade_in(), magnitude.animate.set(80).duration(1.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.force_from_components",
  kind: "factory",
  desc: [Fuerza a partir de sus componentes físicas X e Y respecto a un origen móvil. Combínala con `Parameter.add_updater_fn` para simulaciones: las simulaciones de paso fijo se reconstruyen antes, así que una fuerza derivada del cuerpo simulado ve el estado del mismo fotograma.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
body = scene.geometry.circle(0.3)
fx = scene.viz.parameter(20)
weight = scene.mechanics.force_from_components(body, fx, -40, visual_scale=0.02, label="$W$")
```
]

#api-entry(
  name: "ForceVector.shaft / head / label / number / unit",
  kind: "property",
  signature: "shaft · head · label · number · unit",
  desc: [Tipo que devuelven `vector_between`, `force_at` y `force_from_components`, con partes que se estilizan por separado.],
  none,
)

#api-entry(
  name: "Mechanics.moment_about",
  kind: "factory",
  desc: [Flecha curva de momento que sigue a un centro reactivo; `direction` es `"cw"` o `"ccw"`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
moment = scene.mechanics.moment_about((1.5, -0.5), radius=0.525, label="$M$")
```
]

#api-entry(
  name: "Mechanics.coordinate_frame_at",
  kind: "factory",
  desc: [Sistema de ejes 2D ortogonal y reactivo en un extremo, con el eje X en `x_direction` y etiquetas opcionales.],
)[
```python
# show-code: true
from gaanim import Direction, GOLD, Scene
scene = Scene(frame=(16, 9))
force = scene.mechanics.vector_between((-1.25, 0), (0.875, 0.56), label="$F$", color=GOLD)
moment = scene.mechanics.moment_about((1.5, -0.5), radius=0.525, label="$M$")
frame = scene.mechanics.coordinate_frame_at((0, -1), Direction.RIGHT, labels=("$e_1$", "$e_2$"))
scene.play([force.animate.fade_in(), moment.animate.fade_in(), frame.animate.fade_in()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.contact_on_curve",
  kind: "factory",
  desc: [Agrupa un punto de contacto, su tangente y su normal sobre una curva muestreada, conducidos por un `Parameter` o `Variable` de 0 a 1.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
t = scene.viz.parameter(0.1)
track = scene.geometry.polyline([(x / 10, 0.5 * math.sin(x / 10)) for x in range(-40, 41)]).no_fill()
contact = scene.mechanics.contact_on_curve(track, t, tangent_length=1.0, normal_length=0.6)
scene.play([contact.animate.fade_in(), t.animate.set(0.9).duration(1.5)])
```
]

== Apoyos y articulaciones

Símbolos de apoyo con base rayada y juntas, que siguen a su punto de conexión.

#api-entry(
  name: "Mechanics.support_at",
  kind: "factory",
  params: ((name: "point", type: "Endpoint", default: none, desc: [Punto de conexión.]), (name: "kind", type: "str", default: "\"pin\"", desc: [`fixed`, `pin`, `roller`, `simple`, `guided`, `prismatic`, `cable` o `spring`.]), (name: "direction", type: "Direction | None", default: "None", desc: [De la base hacia la conexión: `UP` pone el suelo debajo y `DOWN` crea un apoyo de techo.]), (name: "size / ground_length", type: "float", default: "0.48 / 0.70", desc: [Medidas en unidades de escena.])),
  desc: [Apoyo vectorial que respeta el tema. Los atajos `fixed_support`, `pin_support`, `roller_support` y `guided_support` fijan `kind`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
anchor = scene.mechanics.support_at((0, 2), kind="cable", direction=Direction.DOWN)
```
]

#api-entry(
  name: "Mechanics.pin_support",
  kind: "factory",
  desc: [Apoyo triangular articulado con junta circular.],
)[
```python
# show-code: true
from gaanim import Direction, Scene
scene = Scene(frame=(16, 9))
pin = scene.mechanics.pin_support((-1.25, 0), direction=Direction.UP)
roller = scene.mechanics.roller_support((1.25, 0), direction=Direction.UP)
scene.play([pin.animate.fade_in(), roller.animate.fade_in()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.roller_support",
  kind: "factory",
  desc: [Apoyo triangular sobre dos rodillos alineados.],
  none,
)

#api-entry(
  name: "Mechanics.fixed_support",
  kind: "factory",
  desc: [Empotramiento (o apoyo de techo) con placa y rayado.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
wall = scene.mechanics.fixed_support((-4, 0), direction=Direction.RIGHT)
```
]

#api-entry(
  name: "Mechanics.guided_support",
  kind: "factory",
  desc: [Carro guiado alineado con `direction`.],
  none,
)

#api-entry(
  name: "Support.joint / body / ground / rollers / guides / hatching",
  kind: "property",
  signature: "joint · body · ground · rollers · guides · hatching",
  desc: [Tipo que devuelven los apoyos, con partes vectoriales que se estilizan por separado.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
base = scene.mechanics.pin_support((0, 0))
base.hatching.opacity(0.5)
```
]

#api-entry(
  name: "Mechanics.joint_at",
  kind: "factory",
  desc: [Símbolo independiente de junta de revolución (`kind="revolute"`) o prismática (`"prismatic"`, alineada con `axis`) que sigue a su punto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
hinge = scene.mechanics.joint_at((0, 0))
slider = scene.mechanics.joint_at((2, 0), kind="prismatic", axis=Direction.RIGHT)
```
]

== Transmisiones

Engranajes, cremalleras y levas editoriales. Son relaciones visuales, no un solver
cinemático ni de contacto; los dientes son esquemáticos.

#api-entry(
  name: "Mechanics.gear",
  kind: "factory",
  desc: [Silueta de engranaje de radio `radius` con `teeth` dientes y agujero central. Acóplalos con `Drawable.bind_rotation_from(source, ratio=..., phase=...)`.],
)[
```python
# show-code: true
from gaanim import Direction, Scene
scene = Scene(frame=(16, 9))
driver = scene.mechanics.gear(0.69, 20).move_to(-0.69, 0.25)
driven = scene.mechanics.gear(0.41, 12).move_to(0.41, 0.25).bind_rotation_from(driver, ratio=-5/3)
rack = scene.mechanics.rack(2.75, 18).move_to(0, -0.81).bind_translation_from_rotation(
  driver, axis=Direction.RIGHT, scale=0.69,
)
scene.play([driver.animate.fade_in(), driven.animate.fade_in(), rack.animate.fade_in(), driver.animate.rotate_by(2.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.rack",
  kind: "factory",
  desc: [Cremallera recta de `length` con `teeth` dientes equiespaciados. Muévela con `Drawable.bind_translation_from_rotation`.],
  none,
)

#api-entry(
  name: "Mechanics.cam_profile",
  kind: "factory",
  desc: [Leva radial cerrada a partir de muestras `(ángulo_en_radianes, radio)`.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
samples = [(2 * math.pi * i / 64, 1 + 0.3 * math.cos(2 * math.pi * i / 64)) for i in range(64)]
cam = scene.mechanics.cam_profile(samples)
scene.play([cam.animate.rotate_by(math.tau).duration(2)])
```
]
