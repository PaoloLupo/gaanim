#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Animaciones",
  description: "Anim, easing, composición, updaters y transiciones entre segmentos",
  route: "/referencia/animations/",
)

= Animaciones

Los handles usan un solo vocabulario. Una llamada directa aplica un corte en el
cursor actual sin avanzar el tiempo; la misma llamada bajo la propiedad
`animate` describe un `Anim` que solo entra en la línea de tiempo al pasarlo a
`scene.play(...)`.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>dot = scene.geometry.dot(0.1)
dot.move_to(1.25, 1).fill(BLUE)
scene.play([dot.animate.move_to(5, 1).fill(RED)])
```

“Inmediato” no significa modificar un objeto ya compilado: registra un corte
reversible en el cursor, y los seeks anteriores conservan el estado anterior.
Construir, configurar o descartar un `Anim` no cambia la visibilidad, el
estado ni el cursor.

Un `Anim` contiene un solo efecto (`fade_in`, `write`, `indicate`…) o una
combinación de destinos de propiedad (`move_to`, `fill`, `opacity`…) que
comparten duración, easing y retraso. Encadenar un efecto después de un
destino de propiedad o de otro efecto, como
`dot.animate.move_to(1, 0).fade_in()`, o un destino después de un efecto, como
`dot.animate.fade_in().move_to(1, 0)`, lanza `ValueError`; combina animaciones
separadas con `parallel()`. Los modificadores de tiempo y de trazo
(`duration`, `easing`, `stroke_width`…) sí se encadenan con cualquier efecto.
Cada `Anim` es de un solo uso. `pulse`, `wave`, `highlight`, `focus`, `cancel`,
`reveal`, `brace` y `annotate` sobre una selección de texto
(`text["parte"].animate.pulse()`) están en #link("/referencia/text/")[Texto].

== Destinos de propiedad

Encadena varios destinos en el mismo `Anim`: se interpolan a la vez desde el
estado que el objeto tenga al empezar el clip.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(1)
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

Los destinos absolutos de posición, rotación, escala y opacidad aceptan un
`ScalarSource`: un número, `Parameter`, `Variable`, `Computed` o `scene.time`.
Bajo `.animate`, una fuente se evalúa al inicio efectivo del clip (incluidos
retrasos y secuencias) y queda congelada durante la interpolación. En el
setter directo, en cambio, la fuente enlaza el canal desde el cursor: otra
fuente lo reemplaza y un número lo termina con un corte reversible. Mientras un
canal está enlazado, anima la fuente o fija antes el canal con un número; las
animaciones que lo escriben lanzan un error.

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>dot = scene.geometry.dot(0.1)
>>>other = scene.geometry.square(0.5).move_to(-3, 1)
phase = scene.viz.parameter(0.0)
height = computed(lambda x: x * x, inputs=[phase])
dot.move_to(phase, height)                      # enlazado
scene.play(phase.animate.set(2).duration(2))
dot.move_to(2, 4)                               # fija el canal
scene.play(dot.animate.move_to(other).opacity(0.5))
```

#api-entry(
  name: "Anim.move_to",
  kind: "method",
  signature: "move_to(x: ScalarSource, y: ScalarSource, anchor: Anchor | None = None) -> Anim | move_to(reference: Drawable) -> Anim | move_to(point: AnchorPoint) -> Anim",
  params: (
    (name: "x, y", type: "ScalarSource", default: none, desc: [Posición de destino.]),
    (name: "anchor", type: "Anchor | None", default: "None", desc: [Punto del objeto que llega a `(x, y)`; `None` usa el centro.]),
    (name: "reference / point", type: "Drawable | AnchorPoint", default: none, desc: [Objeto o punto de anclaje de la misma escena, resuelto al inicio del clip; no lo sigue después.]),
  ),
  returns: (type: "Anim", desc: [Movimiento a una posición absoluta.]),
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
  name: "Anim.shift_by",
  kind: "method",
  params: (
    (name: "dx", type: "float", default: none, desc: [Desplazamiento horizontal.]),
    (name: "dy", type: "float", default: none, desc: [Desplazamiento vertical.]),
  ),
  returns: (type: "Anim", desc: [Movimiento relativo.]),
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
  name: "Anim.path_arc",
  kind: "method",
  params: ((name: "angle", type: "float", default: none, desc: [Giro del arco en radianes; positivo es antihorario.]),),
  returns: (type: "Anim", desc: [El mismo movimiento, por un arco.]),
  desc: [Curva el `move_to` o `shift_by` de este `Anim` en un arco circular en lugar de la línea recta. Sin un destino de traslación lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>import math
>>>ball = scene.geometry.dot(0.2).move_to(-4, 0)
scene.play(ball.animate.move_to(4, 0).path_arc(math.pi / 3))
```
]

#api-entry(
  name: "Anim.move_along",
  kind: "method",
  params: (
    (name: "target", type: "Drawable", default: none, desc: [Objeto cuyo contorno se recorre: círculo, rectángulo, curva, polilínea… Se muestrea su geometría de mundo. Una `arrow` o `curved_arrow` sólida se recorre por su eje, de la cola a la punta.]),
    (name: "orient", type: "bool", default: "False", desc: [Gira el objeto con la tangente del camino.]),
    (name: "rotate_offset", type: "float", default: "0.0", desc: [Radianes que se suman a la tangente al orientar.]),
    (name: "start, end", type: "float", default: "0.0, 1.0", desc: [Tramo recorrido, en fracciones de longitud de arco en `[0, 1]`. `start > end` lo recorre al revés; valores iguales o fuera de rango lanzan `ValueError`.]),
  ),
  returns: (type: "Anim", desc: [Traslación a lo largo del camino.]),
  desc: [Muestrea el contorno por longitud de arco real. Con `.easing(Easing.LINEAR)` la velocidad es uniforme. Con `orient=True` la rotación sigue la tangente, así un avión apunta hacia donde vuela; sin él, rotación y escala no cambian.],
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
  name: "Anim.scale_to",
  kind: "method",
  params: ((name: "factor", type: "ScalarSource", default: none, desc: [Escala uniforme absoluta.]),),
  returns: (type: "Anim", desc: [Cambio de escala.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>badge = scene.geometry.circle(0.5)
scene.play(badge.animate.scale_to(1.5).duration(0.4))
```
]

#api-entry(
  name: "Anim.scale_by",
  kind: "method",
  params: ((name: "factor", type: "float", default: none, desc: [Multiplica la escala actual: mayor que 1 agranda y menor que 1 encoge.]),),
  returns: (type: "Anim", desc: [Cambio de escala relativo.]),
  desc: [Escala alrededor del pivote del objeto; cámbialo con `with_pivot(x, y)`.],
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
  name: "Anim.rotate_to",
  kind: "method",
  params: ((name: "radians", type: "ScalarSource", default: none, desc: [Rotación absoluta alrededor de Z, en radianes.]),),
  returns: (type: "Anim", desc: [Giro hasta un ángulo.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>needle = scene.geometry.rect(2, 0.1).with_pivot(0, 0)
scene.play(needle.animate.rotate_to(1.2))
```
]

#api-entry(
  name: "Anim.rotate_by",
  kind: "method",
  params: ((name: "radians", type: "float", default: none, desc: [Ángulo relativo en radianes; positivo es antihorario.]),),
  returns: (type: "Anim", desc: [Giro relativo.]),
  desc: [Gira alrededor del pivote del objeto. Para una bisagra o una órbita, fija el pivote antes con `with_pivot(x, y)`. Un giro de cualquier tamaño, incluidas varias vueltas, sigue un solo easing durante toda la duración y el pivote queda fijo.],
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
  name: "Anim.pivot",
  kind: "method",
  params: ((name: "x, y", type: "float", default: none, desc: [Punto de giro en unidades de escena.]),),
  returns: (type: "Anim", desc: [El mismo `Anim`.]),
  desc: [*Limitación conocida:* hoy no cambia el punto de giro de `animate.rotate_by(...)` y se ignora sin error. Para girar alrededor de un punto, fija el pivote del objeto con `Drawable.with_pivot(x, y)` (o su alias `Drawable.pivot`) antes de animar, como en el ejemplo.],
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

#api-entry(
  name: "Anim.about_point",
  kind: "method",
  params: ((name: "x, y", type: "float", default: none, desc: [Punto de giro en unidades de escena.]),),
  returns: (type: "Anim", desc: [El mismo `Anim`.]),
  desc: [Alias de `Anim.pivot`, con la misma limitación.],
  none,
)

#api-entry(
  name: "Anim.opacity",
  kind: "method",
  params: ((name: "value", type: "ScalarSource", default: none, desc: [Opacidad de destino, limitada a `[0, 1]`.]),),
  returns: (type: "Anim", desc: [Cambio de opacidad.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>box = scene.geometry.rect(1.5, 0.625).fill(BLUE)
scene.play([box.animate.opacity(0.35).duration(0.6)])
```
]

#api-entry(
  name: "Anim.fill",
  kind: "method",
  params: ((name: "color", type: "Paint", default: none, desc: [Color o `Brush` de destino.]),),
  returns: (type: "Anim", desc: [Cambio de relleno.]),
  desc: [Los gradientes del mismo tipo interpolan su geometría y sus paradas normalizadas; un color sólido puede pasar a un gradiente y al revés. Tipos de gradiente incompatibles (lineal y radial) lanzan `ValueError`. Tras `no_fill()`, el relleno aparece desde transparente. En un `Text`, cada glifo interpola desde su color actual, así que los fragmentos de color propio convergen al destino. En una `Primitive3D` cambia el color base del material PBR y exige un color sólido. Ver #link("/referencia/themes/")[Temas y colores].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(1).fill(BLUE)
paint = Brush.linear([BLUE, GOLD], start=(-1, 0), end=(1, 0))
scene.play(circle.animate.fill(paint).duration(1.5))
```
]

#api-entry(
  name: "Anim.stroke",
  kind: "method",
  params: (
    (name: "color", type: "Paint", default: none, desc: [Color o `Brush` del trazo.]),
    (name: "width", type: "float", default: none, desc: [Ancho del trazo en unidades lógicas.]),
  ),
  returns: (type: "Anim", desc: [Cambio de trazo.]),
  desc: [La pintura se interpola como en `fill`. Tras `no_stroke()`, el trazo aparece desde transparente y crece desde ancho cero. En un `Text` cambia el contorno de cada glifo sin tocar sus rellenos. No existe en `Primitive3D`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>square = scene.geometry.square(2).no_stroke()
scene.play(square.animate.stroke(GOLD, 0.08))
```
]

#api-entry(
  name: "Anim.stroke_width",
  kind: "method",
  params: ((name: "value", type: "float", default: none, desc: [Ancho del trazo en unidades lógicas.]),),
  returns: (type: "Anim", desc: [El mismo `Anim`.]),
  desc: [En un `Anim` de propiedades anima el ancho del trazo. En `write`, `create`, `uncreate`, `unwrite` o `draw_border_then_fill` fija el ancho del trazo dibujado.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>ring = scene.geometry.circle(1).no_fill().stroke(WHITE, 0.02)
scene.play(ring.animate.stroke_width(0.12))
scene.play(ring.animate.uncreate().stroke_width(0.05))
```
]

#api-entry(
  name: "Anim.set",
  kind: "method",
  params: ((name: "value", type: "float", default: none, desc: [Valor finito de destino.]),),
  returns: (type: "Anim", desc: [Cambio del valor.]),
  desc: [Anima un `Parameter` o una `Variable`; todo lo que depende de ellos se actualiza en cada fotograma.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
theta = scene.viz.parameter(0.0)
scene.play(theta.animate.set(3.14).duration(2))
```
]

#api-entry(
  name: "Anim.crop",
  kind: "method",
  params: (
    (name: "x, y, width, height", type: "float", default: none, desc: [Rectángulo de la fuente, en píxeles desde la esquina superior izquierda.]),
    (name: "normalized", type: "bool", default: "False", desc: [Interpreta el rectángulo como fracciones del tamaño original.]),
  ),
  returns: (type: "Anim", desc: [Recorte animado.]),
  desc: [Anima qué parte de una `Image` o un `Video` se ve dentro de su marco fijo. Un rectángulo inválido o un objeto que no es un medio lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
photo = scene.media.image("assets/cover.png").frame(6, 3.4, fit="cover")
scene.play(photo.animate.crop(0.25, 0.25, 0.5, 0.5, normalized=True).duration(1.2))
```
]

#api-entry(
  name: "Anim.fill_level",
  kind: "method",
  params: ((name: "level", type: "float", default: none, desc: [Nivel normalizado en `[0, 1]`.]),),
  returns: (type: "Anim", desc: [Cambio de nivel.]),
  desc: [Anima un relleno creado con #link("/referencia/geometria/#api-geometry-fill-level")[`scene.geometry.fill_level`]. Si el nivel está enlazado a una fuente, lanza `ValueError`: anima la fuente o fija antes un número con `set_fill_level`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
drop = scene.geometry.circle(1.2).no_fill().stroke(WHITE, 0.04)
water = scene.geometry.fill_level(drop, "#38bdf8", 0.0)
scene.play([water.animate.fill_level(0.72).duration(1.4)])
```
]

`Anim.custom` sustituye todos los destinos por una función propia; está en
#link(<personalizadas>)[Animaciones personalizadas]. Los efectos animables
`glow`, `blur`, `shadow` y `trim` están en #link(<efectos>)[Efectos].

== Entradas y salidas

Una entrada programada mantiene oculto el objeto antes de empezar, también si
se declaró después de otras animaciones o dentro de un grupo.

#api-entry(
  name: "Anim.fade_in",
  kind: "method",
  returns: (type: "Anim", desc: [Aparición por opacidad.]),
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
  name: "Anim.fade_out",
  kind: "method",
  returns: (type: "Anim", desc: [Desaparición por opacidad.]),
  none,
)

