#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Layout",
  description: "scene.layout: cajas, filas, columnas, grids, capas, estilos, clases, zonas, animación de estructura y restricciones",
  route: "/referencia/layout/",
  nav: "Layout",
)

= Layout

`scene.layout` compone la escena con *cajas*, igual que CSS compone una página
con `div`s: cada caja mide su contenido, reparte el espacio entre sus hijos y
los coloca, con padding, gap, borde, radio, sombra y recorte. Las *zonas*
dividen el fotograma en regiones con nombre para colocar objetos que siguen
siendo libres. La guía #link("/guias/layout/")[Layout] enseña a usarlas paso a
paso; esta página es la referencia de cada pieza.

```python
# show-code: true
from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", background="#f1f5f9", margin=0.6)
L = scene.layout
L.classes(
    card=BoxStyle(padding="26px", gap="12px", radius="22px", background="white",
                  shadow={"color": "#0f172a22", "y": "-8px", "blur": "26px"}),
    tag=BoxStyle(padding=("4px", "12px"), radius="full", font_size="18px",
                 background="#e0e7ff", color="#3730a3"),
)
article = L.box(
    L.row(L.box("Layout", class_="tag"), L.box("CSS", class_="tag"), gap="8px"),
    L.box("Cajas que se colocan solas", font_size="40px", weight=700),
    L.box("Padding, gap, grids, estilos y zonas: la escena se compone sin coordenadas "
          "y se reorganiza al animar.", font_size="24px", color="#475569", line_spacing=1.4),
    class_="card", max_width="760px",
)
scene.play([article.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```

El modelo tiene tres capas, como en CSS:

+ *La caja* decide el flujo (`direction`), su tamaño, el espacio interior
  (`padding`, `gap`) y cómo reparte y alinea a sus hijos (`justify`, `align`).
+ *Cada hijo* declara con `.item(...)` cómo consume la caja que recibe:
  crecer, alinearse, ocupar celdas, márgenes, ajuste.
+ *El contenido* se mide dentro de esa caja: el texto recompone sus líneas y
  las imágenes se ajustan con `fit`.

== Unidades

Todas las propiedades aceptan las mismas unidades:

- *Números*: unidades de escena (el fotograma por defecto mide 16 × 9).
- *`"Npx"`*: píxeles de diseño. El fotograma mide `design_resolution`
  (1080 por defecto) píxeles de alto, así que `"24px"` se ve igual en
  cualquier resolución de salida: `Scene(design_resolution=1920)` cambia la
  referencia.
- *Tokens del tema*: cualquier otro nombre, como `"space_md"` o
  `"page_padding"`, se lee de `theme.layout`.
- *Tamaños* (`width`, `height`, `basis`): además `"N%"` del padre, `"hug"`
  (o `"auto"`, el tamaño del contenido) y `"fill"` (el espacio libre).
- *Pistas de grid*: además `"auto"` y fracciones `"Nfr"`.
- *Relleno y margen*: uno, dos, tres o cuatro valores, como el atajo de CSS.

Una unidad desconocida, un tamaño negativo o un valor no finito lanzan
`ValueError`.

== Cajas

#api-entry(
  name: "LayoutBuilder.box",
  kind: "factory",
  desc: [Crea una caja. Sus hijos son objetos, cajas o cadenas (que se convierten en `Text` con la tipografía de la caja); las listas se aplanan y `None` se ignora. La dirección por defecto es `"column"`; `direction` acepta `"row"`, `"grid"` y `"stack"`. Las propiedades se describen en la sección Propiedades. `style=` recibe un `BoxStyle` y `class_=` nombres de clases.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
pill = scene.layout.box("Nuevo", padding=("6px", "16px"), radius="full",
                        background="#4f46e5", color="white", font_size="24px")
```
]

#api-entry(
  name: "LayoutBuilder.row / column",
  kind: "factory",
  desc: [Atajos de `box(direction="row")` y `box(direction="column")`. En una fila el eje principal es horizontal y `align` actúa en vertical; en una columna, al revés. Con `wrap=True` los hijos que no caben pasan a la línea siguiente.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
tags = scene.layout.row(*[scene.layout.box(t, padding="6px", background="#e0e7ff") for t in
                          ("rust", "gpu", "vello", "taffy", "python")],
                        gap="8px", wrap=True, max_width="300px")
```
]

