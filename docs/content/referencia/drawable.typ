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
  params: ((name: "paint", type: "Paint", default: none, desc: [`Color` o `Brush`.]), (name: "width", type: "float", default: none, desc: [Ancho en unidades lógicas de escena. No sigue la escala de la figura: `scale_to`, `scale_to_3d`, `matrix_to`, las inclinaciones, la escala de sus grupos y sus animaciones cambian la forma, no el pincel, que queda redondo e igual de ancho en todos los lados. También en SVG escalados.]), (name: "align", type: "str | None", default: "None", desc: [`"inside"`, `"center"` u `"outside"` respecto a los contornos cerrados; `None` deja el trazo dentro. En los tres casos el trazo visible mide `width`.]), (name: "scale_with_object", type: "bool | None", default: "None", desc: [`True` hace que el ancho siga la escala de la figura, como `scale_stroke_with_object`.])),
  desc: [En contornos cerrados, incluidos los glifos de un texto, el trazo queda entero dentro por defecto, así `write` dibuja un ancho constante. `"center"` lo reparte a ambos lados y `"outside"` lo dibuja entero por fuera, por ejemplo como halo bajo una etiqueta que tapa líneas. Los caminos abiertos siempre centran su trazo. La alineación es estado de declaración y no se anima; otros valores lanzan `ValueError`.],
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
  name: "Drawable.scale_stroke_with_object",
  kind: "method",
  params: ((name: "enabled", type: "bool", default: "True", desc: [`False` vuelve al ancho en unidades de escena.]),),
  desc: [Hace que el ancho del trazo siga la escala que acumula la figura (la suya, la de sus grupos y la de un espacio de coordenadas), como un dibujo que se amplía: al escalar una cara ×1.18, las cejas y la nariz engrosan con ella. Una escala no uniforme ensancha el pincel en su eje. En un grupo o un SVG importado se aplica a todos sus trazos. Es estado de declaración y no se anima.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>head = scene.geometry.circle(1).no_fill().stroke(WHITE, 0.04)
>>>brow = scene.geometry.line(-0.5, 0.4, -0.1, 0.5).stroke(WHITE, 0.04)
face = scene.geometry.group([head, brow]).scale_stroke_with_object()
face.scale_to(1.18)
```
]

#api-entry(
  name: "Drawable.chalk",
  kind: "method",
  params: (
    (name: "seed", type: "int", default: "0", desc: [Semilla del temblor y del grano: la misma semilla dibuja la misma tiza en cada fotograma y exportación.]),
    (name: "roughness", type: "float", default: "0.01", desc: [Desplazamiento máximo del contorno, en unidades de escena; `0` deja el contorno limpio y solo aplica el grano.]),
  ),
  desc: [Dibuja el relleno y el trazo como tiza en una pizarra: el contorno tiembla un poco y un grano rompe la pintura. En un texto o un grupo se aplica a cada glifo y miembro. Vale para toda la escena, aunque se llame después de un `play`; no se anima. `write(brush="chalk")` hace lo mismo desde la animación. Los paquetes `.gaanim` la graban y reproducen igual.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), background="#1f3a2e")
box = scene.geometry.rect(3, 1.5).no_fill().stroke(WHITE, 0.05).chalk(seed=2)
scene.play([box.animate.write().duration(1)])
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
  desc: [La opacidad se multiplica por la jerarquía: la de un grupo escala la de sus miembros sin cambiar sus valores propios. Un grupo o un SVG translúcido se compone como una sola capa, como `opacity` en CSS: al fundirlo, lo que una pieza tapa sigue tapado (los ojos no asoman bajo los párpados) y las piezas translúcidas no se marcan más. Con una fuente reactiva el canal queda vinculado desde el cursor; un número termina el vínculo con un corte reversible. Mientras esté vinculado, anima la fuente: animar el canal directamente lanza un error.],
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
  desc: [El `z_index` de un grupo o de un texto se suma al de cada descendiente, así que mueve todo el subárbol. Los empates conservan el orden de creación. Con capas (`Scene.z_layers`), ordena solo dentro de la capa del objeto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
front = scene.geometry.circle(1).fill(GOLD).z_index(5)
back = scene.geometry.rect(3, 1).fill(BLUE)
```
]

#api-entry(
  name: "Drawable.z_layer",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Una de las capas nombradas con `Scene.z_layers`.]),),
  desc: [Dibuja el objeto en la capa `name`: queda encima de todo lo que está en las capas de detrás, sea cual sea su `z_index`, que solo ordena dentro de la capa. Un grupo o un texto se lleva a sus miembros, salvo a los que nombran su propia capa. El orden de dibujo vale para toda la escena, así que una llamada después de mostrar el objeto también rige desde el principio. Un nombre que la escena no declaró lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.z_layers("modelo", "overlay")
muro = scene.geometry.rect(4, 3).fill(BLUE).z_index(400)
lupa = scene.geometry.circle(0.8).fill(GOLD).z_layer("overlay")
```
]

#api-entry(
  name: "Drawable.covers_on_purpose",
  kind: "method",
  desc: [Marca que el objeto, y sus miembros si es un grupo, tapa textos a propósito, como un diálogo dibujado sobre un modelo mientras se explica: el aviso de textos tapados de `gaanim check` deja de contar los textos que tapa. No cambia nada del dibujo y vale para toda la escena.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
dialogo = scene.geometry.rect(4, 2).fill(WHITE).covers_on_purpose()
```
]