#api-entry(
  name: "Anim.fade_in_from",
  kind: "method",
  params: (
    (name: "direction", type: "Direction", default: none, desc: [Lado desde el que entra: `Direction.UP`, `DOWN`, `LEFT`, `RIGHT` o una diagonal.]),
    (name: "distance", type: "float", default: "0.48", desc: [Distancia recorrida hasta su posición.]),
  ),
  returns: (type: "Anim", desc: [Entrada con desplazamiento.]),
  desc: [Empieza invisible y desplazado, y llega a su sitio mientras aparece.],
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

#api-entry(
  name: "Anim.write",
  kind: "method",
  params: (
    (name: "by", type: "\"grapheme\" | \"word\" | \"line\" | \"part\"", default: "\"grapheme\"", desc: [En un texto, qué unidades empiezan juntas: grafemas, palabras, líneas explícitas o partes semánticas. La puntuación se une a su vecina.]),
    (name: "order", type: "\"forward\" | \"reverse\" | \"center\" | \"random\"", default: "\"forward\"", desc: [Orden de los grupos: hacia delante, al revés, desde el centro hacia fuera o en una permutación aleatoria fija.]),
    (name: "stagger", type: "float | None", default: "None", desc: [Retardo relativo entre grupos; `None` lo adapta al número de grupos.]),
  ),
  returns: (type: "Anim", desc: [Escritura trazo a trazo.]),
  desc: [Traza los contornos con un grosor lógico constante y luego funde los rellenos. Si el objeto no tiene contorno, usa uno temporal de 0.03 unidades que desaparece al entrar el relleno. Los descendientes reactivos siguen ocultos hasta la animación y conservan el progreso al regenerarse. La segmentación es la de `text.words`, `text.lines` y `text.parts`. Después de un destino de propiedad o de otro efecto lanza `ValueError`.],
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
  name: "Anim.unwrite",
  kind: "method",
  returns: (type: "Anim", desc: [La escritura al revés.]),
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
  name: "Anim.create",
  kind: "method",
  returns: (type: "Anim", desc: [Trazado progresivo seguido del relleno.]),
  desc: [Dibuja el contorno durante el primer 70 % de la duración y funde el relleno de las formas cerradas en el 30 % final. El ancho del trazo no cambia; si el objeto no tiene contorno, usa uno temporal de 0.03 unidades. Los caminos abiertos usan toda la duración para el trazo. El easing predeterminado es `Easing.DOUBLE_SMOOTH`. En una malla 3D, crece desde el centro mientras aparece. A diferencia de `write`, no sigue el orden de los glifos.],
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

#api-entry(
  name: "Anim.uncreate",
  kind: "method",
  returns: (type: "Anim", desc: [El trazado al revés: borra el objeto.]),
  none,
)

#api-entry(
  name: "Anim.draw_border_then_fill",
  kind: "method",
  returns: (type: "Anim", desc: [Primero el contorno, después el relleno.]),
  desc: [Dibuja el borde y luego inunda el interior; queda bien en formas rellenas.],
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
  name: "Anim.with_pen_tip",
  kind: "method",
  returns: (type: "Anim", desc: [El mismo `Anim`.]),
  desc: [En `write`, `create` y sus inversas, dibuja una punta de pluma en el extremo del trazo, como una escritura a mano.],
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

#api-entry(
  name: "Anim.grow_from_center",
  kind: "method",
  returns: (type: "Anim", desc: [Crecimiento desde escala cero.]),
  desc: [Aparición desde el centro de la caja; útil para gráficas e insignias.],
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
  name: "Anim.shrink_to_center",
  kind: "method",
  returns: (type: "Anim", desc: [Reducción hasta escala cero hacia el centro.]),
  none,
)

#api-entry(
  name: "Anim.grow_from_edge",
  kind: "method",
  params: ((name: "direction", type: "Direction", default: none, desc: [Lado de la caja que queda fijo; una diagonal fija una esquina.]),),
  returns: (type: "Anim", desc: [Crecimiento desde un borde.]),
  desc: [Usa la caja en su posición final: `Direction.DOWN` fija el punto medio del borde inferior, así una barra sube desde su base, y `Direction.custom(x, y)` fija el punto correspondiente de la caja. Termina en la posición y el tamaño declarados. Easing predeterminado: `Easing.SMOOTH`.],
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
  name: "Anim.grow_from_point",
  kind: "method",
  params: ((name: "x, y", type: "float", default: none, desc: [Punto de la escena que queda fijo.]),),
  returns: (type: "Anim", desc: [Crecimiento desde un punto.]),
  desc: [Como `grow_from_center`, pero con un punto fijo arbitrario, por ejemplo el origen de un globo. Coordenadas no finitas lanzan `ValueError`.],
  none,
)

#api-entry(
  name: "Anim.grow_arrow",
  kind: "method",
  returns: (type: "Anim", desc: [Crecimiento de una flecha desde la cola.]),
  desc: [La cola queda fija y la punta recorre el eje de la flecha, recto o curvo en `curved_arrow` y `curved_arrow_arc`. La cabeza aparece con sus proporciones durante la primera longitud de cabeza y luego conserva su tamaño mientras el cuerpo se alarga; el grosor no cambia nunca. Un `scene.geometry.connector` crece por su polilínea viva, pasando por cada punto `via`, mientras sus extremos siguen a sus referencias. Cualquier otro objeto, o una flecha deformada por una transformación, usa `create()`. Easing predeterminado: `Easing.SMOOTH`. En una selección de texto lanza `TypeError`.],
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
  name: "Anim.spin_in_from_nothing",
  kind: "method",
  returns: (type: "Anim", desc: [Entrada girando desde escala cero.]),
  desc: [Una entrada juguetona para estrellas e iconos.],
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
  name: "Anim.show_passing_flash",
  kind: "method",
  params: ((name: "time_width", type: "float", default: "0.2", desc: [Longitud de la ventana visible, como fracción del camino, en `(0, 1]`.]),),
  returns: (type: "Anim", desc: [Una ventana que recorre el trazo.]),
  desc: [Como con `create()`, el objeto queda oculto antes del destello (salvo que una animación de trazo anterior lo muestre) y vuelve a ocultarse cuando la ventana sale por el final. Para un pulso sobre una línea visible, dibuja una segunda línea encima.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
pulse = scene.geometry.line(-5, 0, 5, 0).stroke(CYAN, 0.06)
scene.wait(0.5)                                                  # oculta
scene.play(pulse.animate.show_passing_flash(time_width=0.3).duration(1.0))
scene.wait(0.5)                                                  # oculta
```
]

Las animaciones de trazo de un mismo objeto (`create`, `write`, `trim`,
`show_passing_flash`) comparten el canal `effect`, así que no pueden solaparse
dentro de un `play`. El error indica las dos animaciones, sus tramos y el tipo
de objeto; encadénalas con `sequence` o combínalas en una sola. Dos tramos que
solo se tocan no se consideran solapados.

== Énfasis

#api-entry(
  name: "Anim.indicate",
  kind: "method",
  returns: (type: "Anim", desc: [Un pequeño salto con resaltado.]),
  desc: [Salta un poco hacia arriba desde su centro visual y resalta el objeto; el objeto vuelve a su estado.],
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

#api-entry(
  name: "Anim.wiggle",
  kind: "method",
  returns: (type: "Anim", desc: [Una sacudida breve.]),
  desc: [Útil para señalar una respuesta incorrecta.],
  none,
)

#api-entry(
  name: "Anim.circumscribe",
  kind: "method",
  returns: (type: "Anim", desc: [Un contorno que rodea el objeto.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>result = scene.text("$x = 4$")
scene.play(result.animate.circumscribe().duration(1.0))
```
]