#api-entry(
  name: "LayoutBuilder.grid",
  kind: "factory",
  desc: [Caja de rejilla. `columns` y `rows` son un número de pistas o una lista de pistas (`"160px"`, `"25%"`, `"auto"`, `"1fr"`). `gap`, `row_gap` y `column_gap` separan las pistas; `auto_flow="column"` rellena por columnas. Los hijos se colocan en orden, o en la celda que pidan con `item(row=..., column=..., row_span=..., column_span=...)`, y se estiran por defecto (`align="stretch"`).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
>>>cell = BoxStyle(padding="12px", background="#dbeafe", radius="8px")
bento = scene.layout.grid(
    scene.layout.box("A", style=cell).item(row_span=2),
    scene.layout.box("B", style=cell).item(column_span=2),
    scene.layout.box("C", style=cell),
    scene.layout.box("D", style=cell),
    columns=["160px", "1fr", "2fr"], gap="12px", within="safe", width="fill",
)
```
]

#api-entry(
  name: "LayoutBuilder.stack",
  kind: "factory",
  desc: [Caja de capas: todos los hijos comparten la misma área y se superponen en orden. Cada hijo se coloca con `item(anchor=..., offset=...)` (centro por defecto) o se ajusta con `item(fit=...)`. Ideal para un texto sobre una imagen, una insignia en una esquina o un fondo.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
>>>photo = scene.geometry.rect(4, 2.5).fill("#94a3b8")
cover = scene.layout.stack(
    photo.item(fit="cover"),
    scene.layout.box("Portada", padding="8px", background="white").item(anchor="bottom_left",
                                                                         offset=("16px", "16px")),
    width="420px", height="260px", radius="16px", clip=True,
)
```
]

== Propiedades

Las mismas propiedades valen para `box`, `row`, `column`, `grid`, `stack`,
`BoxStyle` y `Box.set`. Un nombre desconocido lanza `TypeError`.

*Contenedor*

- `direction`: `"column"`, `"row"`, `"grid"` o `"stack"`.
- `width`, `height`, `min_width`, `max_width`, `min_height`, `max_height`,
  `aspect_ratio`.
- `padding`, `gap`, `row_gap`, `column_gap`.
- `align` (eje transversal): `start`, `center`, `end`, `stretch`,
  `baseline`. `justify` (eje principal): `start`, `center`, `end`, `between`,
  `around`, `evenly`.
- `wrap` (filas y columnas), `columns`, `rows`, `auto_flow` (grids).
- `within`: solo en la raíz; `"safe"` (el fotograma menos el margen, por
  defecto) o `"frame"` (a sangre).

*Hijo* (también en `Drawable.item`)

- `grow`, `shrink`, `basis`: reparto del eje principal. `width="fill"` en una
  fila equivale a `grow=1`.
- `align_self`: alineación propia en el eje transversal (en un grid, la
  vertical dentro de la celda).
- `margin`: longitudes o `"auto"`, que se queda con el espacio libre.
- `row`, `column`, `row_span`, `column_span`: celda en un grid.
- `fit`: `none`, `contain`, `cover` (recorta), `stretch`, `scale_down`.
- `anchor`, `offset`: posición en una capa o en su celda.
- `absolute`: fuera del flujo; no ocupa espacio.

*Decoración*

- `background`, `border`, `border_width`, `radius` (`"full"`: píldora),
  `shadow` (`True` o `{"color", "x", "y", "blur"}`), `clip`.

*Tipografía* (la heredan las cadenas hijas)

- `color`, `font`, `font_size`, `weight`, `italic`, `role`, `text_align`,
  `line_spacing`, `letter_spacing`, `max_lines`, `overflow`, `markup`,
  `text_box`. `markup` es `False` por defecto: `$` y `*` se escriben tal cual.
