#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Layout",
  description: "scene.layout: filas, columnas, grids, capas, tarjetas, reglas por hijo, reflow, restricciones y plantillas",
  route: "/referencia/layout/",
  nav: "Layout",
)

= Layout

`scene.layout` compone objetos sin coordenadas manuales: filas, columnas, grids
y capas que miden su contenido, reparten el espacio y se recolocan cuando algo
cambia. Un `Layout` también es un #link("/referencia/drawable/")[`Drawable`] y
es dueño de la traslación de sus hijos directos. La guía
#link("/guias/layout/")[Layout] explica cómo pensar un diseño; esta página es la
referencia de cada pieza.

```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene, TextFlow

scene = Scene(frame=(16, 9), background="#0b1020", margin=0.6)
copy = scene.text(
    "El mismo árbol puede componer una diapositiva, un panel o un video vertical.",
    role="body", color=WHITE, flow=TextFlow(wrap="auto", line_spacing=1.2),
)
card = scene.layout.card(
    [scene.text("Contenido medido", role="heading", color=GOLD), copy],
    background=BLUE, radius=0.2, padding=0.35, gap=0.225, width="fill", align="stretch",
)
body = scene.layout.row(
    [
        scene.layout.item(card, grow=2),
        scene.layout.item(scene.geometry.circle(1.2).fill(GOLD), grow=1, align="center"),
    ],
    width="fill", gap=0.5, align="center",
)
page = scene.layout.column(
    [
        scene.text("Layout sin coordenadas", role="title", color=GOLD),
        scene.layout.item(body, grow=1, align="stretch"),
        scene.text("Ningún hijo llama a move_to", role="caption", color=WHITE),
    ],
    within="safe", width="fill", height="fill", padding=(0.3, 0.5), gap=0.4,
    align="stretch", justify="between",
)
scene.play([page.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```

El modelo tiene tres niveles:

+ El contenedor decide el flujo, el tamaño exterior, el espacio y la alineación.
+ `scene.layout.item(...)` explica cómo un hijo consume la caja que recibe.
+ El contenido se mide dentro de esa caja: el texto recompone sus líneas y los
  medios ajustan su geometría.

Una vez que un objeto pertenece a un Layout, el árbol decide su posición:
`move_to()` o `shift_by()` sobre un hijo lanzan `LayoutOwnershipError`.

== Contenedores

Las fábricas que crean un `Layout`. Comparten estas opciones:

- `width` y `height`: un número fijo en unidades de escena, `"hug"` (el tamaño
  del contenido) o `"fill"` (el espacio disponible).
- `padding`: un valor, `(vertical, horizontal)` o `(arriba, derecha, abajo, izquierda)`.
- `align` (eje transversal): `start`, `center`, `end` o `stretch`. `justify`
  (eje principal): además `between`, `around` y `evenly`.
- `within`: `None` crea un contenedor anidado; las raíces suelen usar `"safe"`
  (respeta los márgenes) o `"frame"` (a sangre completa).

Medidas, rellenos, pistas o alineaciones inválidos lanzan `ValueError`.

#api-entry(
  name: "LayoutBuilder.row",
  kind: "factory",
  desc: [Fila horizontal. El eje principal avanza de izquierda a derecha y `align` actúa en vertical. Con `wrap=True`, abre una fila nueva cuando el siguiente hijo no cabe. El texto adaptable se compone con el ancho que le asigna la fila.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>tags = [scene.text(t, role="label") for t in ("rust", "gpu", "vello", "bevy", "python")]
chips = scene.layout.row(tags, width=7.75, gap=0.15, wrap=True, align="center")
```
]

#api-entry(
  name: "LayoutBuilder.column",
  kind: "factory",
  desc: [Columna vertical. El eje principal avanza de arriba abajo y `align` actúa en horizontal. Con `wrap=True`, abre una columna nueva.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
steps = scene.layout.column(
    [scene.text("Introducción"), scene.text("Explicación"), scene.text("Resultado")],
    height="fill", align="stretch", justify="evenly", within="safe",
)
```
]