#api-entry(
  name: "Anim.flash",
  kind: "method",
  returns: (type: "Anim", desc: [Un destello de líneas alrededor del objeto.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>star = scene.geometry.star(5, 0.8, 0.4).fill(GOLD)
scene.play(star.animate.flash().duration(0.6))
```
]

== Texto

Estas animaciones solo existen sobre un `Text` completo; en otro objeto lanzan
`TypeError`. Se evalúan de forma nativa a partir del tiempo del clip, así que
los seeks, las capturas y la exportación coinciden. El `Text` que escriben
queda oculto hasta que empiezan, así las filas de un `stagger` esperan sin
verse. Las selecciones, anotaciones y transiciones de fórmulas están en
#link("/referencia/text/")[Texto].

#api-entry(
  name: "Anim.typewriter",
  kind: "method",
  params: (
    (name: "cps", type: "float", default: "18.0", desc: [Pulsaciones por segundo; positivo.]),
    (name: "cursor", type: "str | None", default: "\"▍\"", desc: [Cadena dibujada tras el último grafema; `None` o `""` para ninguno. Los caracteres de bloque (de `▏` a `█`) son rectángulos exactos, independientes de la fuente.]),
    (name: "blink", type: "float", default: "2.0", desc: [Parpadeos por segundo del cursor en reposo; `0` lo deja fijo. Mientras escribe está fijo.]),
    (name: "jitter", type: "float", default: "0.2", desc: [Cada intervalo se multiplica por un factor en `[1 - jitter, 1 + jitter]`; debe estar en `[0, 1)`.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla del ritmo de pulsaciones.]),
    (name: "keep_cursor", type: "bool", default: "True", desc: [Mantiene el cursor al terminar; `False` lo quita.]),
  ),
  returns: (type: "Anim", desc: [Escritura tecla a tecla, con tiempo lineal.]),
  desc: [Vacía el `Text` y lo vuelve a escribir un grafema por pulsación, con el diseño final: los glifos aparecen en su sitio y nada se reacomoda. El cursor sigue al último grafema, también entre líneas. Sin duración explícita dura hasta la última pulsación (unos `grafemas / cps` segundos); `.duration(...)` reescala el ritmo. Valores inválidos lanzan `ValueError`.],
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
  name: "Anim.backspace",
  kind: "method",
  params: (
    (name: "count", type: "int | None", default: "None", desc: [Grafemas que se borran desde el final, como máximo los visibles; `None` borra todos.]),
    (name: "cps", type: "float", default: "24.0", desc: [Borrados por segundo; positivo.]),
  ),
  returns: (type: "Anim", desc: [Borrado tecla a tecla.]),
  desc: [El cursor retrocede con cada borrado; un `Text` sin cursor recibe el parpadeante predeterminado. Dura `count / cps` por defecto.],
  none,
)

#api-entry(
  name: "Anim.retype",
  kind: "method",
  params: (
    (name: "text", type: "str", default: none, desc: [Texto plano nuevo, no vacío.]),
    (name: "cps, jitter, seed", type: "float, float, int", default: "18.0, 0.2, 0", desc: [Como en `typewriter`.]),
  ),
  returns: (type: "Anim", desc: [Borrado hasta el prefijo común y escritura del resto.]),
  desc: [Conserva el prefijo que comparte con el texto visible, borra lo demás y escribe el resto con el estilo y el origen del `Text`. Después el `Text` muestra `text`, pero su contenido declarado (tamaño de layout, selecciones `text[...]`) no cambia.],
  none,
)

#api-entry(
  name: "Anim.scramble",
  kind: "method",
  params: (
    (name: "charset", type: "str", default: "\"upper\"", desc: [`"upper"`, `"lower"`, `"digits"`, `"hex"`, `"symbols"` o una cadena literal como `"01"`.]),
    (name: "reveal_delay", type: "float", default: "0.3", desc: [Segundos que todas las posiciones cambian antes de que se asiente la primera.]),
    (name: "speed", type: "float", default: "20.0", desc: [Cambios de glifo por segundo; positivo.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla de la elección de glifos.]),
  ),
  returns: (type: "Anim", desc: [Revelado por decodificación, con tiempo lineal.]),
  desc: [Cada grafema muestra glifos aleatorios del juego de caracteres, centrados en su celda final, y después se asienta de izquierda a derecha. El ancho del texto final queda reservado, así que no salta. Dura `reveal_delay` más 0.05 s por grafema (al menos 0.6 s).],
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

#api-entry(
  name: "Anim.scramble_to",
  kind: "method",
  params: (
    (name: "text", type: "str", default: none, desc: [Texto plano nuevo, no vacío.]),
    (name: "charset, reveal_delay, speed, seed", type: "", default: "", desc: [Como en `scramble`.]),
  ),
  returns: (type: "Anim", desc: [Decodificación hacia un texto nuevo.]),
  desc: [Los glifos actuales desaparecen al empezar y cada posición de `text` se decodifica en el diseño de `text`.],
  none,
)

#api-entry(
  name: "Anim.reveal",
  kind: "method",
  params: (
    (name: "style", type: "\"slide_up\" | \"slide_down\" | \"fade\" | \"scale\" | \"blur\" | None", default: "None", desc: [Estilo de entrada; `None` es `"slide_up"`.]),
    (name: "by", type: "\"grapheme\" | \"word\" | \"line\" | \"part\"", default: "\"line\"", desc: [Unidad que entra de una vez.]),
    (name: "mask", type: "bool", default: "True", desc: [Los estilos de deslizamiento salen de detrás de una máscara vectorial recortada a su fila.]),
    (name: "stagger", type: "float", default: "0.06", desc: [Segundos entre unidades.]),
  ),
  returns: (type: "Anim", desc: [Revelado unidad a unidad.]),
  desc: [Con `"slide_up"` cada unidad sube una altura de fila desde detrás de su máscara, así que también funciona en la exportación SVG. Sin máscara, los deslizamientos recorren menos y aparecen por opacidad. `.easing(...)` suaviza cada unidad (cúbica de salida por defecto) y la duración cubre toda la cascada. Ver #link("/referencia/text/")[Texto] para la versión sobre selecciones.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Una idea\npor línea", role="title")
scene.play(title.animate.reveal(by="line", style="slide_up", stagger=0.06))
scene.play(title.animate.conceal(by="line", style="slide_up"))
```
]

#api-entry(
  name: "Anim.conceal",
  kind: "method",
  params: ((name: "style, by, mask, stagger", type: "", default: "\"slide_up\", \"line\", True, 0.06", desc: [Como en `reveal`.]),),
  returns: (type: "Anim", desc: [Salida unidad a unidad.]),
  desc: [La salida que corresponde a `reveal`: las unidades se van en orden de lectura desde el reposo y quedan ocultas. Los deslizamientos conservan la dirección del revelado. Aceleran al salir (cúbica de entrada por defecto).],
  none,
)

#api-entry(
  name: "Anim.blur_in",
  kind: "method",
  params: (
    (name: "sigma", type: "float", default: "0.3", desc: [Desenfoque inicial en unidades de escena.]),
    (name: "by", type: "\"grapheme\" | \"word\" | \"line\" | \"part\"", default: "\"grapheme\"", desc: [Unidad que entra de una vez.]),
    (name: "stagger", type: "float", default: "0.02", desc: [Segundos entre unidades.]),
  ),
  returns: (type: "Anim", desc: [Entrada desde un desenfoque transparente.]),
  desc: [Cada unidad pierde el desenfoque y aparece con una curva de salida. Un `sigma` o `stagger` negativo lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Enfoque", role="title")
scene.play(title.animate.blur_in(sigma=0.3, by="grapheme", stagger=0.02))
```
]

#api-entry(
  name: "Anim.tracking",
  kind: "method",
  params: ((name: "value", type: "float", default: none, desc: [Espacio extra entre glifos vecinos, en unidades de escena; `0` restaura el original.]),),
  returns: (type: "Anim", desc: [Cambio animado del espaciado.]),
  desc: [Los glifos se desplazan por la línea base sin rehacer el diseño, anclados según la alineación del texto. Easing predeterminado: suave. Se combina con animaciones que no mueven glifos, como `blur_in`; `scene.play` rechaza dos animaciones simultáneas que escriben el mismo canal de glifos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("ESPACIO", role="title")
title.tracking(0.4)
scene.play(title.animate.tracking(0.0).duration(1.2))
```
]

== Transformaciones

#api-entry(
  name: "Anim.transform_to",
  kind: "method",
  params: ((name: "target", type: "Drawable", default: none, desc: [Objeto de la misma escena cuya forma se adopta.]),),
  returns: (type: "Anim", desc: [Transformación en el sitio.]),
  desc: [Transforma la geometría del objeto hasta la del destino. Respeta los tiempos compuestos y un relleno ausente no se inventa. En un `Text`, al terminar adopta la línea base medida del destino, también en ecuaciones con índices o límites.],
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
  name: "Anim.fade_transform_to",
  kind: "method",
  params: ((name: "target", type: "Drawable", default: none, desc: [Objeto de la misma escena.]),),
  returns: (type: "Anim", desc: [Fundido cruzado hacia el destino.]),
  none,
)