- `text_box` elige la caja con que se mide y coloca el texto, como
  `text-box` en CSS: `"line"` (por defecto) ocupa líneas completas, de la
  ascendente a la descendente, así que textos del mismo estilo comparten
  altura; `"cap"` va de la altura de las mayúsculas a la línea base (de un
  texto de una línea), para que las mayúsculas toquen el borde de la caja
  como en un diseño calcado; `"ink"` se ajusta a la tinta de los glifos. Las
  fórmulas `$…$` usan siempre su tinta.

== Estilos y clases

#api-entry(
  name: "BoxStyle",
  kind: "class",
  signature: "BoxStyle(**props)",
  desc: [Conjunto inmutable de propiedades, como una regla CSS. Se aplica con `style=`. La prioridad es: clases, después `style=` y al final las propiedades escritas en la llamada.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
pill = BoxStyle(padding=("4px", "12px"), radius="full", background="#4f46e5", color="white")
badge = scene.layout.box("v2", style=pill)
```
]

#api-entry(
  name: "BoxStyle.but",
  kind: "method",
  desc: [Devuelve una copia con algunas propiedades cambiadas. `to_dict()` devuelve las propiedades.],
)[
```python
>>>from gaanim import *
>>>pill = BoxStyle(padding=("4px", "12px"), radius="full", background="#4f46e5")
ghost = pill.but(background="#e0e7ff", color="#3730a3")
```
]

#api-entry(
  name: "LayoutBuilder.classes",
  kind: "method",
  desc: [Registra estilos con nombre para la escena. `class_="pill soft"` aplica varias en orden, y la última gana. Una clase no registrada lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
scene.layout.classes(card=BoxStyle(padding="20px", radius="16px", background="white"))
panel = scene.layout.box("Hola", class_="card")
```
]

== Reglas por hijo

#api-entry(
  name: "Drawable.item",
  kind: "method",
  desc: [Declara cómo se coloca un objeto dentro de la caja que lo contiene (las propiedades de *Hijo*) y devuelve el propio objeto, así que se usa en línea. `width`/`height` sustituyen al tamaño propio del objeto en el layout. Si el objeto ya está en una caja, esta se reorganiza, animada con `duration`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
>>>bar = scene.geometry.rect(1, 0.2).fill("#4f46e5")
row = scene.layout.row("Carga", bar.item(grow=1, height="12px"), "72%", gap="16px",
                       align="center", width="fill")
```
]

== Cajas vivas

Una `Box` es un `Drawable`: `move_to`, `scale_by`, `animate.fade_in()` o
`place` actúan sobre la caja entera. Decide la posición de reposo de sus
hijos, que siguen aceptando cualquier animación de `.animate`: parten de ese
sitio, no mueven a sus hermanos y conservan su desplazamiento cuando la caja
se reorganiza.
Los cambios de estructura se registran en la línea de tiempo; con
`duration=` se animan (los hijos se deslizan, lo que entra aparece con un
fundido y lo que sale se desvanece) y avanzan el cursor como `scene.play`.
Con `advance=False` el siguiente cambio empieza a la vez.

#api-entry(
  name: "Box.add / remove / replace",
  kind: "method",
  desc: [`add(hijo, at=None)` inserta un hijo (o una cadena) y lo devuelve; `remove(hijo)` lo quita con un fundido; `replace(viejo, nuevo)` pone `nuevo` en su lugar, heredando sus reglas de `item` si no trae las suyas. Un índice fuera de rango lanza `IndexError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
>>>chip = BoxStyle(padding="12px", background="#e0e7ff", radius="10px")
>>>board = scene.layout.row(*[scene.layout.box(t, style=chip) for t in "ABC"], gap="12px")
board.add(scene.layout.box("Nueva", style=chip), at=1, duration=0.6)
board.remove(board[3], duration=0.6)
board.replace(board[0], scene.layout.box("Otra", style=chip), duration=0.6)
```
]

#api-entry(
  name: "Box.set",
  kind: "method",
  desc: [Cambia cualquier propiedad de la caja (las mismas que al crearla, también `style=`) y la reorganiza. Los cambios de decoración y de tipografía se aplican también.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
>>>grid = scene.layout.grid(*[scene.layout.box(str(n)) for n in range(6)], columns=3)
grid.set(columns=2, gap="30px", duration=0.8)
```
]

