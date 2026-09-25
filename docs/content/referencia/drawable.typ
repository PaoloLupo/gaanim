#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Drawable",
  description: "El handle fluido que devuelven todas las fábricas: estilo, posición, efectos, anclajes y relaciones reactivas",
  route: "/referencia/drawable/",
  nav: "Drawable",
)

= Drawable

Cada fábrica de la escena (`scene.geometry`, `scene.text`, `scene.media`,
`scene.viz`, `scene.slides`, `scene.mechanics`, `scene.layout`) devuelve un
`Drawable`: un handle que encadena estilo, posición y efectos. Los tipos
especializados (`Text`, `Image`, `Layout`, `Dimension`…) heredan todos estos
métodos y conservan su tipo al encadenarlos.

```python
# show-code: true
from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
card = scene.geometry.rounded_rect(3, 1.6, 0.2).fill(BLUE).stroke(GOLD, 0.04)
card.move_to(-2, 0).rotate_to(0.1).shadow("#00000099")
scene.play([card.animate.move_to(2, 0).rotate_to(-0.1).duration(1.2)])
# output: preview.webp
scene.render()
```

Hay dos formas de cambiar un objeto:

- Los *setters inmediatos* (`fill`, `move_to`, `opacity`…) fijan el estado
  inicial. Si los llamas después de un `play` o un `wait`, crean un corte
  reversible en el cursor de la línea de tiempo.
- El proxy `.animate` usa el mismo vocabulario, pero devuelve un `Anim` que
  interpola hacia el destino dentro de `scene.play(...)`. Consulta
  #link("/referencia/animations/")[Animaciones] para las entradas, los énfasis
  y el control del tiempo.

== Estilo