#api-entry(
  name: "Drawable.named",
  kind: "method",
  params: ((name: "name", type: "str", default: none, desc: [Nombre no vacío; no hace falta que sea único.]),),
  desc: [Da un nombre al objeto y lo devuelve; `Drawable.name` lo lee (`None` si no tiene). El nombre identifica al mismo objeto en dos estados: `magic_move` y `Transition.magic_move` emparejan los objetos con el mismo nombre. Un nombre en blanco lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
bar = scene.geometry.rect(3, 0.5).fill(BLUE).named("ana")
print(bar.name)
```
]

== Posición y transformaciones

Setters absolutos (`move_to`, `scale_to`, `rotate_to`, `skew_to`) y relativos (`shift_by`,
`scale_by`, `rotate_by`) en unidades de escena y radianes. Sus versiones
animadas están en #link("/referencia/animations/")[Animaciones].

#api-entry(
  name: "Drawable.move_to",
  kind: "method",
  signature: "move_to(x, y, anchor=None) | move_to(reference) | move_to(point) -> Self",
  params: ((name: "x / y", type: "float | fuente reactiva", default: none, desc: [Posición de destino en unidades de escena.]), (name: "anchor", type: "Anchor | None", default: "None", desc: [Punto propio que se coloca en `(x, y)`; posicional o por nombre.]), (name: "reference", type: "Drawable", default: none, desc: [Alternativa: centra este objeto sobre otro.]), (name: "point", type: "AnchorPoint", default: none, desc: [Alternativa: coloca el centro sobre un anclaje de otro objeto.])),
  desc: [Sin `anchor`, los objetos se colocan por su centro visual, medido con la geometría tal como se declaró: mover después una parte de un SVG (`svg.part("g").shift_by(...)`) no mueve el SVG entero; las raíces de sistemas de coordenadas colocan su origen matemático, para que las etiquetas no desplacen los ejes. `move_to(reference)` crea una relación de layout diferida centro con centro que no sigue animaciones posteriores: usa `follow` o `attach_to` para eso. Una referencia o un `AnchorPoint` no se combinan con `y` ni con `anchor`. `Text` acepta además `TextAnchor` (ver #link("/referencia/text/")[Texto]). Ojo: sin `anchor`, un texto de una línea se coloca por su línea base (`TextAnchor.BASELINE_CENTER`), no por su centro visual, así que un título con `move_to(0, y)` queda medio cuerpo más arriba; usa `move_to(0, y, Anchor.CENTER)` para centrarlo. Las coordenadas aceptan fuentes reactivas, con las mismas reglas que `opacity`.],
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
  desc: [Fija la escala uniforme absoluta alrededor del pivote (ver `with_pivot`). Acepta fuentes reactivas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
logo = scene.media.svg("assets/logo.svg").scale_to(2.5)
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
  desc: [Fija la rotación absoluta en radianes alrededor del pivote (ver `with_pivot`); los valores positivos giran en sentido antihorario. Acepta fuentes reactivas.],
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
  name: "Drawable.skew_to",
  kind: "method",
  params: ((name: "x", type: "float", default: none, desc: [Corte horizontal: cada punto se desplaza en x `x` veces su altura sobre el pivote.]), (name: "y", type: "float", default: none, desc: [Corte vertical: cada punto se desplaza en y `y` veces su distancia horizontal al pivote.])),
  desc: [Fija el sesgo (_skew_) absoluto alrededor del pivote; `(0, 0)` lo quita. Los factores son tangentes sin unidades: con el pivote en la base, `skew_to(0.15, 0)` desplaza la parte superior 0.15 unidades por cada unidad de altura. En un grupo deforma a todos los hijos a la vez, sin trocear la figura. `animate.skew_to(x, y)` lo anima. Lanza `ValueError` con factores no finitos.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Easing, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
ground = scene.geometry.rect(8, 0.1).fill(GOLD).move_to(0, -2.05)
house = scene.geometry.group([
    scene.geometry.rect(3, 2.5).fill(BLUE).move_to(0, -0.75),
    scene.geometry.polygon([(-1.8, 0.5), (1.8, 0.5), (0, 2)]).fill(BLUE),
]).with_pivot(0, -2)
scene.play(house.animate.skew_to(0.2, 0).duration(0.4).easing(Easing.SMOOTH))
scene.play(house.animate.skew_to(-0.12, 0).duration(0.4).easing(Easing.SMOOTH))
scene.play(house.animate.skew_to(0, 0).duration(0.5).easing(Easing.SMOOTH))
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.matrix_to",
  kind: "method",
  params: ((name: "matrix", type: "tuple[tuple[float, float], tuple[float, float]]", default: none, desc: [Filas `((a, b), (c, d))` de la aplicación `(x, y) → (a x + b y, c x + d y)`.]),),
  desc: [Aplica una transformación lineal 2D cualquiera alrededor del pivote y sustituye el giro, el sesgo y la escala actuales. A diferencia de `skew_to`, lleva los ejes x e y a cualquier dirección, así que una cara dibujada de frente pasa a una proyección isométrica u oblicua sin trocearla. `animate.matrix_to(...)` la anima: se descompone en giro, sesgo y escala, que se interpolan juntos. Lanza `ValueError` con una matriz no finita o no invertible.],
)[
```python
# show-code: true
import math
from gaanim import BLUE, GOLD, GRAY, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
c, s = math.cos(math.pi / 6), math.sin(math.pi / 6)
# Las caras giran sobre su centro: el eje x baja o sube 30°, el y sigue vertical.
side = scene.geometry.rect(3, 2).fill(BLUE).stroke(GRAY, 0.03).move_to(-1.3, 0)
front = scene.geometry.rect(3, 2).fill(GOLD).stroke(GRAY, 0.03).move_to(1.3, 0)
scene.play([
    side.animate.matrix_to(((c, 0), (-s, 1))).duration(0.8),
    front.animate.matrix_to(((c, 0), (s, 1))).duration(0.8),
])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.bounds",
  kind: "method",
  returns: (type: "Bounds", desc: [`x` e `y` (el centro), `left`, `right`, `bottom`, `top`, `width`, `height` y `center` en unidades de escena.]),
  desc: [Mide la caja de cualquier objeto (figuras, texto, matemática, SVG, imágenes, grupos) en el cursor actual, tal como se dibujaría ahí: cuenta la maquetación, las transformaciones, la composición del texto, las animaciones que terminaron hasta `scene.cursor` y los cambios inmediatos hechos en él (como un `move_to` sobre un objeto ya animado), e incluye a los descendientes. La geometría que los objetos reactivos regeneran en cada fotograma se mide como se declaró. Medir un objeto recién creado, antes del siguiente `play` o `wait` y sin nada que actúe sobre él (animaciones, grupos posteriores, layouts), cuesta alrededor de un milisegundo: se compila solo su declaración y la de sus miembros o referencias de `next_to`/`align_to` que cumplan lo mismo. En cualquier otro caso se compila la escena escrita hasta ese punto. Las mediciones sin nada escrito entre ellas comparten una compilación, y también los miembros de un mismo layout recién creado, así que conviene medir antes de animar y medir juntas las cajas que necesites. Lanza `ValueError` si el objeto no tiene geometría en el cursor. Sustituye a `scene.text.measure`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
label = scene.text("PGA = 0.35 g", role="label").move_to(-2, 1)
box = label.bounds()
frame = scene.geometry.rounded_rect(box.width + 0.56, box.height + 0.32, 0.14)
frame.no_fill().stroke(GOLD, 0.04).move_to(*box.center)
scene.play([frame.animate.create().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.pixel",
  kind: "method",
  params: ((name: "px, py", type: "float", default: none, desc: [Píxel del archivo original, contado desde la esquina superior izquierda con y hacia abajo, como en un editor de imágenes.]),),
  returns: (type: "AnchorPoint", desc: [El punto de la imagen o el vídeo en ese píxel.]),
  desc: [Convierte un píxel de una imagen o un vídeo en un punto de anclaje que sirve en `move_to`, `scene.camera.inset`, `pan_to` o conectores, y que sigue a la imagen si se mueve, escala o gira. Tiene en cuenta el recorte, `width=` y el modo de ajuste; mide sobre el archivo original, no sobre una captura reducida. Lanza `ValueError` en otros objetos o con coordenadas no finitas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ui = scene.media.image("assets/cover.png", width=12)
view = scene.camera.inset(ui.pixel(812, 240), zoom=3)
```
]

#api-entry(
  name: "Drawable.with_pivot",
  kind: "method",
  desc: [Fija el pivote de rotación, escala y sesgo en coordenadas de escena. Úsalo para bisagras y brazos que giran alrededor de un extremo. Sin él, el pivote es el centro de la caja en las figuras declaradas con coordenadas de escena (`line`, `polygon`, arcos, flechas y curvas) y el origen propio en las demás, que en `circle`, `rect` o `text` también es su centro. Una vez mostrado el objeto, el pivote cambia en el cursor sin moverlo y vale para las animaciones siguientes. `pivot(x, y)` es un alias.],
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

#api-entry(
  name: "Drawable.copy",
  kind: "method",
  returns: (type: "Drawable", desc: [Un objeto nuevo e independiente; la copia de un `Text` es un `Text`.]),
  desc: [Copia el objeto tal como está declarado: forma, estilo, efectos y posición, con copias de sus miembros y, en un SVG importado, de sus partes con nombre (`copia.part("cabeza")` es la cabeza de la copia). No copia animaciones ni updaters, y los cambios posteriores de uno no llegan al otro. La copia se dibuja encima de lo declarado antes. Sirve para fantasmas o reflejos de un personaje sin importar el SVG varias veces.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
robot = scene.media.svg("assets/robot.svg").scale_to(2)
ghost = robot.copy().opacity(0.3).shift_by(-1.5, 0)
ghost.part("head").fill(GOLD)
```
]

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
  name: "Drawable.shader_effect",
  kind: "method",
  params: (
    (name: "effect", type: "PostProcess | Sequence[PostProcess] | None", default: none, desc: [Un pase o una cadena de pases WGSL; `None` quita el efecto.]),
    (name: "margin", type: "float", default: "0.25", desc: [Unidades de escena que se agregan alrededor del objeto, para efectos que se salen de su contorno (un brillo, una onda).]),
  ),
  desc: [Dibuja el objeto y sus descendientes en una textura propia, le aplica los pases y devuelve el resultado a la escena en su lugar del orden de dibujo. Es un `PostProcess.shader` sobre un solo objeto: en el shader, `gaanim_scene(uv)` lee solo ese objeto (transparente alrededor), `uv` va de 0 a 1 sobre su caja con el margen, `resolution` es el tamaño de la textura en píxeles y `time` es el tiempo de la escena. Los uniforms aceptan `Parameter`, `Computed` y señales de audio. Los presets de `PostProcess` (`grain`, `chromatic_aberration`…) también sirven. Es estado de declaración: vale para toda la línea de tiempo. Si un objeto con efecto contiene otro, se aplica el efecto exterior. Se ve en la vista previa, en la exportación y en los paquetes `.gaanim`, que guardan los uniforms de cada fotograma.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, PostProcess, Scene
scene = Scene(frame=(16, 9), background="#0b1020")
ripple = PostProcess.shader("""
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let wave = gaanim_uniforms.amount * sin(uv.x * 14.0 + time * 6.0);
    return gaanim_scene(uv + vec2<f32>(0.0, wave));
}
""", uniforms={"amount": 0.12})
title = scene.text("Ondas").fill(WHITE).scale_to(2.0).shader_effect(ripple, margin=0.4)
plain = scene.text("Sin efecto").fill(GOLD).scale_to(1.0).move_to(0, -2.5)
scene.wait(2)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.matte",
  kind: "method",
  params: (
    (name: "source", type: "Drawable | None", default: none, desc: [El mate: el objeto cuya forma o brillo deja ver a este; `None` quita el mate.]),
    (name: "mode", type: "str", default: "\"alpha\"", desc: [`"alpha"`, `"alpha_inverted"`, `"luma"` o `"luma_inverted"`.]),
  ),
  desc: [Muestra el objeto y sus descendientes solo a través de `source`, como un _track matte_ de After Effects. Con `"alpha"` se ve donde `source` es opaco (una foto o un degradado dentro de las letras de un título); con `"luma"`, donde `source` es claro (un rectángulo con degradado que avanza hace un revelado suave); los modos invertidos muestran lo contrario. El mate sigue a `source` cuando se mueve, escala, aparece o se escribe, pero `source` deja de dibujarse por sí mismo. Un `source` desenfocado (`blur`, `blur_in`) suaviza el mate, no el objeto: los bordes de sus partes siguen nítidos dentro de la zona suave, así que unas barras enmascaradas por un texto que entra con `blur_in` se ven enteras donde un glifo aún es una mancha. Para que el resultado entre suave, funde `source` con `animate.opacity` o desenfoca el objeto. Es estado de declaración, vale para toda la línea de tiempo y se ve en la vista previa, la exportación y los paquetes `.gaanim`. Un `source` de otra escena, el propio objeto o un modo desconocido lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import CORAL, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0b1020")
title = scene.text("MATE").fill(WHITE).scale_to(3.0)
stripes = scene.geometry.group([
    scene.geometry.rect(0.5, 3).fill(color).no_stroke().move_to(-3.5 + 0.5 * i, 0)
    for i, color in zip(range(15), [GOLD, CORAL, "#22d3ee"] * 5)
])
stripes.matte(title)
scene.play([title.animate.write().duration(1.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.glass",
  kind: "method",
  params: (
    (name: "blur", type: "float", default: "0.25", desc: [Desenfoque de lo que hay detrás, en unidades de escena (la sigma de una gaussiana).]),
    (name: "saturation", type: "float", default: "1.4", desc: [Saturación de esos colores; 1 los deja igual.]),
    (name: "refraction", type: "float", default: "0.08", desc: [Cuánto se curva hacia dentro, en el borde, lo que hay justo fuera, en unidades de escena.]),
    (name: "edge", type: "float", default: "0.3", desc: [Brillo del borde, con la luz arriba a la izquierda, de 0 a 1.]),
    (name: "dispersion", type: "float", default: "0.0", desc: [Aberración cromática: cuánto se separan el rojo, el verde y el azul en el borde, de 0 a 1.]),
    (name: "bevel", type: "float", default: "0.12", desc: [Ancho del borde redondeado de la lente, en unidades de escena; hacia dentro el vidrio es plano y no deforma.]),
    (name: "twist", type: "float", default: "0.0", desc: [Desliza lo que muestra el borde a lo largo del contorno, como fracción de la curvatura, de -2 a 2: positivo gira en sentido horario (hacia la derecha arriba y hacia la izquierda abajo), como en _Liquid Glass_.]),
    (name: "transparency", type: "float", default: "1.0", desc: [Cuán claro es el vidrio, de 0 a 1: 1 deja ver lo que hay detrás, valores menores lo vuelven lechoso y 0 es blanco opaco.]),
  ),
  desc: [Convierte el objeto en vidrio. Dentro de su contorno se ve el lienzo y todo lo que se dibujó antes, desenfocado y saturado. El contorno es una lente con un borde redondeado de ancho `bevel`: ahí lo que hay detrás se curva, se separa en colores y recibe un brillo. El borde sigue el contorno real de cualquier forma, metaballs incluidos. El objeto se dibuja encima, así que un relleno translúcido tiñe el vidrio y un trazo lo enmarca; el vidrio aparece y se desvanece con él. `liquid_glass()` es el mismo vidrio con valores de _Liquid Glass_ (`refraction=1.0`, `dispersion=0.12`, `bevel=0.6`, `blur=0.04`, `edge=0.8`, `saturation=1.4`, `transparency=0.92`, `twist=0.6`): casi sin desenfoque, con el centro plano, un borde ancho que se empina hacia el contorno, curva con fuerza lo que hay detrás y lo desliza a lo largo del contorno, y una línea fina de luz en el contorno. Con un relleno claro (`"#ffffff26"`) se ve como en iOS. `backdrop_blur(radius, saturation=1.0)` es solo el desenfoque, y `no_glass()` quita el efecto. Es estado de declaración y se ve en la vista previa, la exportación y los paquetes `.gaanim`. Valores negativos o no finitos, o `edge`, `dispersion` o `transparency` mayores que 1, lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, CORAL, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0b1020")
for i, color in enumerate([BLUE, GOLD, CORAL]):
    scene.geometry.circle(1.3).fill(color).no_stroke().move_to(-2.5 + 2.5 * i, 0.4 * (-1) ** i)
card = scene.geometry.rounded_rect(5, 3, 0.4).fill("#ffffff1a").stroke("#ffffff55", 0.03).move_to(-3, 1)
card.glass(blur=0.25, refraction=0.14)
drops = [scene.geometry.circle(0.9).move_to(x, -2.2) for x in (-2.5, 2.5)]
lens = scene.geometry.metaballs(drops, smoothness=0.9).fill("#ffffff10").liquid_glass()
scene.play([card.animate.move_to(3, 1).duration(2.0), drops[0].animate.move_to(1.0, -2.2).duration(2.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.shadow",
  kind: "method",
  params: ((name: "color", type: "Color", default: none, desc: [Color de la sombra; su alfa escala la opacidad.]), (name: "x / y", type: "float", default: "0.08 / -0.08", desc: [Desplazamiento en unidades de escena.]), (name: "blur", type: "float", default: "0.06", desc: [Desenfoque en unidades de escena.])),
  desc: [Sombra proyectada desplazada y desenfocada. En un texto o un grupo, una sombra desenfocada se dibuja una sola vez a partir de la silueta de sus miembros rellenos y debajo de todos, como `drop-shadow` de CSS sobre un grupo: los solapes no se oscurecen y el coste no crece con cada glifo. Los miembros recortados, con mezcla o solo con trazo proyectan la suya.],
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
  name: "Drawable.tip",
  kind: "method",
  params: ((name: "end / start", type: "\"arrow\" | \"dot\" | None", default: "\"arrow\" / None", desc: [Punta de cada extremo del camino.]), (name: "length", type: "float | None", default: "None", desc: [Largo de la punta de flecha; por defecto, cinco grosores de trazo y al menos 0.15.]), (name: "width", type: "float | None", default: "None", desc: [Ancho de la base de la flecha (0.9 del largo) o diámetro del punto (tres grosores de trazo).])),
  desc: [Pone puntas de flecha o puntos en los extremos de cualquier trazo: polilíneas, curvas, arcos o conectores con `via`. Se rellenan con el color del trazo y siguen a los extremos actuales del camino, así que acompañan a `trim`, `create()` y a los caminos que se regeneran; `animate.grow_arrow()` dibuja el trazo desde la cola con la punta delante. El vértice de la flecha es el extremo del camino y el trazo termina bajo su base. Si el camino es más corto que la punta, esta se encoge con él. `tip(None)` las quita. Un tipo desconocido o un tamaño no positivo lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import CYAN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
route = scene.geometry.polyline([(-5, -1.5), (-2, 1.5), (2, -1.5), (5, 1.5)]).no_fill()
route.stroke(CYAN, 0.06).tip(end="arrow", start="dot")
scene.play([route.animate.grow_arrow().duration(1.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.blend",
  kind: "method",
  params: ((name: "mode", type: "str", default: "\"normal\"", desc: [`"normal"`, `"multiply"`, `"screen"`, `"overlay"`, `"darken"`, `"lighten"`, `"color_dodge"`, `"color_burn"`, `"hard_light"`, `"soft_light"`, `"difference"`, `"exclusion"`, `"hue"`, `"saturation"`, `"color"`, `"luminosity"` o `"add"`.]),),
  desc: [Modo de fusión con lo que hay debajo. `"screen"` y `"add"` aclaran (luces, destellos), `"multiply"` oscurece como tinta (resaltadores) y `"normal"` vuelve a pintar encima, también en un miembro de un grupo con otro modo. Como `fill`, en un grupo o un `Text` llega a todos sus miembros; un miembro al que le cambias el modo después conserva el suyo. El objeto se pinta en su propia capa, así que dentro de un recorte, un grupo con opacidad o una transición solo se funde con el contenido de ese grupo. No se anima. Un modo desconocido lanza `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#101826")
panel = scene.geometry.rect(9, 4).fill("#2B6CB0").move_to(0, 0)
leak = scene.geometry.circle(1.6).fill("#F6AD55").move_to(-2.5, 0).blend("screen")
ink = scene.geometry.circle(1.6).fill("#F6AD55").move_to(2.5, 0).blend("multiply")
scene.play([leak.animate.move_to(-1, 0).duration(1.0), ink.animate.move_to(1, 0).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.echo",
  kind: "method",
  params: (
    (name: "count", type: "int", default: "5", desc: [Copias, de 1 a 32; `0` quita el eco.]),
    (name: "delay", type: "float", default: "0.04", desc: [Segundos entre copias; positivo.]),
    (name: "decay", type: "float", default: "0.6", desc: [Opacidad de cada copia respecto a la anterior, en `(0, 1]`.]),
    (name: "hold", type: "bool", default: "False", desc: [Con `True` las copias se retrasan a lo largo del movimiento y no del reloj: cuando el objeto se detiene, quedan congeladas donde estaban (papel cebolla) y siguen cuando vuelve a moverse. Sin él, alcanzan al objeto al detenerse y dejan de dibujarse: una copia que coincide con el objeto no se apila sobre él, así que un objeto translúcido en reposo se ve igual que sin eco.]),
    (name: "start / end", type: "float | None", default: "None", desc: [Segundos de la escena que graban las copias: cada copia muestra el objeto solo como estaba entre `start` y `end`, y se oculta fuera de ese intervalo. Así la estela crece desde `start`, se vacía después de `end` y no aparece en el resto de movimientos. Cualquiera de los dos puede quedar abierto; `end` debe ser mayor que `start`.]),
  ),
  desc: [Copias que siguen al objeto en el tiempo, como el efecto Echo de After Effects: la copia `k` lo muestra como estaba hace `k * delay` segundos, con `decay ** k` de su opacidad y debajo de él. Cada copia repite las animaciones del propio objeto (`animate`, `create`, fundidos, color y forma) con ese retraso, así que es exacta en cualquier búsqueda y en todas las exportaciones, SVG incluido. Se ocultan mientras el objeto está oculto y no cruzan un corte de segmento. No retrasan el movimiento de los _updaters_, de las posiciones reactivas ni de un grupo padre que se mueve, ni copian Lottie o vídeo. En un `Text`, cada glifo repite sus propias animaciones. Se declara una vez y vale para toda la línea de tiempo.],
)[
```python
# show-code: true
from gaanim import CORAL, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0e1422")
ball = scene.geometry.circle(0.45).fill(CORAL).move_to(-6, 0).echo(6, delay=0.05, decay=0.7)
scene.play([ball.animate.move_to(6, 0).duration(1.2)])
scene.play([ball.animate.move_to(0, 0).fill(GOLD).duration(0.6)])
# output: preview.webp
scene.render()
```
]

```python
# show-code: true
from gaanim import CORAL, Scene
scene = Scene(frame=(16, 9), background="#0e1422")
# La estela solo existe durante la sacudida, de 1.0 s a 1.6 s.
head = scene.geometry.circle(0.6).fill(CORAL).move_to(-4, 0).echo(6, delay=0.04, start=1.0, end=1.6)
scene.play([head.animate.move_to(0, 0).duration(1.0)])
scene.play([head.animate.wiggle().duration(0.6)])
scene.play([head.animate.move_to(4, 0).duration(1.0)])
# output: preview.webp
scene.render()
```


#api-entry(
  name: "Drawable.count",
  kind: "method",
  params: (
    (name: "count", type: "float", default: none, desc: [Copias visibles, de 0 al número de copias del grupo.]),
  ),
  desc: [Muestra las primeras `count` copias de un grupo creado con `Geometry.repeat` o `Geometry.duplicate`; las demás siguen en el grupo pero ocultas, de la última hacia atrás. Un recuento fraccionario funde la siguiente copia: `2.5` muestra dos copias y media tercera. Antes del primer `scene.play` fija el recuento inicial; después lo cambia en el cursor. Para animarlo usa `animate.count`. Un valor fuera de rango, o un objeto que no sea uno de esos grupos, lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
ring = scene.geometry.duplicate(scene.geometry.dot(0.1), Distribution.circle(16, 2.5)).count(4)
```
]

#api-entry(
  name: "Drawable.points",
  kind: "method",
  params: (
    (name: "points", type: "Sequence[tuple[float, float]]", default: none, desc: [Un punto por vértice, en las coordenadas en que se declaró la figura.]),
  ),
  desc: [Fija todos los vértices de un polígono o una polilínea. Antes del primer `scene.play` cambia la forma declarada, así que varias figuras pueden nacer iguales y deformarse luego; después la cambia en el cursor. `animate.points` sigue desde ahí. Una polilínea con `closed=True` sigue cerrada. Otro número de puntos, un valor no finito, un objeto que no sea polígono ni polilínea o una polilínea con coordenadas reactivas lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
tri = scene.geometry.polygon([(0, 0), (1, 0), (0, 1)]).points([(-1, -1), (1, -1), (0, 1)])
scene.play([tri.animate.points([(-2, -1), (2, -1), (0, 2)]).duration(1.0)])
```
]

#api-entry(
  name: "Drawable.squash_stretch",
  kind: "method",
  params: (
    (name: "amount", type: "float", default: "0.1", desc: [Estiramiento por unidad de velocidad (unidades de escena por segundo); `0` quita el efecto.]),
    (name: "max_ratio", type: "float", default: "1.6", desc: [Estiramiento máximo, al menos 1.]),
  ),
  desc: [Estira el objeto en la dirección de su velocidad y lo aplasta en la perpendicular, conservando el área: el estiramiento es `1 + amount * velocidad`, como mucho `max_ratio`, y el aplastamiento su inverso. La velocidad sale de las animaciones del propio objeto medidas en `t ± 1/60` s, así que un objeto quieto nunca se deforma y las búsquedas son exactas. Los miembros de un grupo se deforman con él. No cuentan el movimiento de los _updaters_, las posiciones reactivas ni un padre que se mueve. Un `amount` negativo o un `max_ratio` menor que 1 lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import CORAL, Scene
scene = Scene(frame=(16, 9), background="#0f1729")
ball = scene.geometry.circle(0.5).fill(CORAL).move_to(-6, 0).squash_stretch(0.08, max_ratio=1.8)
scene.play([ball.animate.move_to(6, 0).duration(1.0)])
scene.play([ball.animate.move_to(0, -2.5).duration(0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.stroke_profile",
  kind: "method",
  params: ((name: "profile", type: "Sequence[tuple[float, float]] | None", default: none, desc: [Pares `(posición, factor)`: posición en [0, 1] de la longitud visible y factor del ancho del trazo; `None` vuelve al trazo normal.]),),
  desc: [Trazo de grosor variable, como los perfiles de ancho de Illustrator: el ancho se interpola linealmente entre los puntos, así que `[(0, 0), (0.5, 1), (1, 0)]` se ensancha en el centro y termina en punta. El trazo se dibuja como un contorno relleno: no admite guiones, pero `trim`, `create` y `show_passing_flash` recortan la parte visible y el perfil se ajusta a ella, así que un destello afinado sigue acabando en punta. En un grupo o un `Text` llega a todos sus miembros. Una posición fuera de [0, 1] o un factor negativo lanzan `ValueError`.],
)[
```python
# show-code: true
import math
from gaanim import CORAL, GOLD, TEAL, Scene
scene = Scene(frame=(16, 9), background="#101826")
brush = scene.geometry.arc(-4, 0, 2.0, 0.3, 4.2).no_fill().stroke(GOLD, 0.35).stroke_taper(0.35, 0.5)
wave = scene.geometry.polyline([(x / 10, 1 + 0.6 * math.sin(x / 4)) for x in range(-10, 61)])
wave.stroke(TEAL, 0.3).stroke_profile([(0.0, 0.1), (0.5, 1.0), (1.0, 0.1)])
flash = scene.geometry.polyline([(-1, -2.2), (2, -1.2), (5, -2.6), (7, -1.8)]).stroke(CORAL, 0.25).stroke_taper(0.5, 0.5)
scene.play([brush.animate.create(), wave.animate.create()])
scene.play([flash.animate.show_passing_flash(time_width=0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.stroke_taper",
  kind: "method",
  params: ((name: "start / end", type: "float", default: "0.2", desc: [Fracción de la longitud visible en la que el trazo se afina hasta una punta, al principio y al final; `start + end <= 1`.]),),
  desc: [Atajo de `stroke_profile` para pinceladas y flechas caligráficas.],
  none,
)

#api-entry(
  name: "Drawable.modifiers",
  kind: "property",
  returns: (type: "PathModifiers", desc: [La pila de modificadores de trazado del objeto.]),
  desc: [Modificadores no destructivos, como los operadores de forma de After Effects y Lottie: cada método añade uno a partir del cursor y lo devuelve, y sus números se animan con `modificador.animate`. Se aplican en el orden en que se añadieron, sobre el trazado ya animado (recortes, `write`, morph) y justo antes de dibujarlo, a cada miembro con trazado de un grupo o un `Text`. `bounds()` y el layout siguen midiendo la forma original. Un trazado abierto puede ganar área; las primitivas abiertas (`line`, `arc`, `polyline`, `traced_path`…) nacen sin relleno, así que solo se ve el trazo hasta que le des uno con `fill()`.],
)[
```python
# show-code: true
from gaanim import BLUE, CORAL, GOLD, TEAL, WHITE, Scene
scene = Scene(frame=(16, 9))
star = scene.geometry.star(5, 1.1, 0.5).fill(GOLD).move_to(-5.0, 1.0)
zz = star.modifiers.zigzag(size=0.0, ridges=3)
star.modifiers.round_corners(0.04)
square = scene.geometry.square(1.6).fill(CORAL).move_to(-1.7, 1.0)
pb = square.modifiers.pucker_bloat(0.0)
hexagon = scene.geometry.regular_polygon(6, 1.0).fill(TEAL).move_to(1.7, 1.0)
tw = hexagon.modifiers.twist(0.0)
blob = scene.geometry.circle(0.9).fill(BLUE).move_to(5.0, 1.0)
blob.modifiers.wiggle_path(size=0.12, detail=3, frequency=1.5, seed=3)
ring = scene.geometry.circle(0.5).no_fill().stroke(WHITE, 0.04, align="center").move_to(0, -2.2)
rings = ring.modifiers.offset(0.15, join="round", copies=3)
scene.play([zz.animate.size(0.12), pb.animate.amount(-0.5), tw.animate.angle(1.4), rings.animate.amount(0.3)])
scene.play([pb.animate.amount(0.4).duration(0.8), tw.animate.angle(-1.0).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "PathModifiers.zigzag",
  kind: "method",
  params: (
    (name: "size", type: "float", default: "0.1", desc: [Distancia de cada pico a cada lado del trazado, en unidades. Animable.]),
    (name: "ridges", type: "int", default: "4", desc: [Picos por segmento, de 1 a 256.]),
    (name: "smooth", type: "bool", default: "False", desc: [Ondas en lugar de esquinas.]),
  ),
  returns: (type: "PathModifier", desc: [El modificador, que anima `size`.]),
  desc: [Los vértices quedan en su sitio, así que con `size = 0` la forma no cambia.],
  none,
)

#api-entry(
  name: "PathModifiers.round_corners",
  kind: "method",
  params: ((name: "radius", type: "float", default: none, desc: [Radio de cada esquina, `>= 0`. Animable.]),),
  returns: (type: "PathModifier", desc: [El modificador, que anima `radius`.]),
  desc: [Redondea las esquinas entre dos tramos rectos; un lado más corto que dos radios limita sus esquinas y las curvas conservan sus vértices.],
  none,
)

#api-entry(
  name: "PathModifiers.pucker_bloat",
  kind: "method",
  params: ((name: "amount", type: "float", default: none, desc: [Entre `-1` (pucker, puntas) y `1` (bloat, lóbulos). Animable.]),),
  returns: (type: "PathModifier", desc: [El modificador, que anima `amount`.]),
  desc: [Los vértices se acercan al centro del trazado en esa fracción y las curvas entre ellos se alejan, o al revés con valores negativos.],
  none,
)

#api-entry(
  name: "PathModifiers.twist",
  kind: "method",
  params: ((name: "angle", type: "float", default: none, desc: [Giro en el centro, en radianes. Animable.]),),
  returns: (type: "PathModifier", desc: [El modificador, que anima `angle`.]),
  desc: [El giro se desvanece hasta cero en el vértice más lejano del centro.],
  none,
)

#api-entry(
  name: "PathModifiers.wiggle_path",
  kind: "method",
  params: (
    (name: "size", type: "float", default: "0.1", desc: [Desplazamiento máximo a lo largo de la normal, en unidades. Animable.]),
    (name: "detail", type: "int", default: "6", desc: [Puntos por segmento, de 1 a 256.]),
    (name: "frequency", type: "float", default: "1.0", desc: [Cambios por segundo. Animable.]),
    (name: "seed", type: "int", default: "0", desc: [Semilla del ruido.]),
  ),
  returns: (type: "PathModifier", desc: [El modificador, que anima `size` y `frequency`.]),
  desc: [Contorno vivo con ruido coherente con semilla, desde el instante en que se añade; los puntos se unen con curvas suaves. La misma semilla da la misma forma en el mismo tiempo, así que un seek es exacto.],
  none,
)

#api-entry(
  name: "PathModifiers.offset",
  kind: "method",
  params: (
    (name: "amount", type: "float", default: none, desc: [Unidades hacia fuera, o hacia dentro si es negativo. Animable.]),
    (name: "join", type: "\"miter\" | \"round\" | \"bevel\"", default: "\"miter\"", desc: [Cómo gira el contorno en las esquinas.]),
    (name: "copies", type: "int", default: "1", desc: [Contornos a `amount`, `2 * amount`, …, de 1 a 64.]),
  ),
  returns: (type: "PathModifier", desc: [El modificador, que anima `amount`.]),
  desc: [Agranda o encoge las formas cerradas; encoger más allá del radio interior no deja nada. Un trazado abierto se vuelve una banda de `|amount|` a cada lado. Las copias forman un solo trazado, así que un trazo alineado por dentro (el predeterminado) solo muestra la mitad interior del anillo exterior: para anillos usa `align="center"`.],
  none,
)

#api-entry(
  name: "PathModifier.animate",
  kind: "property",
  returns: (type: "PathModifierAnimation", desc: [Métodos `size`, `radius`, `amount`, `angle` y `frequency`, que devuelven un `Anim`.]),
  desc: [Anima los números del modificador como cualquier otra animación. Un nombre que el modificador no tiene lanza `ValueError`; `PathModifier.names` los enumera y `PathModifier.set(size=0.2)` los cambia de golpe desde el cursor.],
  none,
)

#api-entry(
  name: "Drawable.motion_blur",
  kind: "method",
  params: ((name: "enabled", type: "bool", default: "True", desc: [`False` mantiene nítido el objeto y sus miembros.]),),
  desc: [Si el #link("/referencia/themes/#api-canvas-motion_blur")[desenfoque de movimiento] de la escena lo difumina. Con `False`, cada subcuadro lo dibuja como está en el tiempo del cuadro, por ejemplo para un título o un HUD que no deben emborronarse mientras todo lo demás se mueve. Las copias de `echo` heredan el ajuste.],
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

#api-entry(
  name: "Drawable.camera_view",
  kind: "method",
  params: (
    (name: "frame", type: "Drawable | None", default: "None", desc: [Objeto que hace de cámara: su centro marca adónde mira y su giro gira la vista en sentido contrario. Sin `zoom`, su tamaño también fija el aumento, así que reducirlo acerca. Sin `frame`, la cámara mira a través de un marco invisible.]),
    (name: "center", type: "tuple | Drawable | AnchorPoint | None", default: "None", desc: [Adónde mira la cámara cuando no hay `frame`: un punto, el centro de un objeto o un punto de anclaje; por defecto, el origen. Con `frame` lanza `ValueError`.]),
    (name: "zoom", type: "float | Parameter | Computed | None", default: "None", desc: [Aumento fijo en lugar del tamaño del marco. Un número da a la vista su propio zoom, que `animate.zoom_to` cambia a velocidad percibida constante; un `Parameter` se anima de forma lineal; un `Computed` se sigue y no se puede fijar. Sin `frame` ni `zoom`, vale 1.]),
    (name: "fit", type: "str", default: "\"contain\"", desc: [Con un aumento dado por el marco: `"contain"` muestra el marco entero, `"cover"` llena la pantalla y `"stretch"` escala cada eje por separado.]),
    (name: "background", type: "Paint | str | None", default: "\"canvas\"", desc: [Fondo de la vista: `"canvas"` pinta el fondo de la escena tal como lo ve la cámara, un color o `Brush` pinta la pantalla y `None` deja ver el relleno propio de la pantalla.]),
    (name: "exclude", type: "Sequence[Drawable]", default: "()", desc: [Objetos, con sus hijos, que la vista no muestra.]),
    (name: "layers", type: "Sequence[str]", default: "()", desc: [Capas de vista (ver `view_layer`) que la vista muestra además de los objetos normales.]),
  ),
  returns: (type: "CameraView", desc: [La cámara, con `pan_to`, `zoom_to`, `rotate_to`, `follow` y `animate`; ver #link("/referencia/scene/")[Escena].]),
  desc: [Convierte esta figura en una pantalla que muestra lo que ve una segunda cámara, como un _picture-in-picture_ o una lupa. La pantalla dibuja su relleno, encima la vista recortada a su contorno y encima su trazo; la opacidad y los fundidos afectan a todo. La vista no muestra la pantalla, el marco, `exclude` ni las superposiciones HUD; una pantalla que aparece dentro de otra vista muestra su pintura pero no su vista. Solo una figura cerrada 2D puede ser pantalla (rectángulo, cuadrado, círculo, punto, elipse, polígono, estrella, sector, anillo, curva, camino SVG o booleano). Lanza `ValueError` con texto, grupos, imágenes u objetos 3D, con `frame` igual a la propia pantalla, con objetos o parámetros de otra escena, con un zoom no positivo, con un nombre de capa vacío o con un `fit` o `background` desconocido. Una llamada posterior o `no_camera_view()` sustituye la vista.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
for i in range(40):
    scene.geometry.dot(0.04).fill(GOLD if i % 3 else BLUE).move_to(-6 + 0.15 * i, 0.4 * (i % 4) - 0.6)
frame = scene.geometry.rect(1.6, 0.9).no_fill().stroke(GOLD, 0.03).move_to(-5, 0)
screen = scene.geometry.rounded_rect(6.4, 3.6, 0.2).stroke(WHITE, 0.05).move_to(3.5, 0)
screen.camera_view(frame)
scene.play([frame.animate.move_to(-1.5, 0).duration(1.2)])
scene.play([frame.animate.scale_to(0.5).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.no_camera_view",
  kind: "method",
  desc: [Quita la vista de cámara y la figura vuelve a dibujarse como una forma normal.],
  none,
)

#api-entry(
  name: "Drawable.view_layer",
  kind: "method",
  params: ((name: "name", type: "str | None", default: none, desc: [Nombre de la capa; `None` devuelve el objeto a ninguna capa.]),),
  desc: [Pone el objeto y sus hijos en una capa de vista: la cámara principal no lo dibuja y solo lo muestran las vistas de cámara que listan esa capa en `layers`. Sirve para una lupa de rayos X que revela lo que hay debajo, o para etiquetas que solo aparecen en el zoom. Un nombre vacío lanza `ValueError`.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
scene.geometry.rounded_rect(6, 3, 0.4).fill("#334155").no_stroke()
for x in (-2, 0, 2):
    scene.geometry.circle(0.5).fill(GOLD).no_stroke().move_to(x, 0).view_layer("xray")
lens = scene.geometry.circle(1.2).stroke(WHITE, 0.05).move_to(-3, 0)
view = lens.camera_view(center=(-3, 0), layers=["xray"])
scene.play([lens.animate.move_to(3, 0).duration(2.0), view.animate.pan_to(3, 0).duration(2.0)])
# output: preview.webp
scene.render()
```
]

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

Gaanim ya no importa glTF, así que tampoco reproduce sus acciones:
`Drawable.animations()` devuelve una tupla vacía y
`Drawable.animation(name, ...)` lanza `NotImplementedError`. `part` y `parts`
están en #link("/referencia/medios/")[Medios].

== Relaciones reactivas

Hacen que un objeto siga a otro en el mismo fotograma sin callbacks de Python.
El seguidor queda oculto hasta su animación de entrada. Para simulaciones y
series muestreadas, consulta `add_updater_fn` y `drive_from_samples` en
#link("/referencia/animations/")[Animaciones].

#api-entry(
  name: "Drawable.follow",
  kind: "method",
  params: ((name: "source", type: "Endpoint", default: none, desc: [Drawable, `AnchorPoint`, `PointRef` o tupla.]), (name: "offset", type: "tuple[float, float]", default: "(0.0, 0.0)", desc: [Desplazamiento respecto a la fuente.]), (name: "offset_space", type: "str", default: "\"world\"", desc: [`"world"` mantiene el desplazamiento alineado con la pantalla; `"local"` lo rota y escala con la fuente.]), (name: "delay", type: "float", default: "0.0", desc: [Segundos de retardo: el objeto va donde estaba `source` hace `delay` segundos. Con un valor positivo `source` debe ser un Drawable.])),
  desc: [Sigue cualquier extremo en el mismo fotograma y devuelve el objeto. Con `delay` copia al líder con retardo, como `valueAtTime(time - delay)` de After Effects: encadena seguidores con retardos crecientes para estelas y colas (_overlapping action_). Las animaciones del líder se evalúan de nuevo en `t - delay` en cada fotograma, así que un seek a cualquier instante reproduce la estela sin estado acumulado. El líder puede seguir a su vez a otro objeto (`follow` con o sin retardo, `follow_to`, `attach_to`), así que los seguidores de seguidores forman cadenas; hasta que el segmento actual lleva `delay` segundos, el seguidor espera en la posición del líder al inicio del segmento. Offsets o retardos no finitos, retardos negativos o modos inválidos lanzan `ValueError`; un retardo positivo con una fuente que no es Drawable lanza `TypeError`.],
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

Una cola que sigue al líder con retardo:

```python
# show-code: true
from gaanim import CORAL, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
leader = scene.geometry.circle(0.35).fill(GOLD).move_to(-5, 0)
dots = [scene.geometry.circle(0.25 - 0.04 * i).fill(CORAL) for i in range(5)]
for i, dot in enumerate(dots):
    dot.follow(leader, delay=0.06 * (i + 1))
scene.play([dot.animate.fade_in().duration(0.01) for dot in dots])
scene.play(leader.animate.move_to(5, 1.5).duration(1.2))
scene.play(leader.animate.move_to(0, -2).duration(0.8))
scene.wait(0.5)
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
  desc: [Copia la coordenada X, la Y o ambas de `source` en cada fotograma, donde quiera que esté: animada, movida por un updater o colocada con `follow(...)`. `axes` acepta `"x"`, `"y"` o `"xy"`. Útil para proyecciones sobre un eje. Devuelven `None`.],
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
  desc: [Fija el objeto a la imagen como superposición: se queda en su sitio aunque la cámara 2D se desplace, acerque o gire, y aunque se mueva la cámara 3D. Sus coordenadas son las de la cámara sin mover, así que `.move_to(0, 3.5)` lo pone arriba en un fotograma de 16 × 9. Se dibuja encima de la escena y nunca aparece dentro de una vista de cámara. Pertenece a su segmento como cualquier objeto: `wipe`, `iris`, `blinds` y las demás transiciones vectoriales lo recortan con su segmento, y `push` y `slide` lo desplazan con él. En `cross_fade`, `fade_through` y `zoom_through`, que solo funden la opacidad, se ven a la vez el HUD del segmento que sale y el del que entra; si deben ocupar el mismo sitio, usa un solo HUD con `scene.persist(...)` y cambia su contenido, o dale al que entra una entrada que empiece al terminar la transición.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
title = scene.text("Modelo 3D", role="title").hud().move_to(0, 3.5)
```
]