#api-entry(
  name: "Box.detach",
  kind: "method",
  desc: [Saca a un hijo de la caja sin ocultarlo: conserva su posición y vuelve a aceptar `move_to` y las animaciones de posición.],
  none,
)

#api-entry(
  name: "Box.children / background",
  kind: "property",
  desc: [`children` es la lista de hijos directos (también `len(box)`, `box[i]` e iteración); `background` es el objeto que dibuja fondo, borde y radio, o `None`.],
  none,
)

#api-entry(
  name: "Box.reflow",
  kind: "method",
  desc: [Vuelve a calcular la disposición cuando un hijo cambió de tamaño por su cuenta (por ejemplo, al escalarlo). Los cambios de texto y de estructura ya reorganizan solos.],
  none,
)

#api-entry(
  name: "LayoutOwnershipError",
  kind: "class",
  desc: [Se lanza al colocar de forma inmediata (`move_to`, `shift_by`, `next_to`…, sin `.animate`) un hijo de una caja o al meter en una caja un objeto que ya pertenece a otra. Usa `item(offset=...)`, anima al hijo con `.animate`, mueve la caja entera o `detach` al hijo.],
  none,
)

== Zonas

Las zonas dividen una región en rectángulos con nombre. No adoptan objetos:
colocan una vez (o animan) objetos que siguen siendo libres, así que una
misma plantilla sirve para el fotograma, para una zona o para el interior de
un objeto.

#api-entry(
  name: "Zones.rows / columns / grid",
  kind: "factory",
  desc: [Plantillas reutilizables. `rows(pistas)` y `columns(pistas)` cortan en una dirección; `grid(rows=..., columns=...)` en las dos. Aceptan `names=`, `gap=` (y `row_gap`, `column_gap` en `grid`) y `padding=`. Las pistas son las de los grids.],
)[
```python
>>>from gaanim import *
page = Zones.rows(["90px", "1fr", "60px"], names=["cabecera", "cuerpo", "pie"], gap="16px")
split = Zones.columns(["1fr", "2fr"], names=["lateral", "principal"], gap="24px")
```
]

#api-entry(
  name: "LayoutBuilder.zones",
  kind: "method",
  desc: [Aplica una plantilla y devuelve un `ZoneSet`. Por defecto usa el área segura, que respeta el `margin` de la escena; `within` acepta `"frame"`, una `Zone` o un objeto (su caja actual). `scene.layout.safe` y `scene.layout.frame` son zonas ya hechas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper", margin=0.5)
>>>page = Zones.rows(["90px", "1fr", "60px"], names=["cabecera", "cuerpo", "pie"])
z = scene.layout.zones(page)
title = scene.text("Título", size=0.5).place(z["cabecera"], anchor="left")
```
]

#api-entry(
  name: "ZoneSet.fill",
  kind: "method",
  desc: [Un `ZoneSet` se indexa por nombre o posición, se recorre y tiene `names`. `fill(*objetos, anchor=..., fit=..., padding=...)` coloca un objeto en cada zona, en orden. Un nombre desconocido lanza `KeyError`.],
  none,
)

#api-entry(
  name: "Zone.point / inset / split / arrange",
  kind: "method",
  desc: [Una `Zone` expone `left`, `right`, `top`, `bottom`, `width`, `height` y `center` en unidades de escena. `point(anchor)` da uno de sus puntos; `inset(padding)` la encoge; `split(plantilla)` aplica otra plantilla dentro; `arrange(*objetos, direction="row", gap=..., align=..., justify=...)` alinea objetos libres sin adoptarlos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
dots = [scene.geometry.circle(0.3) for _ in range(4)]
scene.layout.safe.inset("80px").arrange(*dots, gap="24px")
```
]