#api-entry(
  name: "LayoutBuilder.grid",
  kind: "factory",
  params: ((name: "rows / columns", type: "int | Sequence[Track]", default: "1", desc: [Número de pistas o lista de pistas: fijas (número), `"auto"` (intrínsecas) o fraccionarias (`"2fr"`).]), (name: "gap / row_gap / column_gap", type: "float", default: "0 / None / None", desc: [Separación común o por eje.]), (name: "auto_flow", type: "str", default: "\"row\"", desc: [Colocación automática por filas o por columnas.])),
  desc: [Rejilla de pistas. Las celdas explícitas y sus _spans_ (vía `item`) se reservan antes de colocar automáticamente al resto, sea cual sea el orden de los hijos. Una colisión, un _span_ fuera de rango o un grid sin celdas libres detienen la resolución e indican el nodo implicado. Prefiérelo a filas anidadas cuando varias regiones deben compartir líneas de alineación.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>hero = scene.geometry.rect(3, 2)
>>>chart = scene.geometry.rect(6, 2)
>>>notes = scene.text("Notas")
cards = scene.layout.grid(
    [hero, scene.layout.item(chart, column_span=2), notes],
    columns=[3, "1fr", "2fr"],
    rows=["auto", "1fr"],
    gap=0.3,
    width="fill",
    within="safe",
)
```
]

#api-entry(
  name: "LayoutBuilder.stack",
  kind: "factory",
  desc: [Capas superpuestas en una caja compartida, para fondos, medios, pies y etiquetas. Coloca cada hijo con `item(anchor=..., offset=...)`; los hijos `absolute` no consumen espacio.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>photo = scene.media.image("foto.jpg")
>>>caption = scene.text("Pie de foto", role="caption")
overlay = scene.layout.stack([
    scene.layout.item(photo, fit="cover"),
    scene.layout.item(caption, anchor=Anchor.BOTTOM_LEFT, offset=(0.3, 0.3)),
], within="frame", width="fill", height="fill")
```
]

