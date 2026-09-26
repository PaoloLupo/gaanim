#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Transiciones",
  description: "Wipes, iris, push, blinds y morph entre segmentos, overlays sobre el corte y cámara con impacto",
  route: "/guias/transiciones/",
)

= Pasar de una idea a la siguiente

En esta guía unes segmentos con transiciones vectoriales (barridos, iris,
persianas, empujes), añades un destello o una fuga de luz sobre el corte,
conviertes un objeto en otro a través del corte con `Transition.morph` y das
impacto a la cámara con una sacudida por trauma y un zoom que se percibe
uniforme.

Una transición pertenece al segmento que *entra*: se declara en
`scene.segment(nombre, transición)` y ocupa el principio de ese segmento, sin
alargar la escena.

```python
from gaanim import WHITE, Scene, Transition

scene = Scene(frame=(16, 9), background="#0f172a")

def tarjeta(titulo, color):
    scene.geometry.rounded_rect(11, 5, 0.3).fill(color)
    scene.text(titulo, role="title").fill(WHITE)

scene.segment("intro", background="#0f172a")
tarjeta("Primera idea", "#1e3a8a")
scene.wait(0.6)
scene.segment("detalle", Transition.wipe(0.6, direction="left", feather=0.15), background="#3b0764")
tarjeta("Segunda idea", "#7e22ce")
scene.wait(1.0)
scene.render()
# output: preview.webp
```

== Elegir una transición

#table(
  columns: (1.4fr, 2fr),
  inset: 7pt,
  [*Transición*], [*Cuándo usarla*],
  [`Transition.cut()`], [Cambio de ritmo seco; con un overlay, un impacto.],
  [`cross_fade(d)`], [Continuidad tranquila entre ideas relacionadas.],
  [`fade_through(d, color)`], [Cierre de capítulo: la pantalla pasa por un color.],
  [`wipe(d, direction, feather)`], [Avanzar en una lista o una línea de tiempo; la dirección sugiere "siguiente".],
  [`clock_wipe(d, start_angle)`], [Paso del tiempo, cuentas atrás.],
  [`iris(d, center, shape)`], [Entrar en un detalle desde un punto concreto del cuadro.],
  [`blinds(d, count, angle)`], [Cambio de sección con textura gráfica.],
  [`push(d, direction)`], [Pasar de página: el segmento nuevo empuja al anterior.],
  [`slide(d, direction)`], [El segmento nuevo se desliza encima del anterior, que queda quieto.],
  [`zoom_through(d)`], [Atravesar una idea para llegar a la siguiente.],
  [`morph(d, pairs=...)`], [Un objeto continúa a través del corte y cambia de forma, tamaño o color.],
)

Los revelados vectoriales (`wipe`, `clock_wipe`, `iris`, `blinds`, `push` y
`slide`) recortan los dos segmentos con geometría animada: son nítidos a
cualquier resolución y funcionan sin texturas. Si los segmentos tienen fondos
distintos, el fondo entrante se revela con la misma forma. Los objetos que
marcaste con `scene.persist(...)` no se recortan ni se desplazan.

```python
from gaanim import WHITE, Scene, Transition

scene = Scene(frame=(16, 9), background="#0f172a")

def tarjeta(titulo):
    scene.text(titulo, role="title").fill(WHITE)
    scene.wait(1.0)

scene.segment("intro", background="#1e3a8a")
tarjeta("Inicio")
scene.segment("reloj", Transition.clock_wipe(0.8, start_angle=90), background="#052e16")
tarjeta("clock_wipe")
scene.segment("iris", Transition.iris(0.7, center=(2, 1), shape="star"), background="#431407")
tarjeta("iris")
scene.segment("persiana", Transition.blinds(0.6, count=8, angle=0), background="#083344")
tarjeta("blinds")
scene.segment("empuje", Transition.push(0.5, direction="up"), background="#422006")
tarjeta("push")
scene.render()
```

`iris` también acepta un `Drawable` como `shape`: su contorno sirve de
plantilla. Ese drawable se sigue dibujando en su propio segmento, así que
ocúltalo si solo lo usas como forma.

== Easing en las transiciones

Todas las transiciones salvo `cut` aceptan `easing=` con cualquier `Easing`,
incluidos los springs, que pueden sobrepasar el final y volver. Sin él, los
revelados vectoriales usan `Easing.SMOOTH` y `cross_fade`, `fade_through`,
`zoom_through` y `morph` avanzan de forma lineal.

```python
from gaanim import WHITE, Easing, Scene, Transition

scene = Scene(frame=(16, 9), background="#0f172a")
scene.segment("a", background="#1e3a8a")
scene.text("Antes", role="title").fill(WHITE)
scene.wait(1.0)
scene.segment("b", Transition.slide(0.6, "left", easing=Easing.spring(bounce=0.2)), background="#7e22ce")
scene.text("Después", role="title").fill(WHITE)
scene.wait(1.0)
scene.render()
```

Un spring con poco `bounce` hace que el segmento nuevo "encaje" en su sitio;
úsalo con `slide` y `push`, donde el sobrepaso se lee como inercia.