#api-entry(
  name: "Drawable.place",
  kind: "method",
  desc: [Coloca el objeto en una zona o sobre la caja de otro objeto: su `anchor` (centro por defecto) coincide con el mismo ancla del destino, reducido por `padding` y desplazado por `offset`. `fit` lo escala al destino. `obj.animate.place(...)` hace el mismo movimiento como animación.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9), theme="paper")
>>>zone = scene.layout.safe
logo = scene.geometry.circle(1).place(zone, anchor="top_right", fit="contain", padding="20px")
scene.play([logo.animate.place(zone, anchor="bottom_left").duration(0.8)])
```
]

== Componentes

`component` convierte una función `(scene, *, huecos...) -> Box` en un
componente reutilizable: comprueba sus argumentos con la firma (un hueco que
falta o sobra lanza `TypeError` con el nombre del componente), expone
`slots` y conserva los tipos para el editor. Cada llamada crea objetos nuevos.
Las plantillas de presentación `title_slide`, `lecture`, `comparison`,
`vertical_short`, `minimal`, `lower_third` y `credits` son componentes y se
usan también con `scene.segment(..., template=...)` y `Segment.bind` (ver
#link("/referencia/scene/")[Escena]).

```python
from gaanim import Scene, component

scene = Scene(frame=(16, 9), theme="paper")

@component
def tag(scene, *, text: str, color: str = "#4f46e5"):
    return scene.layout.box(text, padding=("4px", "12px"), radius="full",
                            background=color, color="white")

row = scene.layout.row(tag(scene, text="Nuevo"), tag(scene, text="Beta", color="#0891b2"), gap="8px")
```

Las plantillas incluidas usan tokens de espaciado del tema. Léelos con
`scene.canvas.layout_token(name)` y cámbialos con
`Theme(..., layout={"page_padding": 0.7, "column_gap": 0.6})`.

== Restricciones lineales

Relaciones entre la geometría de ramas distintas del árbol, como alinear una
etiqueta externa con el centro de un gráfico. Cada objeto expone las
expresiones `left`, `right`, `top`, `bottom`, `center_x`, `center_y`, `width`
y `height`. Complementan a las cajas: para una cuadrícula, usa `grid`.

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
  desc: [Registra restricciones y devuelve un `ConstraintSet` (su atributo `count` es el número registrado). Los conflictos entre restricciones obligatorias y las referencias entre escenas lanzan `ValueError` al instante.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>chart = scene.geometry.rect(5, 3)
>>>page = scene.layout.column(chart, within="safe")
>>>label = scene.text("Máximo")
relations = scene.layout.constrain(
    (label.left == chart.right + 0.3).strong(),
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.30).weak().named("etiqueta estrecha"),
)
```
]

#api-entry(
  name: "LayoutBuilder.check_layout",
  kind: "method",
  desc: [Resuelve el layout de la escena hasta ese punto y devuelve sus diagnósticos: restricciones débiles incumplidas, fallos de composición de texto (que no detienen la recarga del editor) y cajas con fondo o borde que quedan con ancho o alto cero, como una barra vacía sin `width="fill"`. Cada aviso empieza por la ruta de la caja, por ejemplo `column[2] > row[1]`: el segundo hijo de la fila que es el tercer hijo de la columna raíz. `box.diagnostics()` filtra los de una caja y sus cajas anidadas, y `gaanim check` los muestra como avisos.],
)[
```python
# continue
for message in scene.layout.check_layout():
    print(message)
```
]

== Errores frecuentes

- *Mover un hijo con `move_to()`.* Expresa la intención con `align`,
  `justify`, `margin`, `anchor` u `offset`, o usa una zona si el objeto debe
  ser libre.
- *Poner `"fill"` en todos los niveles.* Decide qué caja es dueña del espacio
  y deja que el resto use su tamaño natural.
- *Confundir `align` con `justify`.* Identifica primero el eje principal: en
  una fila es horizontal y en una columna, vertical.
- *Medidas en unidades de escena para UI.* `"24px"` y los tokens del tema
  mantienen la proporción en cualquier formato; los números sirven para
  geometría con significado.
- *Construir una cuadrícula con restricciones.* Usa `grid` y reserva las
  restricciones para relaciones entre ramas.