#api-entry(
  name: "Anim.replacement_transform_to",
  kind: "method",
  params: ((name: "target", type: "Drawable", default: none, desc: [Objeto de la misma escena.]),),
  returns: (type: "Anim", desc: [Transformación que termina sustituyendo el origen por el destino.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>square = scene.geometry.square(1.5).fill(BLUE)
>>>circle = scene.geometry.circle(0.8).fill(GOLD).move_to(3, 0)
scene.play(square.animate.replacement_transform_to(circle))
```
]

#api-entry(
  name: "Geometry.transform_matching_shapes",
  kind: "method",
  params: (
    (name: "source", type: "Drawable", default: none, desc: [Objeto o grupo de origen.]),
    (name: "target", type: "Drawable", default: none, desc: [Objeto o grupo de destino.]),
    (name: "duration", type: "float", default: "1.0", desc: [Segundos; finito y positivo.]),
  ),
  returns: (type: "None", desc: [Programa la transformación en el cursor.]),
  desc: [Empareja las piezas de origen y destino por forma, posición y color, transforma las parejas y funde el resto. *No avanza el cursor*: añade `scene.wait(duration)` después, o la escena puede terminar antes de que acabe.],
)[
```python
from gaanim import GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
e1 = scene.text("$E = m c$").fill(WHITE).scale_to(1.3)
e2 = scene.text("$p = m v$").fill(GOLD).scale_to(1.3)
scene.play(e1.animate.write())
scene.geometry.transform_matching_shapes(e1, e2, duration=1.2)
scene.wait(1.2)
scene.render()
```
]

#api-entry(
  name: "Geometry.transform_matching",
  kind: "method",
  params: (
    (name: "source, target", type: "Drawable", default: none, desc: [Objetos de origen y destino.]),
    (name: "mode", type: "str", default: "\"shapes\"", desc: [`"shapes"` empareja por geometría; `"tex"` empareja glifos de textos y ecuaciones por carácter.]),
    (name: "duration", type: "float", default: "1.0", desc: [Segundos; finito y positivo.]),
  ),
  returns: (type: "None", desc: [Programa la transformación en el cursor, sin avanzarlo.]),
  desc: [Versión general de `transform_matching_shapes`. Para textos estructurados es preferible `animate.transform_to`, que es un `Anim` normal.],
  none,
)

== Animaciones personalizadas <personalizadas>

#api-entry(
  name: "Anim.custom",
  kind: "method",
  params: (
    (name: "callback", type: "Callable[[float], CustomAnimationValues]", default: none, desc: [Recibe el progreso ya con easing y devuelve un diccionario con exactamente los canales declarados.]),
    (name: "channels", type: "Sequence[AnimationChannel]", default: none, desc: [Canales que escribe: `"position"`, `"rotation"`, `"scale"`, `"opacity"`, `"fill"`, `"stroke"`, `"stroke_width"`.]),
  ),
  returns: (type: "Anim", desc: [Una animación normal: admite duración, retraso, easing y composición.]),
  desc: [Los valores son absolutos: `position` es una pareja o terna local en unidades de escena, `rotation` un ángulo Z en radianes, `scale` un factor o una terna, `opacity` un número entre 0 y 1, `fill` y `stroke` pinturas y `stroke_width` un ancho no negativo. La función se evalúa en el instante exacto al reproducir, buscar o exportar; debe ser síncrona y pura, sin modificar la escena. El progreso puede salir de `[0, 1]` con algunos easings. No mezcles `custom` y setters en un mismo `Anim`: usa `parallel`. Si falla durante la reproducción, los canales vuelven a su valor inicial y aparece un diagnóstico; la exportación falla. Los tipos `AnimationChannel` y `CustomAnimationValues` se importan desde `gaanim`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>dot = scene.geometry.dot(0.1)
motion = dot.animate.custom(
    lambda alpha: {
        "position": (3 * alpha, alpha * alpha),
        "opacity": 1 - 0.5 * alpha,
    },
    channels=("position", "opacity"),
).duration(2).easing(Easing.SMOOTH)
scene.play(parallel(motion, dot.animate.fill(BLUE).duration(2)))
```
]

== Efectos y trazos <efectos>

Estos destinos interpolan los efectos estáticos del mismo nombre de
#link("/referencia/drawable/")[Drawable] y se combinan con otros destinos de
propiedad.

#api-entry(
  name: "Anim.glow",
  kind: "method",
  params: (
    (name: "color", type: "Color | None", default: "None", desc: [Color del resplandor; `None` lo desvanece.]),
    (name: "radius", type: "float", default: "0.16", desc: [Radio en unidades de escena.]),
    (name: "intensity", type: "float", default: "1.0", desc: [Intensidad.]),
  ),
  returns: (type: "Anim", desc: [Resplandor animado.]),
  desc: [Un objeto sin resplandor lo hace crecer desde intensidad cero. Valores inválidos lanzan `ValueError`; en una selección de texto, `TypeError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>orb = scene.geometry.circle(0.5).fill(CYAN)
scene.play(orb.animate.glow(CYAN, radius=0.5, intensity=2.0).repeat(3, yoyo=True))
```
]

#api-entry(
  name: "Anim.blur",
  kind: "method",
  params: ((name: "sigma", type: "float", default: "0.04", desc: [Desenfoque final; `0` termina nítido.]),),
  returns: (type: "Anim", desc: [Desenfoque animado.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>hero = scene.text("Hola", role="title")
hero.blur(0.3)
scene.play(hero.animate.blur(0.0).duration(0.6))   # entrada desde el desenfoque
```
]

#api-entry(
  name: "Anim.shadow",
  kind: "method",
  params: (
    (name: "color", type: "Color | None", default: "None", desc: [Color de la sombra; `None` la desvanece. Su alfa escala la opacidad de forma continua.]),
    (name: "x, y", type: "float", default: "0.08, -0.08", desc: [Desplazamiento de la sombra.]),
    (name: "blur", type: "float", default: "0.06", desc: [Desenfoque de la sombra.]),
  ),
  returns: (type: "Anim", desc: [Sombra animada.]),
  desc: [Una sombra nueva crece desde debajo del objeto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>card = scene.geometry.rounded_rect(3, 2, 0.2).fill(WHITE)
card.shadow(BLACK, 0, -0.05, 0.05)
scene.play(card.animate.shadow(BLACK, 0, -0.25, 0.4).scale_to(1.04))   # levantar
```
]

#api-entry(
  name: "Anim.trim",
  kind: "method",
  params: (
    (name: "start, end", type: "float | None", default: "None", desc: [Ventana visible del trazo, en fracciones de longitud de arco en `[0, 1]`.]),
    (name: "offset", type: "float | None", default: "None", desc: [Desplaza la ventana y da la vuelta al final del camino.]),
  ),
  returns: (type: "Anim", desc: [Recorte animado del trazo.]),
  desc: [Los valores omitidos conservan el actual. Animar `offset` hace viajar un segmento. El recorte también se mantiene en trazos que se regeneran en cada fotograma, como conectores, líneas entre extremos y curvas reactivas. Ver #link("/referencia/drawable/#api-drawable-trim")[`Drawable.trim`].],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>logo = scene.geometry.star(5, 1, 0.5).no_fill().stroke(WHITE, 0.04).move_to(-4, 0)
>>>ring = scene.geometry.circle(1).no_fill().stroke(WHITE, 0.04)
>>>orbit = scene.geometry.circle(1.5).no_fill().stroke(CYAN, 0.04).move_to(4, 0)
logo.trim(end=0.0)
scene.play(logo.animate.trim(end=1.0).duration(1.2))             # dibujar
ring.trim(start=0.5, end=0.5)
scene.play(ring.animate.trim(start=0.0, end=1.0))                # desde el centro
orbit.trim(start=0.0, end=0.15)
scene.play(orbit.animate.trim(offset=1.0).duration(2))           # segmento viajero
```
]

== Tiempo

#api-entry(
  name: "Anim.duration",
  kind: "method",
  params: ((name: "seconds", type: "float", default: none, desc: [Duración finita y no negativa.]),),
  returns: (type: "Anim", desc: [El `Anim` configurado.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(0.5).move_to(-4, 0)
scene.play(circle.animate.shift_by(3, 0).duration(1.0).delay(0.3).easing(Easing.LINEAR))
```
]

#api-entry(
  name: "Anim.delay",
  kind: "method",
  params: ((name: "seconds", type: "float", default: none, desc: [Espera antes de empezar; finita y no negativa.]),),
  returns: (type: "Anim", desc: [El `Anim` configurado.]),
  none,
)

#api-entry(
  name: "Anim.easing",
  kind: "method",
  params: ((name: "easing", type: "Easing", default: none, desc: [Preset o resultado de una fábrica de `Easing`.]),),
  returns: (type: "Anim", desc: [El `Anim` configurado.]),
  desc: [No se aceptan nombres en texto: usa siempre un `Easing`. Ver #link(<easing>)[Easing].],
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
  name: "Anim.lag_ratio",
  kind: "method",
  params: ((name: "value", type: "float", default: none, desc: [Solapamiento entre subtrazos, en `[0, 1]`: `0` los anima a la vez y `1` uno detrás de otro.]),),
  returns: (type: "Anim", desc: [El `Anim` configurado.]),
  desc: [Escalona los subcaminos o miembros dentro de un mismo objeto, por ejemplo los de un grupo.],
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

#api-entry(
  name: "Anim.repeat",
  kind: "method",
  params: (
    (name: "count", type: "int", default: none, desc: [Número de ciclos, entre 1 y 10000.]),
    (name: "yoyo", type: "bool", default: "False", desc: [Alterna el sentido de cada ciclo.]),
    (name: "delay", type: "float", default: "0.0", desc: [Segundos entre ciclos; no negativo.]),
  ),
  returns: (type: "Anim", desc: [El `Anim` repetido.]),
  desc: [`duration` y `easing` describen un ciclo. Con `yoyo=True` un número par de ciclos termina donde empezó y las animaciones siguientes continúan desde ahí. La duración total es `count * duration + (count - 1) * delay`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>import math
>>>spinner = scene.geometry.square(0.8).move_to(-4, 0)
>>>badge = scene.geometry.circle(0.4).move_to(-1.5, 0)
scene.play(spinner.animate.rotate_by(math.tau).duration(1.2).repeat(3))
scene.play(badge.animate.scale_to(1.08).duration(0.4).repeat(4, yoyo=True, delay=0.1))
```
]

#api-entry(
  name: "Anim.loop",
  kind: "method",
  params: (
    (name: "mode", type: "\"cycle\" | \"pingpong\" | \"offset\"", default: "\"cycle\"", desc: [`"cycle"` reinicia cada ciclo, `"pingpong"` alterna el sentido y `"offset"` continúa desde donde terminó el anterior, así `rotate_by` o `shift_by` se acumulan.]),
    (name: "until", type: "float", default: none, desc: [Segundos disponibles; caben tantos ciclos completos como sea posible, al menos uno.]),
    (name: "delay", type: "float", default: "0.0", desc: [Segundos entre ciclos.]),
  ),
  returns: (type: "Anim", desc: [El `Anim` repetido.]),
  desc: [El bucle es finito, así que los seeks y la exportación son exactos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>arrow = scene.geometry.arrow(0.5, 0, 2, 0)
scene.play(arrow.animate.shift_by(0.3, 0).duration(0.5).loop("pingpong", until=4.0))
```
]

== Easing <easing>

Un `Easing` es una función de tiempo inmutable: convierte el progreso lineal
del clip en el progreso que se ve. Hay presets con nombre y fábricas que
validan sus argumentos; no se aceptan nombres en texto ni hay un valor de
reserva silencioso. Todas rechazan números no finitos y dominios inválidos con
`ValueError`.

#api-entry(
  name: "Easing.LINEAR / SMOOTH / DOUBLE_SMOOTH / THERE_AND_BACK / LINGERING / RUNNING_START / EXPONENTIAL_DECAY / NOT_QUITE_THERE",
  kind: "constant",
  signature: "Easing.LINEAR · Easing.SMOOTH · …: Easing",
  desc: [Curvas clásicas.],
)[
#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Preset*], [*Forma*],
  [`LINEAR`], [Velocidad constante.],
  [`SMOOTH`], [Arranca y frena suave (`3t² - 2t³`). Es el easing predeterminado de muchas animaciones.],
  [`DOUBLE_SMOOTH`], [`SMOOTH` aplicado dos veces: arranque y frenado más marcados. Predeterminado de `create`.],
  [`THERE_AND_BACK`], [Llega al destino a mitad del clip y vuelve al inicio.],
  [`LINGERING`], [Curva suave de quinto grado (`6t⁵ - 15t⁴ + 10t³`), con extremos todavía más lentos que `SMOOTH`.],
  [`RUNNING_START`], [Retrocede un poco antes de arrancar hacia el destino.],
  [`EXPONENTIAL_DECAY`], [Sale rápido y se acerca al destino cada vez más despacio.],
  [`NOT_QUITE_THERE`], [Como `EXPONENTIAL_DECAY`, pero se queda en el 95 % del recorrido.],
)

```python
# show-code: true
from gaanim import Anchor, Easing, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
presets = [("LINEAR", Easing.LINEAR), ("SMOOTH", Easing.SMOOTH), ("DOUBLE_SMOOTH", Easing.DOUBLE_SMOOTH), ("RUNNING_START", Easing.RUNNING_START), ("EXPONENTIAL_DECAY", Easing.EXPONENTIAL_DECAY)]
moves = []
for row, (name, easing) in enumerate(presets):
    y = 2.4 - 1.2 * row
    scene.text(name, size=0.32).move_to(-3.4, y, Anchor.RIGHT)
    dot = scene.geometry.dot(0.16).fill(GOLD).move_to(-1.6, y)
    moves.append(dot.animate.move_to(6, y).duration(1.5).easing(easing))
scene.play(moves)
scene.wait(0.3)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Easing.SMOOTH_SPRING / GENTLE / QUICK / SNAPPY / BOUNCY",
  kind: "constant",
  signature: "Easing.SMOOTH_SPRING · Easing.GENTLE · …: Easing",
  desc: [Resortes perceptuales con nombre: se asientan dentro de la duración de la animación y terminan exactamente en el destino.],
)[
#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*Preset*], [*Carácter*],
  [`SMOOTH_SPRING`], [Amortiguamiento crítico: el asentamiento más rápido sin pasarse.],
  [`GENTLE`], [Suave y sin prisa, con un sobrepaso apenas visible.],
  [`QUICK`], [Enérgico, con un sobrepaso pequeño que se asienta pronto.],
  [`SNAPPY`], [Rápido, con un sobrepaso nítido del 20 %.],
  [`BOUNCY`], [Juguetón, con un sobrepaso del 45 % y rebotes visibles.],
)

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>card = scene.geometry.rounded_rect(3, 2, 0.2).scale_to(0.2)
scene.play(card.animate.scale_to(1.0).duration(0.6).easing(Easing.SNAPPY))
```
]

