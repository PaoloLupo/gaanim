#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Geometría",
  description: "Fábricas de scene.geometry: primitivas, líneas, flechas, trayectorias, grupos, booleanas, geometría reactiva y 3D",
  route: "/referencia/geometria/",
  nav: "Geometría",
)

= Geometría

`scene.geometry` crea las formas vectoriales de la escena. Cada fábrica
devuelve un #link("/referencia/drawable/")[`Drawable`] centrado en el origen,
listo para encadenar estilo, posición y animación. Las medidas están en
unidades de escena: el marco lógico mide 16 × 9.

```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
disk = scene.geometry.circle(1).fill(BLUE).move_to(-3, 0)
card = scene.geometry.rounded_rect(2.4, 1.4, 0.2).fill(GOLD).move_to(0, 0)
star = scene.geometry.star(5, 1, 0.45).no_fill().stroke(WHITE, 0.05).move_to(3, 0)
scene.play([disk.animate.grow_from_center(), card.animate.create(), star.animate.create()], duration=1.2)
# output: preview.webp
scene.render()
```

== Primitivas

Círculos, rectángulos, elipses y puntos: la base de casi cualquier diagrama.

#api-entry(
  name: "Geometry.circle",
  kind: "factory",
  params: ((name: "radius", type: "float", default: none, desc: [Radio en unidades de escena.]),),
  desc: [Círculo centrado en el origen. Sirve para nodos, marcadores y diagramas radiales; combínalo con `annulus` para anillos.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
node = scene.geometry.circle(0.625).fill(BLUE).move_to(0, 0)
scene.play([node.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.rect",
  kind: "factory",
  desc: [Rectángulo de `width` × `height` centrado en el origen. Úsalo para tarjetas, paneles y barras; `rounded_rect` da esquinas suaves.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
card = scene.geometry.rect(2, 1.125).fill(BLUE).stroke(WHITE, 0.025).move_to(0, 0)
scene.play([card.animate.grow_from_center().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.rounded_rect",
  kind: "factory",
  desc: [Rectángulo con esquinas redondeadas de radio `radius`. Útil para botones, etiquetas y tarjetas.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
btn = scene.geometry.rounded_rect(2, 0.625, 0.15).fill(GOLD).move_to(0, 0)
label = scene.text("PULSA").move_to(0, 0)
scene.play([scene.geometry.group([btn, label]).animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.square",
  kind: "factory",
  desc: [Atajo de `rect(s, s)`. Útil para celdas de rejilla.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
cell = scene.geometry.square(1).fill(BLUE)
```
]

#api-entry(
  name: "Geometry.ellipse",
  kind: "factory",
  desc: [Elipse con radios `rx` y `ry`. Sirve para órbitas, resaltados y diagramas de Venn.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
orbit = scene.geometry.ellipse(1.125, 0.625).no_fill().stroke(GOLD, 0.025).move_to(0, 0)
scene.play([orbit.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.dot",
  kind: "factory",
  params: ((name: "radius", type: "float", default: none, desc: [Radio en unidades de escena; suele estar entre 0.06 y 0.15.]),),
  desc: [Círculo pequeño relleno para marcadores, viñetas y partículas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
marker = scene.geometry.dot(0.1).fill(RED).move_to(1, 0.5)
```
]

#api-entry(
  name: "Geometry.points",
  kind: "factory",
  params: ((name: "positions", type: "Sequence[tuple[float, float]]", default: none, desc: [Centros en coordenadas de escena; al menos uno y todos finitos.]), (name: "radius", type: "float", default: "0.06", desc: [Radio de cada punto, mayor que cero.])),
  desc: [Nube de puntos sin `Cartesian2D`: todos los círculos comparten un camino, así que miles de puntos (una sección de Poincaré, una muestra) siguen siendo un único objeto para `fill`, `opacity`, el layout y la animación. Listas vacías, posiciones no finitas o un radio no positivo lanzan `ValueError`.],
)[
```python
# show-code: true
import math
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
spiral = [(0.02 * i * math.cos(0.2 * i), 0.02 * i * math.sin(0.2 * i)) for i in range(200)]
cloud = scene.geometry.points(spiral, radius=0.04).fill(GOLD)
scene.play([cloud.animate.fade_in().duration(0.5)])
# output: preview.webp
scene.render()
```
]

== Líneas y flechas

Segmentos, flechas rectas y curvas, arcos y conectores. Los extremos
reactivos (`Endpoint`) aceptan tuplas, objetos, `AnchorPoint` y `PointRef`.

#api-entry(
  name: "Geometry.line",
  kind: "factory",
  signature: "line(p1: Endpoint, p2: Endpoint) | line(x1, y1, x2, y2) | line(*, length, direction=Direction.RIGHT) -> Drawable",
  params: ((name: "p1 / p2", type: "Endpoint", default: none, desc: [Extremos fijos o reactivos.]), (name: "x1, y1, x2, y2", type: "float", default: none, desc: [Forma compatible con cuatro coordenadas fijas.]), (name: "length", type: "float", default: none, desc: [Longitud positiva de una línea centrada en el origen.]), (name: "direction", type: "Direction", default: "Direction.RIGHT", desc: [Orientación con `length`; admite diagonales y vectores 2D no nulos.])),
  desc: [Dos tuplas crean una línea normal que se mueve con su grupo. Los extremos que son referencias se resuelven en el espacio del mundo en cada fotograma, así que la línea sigue a los objetos. Con `length`, `next_to` coloca la línea sin cambiar su longitud ni su orientación. Mezclar `length` con extremos lanza `TypeError`; longitudes o direcciones inválidas lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Anchor, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
left = scene.geometry.dot(0.11).fill(GOLD).move_to(-1.5, -0.375)
card = scene.geometry.rect(1.5, 0.875).move_to(1.125, 0.44)
connector = scene.geometry.line(
    left.anchor_point(Anchor.RIGHT),
    card.anchor_point(Anchor.LEFT),
).stroke(WHITE, 0.04)
scene.play([left.animate.shift_by(0.375, 0.75).duration(0.7)])
# output: preview.webp
scene.render()
```

Con `length`, la línea es independiente de las coordenadas:

```python
# show-code: true
from gaanim import Direction, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
title = scene.text("Una línea bajo el título", size=0.5).fill(WHITE)
rule = scene.geometry.line(length=5).stroke(GOLD, 0.035).next_to(
    title, Direction.DOWN, spacing=0.25,
)
scene.play([title.animate.fade_in(), rule.animate.create()], duration=1)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.arrow",
  kind: "factory",
  params: ((name: "x1, y1", type: "float", default: none, desc: [Cola.]), (name: "x2, y2", type: "float", default: none, desc: [Punta.]), (name: "head_length / head_width / body_width", type: "float | None", default: "None", desc: [Medidas en unidades de escena; por defecto 0.18, 0.15 y 0.036.]), (name: "max_head_ratio", type: "float | None", default: "None", desc: [En `(0, 1]`, limita la punta respecto a la longitud total y escala su ancho en proporción.])),
  desc: [Flecha sólida de `(x1, y1)` a `(x2, y2)`. Para flechas cortas añade `max_head_ratio=0.3`, y `.no_stroke()` para una silueta limpia. Extremos no finitos o medidas no positivas lanzan `ValueError`; extremos coincidentes dan un camino vacío.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
arrow = scene.geometry.arrow(-3, 0, 3, 0).fill(GOLD).no_stroke()
scene.play([arrow.animate.create().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.double_arrow",
  kind: "factory",
  desc: [Flecha con punta en ambos extremos, para rangos y relaciones bidireccionales. Las puntas miden 0.18 × 0.15 por defecto, con cuerpo de 0.036, y se acortan para no solaparse.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
span = scene.geometry.double_arrow(-3, 0, 3, 0).fill(WHITE).no_stroke()
```
]

#api-entry(
  name: "Geometry.curved_arrow",
  kind: "factory",
  params: ((name: "x1, y1, x2, y2", type: "float", default: none, desc: [Inicio y final.]), (name: "angle", type: "float", default: none, desc: [Desviación en radianes; el signo elige el lado de la curva.]), (name: "head_length / head_width / body_width / max_head_ratio", type: "float | None", default: "None", desc: [Igual que en `arrow`; la proporción se mide sobre la longitud del arco.])),
  desc: [Flecha curva para ciclos y rotaciones. Comparte medidas y validación con `arrow`, así que flechas rectas y curvas pueden compartir estilo. La punta se acorta en arcos cortos y se estrecha en radios pequeños.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
loop = scene.geometry.curved_arrow(-3, 0, 3, 0, 0.9).fill(WHITE)
bold = scene.geometry.curved_arrow(
    -3, -1.5, 3, -1.5, -0.9, head_length=0.3, head_width=0.24, body_width=0.06,
).fill(GOLD)
scene.play([loop.animate.create().duration(0.9), bold.animate.create().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.curved_arrow_arc",
  kind: "factory",
  desc: [Flecha a lo largo de un arco circular con centro `(cx, cy)`, radio y ángulos en radianes. Acepta las mismas medidas que `curved_arrow`; `max_head_ratio` se mide sobre `radius * abs(sweep_angle)`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
turn = scene.geometry.curved_arrow_arc(0, 0, 2.5, 0.2, 1.8).fill(GOLD)
```
]

#api-entry(
  name: "Geometry.arc",
  kind: "factory",
  desc: [Arco de circunferencia con centro `(cx, cy)`; `start_angle` y `sweep_angle` en radianes. Usa `no_fill().stroke(...)` para verlo como contorno.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
arc = scene.geometry.arc(0, 0, 0.75, 0.0, 2.0).no_fill().stroke(GOLD, 0.05)
scene.play([arc.animate.create().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.dashed_line",
  kind: "factory",
  desc: [Línea discontinua para guías y aristas ocultas. `create()` dibuja los guiones uno tras otro desde el inicio.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
guide = scene.geometry.dashed_line(-1.75, 0, 1.75, 0, dash_length=0.15, gap_length=0.1).stroke(WHITE, 0.025)
scene.play([guide.animate.create().duration(0.8)])
```
]

#api-entry(
  name: "Geometry.connector",
  kind: "factory",
  params: ((name: "start / end", type: "Endpoint", default: none, desc: [Cola y punta, fijas o reactivas.]), (name: "via", type: "Sequence[Endpoint] | None", default: "None", desc: [Puntos intermedios en orden.]), (name: "head_length / head_width / body_width", type: "float", default: "0.18 / 0.15 / 0.036", desc: [Medidas positivas en unidades de escena.])),
  desc: [Flecha rellena y reactiva que sigue a sus extremos durante la animación y el seek. Usa `via` para los codos: no esquiva obstáculos. La punta se limita al último segmento no nulo. Referencias de otra escena lanzan `ValueError`. Anima el conector completo con `create()`.],
)[
```python
from gaanim import Anchor, Scene
scene = Scene(frame=(16, 9))
left = scene.geometry.rect(2, 1).move_to(-3, 0)
right = scene.geometry.rect(2, 1).move_to(3, 0)
link = scene.geometry.connector(left.anchor_point(Anchor.RIGHT), right.anchor_point(Anchor.LEFT))
scene.play(link.animate.create())
scene.play(right.animate.shift_by(0, 1))
scene.render()
```
]

== Polígonos y símbolos

Polígonos arbitrarios y regulares, sectores y marcas habituales en diagramas y
demostraciones.

#api-entry(
  name: "Geometry.polygon",
  kind: "factory",
  params: ((name: "points", type: "Sequence[tuple[float, float]]", default: none, desc: [Al menos tres puntos finitos; el polígono se cierra solo.]),),
  desc: [Polígono cerrado con vértices arbitrarios.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tri = scene.geometry.polygon([(0, 0.875), (-0.81, -0.625), (0.81, -0.625)]).fill(BLUE).stroke(WHITE, 0.025)
scene.play([tri.animate.grow_from_center().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.regular_polygon",
  kind: "factory",
  desc: [Polígono regular de `sides` lados (al menos 3) inscrito en un círculo de radio `radius`. Con 6 lados obtienes un hexágono.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
hexa = scene.geometry.regular_polygon(6, 0.75).fill(BLUE).stroke(WHITE, 0.025)
scene.play([hexa.animate.spin_in_from_nothing().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.star",
  kind: "factory",
  desc: [Estrella de `points` puntas (al menos 2) con radios exterior e interior.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
star = scene.geometry.star(5, 0.875, 0.4).fill(GOLD).move_to(0, 0)
scene.play([star.animate.spin_in_from_nothing().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.sector",
  kind: "factory",
  desc: [Porción de círculo con centro `(cx, cy)`; ángulos en radianes. Sirve para gráficos de tarta y cuñas de progreso.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
slice_ = scene.geometry.sector(0, 0, 0.875, 0.0, 2.0).fill(GOLD)
```
]

#api-entry(
  name: "Geometry.annulus",
  kind: "factory",
  desc: [Anillo entre `inner_radius` (mayor que cero) y `outer_radius` (mayor que el interior). Útil para donuts, halos y dianas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ring = scene.geometry.annulus(0.75, 0.425).fill(BLUE).stroke(WHITE, 0.025)
```
]

#api-entry(
  name: "Geometry.brace",
  kind: "factory",
  desc: [Llave entre `(x1, y1)` y `(x2, y2)` con profundidad `height` distinta de cero; el signo elige el lado. Para una llave bajo una parte de un texto, usa `selection.animate.brace(...)` (ver #link("/referencia/text/")[Texto]).],
)[
```python
# show-code: true
from gaanim import WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
brace = scene.geometry.brace(-1, -0.25, 1, -0.25, 0.3).stroke(WHITE, 0.04).no_fill()
label = scene.text("intervalo").move_to(0, -0.69)
scene.play([brace.animate.create().duration(0.7), label.animate.fade_in().duration(0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.checkmark",
  kind: "factory",
  desc: [Marca de verificación de tamaño `size`. Combínala con `cross` para respuestas correctas e incorrectas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ok = scene.geometry.checkmark(0.425).fill(GREEN)
```
]

#api-entry(
  name: "Geometry.cross",
  kind: "factory",
  desc: [Aspa de tamaño `size` para errores, rechazos o botones de cierre.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
no = scene.geometry.cross(0.425).stroke(WHITE, 0.05)
```
]

#api-entry(
  name: "Geometry.right_angle",
  kind: "factory",
  desc: [Marca de ángulo recto con brazos de `arm_length`. Colócala en la esquina de un triángulo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
corner = scene.geometry.right_angle(0.3).stroke(WHITE, 0.04).move_to(0, 0)
```
]

== Trayectorias

Caminos libres: polilíneas, curvas de Bézier y comandos de cursor.

#api-entry(
  name: "Geometry.path",
  kind: "factory",
  signature: "path(definition: Sequence[CurvePoint] | Sequence[CurveCommand]) -> Drawable",
  desc: [Entrada compacta para geometría propia: una lista de puntos crea una polilínea y una lista de comandos, una curva (como `curve`).],
)[
```python
# show-code: true
from gaanim import WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
rail = scene.geometry.path([(-1.75, 0), (0, 0.75), (1.75, 0)]).no_fill().stroke(WHITE, 0.05)
scene.play([rail.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.polyline",
  kind: "factory",
  desc: [Polilínea abierta que une al menos dos puntos en orden.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
zig = scene.geometry.polyline([(-1.25, -0.375), (0, 0.375), (1.25, -0.375)]).no_fill().stroke(GOLD, 0.04)
```
]

#api-entry(
  name: "Geometry.bezier",
  kind: "factory",
  params: ((name: "start / end", type: "tuple[float, float]", default: none, desc: [Extremos.]), (name: "controls", type: "Sequence[tuple[float, float]]", default: none, desc: [Uno para una curva cuadrática, dos para una cúbica.])),
  desc: [Curva de Bézier. Se conserva como Bézier, lo que la hace útil con la geometría reactiva sobre curvas.],
)[
```python
# show-code: true
from gaanim import WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
curve = scene.geometry.bezier((-1.75, 0), [(-0.625, 1.125), (0.625, -1.125)], (1.75, 0)).no_fill().stroke(WHITE, 0.04)
scene.play([curve.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.curve",
  kind: "factory",
  params: ((name: "commands", type: "Sequence[CurveCommand]", default: none, desc: [Pares `(nombre, puntos)`: `move`, `line`, `quad`, `cubic`, `close` y `close_smooth`. Añade `_rel` a un comando de dibujo para usar puntos relativos al cursor.]),),
  desc: [Curva compuesta con comandos de cursor, al estilo de Typst. Los controles de `quad` y `cubic` aceptan un punto, `None` o `"auto"` para tangentes automáticas. `close` y `close_smooth` llevan una lista vacía.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
shape = scene.geometry.curve([
    ("move", [(0, 0)]),
    ("cubic", [(0.625, 0.75), (1.375, -0.75), (2, 0)]),
    ("line_rel", [(0, -1)]),
    ("close", []),
]).no_fill().stroke(WHITE, 0.04)
```
]

== Grupos y marcos

Agrupa objetos para moverlos como uno o dibuja un marco vivo a su alrededor.

#api-entry(
  name: "Geometry.group",
  kind: "factory",
  desc: [Agrupa objetos para moverlos, rotarlos y escalarlos como uno, conservando las coordenadas locales de cada miembro (también las de sus updaters). Los miembros ya visibles no se ocultan al unirse a un grupo que contiene un trazo o una fuerza diferidos. `write`, `create` y los fundidos del grupo son entradas explícitas de todo el subárbol. Para distribuir miembros usa #link("/referencia/layout/")[Layout].],
)[
```python
# show-code: true
from gaanim import BLUE, Direction, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
row = scene.geometry.group([scene.geometry.dot(0.125).fill(BLUE), scene.text("Etiqueta").move_to(0.9, 0)]).move_to(0, 0)
scene.play([row.animate.fade_in_from(Direction.DOWN, distance=0.3).duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.surrounding_rect",
  kind: "factory",
  params: ((name: "targets", type: "Drawable | TextSelection | Sequence", default: none, desc: [Uno o varios objetos, partes de texto o partes de ecuación.]), (name: "padding", type: "float | (v, h) | (t, r, b, l)", default: "0.12", desc: [Margen no negativo alrededor de la unión.]), (name: "corner_radius", type: "float", default: "0.08", desc: [Radio no negativo, limitado al tamaño del marco.])),
  desc: [Marco vivo alrededor de los límites de sus objetivos en el mundo: los sigue al moverse, escalarse, rotar o reordenarse en un layout. Usa el trazo del tema y ningún relleno. Su posición y geometría pertenecen al vínculo: anima los objetivos o cambia de objetivo con `retarget`. Objetivos vacíos, de otra escena o medidas inválidas lanzan `TypeError` o `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
eq = scene.text.equation("E =", part("mass", "m"), part("light", "c^2"))
frame = scene.geometry.surrounding_rect(eq["mass"]).stroke(GOLD, 0.04)
scene.play([eq.animate.fade_in(), frame.animate.create()])
scene.play([frame.retarget(eq["light"]).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SurroundingRect.retarget",
  kind: "method",
  desc: [Devuelve un `Anim` que interpola los cuatro bordes del marco hacia nuevos objetivos y después los sigue. Origen y destino pueden seguir moviéndose durante la transición; los seeks reproducen la misma geometría. Configura el tiempo con `duration` y `easing`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
equation = scene.text("$", part("lhs", "x + 3"), " = ", part("result", "7"), "$")
frame = scene.geometry.surrounding_rect(equation["lhs"])
scene.play([frame.retarget(equation["result"]).duration(0.9).easing(Easing.spring(stiffness=90, damping=12))])
```
]

== Operaciones booleanas

Unión, intersección, diferencia y diferencia simétrica de áreas vectoriales
cerradas.

Los operandos se conservan como objetos independientes y el resultado hereda el
estilo del primero. Con `live=True`, el resultado se reconstruye cuando cambian
los caminos o las transformaciones de sus fuentes. `tolerance` es el error
máximo, en unidades de escena, al aproximar las curvas; el valor predeterminado
(`0.0025`, unos 0.3 px a 1080p) mantiene suaves los círculos. Súbelo solo para
acelerar resultados `live=True` muy complejos. Debe ser finita y positiva. `rule` acepta `"nonzero"` o
`"evenodd"`; operandos de otra escena o valores inválidos lanzan `ValueError`.

#api-entry(
  name: "Geometry.union",
  kind: "factory",
  desc: [Unión de al menos dos áreas.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
left = scene.geometry.circle(1).move_to(-0.5, 0)
right = scene.geometry.circle(1).move_to(0.5, 0)
merged = scene.geometry.union(left, right).fill(BLUE)
left.opacity(0)
right.opacity(0)
scene.play([merged.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.intersection",
  kind: "factory",
  desc: [Área común de al menos dos objetos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>left = scene.geometry.circle(1).move_to(-0.5, 0)
>>>right = scene.geometry.circle(1).move_to(0.5, 0)
lens = scene.geometry.intersection(left, right).fill(GOLD)
```
]

#api-entry(
  name: "Geometry.difference",
  kind: "factory",
  desc: [Resta cada recorte del sujeto, de izquierda a derecha.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
plate = scene.geometry.rect(3, 2).fill(GOLD)
hole = scene.geometry.circle(0.6).fill(WHITE).move_to(-0.8, 0)
cut = scene.geometry.difference(plate, hole, live=True)
plate.opacity(0)
hole.opacity(0.2)
scene.play([hole.animate.shift_by(1.6, 0).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.xor",
  kind: "factory",
  desc: [Diferencia simétrica: las áreas cubiertas por un número impar de operandos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>left = scene.geometry.circle(1).move_to(-0.5, 0)
>>>right = scene.geometry.circle(1).move_to(0.5, 0)
either = scene.geometry.xor(left, right, rule="evenodd")
```
]

== Relleno por nivel

Llena una silueta hasta un porcentaje, como un depósito de agua o una barra de
progreso con forma.

#api-entry(
  name: "Geometry.fill_level",
  kind: "factory",
  params: ((name: "mask", type: "Drawable", default: none, desc: [Silueta que recorta el relleno; conserva su visibilidad propia.]), (name: "paint", type: "Paint", default: none, desc: [Color o pincel del interior.]), (name: "level", type: "float | Parameter | Variable | Computed | TimeInput | None", default: "None", desc: [Nivel en `[0, 1]`; `None` empieza vacío.]), (name: "direction", type: "str", default: "\"up\"", desc: [`up`, `down`, `left` o `right`.]), (name: "keep_outline", type: "bool", default: "True", desc: [Añade una copia reactiva que solo usa el trazo de la máscara.])),
  desc: [Genera un interior vectorial que se intersecta con la silueta en cada fotograma. Oculta la máscara original si su relleno no debe tapar el nivel. Con una fuente reactiva, las muestras finitas se limitan a `[0, 1]`; una muestra no finita genera un diagnóstico y usa el valor de respaldo. Anima el nivel con `animate.fill_level(...)`; los saltos en la línea de tiempo restauran vínculos, valores y cortes.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
drop = scene.media.svg("drop.svg").scale_to(2.5).no_fill().stroke("#dbeafe", 0.05).opacity(0)
water = scene.geometry.fill_level(drop, "#38bdf8", 0.0)
scene.play([water.animate.fill_level(0.72).duration(1.4)])
```

Con una fuente reactiva, el nivel sigue a un parámetro que también puede
mostrarse:

```python
# continue
amount = scene.viz.parameter(0)
fraction = computed(lambda value: value / 100, inputs=[amount])
water = scene.geometry.fill_level(drop, "#38bdf8", fraction)
label = scene.viz.readout(amount, format=".1f", suffix="%")
scene.play(amount.animate.set(72), duration=1.4)
```
]

#api-entry(
  name: "Drawable.set_fill_level",
  kind: "method",
  desc: [Cambia el nivel de un objeto creado con `fill_level` desde el cursor. Otra fuente reactiva reemplaza el vínculo; un número en `[0, 1]` lo termina con un corte reversible. Mientras esté vinculado, anima la fuente: `animate.fill_level(...)` se rechaza. Valores fijos inválidos, fuentes de otra escena u otros tipos de objeto lanzan `ValueError`.],
)[
```python
# continue
water.set_fill_level(0.4)
```
]

== Geometría reactiva

Relaciones que se recalculan en el mismo fotograma: «este extremo es la esquina
del rectángulo» o «este punto está al 35 % de la curva».

En lugar de recalcular una línea desde Python, declaras la relación y el motor
regenera solo la geometría afectada después de aplicar las transformaciones de
sus fuentes. La relación se evalúa para cualquier instante, así que el seek es
exacto. Los objetos reactivos suelen empezar ocultos: anima su entrada
(`fade_in`, `create` o `write`) junto con lo que los conduce. Las magnitudes
reactivas (`Parameter`, `Variable`, `Computed`) están en
#link("/referencia/visualization/")[Visualización] y la guía
#link("/guias/reactividad/")[Reactividad] enseña a combinarlas.

=== Puntos de referencia

Puntos no dibujados que se aceptan en cualquier lugar donde se admite un
`Endpoint`. Sus coordenadas aceptan números o fuentes reactivas.

#api-entry(
  name: "Geometry.point_ref",
  kind: "factory",
  desc: [Punto de escena cuyas coordenadas pueden ser fuentes reactivas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
x = scene.viz.parameter(-3)
probe = scene.geometry.point_ref(x, 1)
marker = scene.geometry.dot(0.1).fill(GOLD).follow(probe)
scene.play([marker.animate.fade_in(), x.animate.set(3).duration(1.5)])
```
]

#api-entry(
  name: "Geometry.offset_point",
  kind: "factory",
  desc: [Punto desplazado `(dx, dy)` desde un origen móvil, en el espacio de la escena.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
body = scene.geometry.circle(0.4).move_to(-2, 0)
above = scene.geometry.offset_point(body, 0, 0.8)
tag = scene.text("m", role="label").follow(above)
```
]

#api-entry(
  name: "Geometry.point_between",
  kind: "factory",
  desc: [Punto afín entre dos extremos (`alpha=0` en el primero, `1` en el segundo) más un desplazamiento en el mundo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
a = scene.geometry.dot(0.1).move_to(-3, 0)
b = scene.geometry.dot(0.1).move_to(3, 1)
middle = scene.geometry.point_between(a, b, alpha=0.5, offset=(0, 0.3))
label = scene.text("M", role="label").follow(middle)
```
]

#api-entry(
  name: "Geometry.polar_point",
  kind: "factory",
  desc: [Punto a distancia `radius` y ángulo `angle` (radianes) de un origen; ambos pueden ser reactivos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
theta = scene.viz.parameter(0.0)
tip = scene.geometry.polar_point((0, 0), 1.5, theta)
hand = scene.geometry.tracking_line((0, 0), tip).stroke(WHITE, 0.05)
scene.play([hand.animate.create(), theta.animate.set(3.14).duration(1.5)])
```
]

=== Líneas y marcas que siguen a otros objetos

#api-entry(
  name: "Geometry.tracking_line",
  kind: "factory",
  desc: [Línea oculta cuyos extremos siguen a sus fuentes en el mismo fotograma y que conserva el progreso de `create` o `write` activos. Revélala con una animación de entrada.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tip = scene.geometry.dot(0.11).fill(GOLD).move_to(1.25, 0.375)
rod = scene.geometry.tracking_line((-1.25, -0.375), tip).no_fill().stroke(WHITE, 0.05)
scene.play([rod.animate.create().duration(0.8), tip.animate.shift_by(-0.5, 1).duration(0.8)])
scene.play([rod.animate.write().duration(0.8), tip.animate.shift_by(1, -0.625).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.point_on_curve",
  kind: "factory",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Polilínea o Bézier muestreada.]), (name: "tracker", type: "Parameter", default: none, desc: [Posición de 0 a 1 por longitud de arco; se limita a ese rango.])),
  desc: [Punto oculto que recorre la curva por longitud de arco, sin callbacks de Python durante la reproducción.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
from math import cos, sin, pi
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.0)
curve = scene.geometry.polyline([(1.375*cos(u), 0.75*sin(2*u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 0.025)
dot = scene.geometry.point_on_curve(curve, t).fill(GOLD)
scene.play([dot.animate.fade_in().duration(0.3), t.animate.set(1.0).duration(1.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.tangent_on_curve",
  kind: "factory",
  desc: [Segmento oculto de longitud `length`, centrado en el punto de la curva y orientado según la tangente. Usa el mismo muestreo por longitud de arco que `point_on_curve`.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
from math import cos, sin, pi
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.35)
curve = scene.geometry.polyline([(1.375*cos(u), 0.75*sin(u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 0.025)
tangent = scene.geometry.tangent_on_curve(curve, t, length=0.875).stroke(GOLD, 0.04)
scene.play([tangent.animate.fade_in().duration(0.3), t.animate.set(0.9).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.normal_on_curve",
  kind: "factory",
  desc: [Como `tangent_on_curve`, girado 90° en sentido antihorario.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
t = scene.viz.parameter(0.2)
curve = scene.geometry.polyline([(math.cos(u / 40), math.sin(u / 40)) for u in range(252)])
normal = scene.geometry.normal_on_curve(curve, t, length=0.6).stroke(RED, 0.04)
```
]

#api-entry(
  name: "Geometry.curvature_on_curve",
  kind: "factory",
  desc: [Círculo osculador oculto, estimado a partir de muestras vecinas por longitud de arco (`window` es la fracción de la curva usada). Dale estilo con `no_fill().stroke(...)`.],
)[
```python
# show-code: true
from gaanim import RED, WHITE, Scene
from math import cos, sin, pi
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.25)
curve = scene.geometry.polyline([(1.375*cos(u), 0.75*sin(u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 0.025)
circle = scene.geometry.curvature_on_curve(curve, t).no_fill().stroke(RED, 0.025)
scene.play([circle.animate.fade_in().duration(0.3), t.animate.set(0.7).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.always_redraw_arc",
  kind: "factory",
  params: ((name: "tracker", type: "Parameter", default: none, desc: [Conduce el ángulo de barrido.]), (name: "cx, cy, radius, start_angle", type: "float", default: none, desc: [Centro, radio y ángulo inicial en radianes.]), (name: "sweep_scale / sweep_offset", type: "float", default: "1.0 / 0.0", desc: [El barrido es `tracker * sweep_scale + sweep_offset`.])),
  desc: [Arco oculto que se regenera en cada fotograma a partir de un `Parameter`, sin callbacks de Python. Ideal para marcar un ángulo que cambia.],
)[
```python
# show-code: true
from gaanim import WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
theta = scene.viz.parameter(0.3)
rot = scene.geometry.always_redraw_arc(theta, 0, 0, 0.69, 0.0).fill(WHITE)
scene.play([rot.animate.fade_in().duration(0.3), theta.animate.set(5.0).duration(1.6)])
# output: preview.webp
scene.render()
```
]

=== Trazos

#api-entry(
  name: "Geometry.traced_path",
  kind: "factory",
  params: ((name: "source", type: "Drawable", default: none, desc: [Objeto cuya posición se muestrea.]), (name: "dissipating_time", type: "float | None", default: "None", desc: [Segundos que dura cada muestra en la estela; positivo.]), (name: "max_points", type: "int | None", default: "None", desc: [Límite positivo de muestras retenidas.]), (name: "min_distance", type: "float", default: "0.01", desc: [Distancia mínima entre muestras.])),
  desc: [Estela 2D que dibuja el recorrido de un objeto. El muestreo empieza en el cursor donde se declara, así que los segmentos anteriores y los seeks no la rellenan de antemano. Revélala con `trail.animate.fade_in()`. Con `dissipating_time` las muestras caducan por la cola en la vista previa, los seeks, las capturas y la exportación. Tras un seek, la estela se reconstruye si la fuente la mueven updaters, `drive_from_samples`, vínculos reactivos o `follow`; una fuente movida solo con `.animate` empieza una estela nueva.],
)[
```python
# show-code: true
from gaanim import GOLD, RED, Scene, Updater
scene = Scene(frame=(16, 9), background="#0f172a")
dot = scene.geometry.dot(0.09).fill(GOLD).move_to(1.5, 0)
dot.add_updater(Updater.orbit(0, 0, 1.5, 2.0))
trail = scene.geometry.traced_path(dot, dissipating_time=1.0).stroke(RED, 0.04).no_fill()
scene.play([trail.animate.fade_in().duration(0.2)])
scene.wait(1.6)
# output: preview.webp
scene.render()
```
]

== Geometría 3D

#experimental()

Primitivas con material PBR, líneas en el espacio y estelas 3D. Gaanim usa un
mundo dextrógiro con Y hacia arriba: cilindros y conos crecen a lo largo de Y y
los planos yacen en XZ. Los puntos 3D son tuplas `(x, y, z)`. La guía
#link("/guias/camara-y-3d/")[Cámara y 3D] explica la cámara en perspectiva.

`Primitive3D` extiende `Drawable`, así que posición, rotación, escala, opacidad
y material se animan y son deterministas al hacer seek. Los parámetros de
geometría quedan fijos tras la construcción. `create()` hace crecer la malla
desde su centro mientras aparece; `write()`, que es solo vectorial, se rechaza.

```python
# show-code: true
from gaanim import BLUE, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9))
cube = scene.geometry.cube(2, material=Material3D.matte(BLUE))
sphere = scene.geometry.sphere(1, segments=32, rings=16,
                      material=Material3D.metal(GOLD)).move_to_3d(3, 0, 0)
floor = scene.geometry.plane(10, 8, subdivisions=(4, 4)).move_to_3d(0, -2, 0)
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)
scene.camera.look_at(eye=(6, 4, 8), target=(1, 0, 0))
scene.play([cube.animate.create(), sphere.animate.create(), floor.animate.fade_in()])
scene.play([cube.animate.material(Material3D.metal(GOLD)).duration(1.0)])
scene.render()
```

#api-entry(
  name: "Geometry.cube",
  kind: "factory",
  desc: [Cubo centrado de caras planas.],
  none,
)

#api-entry(
  name: "Geometry.sphere",
  kind: "factory",
  desc: [Esfera UV con sombreado suave; `segments` y `rings` controlan la resolución.],
  none,
)

#api-entry(
  name: "Geometry.cylinder",
  kind: "factory",
  desc: [Cilindro a lo largo de Y; `caps=False` lo deja abierto.],
  none,
)

#api-entry(
  name: "Geometry.cone",
  kind: "factory",
  desc: [Cono a lo largo de Y; `cap=False` quita la base.],
  none,
)

#api-entry(
  name: "Geometry.plane",
  kind: "factory",
  desc: [Plano en XZ con normales hacia arriba, subdividido en `subdivisions` celdas.],
  none,
)

#api-entry(
  name: "Geometry.lighting_3d",
  kind: "method",
  desc: [Configura el único equipo automático de luces 3D de la escena: `"studio"` o `"none"`, con intensidad y sombras.],
  none,
)

#api-entry(
  name: "Material3D.matte / metal / emissive / color / roughness / metallic / emissive_color / emissive_strength",
  kind: "class",
  signature: "Material3D(color=WHITE, roughness=0.55, metallic=0.0, emissive=None, emissive_strength=0.0)",
  desc: [Material PBR cuyas propiedades numéricas se interpolan en espacio lineal y cuyos rangos se validan. Los atajos `Material3D.matte(color)`, `Material3D.metal(color)` y `Material3D.emissive(color, strength=1.0)` cubren los aspectos habituales. Las propiedades `color`, `roughness`, `metallic`, `emissive_color` y `emissive_strength` son de solo lectura.],
)[
```python
>>>from gaanim import *
glass = Material3D(color="#93c5fd", roughness=0.1, metallic=0.0)
lamp = Material3D.emissive(GOLD, strength=2.0)
```
]

#api-entry(
  name: "Primitive3D.material",
  kind: "method",
  desc: [Cambia el material al instante; `.animate.material(...)` lo interpola.],
  none,
)

#api-entry(
  name: "Geometry.polyline_3d",
  kind: "factory",
  params: ((name: "points", type: "Sequence[tuple[float, float, float]]", default: none, desc: [Al menos dos puntos finitos del mundo.]), (name: "color", type: "Color | None", default: "None", desc: [Color uniforme.]), (name: "colors", type: "Sequence[Color] | None", default: "None", desc: [Un color por punto; tiene prioridad sobre `colormap`.]), (name: "colormap", type: "str | None", default: "None", desc: [Degradado ordenado a lo largo de la línea, por ejemplo `"inferno"`, `"viridis"` o `"plasma"`.])),
  desc: [Tira de líneas 3D en el espacio del mundo. La lista `colors` debe tener la misma longitud que `points`.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
points = [(math.cos(t / 8), math.sin(t / 8), t / 40 - 1) for t in range(81)]
helix = scene.geometry.polyline_3d(points, colormap="inferno")
```
]

#api-entry(
  name: "Geometry.traced_path_3d",
  kind: "factory",
  desc: [Estela 3D de la posición de un objeto en el mundo. Funciona con `Updater` y `add_updater_fn`; el muestreo empieza donde se declara. Revélala con `fade_in` o `create`. `dissipating_time` hace caducar la cola, `max_points` limita la memoria y `min_distance` (0.1 por defecto) filtra muestras casi iguales. Admite los mapas `"inferno"`, `"viridis"` y `"plasma"`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
dot = scene.geometry.dot(0.09).move_to_3d(1, 0, 0)
dot.add_updater(Updater.orbit(0, 0, 1, 1.5))
trail = scene.geometry.traced_path_3d(
    dot, colormap="viridis", dissipating_time=2.0, max_points=600
)
scene.play([trail.animate.fade_in()])
```
]

Este ejemplo combina ejes 3D, una polilínea, un punto que orbita con su estela
y la cámara en perspectiva:

```python
# show-code: true
from gaanim import Axis, BLACK, GOLD, RED, WHITE, Scene, Updater
scene = Scene(frame=(16, 9), background=BLACK)

axes = scene.viz.cartesian_3d(
    Axis.linear(-3, 3).ticks(1).label("x").style(color=WHITE),
    Axis.linear(-3, 3).ticks(1).label("y").style(color=WHITE),
    Axis.linear(-2, 2).ticks(1).label("z").style(color=WHITE),
    size=(6, 6, 4),
)
path = scene.geometry.polyline_3d([(-2, -1, -1), (0, 1, 0), (2, -1, 1)], color=RED)
dot = scene.geometry.dot(0.1).fill(GOLD).move_to_3d(1, 0, 0).billboard()
dot.add_updater(Updater.orbit(0, 0, 1, 1.2))
trail = scene.geometry.traced_path_3d(dot, colormap="viridis", max_points=120)

scene.camera.perspective(fov_y=0.785, near=0.1, far=100.0)
scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0))
scene.play([
    axes.animate.create().duration(0.8),
    path.animate.create().duration(0.8),
    dot.animate.fade_in().duration(0.4),
    trail.animate.fade_in().duration(0.4),
])
scene.play([scene.camera.animate.orbit(delta_yaw=0.5, delta_pitch=0.1).duration(0.8)])
scene.play([scene.camera.animate.dolly(factor=0.85).duration(0.5)])
scene.wait(0.5)
scene.render()
```