== Overlays sobre el corte

`overlay=` dibuja algo encima de cualquier transición, centrado en su punto
medio (en `cut`, en el propio corte), sin cambiar la duración de los
segmentos.

- `Overlay.flash(color, duration)`: un destello que sube y se apaga. Da
  impacto a un corte seco.
- `Overlay.light_leak(seed, hue, duration, intensity)`: manchas de luz suaves
  que derivan por el cuadro. `hue` es el tono en vueltas: `0.1` es un naranja
  cálido y `0.6` un azul.

```python
from gaanim import WHITE, Overlay, Scene, Transition

scene = Scene(frame=(16, 9), background="#0f172a")
scene.segment("intro", background="#1e3a8a")
scene.text("Intro", role="title").fill(WHITE)
scene.wait(1.0)
scene.segment("impacto", Transition.cut(overlay=Overlay.flash(WHITE, 0.15)), background="#3b0764")
scene.text("¡Impacto!", role="title").fill(WHITE)
scene.wait(1.0)
scene.segment("cálido", Transition.cross_fade(0.6, overlay=Overlay.light_leak(seed=2, hue=0.1)), background="#052e16")
scene.text("Recuerdo", role="title").fill(WHITE)
scene.wait(1.0)
scene.render()
```

Los overlays son funciones puras del tiempo y de la semilla: la vista previa,
un seek y la exportación muestran el mismo fotograma.

== Morph: un objeto que cruza el corte

`Transition.morph(d, pairs=[(origen, destino)])` empareja un drawable del
segmento saliente con uno del entrante. La pareja comparte una caja que viaja
de la posición y el tamaño del origen a los del destino, mientras uno se
desvanece en el otro, así que se lee como un solo objeto que cambia. Lo que no
está emparejado se funde.

Las parejas se resuelven al compilar la escena, cuando ya existen los dos
segmentos. Por eso morph se declara con `scene.link(saliente, entrante,
transición)` después de crear ambos.

```python
from gaanim import CYAN, BLUE, WHITE, Scene, Transition

scene = Scene(frame=(16, 9), background="#0f172a")
barras_seg = scene.segment("barras")
barras = [scene.geometry.rect(1.4, h).fill(c).move_to(x, h / 2 - 2.5) for x, h, c in [(-3, 2, BLUE), (-1, 3.5, CYAN), (1, 2.75, BLUE)]]
scene.wait(0.6)
detalle_seg = scene.segment("detalle")
panel = scene.geometry.rounded_rect(9, 4.5, 0.3).fill("#16213a").move_to(0, 0)
scene.text("Detalle de la barra", role="heading").fill(WHITE).move_to(0, 1.2)
scene.wait(1.2)
scene.link(barras_seg, detalle_seg, Transition.morph(0.8, pairs=[(barras[1], panel)]))
scene.render()
# output: preview.webp
```

Morph encaja cuando el espectador debe seguir *un* elemento: la barra que se
convierte en su ficha, el icono que se convierte en el título de la sección.

== Cámara con impacto

La cámara también marca transiciones dentro de un segmento.

`scene.camera.animate.shake(trauma=0.8, decay=1.5, ...)` sigue el modelo de
trauma: la sacudida es proporcional a `trauma²`, así que un golpe suave apenas
se nota y uno fuerte sacude de verdad, y el trauma decae `decay` por segundo.
El movimiento sale de ruido coherente con semilla (`seed`), `frequency` fija
su rapidez y `rotation` el giro máximo.

`scene.camera.animate.zoom_to(z)` interpola el zoom de forma exponencial: el
área visible cambia en la misma proporción cada fotograma, así que un zoom de
×8 se percibe a velocidad constante en lugar de acelerar al final.
`interpolation="linear"` recupera la interpolación lineal. `frame_to` usa el
mismo modelo.

```python
from gaanim import BLUE, CYAN, GOLD, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
for i, r in enumerate([3.2, 1.6, 0.8, 0.4, 0.2, 0.1]):
    scene.geometry.circle(r).no_fill().stroke([CYAN, BLUE][i % 2], r * 0.06)
scene.geometry.dot(0.05).fill(GOLD)
scene.play(scene.camera.animate.zoom_to(6.0).duration(1.0))
scene.play(scene.camera.animate.shake(trauma=0.9, decay=1.8, rotation=0.03, seed=2))
scene.render()
# output: preview.webp
```

La duración por defecto de `shake` es `trauma / decay` segundos. Un
`shake(0.12, 8)` con solo amplitud y frecuencia conserva la sacudida
sinusoidal de versiones anteriores.

== Referencia

- #link("/referencia/animations/")[Animaciones]: `Transition` con todas sus
  fábricas y `Overlay`.
- #link("/referencia/scene/")[Scene]: `scene.segment`, `scene.link`,
  `scene.persist` y la cámara (`zoom_to`, `frame_to`, `shake`).
- #link("/guias/presentaciones/")[Presentaciones]: segmentos, paradas y
  navegación.