#api-entry(
  name: "EasingCurve.QUADRATIC / CUBIC / QUARTIC / QUINTIC / EXPONENTIAL / SINE / CIRCULAR / BACK / ELASTIC / BOUNCE",
  kind: "constant",
  signature: "EasingCurve.QUADRATIC · EasingCurve.CUBIC · …: EasingCurve",
  desc: [Familias de curvas para `Easing.ease_in`, `ease_out` y `ease_in_out`. `QUADRATIC` a `QUINTIC` son potencias de grado 2 a 5; `EXPONENTIAL`, `SINE` y `CIRCULAR`, las curvas clásicas de CSS; `BACK` se pasa un poco del destino, `ELASTIC` oscila y `BOUNCE` rebota.],
  none,
)

#api-entry(
  name: "Easing.ease_in",
  kind: "factory",
  params: ((name: "curve", type: "EasingCurve", default: none, desc: [Familia de la curva.]),),
  returns: (type: "Easing", desc: [Arranque lento y final rápido.]),
  none,
)

#api-entry(
  name: "Easing.ease_out",
  kind: "factory",
  params: ((name: "curve", type: "EasingCurve", default: none, desc: [Familia de la curva.]),),
  returns: (type: "Easing", desc: [Arranque rápido y final lento.]),
  none,
)

#api-entry(
  name: "Easing.ease_in_out",
  kind: "factory",
  params: ((name: "curve", type: "EasingCurve", default: none, desc: [Familia de la curva.]),),
  returns: (type: "Easing", desc: [Arranque y final lentos.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>circle = scene.geometry.circle(0.5).move_to(-4, 0)
scene.play(circle.animate.shift_by(8, 0).easing(Easing.ease_in_out(EasingCurve.CUBIC)))
scene.play(circle.animate.shift_by(-8, 0).easing(Easing.ease_out(EasingCurve.BOUNCE)))
```
]

#api-entry(
  name: "Easing.spring",
  kind: "factory",
  params: (
    (name: "stiffness", type: "float | None", default: "90", desc: [Rigidez física; positiva.]),
    (name: "damping", type: "float | None", default: "12", desc: [Amortiguamiento físico; no negativo.]),
    (name: "mass", type: "float", default: "1.0", desc: [Masa física; positiva.]),
    (name: "velocity", type: "float", default: "0.0", desc: [Velocidad inicial, en distancias por duración del clip.]),
    (name: "bounce", type: "float | None", default: "None", desc: [Resorte perceptual: sobrepaso máximo en `[0, 1)`; `0` es amortiguamiento crítico.]),
  ),
  returns: (type: "Easing", desc: [Un resorte que termina exactamente en el destino.]),
  desc: [Con `bounce`, el resorte se describe por cómo se ve: se asienta dentro de la duración de la animación, así que `duration` marca el ritmo. Sin él es físico (`stiffness`, `damping`, `mass`) y el clip abarca cinco segundos físicos. Combinar `bounce` con parámetros físicos lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>logo = scene.geometry.star(5, 1, 0.5).scale_to(0.3)
scene.play(logo.animate.scale_to(1.0).duration(0.6).easing(Easing.spring(bounce=0.35)))
scene.play(logo.animate.rotate_by(1.0).easing(Easing.spring(stiffness=90, damping=12)))
```
]

#api-entry(
  name: "Easing.back",
  kind: "factory",
  params: (
    (name: "overshoot", type: "float", default: "1.70158", desc: [Cuánto retrocede o se pasa; no negativo.]),
    (name: "mode", type: "\"in\" | \"out\" | \"in_out\"", default: "\"out\"", desc: [`"in"` retrocede al empezar, `"out"` se pasa al llegar y `"in_out"` hace las dos cosas.]),
  ),
  returns: (type: "Easing", desc: [Retroceso o sobrepaso.]),
  none,
)

#api-entry(
  name: "Easing.elastic",
  kind: "factory",
  params: (
    (name: "amplitude", type: "float", default: "1.0", desc: [Amplitud de la oscilación; al menos 1.]),
    (name: "period", type: "float", default: "0.3", desc: [Periodo en fracciones del clip; positivo.]),
    (name: "mode", type: "\"in\" | \"out\" | \"in_out\"", default: "\"out\"", desc: [Extremo en el que oscila.]),
  ),
  returns: (type: "Easing", desc: [Oscilación como una banda elástica.]),
  none,
)

#api-entry(
  name: "Easing.bounce",
  kind: "factory",
  params: (
    (name: "strength", type: "float", default: "1.0", desc: [Mezcla de una curva cúbica (`0`) al rebote clásico (`1`).]),
    (name: "mode", type: "\"in\" | \"out\" | \"in_out\"", default: "\"out\"", desc: [Extremo en el que rebota.]),
  ),
  returns: (type: "Easing", desc: [Rebotes al llegar.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>ball = scene.geometry.circle(0.3).move_to(0, 3)
scene.play(ball.animate.move_to(0, -3).duration(1.2).easing(Easing.bounce()))
scene.play(ball.animate.move_to(0, 0).easing(Easing.elastic(amplitude=1.2, period=0.4)))
scene.play(ball.animate.shift_by(3, 0).easing(Easing.back(mode="in_out")))
```
]

#api-entry(
  name: "Easing.slow_mo",
  kind: "factory",
  params: (
    (name: "linear_ratio", type: "float", default: "0.7", desc: [Fracción central que avanza lenta y lineal, en `[0, 1]`.]),
    (name: "power", type: "float", default: "0.7", desc: [Fuerza de la aceleración en los extremos, en `[0, 1]`.]),
  ),
  returns: (type: "Easing", desc: [Rápido, cámara lenta, rápido.]),
  none,
)

#api-entry(
  name: "Easing.rough",
  kind: "factory",
  params: (
    (name: "strength", type: "float", default: "1.0", desc: [Intensidad del temblor.]),
    (name: "points", type: "int", default: "20", desc: [Número de nudos aleatorios.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla de los nudos.]),
  ),
  returns: (type: "Easing", desc: [Rampa temblorosa determinista, para parpadeos y glitches.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>sign = scene.text("ABIERTO", role="title")
scene.play(sign.animate.opacity(0.2).duration(1.0).easing(Easing.rough(strength=1, points=20, seed=4)))
scene.play(sign.animate.shift_by(4, 0).duration(1.5).easing(Easing.slow_mo(0.7, 0.7)))
```
]

#api-entry(
  name: "Easing.squish",
  kind: "factory",
  params: (
    (name: "easing", type: "Easing", default: none, desc: [Easing que se comprime.]),
    (name: "start, end", type: "float", default: none, desc: [Tramo del clip en el que actúa, con `0 <= start < end <= 1`.]),
  ),
  returns: (type: "Easing", desc: [El easing solo dentro del tramo; fuera mantiene sus extremos.]),
  desc: [Útil para que una animación de una lista paralela empiece más tarde o termine antes que las demás sin cambiar su duración.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>a = scene.geometry.dot(0.2).move_to(-4, 1)
>>>b = scene.geometry.dot(0.2).move_to(-4, -1)
scene.play([
    a.animate.shift_by(8, 0).duration(2),
    b.animate.shift_by(8, 0).duration(2).easing(Easing.squish(Easing.SMOOTH, 0.5, 1.0)),
])
```
]

#api-entry(
  name: "Easing.steps",
  kind: "factory",
  params: (
    (name: "count", type: "int", default: none, desc: [Número de escalones.]),
    (name: "jump", type: "\"start\" | \"end\" | \"none\" | \"both\"", default: "\"end\"", desc: [Dónde ocurren los saltos, como en `steps()` de CSS.]),
  ),
  returns: (type: "Easing", desc: [Interpolación discreta.]),
  none,
)

#api-entry(
  name: "Easing.mirror",
  kind: "factory",
  params: ((name: "easing", type: "Easing", default: none, desc: [Easing de la primera mitad.]),),
  returns: (type: "Easing", desc: [El easing comprimido en la primera mitad y reflejado en la segunda.]),
  desc: [Convierte un `ease_in` en una curva simétrica de entrada y salida.],
  none,
)

#api-entry(
  name: "Easing.there_and_back",
  kind: "factory",
  params: ((name: "pause", type: "float", default: "0.0", desc: [Fracción del clip que se detiene en el destino, en `[0, 1]`.]),),
  returns: (type: "Easing", desc: [Ida y vuelta.]),
  desc: [Llega al destino y regresa al inicio dentro del mismo clip.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>clock = scene.geometry.rect(0.1, 1.5).with_pivot(0, 0)
>>>badge = scene.geometry.circle(0.5).move_to(3, 0)
scene.play(clock.animate.rotate_by(-6.283).duration(2).easing(Easing.steps(12)))
scene.play(badge.animate.scale_to(1.3).duration(0.8).easing(Easing.there_and_back(pause=0.2)))
scene.play(badge.animate.shift_by(-6, 0).easing(Easing.mirror(Easing.ease_in(EasingCurve.QUINTIC))))
```
]

#api-entry(
  name: "Easing.cubic_bezier",
  kind: "factory",
  params: ((name: "x1, y1, x2, y2", type: "float", default: none, desc: [Puntos de control, como en `cubic-bezier()` de CSS; `x1` y `x2` en `[0, 1]`.]),),
  returns: (type: "Easing", desc: [Una curva de Bézier cúbica.]),
  none,
)

#api-entry(
  name: "Easing.from_svg",
  kind: "factory",
  params: (
    (name: "path", type: "str", default: none, desc: [Camino SVG dibujado en el cuadrado unidad (x = tiempo, y = progreso).]),
    (name: "samples", type: "int", default: "256", desc: [Muestras de la tabla interpolada.]),
  ),
  returns: (type: "Easing", desc: [La curva dibujada.]),
  desc: [El camino debe ir de x = 0 a x = 1 sin retroceder en x y quedarse con y en `[-1, 2]`; si no, lanza `ValueError`. Sirve para una curva copiada de cualquier editor.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>panel = scene.geometry.rect(4, 2).move_to(-4, 0)
scene.play(panel.animate.shift_by(8, 0).easing(Easing.cubic_bezier(0.2, 0.8, 0.2, 1.0)))
scene.play(panel.animate.shift_by(-8, 0).easing(Easing.from_svg("M0,0 C0.3,0 0.2,1.2 1,1")))
```
]

#api-entry(
  name: "Easing.custom",
  kind: "factory",
  params: (
    (name: "function", type: "Callable[[float], float]", default: none, desc: [Función de `t` en `[0, 1]` que devuelve el progreso.]),
    (name: "samples", type: "int", default: "256", desc: [Muestras, en `[2, 65536]`.]),
  ),
  returns: (type: "Easing", desc: [Una tabla interpolada linealmente.]),
  desc: [Llama a la función `samples` veces, en tiempos equiespaciados, al crear el easing. El render nunca vuelve a Python, así que la previsualización, los seeks y la exportación coinciden. Los valores deben ser finitos y estar en `[-1, 2]` (se permite pasarse del destino); si no, lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>import math
>>>ball = scene.geometry.circle(0.3).move_to(0, 2)
salto = Easing.custom(lambda t: 1 - abs(math.cos(3 * math.pi * t)) * (1 - t) ** 2)
scene.play(ball.animate.move_to(0, -2).duration(1.2).easing(salto))
```
]