Relleno, trazo, opacidad y orden de dibujo. Los colores aceptan `Color`,
cadenas CSS o hex y `Brush` para degradados (ver
#link("/referencia/themes/")[Colores y temas]).

#api-entry(
  name: "Drawable.fill",
  kind: "method",
  params: ((name: "paint", type: "Paint", default: none, desc: [`Color`, cadena CSS/hex, tupla RGB(A) o `Brush`.]),),
  desc: [Pinta el interior de los contornos cerrados. En un grupo o un SVG importado, el color llega a todos los descendientes.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
disk = scene.geometry.circle(1).fill(BLUE)
badge = scene.geometry.square(1).fill("#f97316").move_to(3, 0)
```
]

#api-entry(
  name: "Drawable.no_fill",
  kind: "method",
  desc: [Quita el relleno. Úsalo para contornos: `circle(1).no_fill().stroke(WHITE, 0.04)`.],
  none,
)

#api-entry(
  name: "Drawable.stroke",
  kind: "method",
  params: ((name: "paint", type: "Paint", default: none, desc: [`Color` o `Brush`.]), (name: "width", type: "float", default: none, desc: [Ancho en unidades lógicas de escena, también en SVG escalados.]), (name: "align", type: "str | None", default: "None", desc: [`"inside"`, `"center"` u `"outside"` respecto a los contornos cerrados; `None` deja el trazo dentro.])),
  desc: [En contornos cerrados, incluidos los glifos de un texto, el trazo queda dentro por defecto, así `write` dibuja un ancho constante. `"center"` lo reparte a ambos lados y `"outside"` lo dibuja entero por fuera, por ejemplo como halo bajo una etiqueta que tapa líneas. Los caminos abiertos siempre centran su trazo. La alineación es estado de declaración y no se anima; otros valores lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Anchor, BLACK, RED, WHITE, Scene
scene = Scene(frame=(16, 9), background=WHITE)
scene.geometry.line(-6, -0.4, 6, 0.4).stroke(RED, 0.04)
scene.text("halo", size=0.8).fill(WHITE).stroke(WHITE, 0.15, align="outside").move_to(0, 0, Anchor.CENTER)
scene.text("halo", size=0.8).fill(BLACK).move_to(0, 0, Anchor.CENTER)
scene.wait(0.1)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.no_stroke",
  kind: "method",
  desc: [Quita el trazo. Las flechas sólidas se ven mejor con `.fill(color).no_stroke()`.],
  none,
)

#api-entry(
  name: "Drawable.stroke_style",
  kind: "method",
  params: ((name: "style", type: "StrokeStyle", default: none, desc: [Pintura, ancho, extremo, unión, límite de inglete, guiones y desfase de guiones.]),),
  desc: [Aplica la geometría completa del trazo. En una llamada directa, la pintura debe ser un color CSS literal o un `Brush`; los nombres de token se resuelven cuando el `StrokeStyle` va dentro de `Theme.styles`. Métricas inválidas lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene, StrokeStyle
scene = Scene(frame=(16, 9))
guide = scene.geometry.line(-2, 0, 2, 0).stroke_style(
    StrokeStyle("#2563eb", 5, cap="round", dashes=[18, 10])
)
scene.wait(0.1)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.style_class",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Clase del tema sin el punto inicial; admite letras, dígitos, `_` y `-`.]),),
  desc: [Añade una clase ordenada que usa `Theme.styles`. Varias llamadas se aplican en cascada, en orden; los estilos del constructor y los setters fluidos siguen teniendo prioridad. En un grupo, la clase llega a todos los miembros visuales.],
)[
```python
# show-code: true
from gaanim import Scene, Style, Theme
theme = Theme("paper", styles={".warning": Style(fill="#e11d48")})
scene = Scene(frame=(16, 9), theme=theme)
warning = scene.geometry.square(1.125).style_class("warning")
scene.wait(0.1)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.opacity",
  kind: "method",
  params: ((name: "op", type: "float | Parameter | Variable | Computed | TimeInput", default: none, desc: [Opacidad de 0 a 1, o una fuente reactiva.]),),
  desc: [La opacidad se multiplica por la jerarquía: la de un grupo escala la de sus miembros sin cambiar sus valores propios. Con una fuente reactiva el canal queda vinculado desde el cursor; un número termina el vínculo con un corte reversible. Mientras esté vinculado, anima la fuente: animar el canal directamente lanza un error.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ghost = scene.geometry.circle(1).fill(WHITE).opacity(0.3)
level = scene.viz.parameter(0.2)
halo = scene.geometry.circle(1.4).fill(GOLD).opacity(level)
scene.play([level.animate.set(1.0).duration(1)])
```
]

#api-entry(
  name: "Drawable.z_index",
  kind: "method",
  params: ((name: "z", type: "int", default: none, desc: [Capa de dibujo; los valores mayores quedan encima.]),),
  desc: [El `z_index` de un grupo o de un texto se suma al de cada descendiente, así que mueve todo el subárbol. Los empates conservan el orden de creación.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
front = scene.geometry.circle(1).fill(GOLD).z_index(5)
back = scene.geometry.rect(3, 1).fill(BLUE)
```
]

== Posición y transformaciones

Setters absolutos (`move_to`, `scale_to`, `rotate_to`) y relativos (`shift_by`,
`scale_by`, `rotate_by`) en unidades de escena y radianes. Sus versiones
animadas están en #link("/referencia/animations/")[Animaciones].

#api-entry(
  name: "Drawable.move_to",
  kind: "method",
  signature: "move_to(x, y, anchor=None) | move_to(reference) | move_to(point) -> Self",
  params: ((name: "x / y", type: "float | fuente reactiva", default: none, desc: [Posición de destino en unidades de escena.]), (name: "anchor", type: "Anchor | None", default: "None", desc: [Punto propio que se coloca en `(x, y)`; posicional o por nombre.]), (name: "reference", type: "Drawable", default: none, desc: [Alternativa: centra este objeto sobre otro.]), (name: "point", type: "AnchorPoint", default: none, desc: [Alternativa: coloca el centro sobre un anclaje de otro objeto.])),
  desc: [Sin `anchor`, los objetos se colocan por su centro visual; las raíces de sistemas de coordenadas colocan su origen matemático, para que las etiquetas no desplacen los ejes. `move_to(reference)` crea una relación de layout diferida centro con centro que no sigue animaciones posteriores: usa `follow` o `attach_to` para eso. Una referencia o un `AnchorPoint` no se combinan con `y` ni con `anchor`. `Text` acepta además `TextAnchor` (ver #link("/referencia/text/")[Texto]). Las coordenadas aceptan fuentes reactivas, con las mismas reglas que `opacity`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
box = scene.geometry.rect(3, 1.5).move_to(-4, 1, Anchor.TOP_LEFT)
label = scene.text("centro").move_to(box)
corner = scene.geometry.dot(0.1).move_to(box.anchor_point(Anchor.BOTTOM_RIGHT))
```
]

#api-entry(
  name: "Drawable.shift_by",
  kind: "method",
  desc: [Traslada el objeto `(dx, dy)` unidades de escena respecto a su posición actual, como un corte inmediato en el cursor.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
dot = scene.geometry.dot(0.1).move_to(0, 0)
scene.wait(0.5)
dot.shift_by(1, 0.5)
```
]

#api-entry(
  name: "Drawable.scale_to",
  kind: "method",
  desc: [Fija la escala uniforme absoluta alrededor del pivote. Acepta fuentes reactivas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
logo = scene.media.svg("assets/logo.svg").scale_to(0.025)
```
]

#api-entry(
  name: "Drawable.scale_by",
  kind: "method",
  desc: [Multiplica la escala actual por `factor` en el cursor. Mayor que 1 agranda y menor que 1 reduce.],
  none,
)

#api-entry(
  name: "Drawable.rotate_to",
  kind: "method",
  desc: [Fija la rotación absoluta en radianes alrededor del pivote; los valores positivos giran en sentido antihorario. Acepta fuentes reactivas.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
tilted = scene.geometry.square(1).rotate_to(math.pi / 4)
```
]

#api-entry(
  name: "Drawable.rotate_by",
  kind: "method",
  desc: [Suma `radians` a la rotación actual en el cursor.],
  none,
)

#api-entry(
  name: "Drawable.with_pivot",
  kind: "method",
  desc: [Fija el pivote de rotación y escala en coordenadas de escena. Úsalo para bisagras y brazos que giran alrededor de un extremo. `pivot(x, y)` es un alias.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene
from math import pi
scene = Scene(frame=(16, 9), background="#0f172a")
hinge = scene.geometry.dot(0.09).fill(GOLD).move_to(-2.5, 1.25)
arm = scene.geometry.rect(1.125, 0.225).fill(BLUE).move_to(hinge).with_pivot(-2.5, 1.25)
scene.play([arm.animate.rotate_by(pi/2.5).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.pivot",
  kind: "method",
  desc: [Alias de `with_pivot`.],
  none,
)

== Posición relativa

Coloca un objeto libre respecto a otro o a los bordes del área segura. Los hijos
de un `Layout` no usan estos métodos: su posición la decide el contenedor (ver
#link("/referencia/layout/")[Layout]).

#api-entry(
  name: "Drawable.next_to",
  kind: "method",
  params: ((name: "reference", type: "Drawable", default: none, desc: [Objeto de referencia.]), (name: "direction", type: "Direction", default: none, desc: [Lado donde se coloca este objeto.]), (name: "spacing", type: "float", default: "0.24", desc: [Separación entre bordes en unidades de escena.]), (name: "aligned_edge", type: "Anchor | None", default: "None", desc: [Borde común que se alinea; `None` centra en el eje transversal.])),
  desc: [Coloca este objeto junto a otro sin superponerlos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Resultados", role="title")
rule = scene.geometry.line(length=4).next_to(title, Direction.DOWN, spacing=0.2)
note = scene.text("n = 42", role="caption").next_to(title, Direction.RIGHT, spacing=0.3, aligned_edge=Anchor.BOTTOM)
```
]

#api-entry(
  name: "Drawable.align_to",
  kind: "method",
  params: ((name: "reference", type: "Drawable", default: none, desc: [Objeto de referencia.]), (name: "target_anchor", type: "Anchor", default: none, desc: [Anclaje propio que se alinea.]), (name: "reference_anchor", type: "Anchor | None", default: "None", desc: [Anclaje de la referencia; `None` usa el mismo que `target_anchor`.])),
  desc: [Hace coincidir un anclaje propio con un anclaje de otro objeto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
panel = scene.geometry.rect(5, 3).move_to(1, 0)
tag = scene.slides.badge("NUEVO").align_to(panel, Anchor.TOP_LEFT)
```
]

#api-entry(
  name: "Drawable.to_edge",
  kind: "method",
  desc: [Lleva el objeto al borde indicado del área segura, separado `buff` unidades.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Capítulo 2", role="title").to_edge(Direction.UP, buff=0.4)
```
]

#api-entry(
  name: "Drawable.to_corner",
  kind: "method",
  desc: [Lleva el objeto a una esquina del área segura (`Anchor.TOP_LEFT`, `Anchor.BOTTOM_RIGHT`…), separado `buff` unidades.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
page = scene.text("3 / 12", role="caption").to_corner(Anchor.BOTTOM_RIGHT, buff=0.3)
```
]

== Anclajes y puertos

Puntos no dibujados que siguen al objeto en cada fotograma. Sirven como extremos
de líneas, conectores, cotas y seguidores (ver
#link("/referencia/geometria/")[Geometría]).

#api-entry(
  name: "Drawable.anchor_point",
  kind: "method",
  params: ((name: "anchor", type: "Anchor | None", default: "None", desc: [Uno de los nueve anclajes de los límites locales; `None` equivale a `Anchor.CENTER`.]), (name: "offset", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Desplazamiento adicional en el espacio local.])),
  desc: [El desplazamiento local rota y escala con el objeto y con sus padres. Valores no finitos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
frame = scene.geometry.rect(2.25, 1.125)
corner = frame.anchor_point(Anchor.TOP_RIGHT, offset=(0.1, 0))
link = scene.geometry.line((-4, 2), corner)
```
]

#api-entry(
  name: "Drawable.with_port",
  kind: "method",
  desc: [Define un anclaje local con nombre y devuelve el mismo objeto. El puerto sigue los límites, el reflow y las transformaciones de los padres. Los nombres son únicos e inmutables: vacíos, repetidos u offsets no finitos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
node = scene.geometry.rect(2, 1).with_port("out", Anchor.RIGHT, offset=(0.1, 0))
```
]

#api-entry(
  name: "Drawable.port",
  kind: "method",
  desc: [Devuelve el `AnchorPoint` reactivo de un puerto definido con `with_port`. Un nombre desconocido lanza `KeyError`.],
)[
```python
# continue
target = scene.geometry.circle(0.5).move_to(4, 1)
edge = scene.geometry.connector(node.port("out"), target)
```
]

#api-entry(
  name: "Drawable.at_coordinate",
  kind: "method",
  desc: [Coloca el objeto en una coordenada simbólica de un espacio de coordenadas (`plane.coord(x, y)`). La posición sigue al espacio y a sus cambios de vista.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
peak = scene.geometry.dot(0.1).fill(GOLD).at_coordinate(plane.coord(1, 1.5))
```
]

Cada objeto expone también las expresiones lineales `left`, `right`, `top`,
`bottom`, `center_x`, `center_y`, `width` y `height` para las restricciones de
#link("/referencia/layout/")[Layout].

== Efectos y recortes

Resplandor, desenfoque, sombra, recorte por máscara y recorte del trazo. Los
efectos se aplican a rellenos y trazos, incluidos los degradados.

#api-entry(
  name: "Drawable.glow",
  kind: "method",
  params: ((name: "color", type: "Color", default: none, desc: [Color del resplandor.]), (name: "radius", type: "float", default: "0.16", desc: [Radio en unidades de escena.]), (name: "intensity", type: "float", default: "1.0", desc: [Intensidad relativa.])),
  desc: [Añade un resplandor alrededor del objeto.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
obj = scene.geometry.circle(0.56).fill(BLUE).stroke(GOLD, 0.04).move_to(0, 0)
obj.glow(GOLD, radius=0.225)
scene.play([obj.animate.grow_from_center().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.blur",
  kind: "method",
  desc: [Desenfoque gaussiano de `sigma` unidades de escena.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
blob = scene.geometry.circle(2).fill("#1E3A8A").blur(0.15)
```
]

#api-entry(
  name: "Drawable.shadow",
  kind: "method",
  params: ((name: "color", type: "Color", default: none, desc: [Color de la sombra; su alfa escala la opacidad.]), (name: "x / y", type: "float", default: "0.08 / -0.08", desc: [Desplazamiento en unidades de escena.]), (name: "blur", type: "float", default: "0.06", desc: [Desenfoque en unidades de escena.])),
  desc: [Sombra proyectada desplazada y desenfocada.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
card = scene.geometry.rounded_rect(4, 2, 0.2).fill("#18202E").shadow("#00000080", x=0.12, y=-0.12, blur=0.1)
```
]

#api-entry(
  name: "Drawable.no_effects",
  kind: "method",
  desc: [Quita `glow`, `blur` y `shadow` sin tocar el relleno ni el trazo.],
  none,
)

#api-entry(
  name: "Drawable.trim",
  kind: "method",
  params: ((name: "start / end", type: "float | None", default: "None", desc: [Ventana visible del camino como fracción de su longitud; al principio valen 0 y 1.]), (name: "offset", type: "float | None", default: "None", desc: [Desplaza la ventana; da la vuelta al final del camino.]), (name: "mode", type: "str | None", default: "None", desc: [`"simultaneous"` (predeterminado) recorta cada subcamino a la vez; `"sequential"` recorta la longitud total y los subcaminos aparecen uno tras otro.])),
  desc: [Muestra solo una parte del trazo, como los _trim paths_ de After Effects. Los valores omitidos conservan el ajuste anterior. Anímalo con `animate.trim(...)`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
ring = scene.geometry.circle(1.5).no_fill().stroke(GOLD, 0.08).trim(end=0.0)
scene.play([ring.animate.trim(end=1.0).duration(1.0)])
scene.play([ring.animate.trim(start=0.7, offset=0.5).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.clip",
  kind: "method",
  params: ((name: "mask", type: "Drawable", default: none, desc: [Silueta vectorial que recorta este objeto.]), (name: "rule", type: "str", default: "\"nonzero\"", desc: [Regla de relleno de la máscara: `"nonzero"` o `"evenodd"`.]), (name: "invert", type: "bool", default: "False", desc: [Recorta al exterior de la máscara.])),
  desc: [Recorte vectorial vivo: la máscara puede moverse, escalarse o rotar y el recorte se recalcula en cada fotograma. La máscara sigue visible; usa `no_fill().no_stroke()` si solo debe definir la silueta. Una regla desconocida lanza `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
stripes = scene.geometry.group([
    scene.geometry.rect(0.4, 4).fill(GOLD if i % 2 else BLUE).move_to(-3 + 0.4 * i, 0)
    for i in range(16)
])
lens = scene.geometry.circle(1.2).no_fill().no_stroke().move_to(-2, 0)
stripes.clip(lens)
scene.play([lens.animate.move_to(2, 0).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.no_clip",
  kind: "method",
  desc: [Quita el recorte por máscara. En las marcas de datos de un espacio cartesiano, deja que la marca sobresalga del área de trazado.],
  none,
)

== Animación

El proxy que convierte los setters en animaciones y las acciones de modelos
importados.

#api-entry(
  name: "Drawable.animate",
  kind: "property",
  desc: [Proxy puro de animación compuesta. Encadena destinos de transformación, opacidad, relleno, trazo, color o material; todos comparten duración y easing y corren a la vez. El resultado es un `Anim` que no cambia nada hasta pasarlo a `scene.play`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>node = scene.geometry.circle(0.5).fill(BLUE)
scene.play([node.animate.move_to(3, 0).fill(GOLD).scale_to(1.5).duration(1.2)])
```
]

`Drawable.animation(name, ...)` reproduce una acción de un modelo glTF; está en
#link("/referencia/medios/")[Medios], junto con `part`, `parts` y `animations`.

== Relaciones reactivas

Hacen que un objeto siga a otro en el mismo fotograma sin callbacks de Python.
El seguidor queda oculto hasta su animación de entrada. Para simulaciones y
series muestreadas, consulta `add_updater_fn` y `drive_from_samples` en
#link("/referencia/animations/")[Animaciones].

#api-entry(
  name: "Drawable.follow",
  kind: "method",
  params: ((name: "source", type: "Endpoint", default: none, desc: [Drawable, `AnchorPoint`, `PointRef` o tupla.]), (name: "offset", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Desplazamiento respecto a la fuente.]), (name: "offset_space", type: "str", default: "\"world\"", desc: [`"world"` mantiene el desplazamiento alineado con la pantalla; `"local"` lo rota y escala con la fuente.])),
  desc: [Sigue cualquier extremo en el mismo fotograma y devuelve el objeto. Offsets no finitos o modos inválidos lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
theta = scene.viz.parameter(0.2)
tip = scene.geometry.polar_point((0, 0), 1.5, theta)
bar = scene.mechanics.bar_between((0, 0), tip).stroke(WHITE, 0.09)
label = scene.text("punta").fill(GOLD).follow(tip, offset=(0, 0.225))
scene.play([bar.animate.fade_in(), label.animate.write(), theta.animate.set(2.2).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.attach_to",
  kind: "method",
  desc: [Coloca el objeto sobre el centro de `source` y lo sigue mientras se mueve. Para mantener una separación usa `follow_to` o `follow`. Devuelve `None`, así que llámalo en su propia línea.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
from gaanim import WHITE
mass = scene.geometry.dot(0.15).fill(GOLD).move_to(-1.5, 0)
ring = scene.geometry.circle(0.4).no_fill().stroke(WHITE, 0.04)
ring.attach_to(mass)
scene.play([ring.animate.create().duration(0.3), mass.animate.shift_by(3, 0).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.follow_to",
  kind: "method",
  desc: [Sigue el centro de `source` con un desplazamiento fijo `offset`. Devuelve `None`; para encadenar, usa `follow`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ball = scene.geometry.circle(0.3).fill(BLUE)
tag = scene.text("v", role="label")
tag.follow_to(ball, (0, 0.6))
scene.play([tag.animate.fade_in(), ball.animate.shift_by(3, 0)])
```
]

#api-entry(
  name: "Drawable.bind_x_from / bind_y_from / bind_position_from",
  kind: "method",
  signature: "bind_x_from(source) · bind_y_from(source) · bind_position_from(source, axes=\"xy\") -> None",
  desc: [Copia la coordenada X, la Y o ambas de `source`. `axes` acepta `"x"`, `"y"` o `"xy"`. Útil para proyecciones sobre un eje. Devuelven `None`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
point = scene.geometry.dot(0.1).fill(GOLD).move_to(-2, 1)
shadow = scene.geometry.dot(0.08).fill(GRAY).move_to(0, -2)
shadow.bind_x_from(point)
scene.play([shadow.animate.fade_in(), point.animate.shift_by(4, 0.5)])
```
]

#api-entry(
  name: "Drawable.bind_rotation_from",
  kind: "method",
  desc: [Copia la rotación mundial de `source` como `rotación * ratio + phase`, en radianes. Es la base del acoplamiento de engranajes.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
driver = scene.mechanics.gear(0.69, 20).move_to(-0.69, 0)
driven = scene.mechanics.gear(0.41, 12).move_to(0.41, 0).bind_rotation_from(driver, ratio=-5/3)
scene.play([driven.animate.fade_in(), driver.animate.rotate_by(2.0)])
```
]

#api-entry(
  name: "Drawable.bind_translation_from_rotation",
  kind: "method",
  desc: [Traslada el objeto a lo largo de `axis` según el giro acumulado de `source` multiplicado por `scale`, como una cremallera movida por un piñón. Es una relación visual, no un solver de contacto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
pinion = scene.mechanics.gear(0.69, 20).move_to(0, 0.25)
rack = scene.mechanics.rack(2.75, 18).move_to(0, -0.81).bind_translation_from_rotation(
    pinion, axis=Direction.RIGHT, scale=0.69,
)
scene.play([rack.animate.fade_in(), pinion.animate.rotate_by(2.0)])
```
]

#api-entry(
  name: "Drawable.add_updater",
  kind: "method",
  desc: [Asocia un `Updater` nativo, por ejemplo `Updater.orbit(cx, cy, radio, velocidad)`. Se evalúa en Rust y sigue siendo exacto al hacer seek.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
planet = scene.geometry.dot(0.12).fill(BLUE)
planet.add_updater(Updater.orbit(0, 0, 2, 1.0))
scene.wait(1)
```
]

#api-entry(
  name: "Drawable.remove_updater",
  kind: "method",
  desc: [Quita los updaters y las series muestreadas asociados al objeto desde el cursor.],
  none,
)

== Transformaciones 3D

#experimental()

Los mismos setters en tres ejes, para objetos colocados en el espacio de la
cámara en perspectiva. Las rotaciones de Euler usan orden XYZ y radianes.

#api-entry(
  name: "Drawable.move_to_3d",
  kind: "method",
  desc: [Coloca el objeto en una posición del mundo 3D. Añade `.billboard()` para que una etiqueta mire siempre a la cámara o `.hud()` para fijarla a la pantalla. Acepta fuentes reactivas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
label = scene.text("origen").move_to_3d(0, 0, 0.5).billboard()
```
]

#api-entry(
  name: "Drawable.shift_by_3d",
  kind: "method",
  desc: [Traslación relativa en tres ejes como corte inmediato.],
  none,
)

#api-entry(
  name: "Drawable.scale_to_3d",
  kind: "method",
  desc: [Escala absoluta independiente en X, Y y Z. Acepta fuentes reactivas.],
  none,
)

#api-entry(
  name: "Drawable.scale_by_3d",
  kind: "method",
  desc: [Multiplica la escala de cada eje en el cursor.],
  none,
)

#api-entry(
  name: "Drawable.rotate_to_3d",
  kind: "method",
  desc: [Rotación de Euler absoluta en radianes, en orden XYZ. Acepta fuentes reactivas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
panel = scene.geometry.rect(2, 1).move_to_3d(0, 0, 0).rotate_to_3d(0.3, 0.6, 0)
```
]

#api-entry(
  name: "Drawable.rotate_by_3d",
  kind: "method",
  desc: [Suma `radians` a la rotación alrededor del eje `"x"`, `"y"` o `"z"`.],
  none,
)

#api-entry(
  name: "Drawable.with_pivot_3d",
  kind: "method",
  desc: [Fija el pivote 3D de rotación y escala.],
  none,
)

#api-entry(
  name: "Drawable.billboard",
  kind: "method",
  desc: [Mantiene un objeto 3D orientado hacia la cámara en perspectiva. Útil para etiquetas y marcadores.],
  none,
)

#api-entry(
  name: "Drawable.hud",
  kind: "method",
  desc: [Fija el objeto a la pantalla como superposición: usa coordenadas de pantalla y la cámara 3D no lo afecta. Colócalo con `.move_to(x, y)` después de `.hud()`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Modelo 3D", role="title").hud().move_to(0, 3.5)
```
]