#api-entry(
  name: "LayoutBuilder.card",
  kind: "factory",
  params: ((name: "children", type: "Sequence[Drawable | Layout | LayoutItem]", default: none, desc: [Contenido libre.]), (name: "direction", type: "str", default: "\"column\"", desc: [`column`, `row` o `stack`.]), (name: "background / border", type: "Paint | None", default: "None", desc: [Fondo y borde; transparentes por defecto.]), (name: "border_width / radius", type: "float", default: "0.025 / 0.08", desc: [El radio se limita a la mitad de la dimensión menor.]), (name: "ports", type: "dict[str, Anchor | (Anchor, (dx, dy))] | None", default: "None", desc: [Puertos con nombre para conectores.])),
  desc: [Contenedor con fondo redondeado que sigue la caja exterior, relleno incluido, durante el reflow, las transformaciones y los seeks; no cuenta en la medición ni en `count`. A diferencia de `scene.slides.card`, no impone título ni estilos de texto. Radio o borde negativos, nombres o desplazamientos inválidos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
card = scene.layout.card(
    [scene.text("Modelo"), scene.text("Resultados")],
    padding=0.3, gap=0.2, background="#f5f5f5", border="#626878",
    ports={"entrada": Anchor.LEFT, "salida": (Anchor.RIGHT, (0.1, 0))},
)
link = scene.geometry.connector(card.port("salida"), (5, 0))
card.background.fill("#e0f2fe")
```
]

== Reglas por hijo

Cómo consume cada hijo la caja que recibe, sin crear otro objeto.

#api-entry(
  name: "LayoutBuilder.item",
  kind: "factory",
  params: (
    (name: "grow / shrink", type: "float", default: "0.0 / 1.0", desc: [Parte del espacio sobrante que toma el hijo y permiso relativo para encogerse; `shrink=0` protege su tamaño preferido.]),
    (name: "align", type: "str | None", default: "None", desc: [Sustituye la alineación transversal del contenedor.]),
    (name: "row / column", type: "int | None", default: "None", desc: [Celda explícita del grid, contando desde cero.]),
    (name: "row_span / column_span", type: "int", default: "1", desc: [Pistas que ocupa en el grid.]),
    (name: "absolute", type: "bool", default: "False", desc: [Saca al hijo del flujo y lo coloca respecto a la caja contenedora.]),
    (name: "anchor / offset", type: "Anchor | None / (float, float)", default: "None / (0, 0)", desc: [Punto de la celda o capa donde se coloca y desplazamiento editorial intencionado.]),
    (name: "fit", type: "str", default: "\"none\"", desc: [`none`, `contain`, `cover` (también recorta), `stretch` o `scale_down`.]),
  ),
  desc: [Devuelve un `LayoutItem`: reglas inmutables para un hijo. Valores negativos de `grow` o `shrink` lanzan un error. Empieza con `"hug"`, usa `"fill"` solo en el eje que deba consumir espacio y `grow` para repartir el sobrante entre hermanos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>sidebar = scene.geometry.rect(2.5, 4)
>>>content = scene.geometry.rect(6, 4)
>>>inspector = scene.geometry.rect(3, 4)
workspace = scene.layout.row(
    [
        scene.layout.item(sidebar, grow=0, shrink=0),
        scene.layout.item(content, grow=3, align="stretch"),
        scene.layout.item(inspector, grow=1),
    ],
    width="fill", height="fill", padding=(0.3, 0.5), gap=0.4, align="stretch", within="safe",
)
```
]

#api-entry(
  name: "LayoutItem",
  kind: "class",
  signature: "scene.layout.item(child, ...) -> LayoutItem",
  desc: [Reglas inmutables de un hijo creadas con `scene.layout.item(...)`. Se pasan en la lista de hijos en lugar del objeto; el objeto sigue siendo el `Drawable` que se anima.],
  none,
)

== Estructura viva

Cambia hijos y reglas después de crear el árbol. Cada cambio registra en el
cursor un reflow instantáneo y determinista; un seek directo y una reproducción
secuencial resuelven la misma geometría.

#api-entry(
  name: "Layout.add",
  kind: "method",
  desc: [Inserta un hijo directo (en la posición `at` o al final) y lo devuelve. Un índice inválido lanza `IndexError`; un hijo colocado a mano, de otra escena o ya gestionado, `LayoutOwnershipError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
page = scene.layout.column([scene.text("Título", role="title"), scene.text("Antes")], within="safe")
scene.wait(0.5)
page.add(scene.text("Extra"), at=1)
```
]

#api-entry(
  name: "Layout.remove",
  kind: "method",
  desc: [Quita un hijo directo con una salida visual y libera su posición.],
  none,
)

#api-entry(
  name: "Layout.detach",
  kind: "method",
  desc: [Libera un hijo sin ocultarlo: conserva su posición en el mundo, su opacidad y su pertenencia a la escena, así que `move_to` y el resto de métodos de posición valen justo después. Los demás hijos se recolocan al instante. Un objeto que no es hijo lanza `ValueError`. Útil para llevar un hijo a un segmento nuevo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>scene.segment("intro")
>>>title = scene.text("Título", role="title")
>>>page = scene.layout.column([title, scene.text("Contenido")], within="safe")
>>>scene.wait(1)
scene.segment("detail", Transition.cross_fade(0.4))
scene.reuse(title)
page.detach(title)
scene.play([title.animate.move_to(0, 2.5).duration(0.35)])
```
]

#api-entry(
  name: "Layout.replace",
  kind: "method",
  desc: [Sustituye un hijo directo por otro (o por un `LayoutItem`) y devuelve el nuevo.],
  none,
)

#api-entry(
  name: "Layout.configure",
  kind: "method",
  desc: [Cambia las reglas del contenedor (`gap`, `padding`, tamaños, `min_*`/`max_*`, `aspect_ratio`, alineaciones, `wrap`, `within`) y registra un reflow. `aspect_ratio` debe ser positivo y `wrap` solo vale en filas y columnas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>page = scene.layout.column([scene.text("A"), scene.text("B")], within="safe")
page.configure(gap=0.5, padding=0.7)
page.configure(min_width=6, max_width=12, aspect_ratio=16 / 9)
```
]