== Composición

`scene.play([...])` ya reproduce una lista en paralelo. Para estructuras más
ricas, estas funciones construyen un árbol inmutable, `Composition`, que
`scene.play` resuelve de forma atómica: valores por defecto, tramos, solapes,
conflictos de canales y destinos relativos.

#api-entry(
  name: "parallel",
  kind: "function",
  signature: "parallel(*items: Playable) -> Composition",
  params: ((name: "items", type: "Playable", default: none, desc: [Uno o más `Anim`, medios o composiciones.]),),
  returns: (type: "Composition", desc: [Todos empiezan en el mismo origen.]),
  none,
)

#api-entry(
  name: "sequence",
  kind: "function",
  signature: "sequence(*items: Playable, gap: float = 0.0) -> Composition",
  params: (
    (name: "items", type: "Playable", default: none, desc: [Uno o más elementos.]),
    (name: "gap", type: "float", default: "0.0", desc: [Segundos entre pasos; un valor negativo acotado los solapa.]),
  ),
  returns: (type: "Composition", desc: [Uno detrás de otro.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>title = scene.text("Título").move_to(0, 2)
>>>box = scene.geometry.rect(2, 1)
>>>label = scene.text("Etiqueta").move_to(-3, -2)
>>>badge = scene.geometry.circle(0.3).move_to(3, -2)
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
]

#api-entry(
  name: "stagger",
  kind: "function",
  signature: "stagger(*items: Playable, each: float = 0.1, total: float | None = None, origin: StaggerOrigin | None = None, grid: Literal[\"auto\"] | tuple[int, int] | None = None, easing: Easing | None = None, seed: int = 0) -> Composition",
  params: (
    (name: "items", type: "Playable", default: none, desc: [Elementos escalonados.]),
    (name: "each", type: "float", default: "0.1", desc: [Retardo por índice, o por paso de separación con `origin`; no negativo.]),
    (name: "total", type: "float | None", default: "None", desc: [Fija la duración de toda la onda en lugar de `each`.]),
    (name: "origin", type: "StaggerOrigin | None", default: "None", desc: [`"start"`, `"end"`, `"center"`, `"edges"` (de los bordes hacia dentro), `"random"` o un punto `(x, y)`.]),
    (name: "grid", type: "\"auto\" | tuple[int, int] | None", default: "None", desc: [Distancias según las posiciones declaradas (`"auto"`) o las celdas de `(filas, columnas)`.]),
    (name: "easing", type: "Easing | None", default: "None", desc: [Da forma al reparto de retardos.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla de `origin="random"`.]),
  ),
  returns: (type: "Composition", desc: [Elementos desplazados en el tiempo.]),
  desc: [Sin `origin`, `grid`, `total` ni `easing`, escalona por índice. Con cualquiera de ellos, el retardo crece con la distancia al origen. Los elementos cuya posición depende de un layout usan su índice.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>dots = [scene.geometry.dot(0.12).move_to(x, y) for x in range(-3, 4) for y in (-1, 0, 1)]
scene.play(stagger(*[d.animate.grow_from_center() for d in dots], each=0.03, origin="center"))
scene.play(stagger(*[d.animate.indicate() for d in dots], total=1.2, origin="random", seed=7))
scene.play(stagger(*[d.animate.fill(GOLD) for d in dots], each=0.05, origin=(0.0, -3.0)))
```
]

#api-entry(
  name: "distribute",
  kind: "function",
  signature: "distribute(items: Sequence[Drawable], low: float, high: float, *, origin: StaggerOrigin | None = None, grid: Literal[\"auto\"] | tuple[int, int] | None = None, easing: Easing | None = None, seed: int = 0) -> list[float]",
  params: (
    (name: "items", type: "Sequence[Drawable]", default: none, desc: [Objetos que reciben un valor cada uno.]),
    (name: "low, high", type: "float", default: none, desc: [Rango de valores.]),
    (name: "origin, grid, easing, seed", type: "", default: "None, None, None, 0", desc: [Como en `stagger`.]),
  ),
  returns: (type: "list[float]", desc: [Un valor por objeto.]),
  desc: [Usa el mismo orden que `stagger` para repartir tamaños, colores u opacidades en lugar de tiempos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>dots = [scene.geometry.dot(0.12).move_to(x, 0) for x in range(-3, 4)]
for dot, size in zip(dots, distribute(dots, 0.4, 1.4, origin="edges")):
    dot.scale_by(size)
```
]

`Playable` es la unión de todo lo que acepta `scene.play` (`Anim`, `Audio`,
`Video`, `VideoSegment`, `Lottie` y `Composition`). Impórtala desde `gaanim`
para anotar tus funciones; también funciona con `isinstance`.

```python
from gaanim import Playable, parallel

def entrada(*items: Playable) -> Playable:
    return parallel(*items).delay(0.2)
```

#api-entry(
  name: "label",
  kind: "function",
  signature: "label(name: str) -> Composition",
  params: ((name: "name", type: "str", default: none, desc: [Nombre único en el árbol; se recortan los espacios.]),),
  returns: (type: "Composition", desc: [Un instante con nombre y sin duración.]),
  desc: [En un `sequence` no ocupa un paso ni suma `gap`: marca dónde empieza el siguiente paso, o dónde termina el anterior si es el último. Se referencia desde `Composition.insert` y sus tiempos aparecen en `Schedule.labels`. Un nombre vacío, numérico o que empieza por `<`, `>`, `+`, `-` o `=` lanza `ValueError`. Para instantes de la línea de tiempo global, usa #link("/referencia/scene/#api-scene-marker")[`scene.marker`].],
  none,
)

#api-entry(
  name: "Composition.insert",
  kind: "method",
  params: (
    (name: "item", type: "Playable", default: none, desc: [Elemento que se añade; puede ser otro `label`.]),
    (name: "at", type: "str | float", default: none, desc: [Posición local, con la sintaxis de la tabla.]),
  ),
  returns: (type: "Composition", desc: [Una copia; el original no cambia.]),
  desc: [Las inserciones se colocan después de los hijos y antes de `stretch`, `repeat` y `delay`, que también las afectan. Una posición mal escrita lanza `ValueError` enseguida; una etiqueta desconocida (el error lista las definidas) o una posición antes de 0 lo lanzan en `schedule()` o `scene.play`. Un nombre exacto de etiqueta siempre gana, así que `"parte-2"` encuentra la etiqueta `"parte-2"`.],
)[
#table(
  columns: (auto, 1fr),
  inset: 7pt,
  [*`at`*], [*Posición*],
  [`"nombre"`, `"nombre+0.15"`, `"nombre-0.2"`, `"nombre+=0.15"`], [Una etiqueta más un desplazamiento.],
  [`"<"`, `">"`, `"<+0.1"`, `">-0.2"`], [Inicio o final del elemento anterior: la última inserción que no es etiqueta o, si no hay, el último hijo.],
  [`"+=0.3"`, `"-=0.3"`], [Relativo al final actual de la composición.],
  [`2.5`], [Segundos locales absolutos.],
)

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>title = scene.text("Título").move_to(0, 2)
>>>subtitle = scene.text("Subtítulo").move_to(0, 1)
>>>logo = scene.geometry.circle(0.6)
>>>glow = scene.geometry.circle(0.8).no_fill().stroke(GOLD, 0.04)
>>>footer = scene.text("Pie").move_to(0, -3)
intro = (
    sequence(
        title.animate.write().duration(0.8),
        label("golpe"),
        subtitle.animate.fade_in().duration(0.4),
    )
    .insert(logo.animate.grow_from_center(), at="golpe+0.15")
    .insert(glow.animate.flash(), at="<")          # empieza con el logo
    .insert(footer.animate.fade_in(), at="-=0.2")  # se solapa con el final
)
print(intro.schedule().labels)  # {'golpe': 0.8}
scene.play(intro)
```
]

#api-entry(
  name: "Composition.defaults",
  kind: "method",
  params: (
    (name: "duration", type: "float | None", default: "None", desc: [Duración para las animaciones descendientes que no fijan la suya.]),
    (name: "easing", type: "Easing | None", default: "None", desc: [Easing para las que no fijan el suyo.]),
  ),
  returns: (type: "Composition", desc: [Una copia configurada.]),
  none,
)

#api-entry(
  name: "Composition.delay",
  kind: "method",
  params: ((name: "seconds", type: "float", default: none, desc: [Espera antes de todo el subárbol.]),),
  returns: (type: "Composition", desc: [Una copia retrasada.]),
  none,
)

#api-entry(
  name: "Composition.stretch",
  kind: "method",
  params: ((name: "seconds", type: "float", default: none, desc: [Duración exacta del subárbol.]),),
  returns: (type: "Composition", desc: [Una copia reescalada.]),
  desc: [Solo para árboles de animaciones: con medios lanza un error, porque cambiaría su velocidad de reproducción.],
  none,
)

#api-entry(
  name: "Composition.repeat",
  kind: "method",
  params: (
    (name: "count", type: "int", default: none, desc: [Repeticiones; al menos 1.]),
    (name: "delay", type: "float", default: "0.0", desc: [Segundos entre repeticiones.]),
  ),
  returns: (type: "Composition", desc: [El árbol repetido.]),
  desc: [Cada repetición parte del estado en que la dejó la anterior, así que las animaciones relativas se acumulan. Con medios, `count < 1` o un `delay` negativo lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>import math
>>>a = scene.geometry.square(0.5).move_to(3, 1)
>>>b = scene.geometry.dot(0.1).move_to(3, -1)
scene.play(parallel(a.animate.rotate_by(math.tau), b.animate.shift_by(1, 0)).repeat(2))
```
]

