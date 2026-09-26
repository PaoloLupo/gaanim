#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Layout",
  description: "Organiza títulos, columnas y tarjetas sin escribir coordenadas a mano",
  route: "/guias/layout/",
)

En esta guía aprenderás a componer páginas didácticas (un título, varias
tarjetas, un pie) que se colocan solas y se reorganizan cuando añades o
cambias contenido. No escribirás ni una coordenada.

```python
# output: preview.webp
from gaanim import CYAN, GOLD, WHITE, Color, Scene

scene = Scene(frame=(16, 9), background="#0f172a", margin=0.5)
CARD = Color(30, 41, 59)

def card(title, body, accent):
    return scene.layout.card(
        [
            scene.text(title, role="subtitle").fill(accent),
            scene.text(body, role="body", wrap=False).fill(WHITE),
        ],
        padding=0.4, gap=0.15, background=CARD,
    )

title = scene.text("Tres ideas", role="title").fill(WHITE)
cards = scene.layout.row([
    card("Medir", "Cada objeto\nconoce su tamaño", CYAN),
    card("Repartir", "El layout\nreparte el espacio", GOLD),
], gap=0.5)
footer = scene.text("Sin coordenadas manuales", role="caption").fill(WHITE)
page = scene.layout.column(
    [title, scene.layout.item(cards, grow=1), footer],
    within="safe", width="fill", height="fill", align="center", justify="between",
)

scene.play([page.animate.fade_in().duration(0.6)])
scene.wait(0.4)
extra = card("Adaptar", "Una tarjeta más\nreorganiza el resto", CYAN)
cards.add(extra)
scene.play([extra.animate.fade_in().duration(0.5)])
scene.wait(0.6)
scene.render()
```

= Cuándo usar Layout

Layout organiza contenido editorial: títulos, columnas, tarjetas, leyendas y
paneles. Describes un árbol de filas, columnas, rejillas y capas; Gaanim mide
cada objeto, reparte el espacio y coloca los hijos. El resultado se registra
en la línea de tiempo como cualquier otro cambio.

Para un movimiento geométrico deliberado (un punto que recorre una órbita, un
objeto que se desplaza) sigue usando transformaciones y coordenadas. Para las
relaciones de espacio entre bloques, usa Layout.

= Una página en columna

```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), margin=0.5)
>>>title = scene.text("Título", role="title")
>>>content = scene.text("Contenido principal", role="body")
>>>footer = scene.text("Pie de página", role="body")
page = scene.layout.column(
    [title, scene.layout.item(content, grow=1), footer],
    within="safe",
    width="fill",
    height="fill",
    padding=0.6,
    gap=0.4,
    align="stretch",
    justify="between",
)
```

- `within="safe"` usa el área segura, es decir, el fotograma menos el margen
  de la escena.
- `width` y `height` aceptan `"hug"` (el tamaño propio del contenido),
  `"fill"` (todo el espacio disponible) o un número en unidades de escena.
- `scene.layout.item(hijo, grow=1)` hace que un hijo crezca para ocupar el
  espacio que sobra.
- `align` coloca los hijos en el eje transversal y `justify` los reparte en
  el eje principal.

= Filas, rejillas y capas

- `scene.layout.row` reparte elementos en horizontal; con `wrap=True` pasan a
  la línea siguiente cuando no caben.
- `scene.layout.grid` organiza filas y columnas. `rows` y `columns` aceptan un
  número o una lista de pistas: un tamaño fijo, `"auto"` o fracciones como
  `"1fr"`.
- `scene.layout.stack` superpone sus hijos, por ejemplo un texto sobre una
  imagen.
- `scene.layout.card` es una columna, fila o capa con fondo, borde y relleno.

Los layouts se anidan: cada uno recibe el espacio que le ofrece su padre.

= Relaciones entre ramas

Dentro de un layout, él controla la posición de sus hijos. Para un ajuste
editorial pequeño usa `offset=` en `scene.layout.item`. Para relacionar
objetos de ramas distintas, declara restricciones con `scene.layout.constrain`:

```python
# continue
>>>chart = scene.geometry.rect(4, 2.5)
>>>label = scene.text("Etiqueta", role="body")
scene.layout.constrain(
    label.left == chart.right + 0.3,
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.3).weak(),
)
```

Si dos restricciones obligatorias son incompatibles, la escena falla antes de
renderizar. Las restricciones débiles (`.weak()`) se cumplen cuando es posible;
`scene.layout.check_layout()` y `page.diagnostics()` explican cuáles no se
cumplieron.

= Un mismo árbol para 16:9 y 9:16

Para producir la misma escena en horizontal y en vertical, conserva el árbol y
cambia solo el formato, por ejemplo con `scene.canvas.set_preset("vertical")`.
Usa `"fill"`, `grow`, pistas fraccionarias y `wrap=True` para que el
contenido se redistribuya solo. Reserva `absolute=True` para elementos
superpuestos y evita `move_to()` dentro del árbol.

= Cambios que reorganizan la página

Añadir, quitar o reemplazar hijos (`add`, `remove`, `replace`) vuelve a
calcular la disposición en el cursor actual. Un `Text` también se vuelve a
medir cuando cambian su contenido, fuente, tamaño, espaciado o ajuste de
línea. Las transiciones de estructura, como `become` o
`text.animate.transform_to(otro)`, reorganizan el layout padre con la misma
duración que la animación.

Consulta #link("/referencia/layout/")[la referencia de Layout] para todas las
firmas, las opciones de cada elemento, las restricciones y las plantillas.