#api-entry(
  name: "Layout.configure_item",
  kind: "method",
  desc: [Cambia las reglas de un hijo directo (las mismas de `item`) y aplica el reflow.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>chart = scene.geometry.rect(4, 2.5)
>>>page = scene.layout.column([scene.text("Ventas", role="title"), chart], within="safe")
page.configure_item(chart, grow=2, align="stretch", offset=(0.15, 0), fit="contain")
```
]

#api-entry(
  name: "Layout.reflow",
  kind: "method",
  desc: [Resuelve en el cursor los cambios de geometría externos al árbol.],
  none,
)

#api-entry(
  name: "Layout.count / background / animate",
  kind: "property",
  signature: "count: int · background: Drawable | None · animate",
  desc: [Número de hijos directos; en una `card`, el fondo como `Drawable` para darle estilo (en otros contenedores, `None`); y el proxy de animación de la raíz, que conserva el tipo `Layout`.],
  none,
)

#api-entry(
  name: "Layout.move_to / shift_by",
  kind: "method",
  signature: "move_to(x, y=None, anchor=None) · shift_by(dx, dy) -> Layout",
  desc: [La raíz sí se posiciona: `move_to`, `next_to`, `align_to`, `to_edge`, `to_corner`, rotación y escala se reaplican sobre la caja final después de cada reflow y transforman todo el árbol. Ambos devuelven el mismo `Layout`, así que se encadenan con `add`, `configure` o `port`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>formula = scene.text("$a^2 + b^2 = c^2$")
>>>explanation = scene.text("Teorema de Pitágoras")
panel = scene.layout.column([formula, explanation], gap=0.625, align="center")
panel.move_to(5, 2.5)
```
]

#api-entry(
  name: "LayoutOwnershipError",
  kind: "class",
  signature: "class LayoutOwnershipError(Exception)",
  desc: [Se lanza al posicionar a mano un hijo gestionado (`move_to`, `next_to`, `align_to`, `to_edge`, animaciones de posición) o al mezclar propietarios. La rotación y la escala de los hijos siguen siendo válidas; expresa un desplazamiento intencionado con `offset`. Protege el seek: si Layout y una animación escribieran la misma posición, el resultado dependería del orden de evaluación.],
  none,
)

== Restricciones lineales

Relaciones entre la geometría de ramas distintas del árbol, como alinear una
etiqueta externa con el centro de un gráfico.

Cada objeto expone las expresiones `left`, `right`, `top`, `bottom`,
`center_x`, `center_y`, `width` y `height`. Complementan el flujo: para una
cuadrícula completa, usa `grid`.

#api-entry(
  name: "LayoutExpression / Drawable.left / right / top / bottom / center_x / center_y / width / height",
  kind: "property",
  signature: "drawable.left | right | top | bottom | center_x | center_y | width | height",
  desc: [Expresión lineal de geometría: admite suma y resta con otras expresiones o números y multiplicación o división por escalares finitos. `==`, `<=` y `>=` crean un `LayoutConstraint`. Mezclar objetos de escenas distintas lanza `ValueError`.],
  none,
)