#api-entry(
  name: "Composition.schedule",
  kind: "method",
  params: ((name: "duration", type: "float | None", default: "None", desc: [Duración por defecto exterior con la que se resuelve.]),),
  returns: (type: "Schedule", desc: [Los tiempos locales resueltos.]),
  desc: [Inspecciona el árbol sin programarlo ni consumir sus hojas y sin mover el cursor.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>a = scene.geometry.dot(0.1)
>>>b = scene.geometry.dot(0.1).move_to(1, 0)
plan = sequence(a.animate.fade_in().duration(0.5), b.animate.fade_in().duration(0.5), gap=0.2)
schedule = plan.schedule()
print(schedule.span, [(e.kind, e.start, e.end) for e in schedule.entries])
```
]

#api-entry(
  name: "Schedule.span",
  kind: "property",
  returns: (type: "float", desc: [Duración total de la composición.]),
  none,
)

#api-entry(
  name: "Schedule.entries",
  kind: "property",
  returns: (type: "tuple[ScheduleEntry, ...]", desc: [Una entrada por hoja.]),
  desc: [Cada `ScheduleEntry` tiene `path` (índices en el árbol), `kind` (`"animation"`, `"audio"`, `"video"` o `"lottie"`), `start`, `duration` y `end`, en segundos locales; `duration` y `end` son `None` para un medio sin final.],
  none,
)

#api-entry(
  name: "Schedule.labels",
  kind: "property",
  returns: (type: "dict[str, float]", desc: [Tiempos de las etiquetas, en segundos locales y en orden temporal.]),
  desc: [Incluye etiquetas anidadas e insertadas. Una composición reescalada las escala; una repetida informa de la primera repetición.],
  none,
)

Los medios también son hojas de composición: `VideoSegment` admite
`parallel`, `sequence` y `stagger`, pero no `stretch`, y su velocidad se fija
al crearlo (ver #link("/referencia/medios/")[Medios]).

== Updaters

Un updater mueve un objeto en cada fotograma sin programar un `Anim`. Los
presets de `Updater` se evalúan en Rust; se asocian con
#link("/referencia/drawable/#api-drawable-add-updater")[`Drawable.add_updater`]
y se quitan con `Drawable.remove_updater`. Los objetos generados por updaters
siguen ocultos hasta que su propia animación de entrada entra en
`scene.play`.

#api-entry(
  name: "Updater.orbit",
  kind: "factory",
  params: (
    (name: "cx, cy", type: "float", default: none, desc: [Centro de la órbita.]),
    (name: "radius", type: "float", default: none, desc: [Radio.]),
    (name: "speed", type: "float", default: none, desc: [Velocidad angular en radianes por segundo.]),
  ),
  returns: (type: "Updater", desc: [Órbita circular.]),
  desc: [Coloca el objeto en el círculo según el tiempo transcurrido desde que se asocia.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
planet = scene.geometry.dot(0.12).fill(BLUE)
planet.add_updater(Updater.orbit(0, 0, 2, 1.0))
scene.wait(2)
```
]

#api-entry(
  name: "Updater.advance_x",
  kind: "factory",
  params: ((name: "speed", type: "float", default: none, desc: [Unidades por segundo hacia la derecha; negativo va hacia la izquierda.]),),
  returns: (type: "Updater", desc: [Avance horizontal constante.]),
  none,
)

#api-entry(
  name: "Updater.bob",
  kind: "factory",
  params: (
    (name: "amplitude", type: "float", default: none, desc: [Desplazamiento vertical máximo.]),
    (name: "frequency", type: "float", default: none, desc: [Oscilaciones por segundo.]),
  ),
  returns: (type: "Updater", desc: [Oscilación vertical senoidal alrededor de la posición inicial.]),
  none,
)

#api-entry(
  name: "Updater.rotate",
  kind: "factory",
  params: ((name: "speed", type: "float", default: none, desc: [Radianes por segundo.]),),
  returns: (type: "Updater", desc: [Giro continuo alrededor de Z.]),
  none,
)

#api-entry(
  name: "Updater.pulse",
  kind: "factory",
  params: (
    (name: "min_scale, max_scale", type: "float", default: none, desc: [Factores de escala mínimo y máximo respecto de la escala inicial.]),
    (name: "frequency", type: "float", default: none, desc: [Pulsos por segundo.]),
  ),
  returns: (type: "Updater", desc: [Escala que late entre dos factores.]),
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>boat = scene.geometry.rect(1.2, 0.4).move_to(-6, 0)
>>>fan = scene.geometry.star(4, 0.6, 0.2).move_to(4, 1)
>>>heart = scene.geometry.circle(0.4).fill(RED).move_to(4, -2)
boat.add_updater(Updater.advance_x(1.5))
fan.add_updater(Updater.rotate(3.0))
heart.add_updater(Updater.pulse(0.9, 1.1, 1.2))
scene.wait(2)
```
]

#api-entry(
  name: "Updater.wiggle",
  kind: "factory",
  params: (
    (name: "position", type: "float", default: "0.08", desc: [Amplitud del ruido de posición, en unidades de escena.]),
    (name: "rotation", type: "float", default: "0.0", desc: [Amplitud de giro, en radianes.]),
    (name: "scale", type: "float", default: "0.0", desc: [Amplitud de escala, como fracción.]),
    (name: "frequency", type: "float", default: "2.0", desc: [Rapidez del temblor.]),
    (name: "octaves", type: "int", default: "2", desc: [Capas de detalle, de 1 a 8.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla del ruido.]),
  ),
  returns: (type: "Updater", desc: [Temblor orgánico con semilla.]),
  desc: [Es una capa sobre la animación del objeto: empieza en cero, es función pura del tiempo de la línea de tiempo y se suma a `animate.move_to` y a otros clips en lugar de sustituirlos. Un seek cae en el mismo fotograma que la reproducción. Valores inválidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>logo = scene.media.image("assets/logo.webp").scale_to(0.25)
logo.add_updater(Updater.wiggle(position=0.08, rotation=0.03, seed=1))
scene.play([logo.animate.move_to(3, 0).duration(2)])  # sigue temblando mientras se mueve
```
]

#api-entry(
  name: "Updater.oscillate",
  kind: "factory",
  params: (
    (name: "channel", type: "\"x\" | \"y\" | \"rotation\" | \"scale\" | \"opacity\"", default: none, desc: [Canal que oscila.]),
    (name: "waveform", type: "\"sine\" | \"square\" | \"triangle\" | \"saw\"", default: "\"sine\"", desc: [Forma de la onda.]),
    (name: "frequency", type: "float", default: "1.0", desc: [Ciclos por segundo.]),
    (name: "low, high", type: "float", default: "0.0, 1.0", desc: [Valores entre los que oscila; toda onda empieza en `low`.]),
    (name: "phase", type: "float", default: "0.0", desc: [Desfase en ciclos.]),
  ),
  returns: (type: "Updater", desc: [Una capa periódica.]),
  desc: [En `x`, `y` y `rotation` el valor se suma al animado; en `scale` y `opacity` lo multiplica (los factores de opacidad deben estar en `[0, 1]`). Como `wiggle`, es función pura del tiempo y se combina con animaciones.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
light = scene.geometry.circle(0.3).fill(GOLD)
light.add_updater(Updater.oscillate("opacity", waveform="triangle", frequency=0.5, low=0.4, high=1.0))
scene.wait(2)
```
]

#api-entry(
  name: "Drawable.add_updater_fn",
  kind: "method",
  signature: "add_updater_fn(callback, *, reset=None, fixed_dt=None) -> Drawable",
  params: (
    (name: "callback", type: "Callable", default: none, desc: [`callback((x, y, z), dt, elapsed)` devuelve la nueva posición local.]),
    (name: "reset", type: "Callable | None", default: "None", desc: [Restaura todo el estado de Python que guarda una simulación.]),
    (name: "fixed_dt", type: "float | None", default: "None", desc: [Paso de simulación positivo, en segundos.]),
  ),
  returns: (type: "Drawable", desc: [El mismo objeto.]),
  desc: [Pasa `reset` y `fixed_dt` juntos para física o cualquier estado incremental: tras un seek y durante la exportación, Gaanim restaura la posición inicial, llama a `reset()` y repite los subpasos constantes, así el resultado es determinista. El updater empieza en el cursor donde se declara, no en segmentos anteriores. Sin esa pareja, la función está pensada para comportamientos ligeros por fotograma o de tiempo absoluto. Coordenadas inválidas o una excepción detienen el updater.],
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

#api-entry(
  name: "Drawable.drive_from_samples",
  kind: "method",
  signature: "drive_from_samples(times, values, property=\"x\", *, interpolation=\"linear\", scale=1.0, offset=0.0) -> Drawable",
  params: (
    (name: "times, values", type: "Sequence[float] | Sequence[tuple[float, float]]", default: none, desc: [Series de igual longitud; los tiempos deben ser finitos y no decrecientes. Con `"xy"`, los valores son parejas `(x, y)`.]),
    (name: "property", type: "\"x\" | \"y\" | \"xy\" | \"z\" | \"rotation\" | \"scale\" | \"opacity\" | \"signal\"", default: "\"x\"", desc: [Canal que se mueve. `"xy"` mueve los dos ejes de traslación como canales `"x"` e `"y"`.]),
    (name: "interpolation", type: "\"linear\" | \"step\"", default: "\"linear\"", desc: [Interpolación entre muestras.]),
    (name: "scale, offset", type: "float", default: "1.0, 0.0", desc: [Transformación aplicada a cada muestra.]),
  ),
  returns: (type: "Drawable", desc: [El mismo objeto.]),
  desc: [Mueve el canal como función pura del tiempo, evaluada en Rust, sin llamar a Python en cada fotograma. Los ejes de traslación y `rotation` son relativos a la pose declarada (`base + offset + scale * muestra`); `scale`, `opacity` y `signal` son absolutos. Fuera de la serie se mantiene la primera o la última muestra. Los tiempos cuentan desde el cursor donde se llama: una serie declarada después de `scene.wait(2.0)` reproduce su muestra `t = 0` a los dos segundos. Cada canal es independiente; volver a mover el mismo canal lo reemplaza. `remove_updater()` lo quita. `Parameter.drive_from_samples(times, values, *, ...)` hace lo mismo con la señal de un parámetro.],
)[
```python
>>>import math
>>>accel = [0.05 * math.sin(0.3 * i) for i in range(200)]
from gaanim import CYAN, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
times = [i * 0.02 for i in range(len(accel))]
building = scene.geometry.rounded_rect(2, 4.5, 0.125).fill(CYAN).move_to(-2.5, -1.5)
# El edificio oscila con el registro medido; el seek es determinista.
building.drive_from_samples(times, accel, "x", scale=6.5)
scene.play([building.animate.grow_from_center()])
scene.wait(4.0)
```
]

Para un ejemplo con varilla, cota y estela, consulta
`examples/pendulum_simulation.py` en el repositorio de Gaanim.

== Transformaciones 3D

#experimental()

Los mismos destinos en tres ejes, para objetos en el espacio de una cámara en
perspectiva. Las rotaciones de Euler usan orden XYZ y radianes. Las acciones
de modelos glTF están en
#link("/referencia/medios/#api-drawable-animation")[`Drawable.animation`].

#api-entry(
  name: "Anim.move_to_3d",
  kind: "method",
  params: ((name: "x, y, z", type: "ScalarSource", default: none, desc: [Posición de destino.]),),
  returns: (type: "Anim", desc: [Movimiento a una posición 3D.]),
  none,
)

#api-entry(
  name: "Anim.shift_by_3d",
  kind: "method",
  params: ((name: "dx, dy, dz", type: "float", default: none, desc: [Desplazamiento en unidades de escena.]),),
  returns: (type: "Anim", desc: [Movimiento relativo en 3D.]),
  none,
)

#api-entry(
  name: "Anim.rotate_to_3d",
  kind: "method",
  params: ((name: "x, y, z", type: "ScalarSource", default: none, desc: [Orientación de Euler XYZ absoluta, en radianes.]),),
  returns: (type: "Anim", desc: [Giro a una orientación.]),
  none,
)

#api-entry(
  name: "Anim.rotate_by_3d",
  kind: "method",
  params: (
    (name: "axis", type: "\"x\" | \"y\" | \"z\"", default: none, desc: [Eje de giro; otro valor lanza `ValueError`.]),
    (name: "radians", type: "float", default: none, desc: [Ángulo relativo.]),
  ),
  returns: (type: "Anim", desc: [Giro relativo alrededor de un eje.]),
  none,
)

#api-entry(
  name: "Anim.scale_to_3d",
  kind: "method",
  params: ((name: "x, y, z", type: "ScalarSource", default: none, desc: [Escala absoluta por eje.]),),
  returns: (type: "Anim", desc: [Cambio de escala por eje.]),
  none,
)

#api-entry(
  name: "Anim.scale_by_3d",
  kind: "method",
  params: ((name: "x, y, z", type: "float", default: none, desc: [Factores que multiplican la escala de cada eje.]),),
  returns: (type: "Anim", desc: [Cambio de escala relativo por eje.]),
  none,
)

#api-entry(
  name: "Anim.material",
  kind: "method",
  params: ((name: "material", type: "Material3D", default: none, desc: [Material PBR de destino.]),),
  returns: (type: "Anim", desc: [Interpolación del material.]),
  desc: [Solo en una `Primitive3D` nativa. Color, color emisivo, rugosidad, metalicidad e intensidad de emisión se interpolan de forma determinista y los extremos exactos se restauran al buscar. En una malla, `write()` lanza `TypeError`: usa `create()`, que crece desde el centro mientras aparece.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.camera.look_at(eye=(4, 3, 5), target=(0, 0, 0))
cube = scene.geometry.cube(1.5).material(Material3D.matte(BLUE))
scene.play(cube.animate.material(Material3D.metal(GOLD)).rotate_by_3d("y", 1.2).duration(1.2))
```
]