#api-entry(
  name: "LayoutConstraint.strong / medium / weak / named",
  kind: "method",
  signature: "strong() · medium() · weak() · named(label) -> LayoutConstraint",
  desc: [Las restricciones son obligatorias por defecto; estos métodos devuelven una copia con prioridad fuerte, media o débil, o con una etiqueta para los diagnósticos. Las débiles incumplidas aparecen en `check_layout`.],
  none,
)

#api-entry(
  name: "LayoutBuilder.constrain",
  kind: "method",
  desc: [Registra restricciones y devuelve un `ConstraintSet` (su atributo `count` es el número registrado). Los conflictos entre restricciones obligatorias y las referencias entre escenas lanzan `ValueError` al instante. Los IDs estables y el orden canónico hacen reproducibles las soluciones equivalentes.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>chart = scene.geometry.rect(5, 3)
>>>page = scene.layout.column([chart], within="safe")
>>>label = scene.text("Máximo")
relations = scene.layout.constrain(
    (label.left == chart.right + 0.3).strong(),
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.30).weak().named("etiqueta estrecha"),
)
```
]

#api-entry(
  name: "ConstraintSet.count",
  kind: "property",
  signature: "count: int",
  desc: [Resultado de `constrain`; `count` indica cuántas restricciones se registraron.],
  none,
)

#api-entry(
  name: "LayoutBuilder.check_layout",
  kind: "method",
  desc: [Lista de diagnósticos: restricciones débiles incumplidas y fallos de composición intrínseca, como texto Typst adaptable inválido (que no detiene la recarga del editor). Cada mensaje incluye la etiqueta o el índice de la restricción y los IDs implicados.],
)[
```python
# continue
for message in scene.layout.check_layout():
    print(message)
```
]

#api-entry(
  name: "Layout.diagnostics",
  kind: "method",
  desc: [Como `check_layout`, filtrado a los diagnósticos de esta raíz.],
  none,
)

== Plantillas

Funciones de Python tipadas que construyen un árbol a partir de huecos con nombre.

#api-entry(
  name: "LayoutBuilder.template",
  kind: "method",
  desc: [Instancia una plantilla comprobando sus huecos y devuelve su `Layout` raíz. Decora tus funciones con `layout_template`; un hueco que falta o que no existe lanza `TypeError`. Gaanim incluye `title_slide`, `lecture`, `comparison`, `vertical_short`, `minimal`, `lower_third` y `credits`, que también se usan con `scene.segment(..., template=...)` y `Segment.bind` (ver #link("/referencia/scene/")[Escena]).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>diagram = scene.geometry.circle(1.5)
from gaanim import layout_template

@layout_template
def two_columns(scene, *, title, left, right):
    return scene.layout.column(
        [title, scene.layout.row([scene.layout.item(left, grow=1), scene.layout.item(right, grow=1)])],
        within="safe", width="fill", height="fill",
    )

page = scene.layout.template(
    two_columns,
    title=scene.text("Comparación", role="title"),
    left=scene.text("Texto", flow=TextFlow(wrap="auto")),
    right=diagram,
)
```
]

Las plantillas incluidas usan tokens de espaciado del tema en lugar de medidas
sueltas. Léelos con `scene.canvas.layout_token(name)` y sustitúyelos con
`Theme(..., layout={"page_padding": 0.7, "column_gap": 0.6})`.

== Errores frecuentes

- *Usar `move_to()` en un hijo.* Expresa la intención con `align`, `anchor`,
  `offset` o una restricción.
- *Poner `"fill"` en todos los niveles.* Decide qué contenedor es dueño del
  espacio y deja que los descendientes usen `"hug"`.
- *Confundir `align` con `justify`.* Identifica primero el eje principal: en una
  fila es horizontal y en una columna, vertical.
- *Construir una cuadrícula con restricciones.* Usa `grid` y reserva las
  restricciones para relaciones entre ramas.
- *Crear un árbol por resolución.* Prueba antes pistas fraccionarias, `wrap`,
  `grow` y texto adaptable; el mismo árbol sirve para 16:9 y 9:16.