== Transiciones entre segmentos

`Transition` describe el paso de un segmento al siguiente en
`scene.segment(...)` o `scene.link(...)`. Todas salvo `cut` aceptan `easing=`,
con cualquier `Easing`, incluidos los resortes que se pasan del destino, y
todas aceptan `overlay=`, un `Overlay` dibujado encima del corte. Ninguna de
las dos opciones cambia la duración de los segmentos. Sin `easing`,
`cross_fade`, `fade_through`, `zoom_through` y `morph` avanzan de forma lineal
y los revelados vectoriales (`wipe`, `clock_wipe`, `iris`, `blinds`, `push` y
`slide`) usan `Easing.SMOOTH`.

Los revelados vectoriales recortan ambos segmentos con caminos animados dentro
del marco visible de la cámara: el entrante se ve dentro de la región revelada
y el saliente en el resto; si los fondos son distintos, el entrante se revela
con la misma forma. Son geometría vectorial, sin texturas intermedias, así que
la transición es nítida a cualquier resolución y un seek reproduce exactamente
el mismo fotograma. Los objetos persistentes (`scene.persist`) no se recortan
ni se desplazan.

#api-entry(
  name: "Transition.cut",
  kind: "factory",
  params: ((name: "overlay", type: "Overlay | None", default: "None", desc: [Capa dibujada sobre el corte.]),),
  returns: (type: "Transition", desc: [Cambio instantáneo.]),
  none,
)

#api-entry(
  name: "Transition.cross_fade",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "easing, overlay", type: "Easing | None, Overlay | None", default: "None", desc: [Comunes a todas las transiciones.]),
  ),
  returns: (type: "Transition", desc: [Fundido cruzado.]),
  desc: [El segmento saliente se desvanece mientras aparece el entrante.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("detalle", Transition.cross_fade(0.4))
scene.wait(1)
```
]

#api-entry(
  name: "Transition.fade_through",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "color", type: "Color", default: none, desc: [Color intermedio.]),
  ),
  returns: (type: "Transition", desc: [Fundido a través de un color.]),
  desc: [Funde a `color` durante la primera mitad y desde él al segmento entrante en la segunda.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("capítulo 2", Transition.fade_through(1.0, BLACK))
scene.wait(1)
```
]

#api-entry(
  name: "Transition.slide",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "direction", type: "str", default: none, desc: [Movimiento del marco entrante: `"left"`, `"right"`, `"up"` o `"down"`.]),
  ),
  returns: (type: "Transition", desc: [El entrante se desliza sobre el saliente.]),
  desc: [El segmento saliente queda quieto y el entrante lo cubre al recorrer un ancho o un alto de marco. Usa `push` para mover los dos. Otra dirección lanza `ValueError`.],
  none,
)

#api-entry(
  name: "Transition.push",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "direction", type: "str", default: "\"up\"", desc: [Movimiento de ambos marcos: `"left"`, `"right"`, `"up"` o `"down"`.]),
  ),
  returns: (type: "Transition", desc: [Empuje.]),
  desc: [El segmento entrante empuja al saliente fuera del marco: ambos se desplazan un ancho o un alto de marco y cada uno queda recortado a su propio marco.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("siguiente", Transition.push(0.5, direction="up"))
scene.wait(1)
scene.segment("final", Transition.slide(0.5, "left", easing=Easing.spring(bounce=0.2)))
scene.wait(1)
```
]

#api-entry(
  name: "Transition.zoom_through",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "center", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Punto de la escena hacia el que se acerca.]),
    (name: "max_zoom", type: "float", default: "4.0", desc: [Zoom máximo, en el corte.]),
  ),
  returns: (type: "Transition", desc: [Zoom a través del corte.]),
  desc: [Acerca la cámara a un punto del segmento saliente y la aleja en el entrante; útil cuando un detalle presenta la sección siguiente.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("detalle", Transition.zoom_through(1.0, center=(2, 1), max_zoom=4))
scene.wait(1)
```
]

#api-entry(
  name: "Transition.morph",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "pairs", type: "Sequence[tuple[Drawable, Drawable]]", default: "()", desc: [Parejas `(origen, destino)`: el origen en el segmento saliente y el destino en el entrante.]),
  ),
  returns: (type: "Transition", desc: [Continuidad de objetos a través del corte.]),
  desc: [Cada pareja comparte una caja que viaja de la caja del origen a la del destino, mientras el destino aparece en la primera mitad y el origen desaparece en la segunda: se lee como un solo objeto que cambia de lugar, tamaño, color y forma. El resto del contenido hace un fundido cruzado. Las parejas se resuelven al compilar, así que se pasan a #link("/referencia/scene/#api-scene-link")[`scene.link`] cuando existen los dos segmentos. Una duración no positiva, un objeto emparejado consigo mismo o repetido en un lado lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
overview = scene.segment("Resumen")
card = scene.geometry.rect(3, 2).fill(BLUE).move_to(-4, 0)
scene.wait(1)
detail = scene.segment("Detalle")
panel = scene.geometry.rect(12, 6).fill(BLUE)
scene.wait(1)
scene.link(overview, detail, Transition.morph(0.8, pairs=[(card, panel)]))
```
]

#api-entry(
  name: "Transition.wipe",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "direction", type: "str", default: "\"left\"", desc: [Sentido en que viaja el borde: `left`, `right`, `up`, `down` o una diagonal como `up_left`. Con `"left"` el revelado empieza en el lado derecho.]),
    (name: "feather", type: "float", default: "0.1", desc: [Ancho del borde suave como fracción del recorrido, en `[0, 1]`; `0` da un borde duro.]),
  ),
  returns: (type: "Transition", desc: [Barrido lineal.]),
  desc: [Un borde recto cruza el marco y descubre el segmento entrante. El borde suave es una rampa de alfa vectorial. Una duración no positiva, una dirección desconocida o un `feather` fuera de `[0, 1]` lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("detalle", Transition.wipe(0.6, direction="left", feather=0.1))
```
]

#api-entry(
  name: "Transition.clock_wipe",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "start_angle", type: "float", default: "90.0", desc: [Grados en sentido antihorario desde +x; `90` empieza a las doce.]),
  ),
  returns: (type: "Transition", desc: [Barrido radial.]),
  desc: [Una aguja gira en sentido horario alrededor del centro del marco y deja ver el segmento entrante en el sector recorrido.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("resumen", Transition.clock_wipe(0.8, start_angle=90))
```
]

#api-entry(
  name: "Transition.iris",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "center", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Centro del iris en unidades de escena.]),
    (name: "shape", type: "str | Drawable", default: "\"circle\"", desc: [`circle`, `diamond`, `square`, `star` o un `Drawable` cuyo contorno vectorial se usa como plantilla.]),
  ),
  returns: (type: "Transition", desc: [Iris que crece hasta cubrir el marco.]),
  desc: [La forma crece desde `center` hasta que el segmento entrante ocupa todo el marco. Un `Drawable` se centra en su caja; se sigue dibujando en su propio segmento, así que conviene ocultarlo si solo sirve de plantilla. Uno sin camino vuelve al círculo. Un nombre desconocido lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("zoom", Transition.iris(0.7, center=(2, 1), shape="star"))
```
]

#api-entry(
  name: "Transition.blinds",
  kind: "factory",
  params: (
    (name: "duration", type: "float", default: none, desc: [Segundos, positivo.]),
    (name: "count", type: "int", default: "8", desc: [Número de lamas, entre 1 y 512.]),
    (name: "angle", type: "float", default: "0.0", desc: [Inclinación de las lamas en grados; `0` son lamas horizontales que se abren hacia abajo.]),
  ),
  returns: (type: "Transition", desc: [Persiana veneciana.]),
  desc: [Todas las lamas se abren a la vez y en la misma fracción.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("datos", Transition.blinds(0.6, count=8, angle=0))
```
]

#api-entry(
  name: "Overlay.flash",
  kind: "factory",
  params: (
    (name: "color", type: "Color | None", default: "None", desc: [Color del destello; blanco si se omite.]),
    (name: "duration", type: "float", default: "0.2", desc: [Segundos, positivo.]),
  ),
  returns: (type: "Overlay", desc: [Capa para `overlay=` de cualquier `Transition`.]),
  desc: [Cubre el marco con un color que sube durante la primera mitad y se apaga en la segunda, centrado en el punto medio de la transición (en `cut`, en el propio corte).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("impacto", Transition.cut(overlay=Overlay.flash(WHITE, 0.15)))
```
]

#api-entry(
  name: "Overlay.light_leak",
  kind: "factory",
  params: (
    (name: "seed", type: "int", default: "0", desc: [Semilla de la disposición de las manchas de luz.]),
    (name: "hue", type: "float", default: "0.1", desc: [Tono base en vueltas: `0.1` naranja cálido, `0.6` azul.]),
    (name: "duration", type: "float", default: "0.8", desc: [Segundos, positivo.]),
    (name: "intensity", type: "float", default: "0.8", desc: [Brillo máximo, en `[0, 4]`.]),
  ),
  returns: (type: "Overlay", desc: [Capa para `overlay=`.]),
  desc: [Manchas de luz suaves en modo pantalla (`screen`) que derivan por el marco alrededor del corte. Es función pura del tiempo y de la semilla, así que la previsualización, los seeks y la exportación coinciden. Valores fuera de rango lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>scene.wait(1)
scene.segment("cálido", Transition.cross_fade(0.4, overlay=Overlay.light_leak(seed=2, hue=0.1)))
```
]
