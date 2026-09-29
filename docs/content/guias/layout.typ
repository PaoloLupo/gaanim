#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Layout",
  description: "Diseña como en CSS: cajas, filas, grids, estilos, zonas y animación, sin una sola coordenada",
  route: "/guias/layout/",
)

Si sabes armar una página web, ya sabes componer en Gaanim. `scene.layout`
te da *cajas* que funcionan como un `div` de CSS: tienen padding, gap, borde,
radio y sombra; ponen a sus hijos en fila, en columna, en una rejilla o en
capas; miden el texto y reparten el espacio. Tú describes la estructura y
Gaanim coloca cada cosa, en cualquier formato y durante toda la animación.

```python
# output: preview.webp
from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", background="#eef2ff", margin=0.5)
L = scene.layout
L.classes(
    card=BoxStyle(padding="28px", gap="14px", radius="24px", background="white",
                  shadow={"color": "#1e1b4b26", "y": "-10px", "blur": "30px"}),
    pill=BoxStyle(direction="row", padding=("6px", "16px"), radius="full", font_size="20px"),
)

def stat(label, value, delta, color):
    return L.box(
        L.box(label, font_size="20px", color="#64748b"),
        L.row(L.box(value, font_size="52px", weight=700),
              L.box(delta, class_="pill", background=color + "22", color=color),
              gap="12px", align="center"),
        class_="card", grow=1,
    )

cards = L.row(
    stat("Visitas", "48.2k", "+12%", "#16a34a"),
    stat("Registros", "1 284", "+4%", "#16a34a"),
    stat("Rebote", "31%", "-2%", "#dc2626"),
    gap="24px", width="fill",
)
page = L.column(
    L.box("Panel semanal", font_size="64px", weight=700, color="#1e1b4b"),
    cards,
    gap="36px", within="safe", width="fill", height="fill", justify="center",
)
scene.play([page.animate.fade_in().duration(0.6)])
cards.add(stat("Ventas", "$9.1k", "+30%", "#4f46e5"), duration=0.7)
scene.wait(0.6)
scene.render()
```

Nada de lo anterior usa coordenadas: la tarjeta nueva entra y las demás se
hacen sitio solas.

= Tu primera caja

`scene.layout.box(...)` crea una caja. Sus hijos pueden ser objetos de la
escena, otras cajas o simplemente cadenas de texto, que se convierten en
`Text` con la tipografía de la caja.

```python
# output: preview.webp
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout

tag = L.box("Nuevo", padding=("8px", "20px"), radius="full",
            background="#4f46e5", color="white", font_size="32px")

scene.play([tag.animate.fade_in().duration(0.5)])
scene.render()
```

Una caja sin `within` se coloca donde la ponga su padre; si es la raíz, se
centra en el área segura. Las propiedades usan las mismas ideas que CSS:

- *Longitudes*: un número (unidades de escena), `"24px"` (píxeles de diseño:
  el fotograma mide 1080 px de alto, sea cual sea la resolución de salida) o
  un token del tema como `"space_md"`.
- *Tamaños* (`width`, `height`): una longitud, `"50%"` del padre, `"hug"`
  (lo que mide el contenido) o `"fill"` (todo el espacio libre).
- *Relleno y margen*: como en CSS, uno, dos, tres o cuatro valores:
  `padding=("8px", "20px")` es 8 px arriba y abajo, 20 px a los lados.

= Filas y columnas

`L.row` y `L.column` son cajas con dirección. `gap` separa a los hijos,
`justify` los reparte en el eje principal y `align` los alinea en el otro.

```python
# output: preview.webp
from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.6)
L = scene.layout
chip = BoxStyle(padding=("10px", "18px"), radius="10px", background="#e0e7ff", font_size="26px")

def lane(justify):
    return L.row(L.box("A", style=chip), L.box("B", style=chip), L.box("C", style=chip),
                 justify=justify, width="fill", padding="8px", radius="14px", background="#f1f5f9")

page = L.column(
    *[L.row(L.box(name, font_size="22px", color="#64748b", width="170px"), lane(name).item(grow=1),
            align="center", width="fill")
      for name in ("start", "center", "end", "between", "evenly")],
    gap="14px", within="safe", width="fill",
)
scene.play([page.animate.fade_in().duration(0.5)])
scene.render()
```

- `justify`: `start`, `center`, `end`, `between`, `around`, `evenly`.
- `align`: `start`, `center`, `end`, `stretch` (estira a todos los hijos),
  `baseline`.
- `wrap=True` pasa los hijos a la línea siguiente cuando no caben, ideal para
  etiquetas.

= El modelo de caja

Una caja dibuja su propio fondo: `background`, `border`, `border_width`,
`radius` (`"full"` para una píldora), `shadow` y `clip` para recortar lo que
se salga de sus bordes redondeados.

```python
# output: preview.webp
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.6)
L = scene.layout

plain = L.box("Borde", padding="28px", radius="18px", border="#94a3b8", border_width="2px")
lifted = L.box("Sombra", padding="28px", radius="18px", background="white",
               shadow={"color": "#0f172a33", "y": "-8px", "blur": "24px"})
clipped = L.stack(scene.geometry.circle(1.5).fill("#f97316").item(anchor="top"),
                  width="220px", height="110px", radius="18px", clip=True, background="#fff7ed")
page = L.row(plain, lifted, clipped, gap="48px", align="center", within="safe",
             font_size="30px")
scene.play([page.animate.fade_in().duration(0.5)])
scene.render()
```

La tipografía también se hereda: `font_size`, `color`, `weight`, `italic`,
`font`, `line_spacing`, `letter_spacing`… definidas en una caja se aplican a
todas las cadenas de texto que contiene, como el `font-size` de CSS.

== La caja del texto: `text_box`

Por defecto un texto ocupa líneas completas, de la ascendente a la
descendente: dos textos del mismo estilo miden lo mismo aunque uno tenga
«g» o «Á». Para calcar un diseño en el que las mayúsculas tocan el borde de
la caja, usa `text_box="cap"`: el texto va de la altura de las mayúsculas a
su línea base. `text_box="ink"` se ajusta a la tinta de los glifos.

```python
# output: preview.webp
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.6)
L = scene.layout

def sample(mode):
    return L.column(
        L.box(f'text_box="{mode}"', font_size="24px", color="#64748b"),
        L.box("Título", font_size="96px", weight=700, text_box=mode,
              background="#e0e7ff", radius="8px"),
        gap="12px", align="center",
    )

page = L.row(sample("line"), sample("cap"), sample("ink"), gap="64px",
             align="end", within="safe")
scene.play([page.animate.fade_in().duration(0.5)])
scene.render()
```

= Estilos reutilizables: `BoxStyle` y clases

Un `BoxStyle` agrupa propiedades, como una regla CSS. Pásalo con `style=`,
derívalo con `.but(...)`, o regístralo por nombre y úsalo con `class_=`.

```python
# output: preview.webp
from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout
L.classes(
    pill=BoxStyle(direction="row", padding=("6px", "16px"), radius="full", font_size="24px",
                  background="#4f46e5", color="white"),
    soft=BoxStyle(background="#e0e7ff", color="#3730a3"),
)
warning = BoxStyle(background="#fef3c7", color="#92400e")

row = L.row(
    L.box("Diseño", class_="pill"),
    L.box("Datos", class_="pill soft"),
    L.box("Revisión", class_="pill", style=warning),
    L.box("Urgente", class_="pill", background="#dc2626"),
    gap="12px",
)
scene.play([row.animate.fade_in().duration(0.5)])
scene.render()
```

El orden de prioridad es el de CSS: clases, después `style=` y al final las
propiedades escritas en la llamada. Varias clases se separan con espacios y
la última gana.

= Grids

`L.grid` coloca a sus hijos en celdas. `columns` y `rows` aceptan un número
de pistas o una lista: longitudes, `"N%"`, `"auto"` o fracciones `"1fr"`
del espacio libre. Cada hijo puede ocupar varias celdas con `row_span` y
`column_span`.

```python
# output: preview.webp
from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.5)
L = scene.layout
cell = BoxStyle(padding="16px", radius="14px", background="#dbeafe", font_size="26px",
                align="center", justify="center")
hot = cell.but(background="#fde68a")

bento = L.grid(
    L.box("Destacado", style=hot).item(row_span=2),
    L.box("Ancho", style=hot).item(column_span=2),
    L.box("A", style=cell), L.box("B", style=cell),
    L.box("Pie", style=hot).item(column_span=3),
    columns=["260px", "1fr", "2fr"], rows=["1fr", "1fr", "90px"],
    gap="14px", within="safe", width="fill", height="fill",
)
scene.play([bento.animate.fade_in().duration(0.5)])
scene.render()
```

Las celdas se estiran por defecto. Con `align` en la rejilla, o
`align_self` en un hijo, lo colocas arriba, en el centro o abajo de su celda.

= Cada hijo decide cómo se coloca: `item()`

Cualquier objeto dentro de una caja acepta `.item(...)`, que es el CSS del
elemento dentro de su contenedor:

- `grow` / `shrink` / `basis`: cuánto crece o encoge en el eje principal.
- `align_self`: su alineación propia en el otro eje.
- `margin`: espacio alrededor; `"auto"` se come el espacio libre, así que
  `margin=(0, 0, 0, "auto")` empuja un botón al extremo derecho.
- `row`, `column`, `row_span`, `column_span`: su celda en un grid.
- `fit`: cómo se ajusta una imagen o figura a su caja (`contain`, `cover`,
  `stretch`, `scale_down`).
- `anchor`, `offset`, `absolute`: posición dentro de una capa (`L.stack`) o
  fuera del flujo.
- `width` / `height`: un tamaño de layout que sustituye al propio del objeto.

```python
# output: preview.webp
from gaanim import Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.6)
L = scene.layout

bar = L.row(
    L.box("Gaanim", font_size="30px", weight=700),
    L.box("Docs", font_size="24px", color="#475569"),
    L.box("Blog", font_size="24px", color="#475569"),
    L.box("Empezar", font_size="24px", color="white", background="#4f46e5", radius="full",
          padding=("8px", "22px")).item(margin=(0, 0, 0, "auto")),
    gap="28px", align="center", width="fill", padding=("14px", "24px"),
    radius="18px", background="white", border="#e2e8f0",
)
progress = L.row(L.box("Carga", font_size="24px"),
                 L.box(height="14px", radius="full", background="#4f46e5").item(grow=1),
                 L.box("72%", font_size="24px", weight=700),
                 gap="16px", align="center", width="fill")
page = L.column(bar, progress, gap="60px", within="safe", width="fill", justify="center",
                height="fill")
scene.play([page.animate.fade_in().duration(0.5)])
scene.render()
```

= Animar la estructura

Una caja es un objeto más de la escena: `page.animate.fade_in()` y el resto
de animaciones actúan sobre ella entera. Además, todos los cambios de
estructura aceptan `duration=`: los hijos se deslizan a su nuevo sitio, lo que
entra aparece con un fundido y lo que sale se desvanece.

- `box.add(hijo, at=1, duration=0.6)`, `box.remove(hijo, duration=0.6)`,
  `box.replace(viejo, nuevo, duration=0.6)`.
- `box.set(columns=2, gap="40px", duration=0.8)` cambia cualquier propiedad.
- `hijo.item(column_span=2, duration=0.8)` cambia las reglas de un hijo.

Cada cambio con `duration` avanza el cursor de la escena, como `scene.play`.
Con `advance=False` el siguiente cambio empieza a la vez.

```python
# output: preview.webp
from gaanim import BoxStyle, Scene

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout
card = BoxStyle(padding="20px", radius="16px", background="#e0e7ff", font_size="28px",
                width="170px", align="center")

board = L.grid(*[L.box(name, style=card) for name in ("Uno", "Dos", "Tres")],
               columns=3, gap="18px")
scene.play([board.animate.fade_in().duration(0.4)])
board.add(L.box("Nueva", style=card, background="#bbf7d0"), at=1, duration=0.6)
board.set(columns=2, duration=0.8)
board.replace(board[0], L.box("Cambio", style=card, background="#fecaca"), duration=0.6)
board.set(gap="40px", duration=0.8, advance=False)
board[1].item(column_span=2, width="fill", duration=0.8)
scene.wait(0.4)
scene.render()
```

= Zonas: plantillas que no atan a los objetos

A veces no quieres que un objeto viva dentro de una caja: un rótulo que entra
deslizándose, una fórmula que cruza la pantalla, una ilustración libre. Las
*zonas* dividen una región en rectángulos con nombre y te dejan colocar ahí
objetos que siguen siendo independientes.

```python
# output: preview.webp
from gaanim import Scene, Zones

scene = Scene(frame=(16, 9), theme="paper", margin=0.5)
L = scene.layout
page = Zones.rows(["90px", "1fr", "60px"], names=["cabecera", "cuerpo", "pie"], gap="16px")
split = Zones.columns(["1fr", "2fr"], names=["lateral", "principal"], gap="24px")

z = L.zones(page)
body = z["cuerpo"].split(split)
for zona in (z["cabecera"], z["pie"], *body):  # contornos solo para verlas
    scene.geometry.rect(zona.width, zona.height).no_fill().stroke("#cbd5e1", 0.02).place(zona)

title = scene.text("Zonas", size=0.5).place(z["cabecera"], anchor="left")
note = scene.text("Misma plantilla, cualquier escena", size=0.3).place(z["pie"], anchor="right")
ball = scene.geometry.circle(1).fill("#f97316").place(body["lateral"], fit="contain", padding="20px")
dots = [scene.geometry.circle(0.3).fill("#10b981") for _ in range(4)]
body["principal"].arrange(*dots, gap="24px")

scene.play([ball.animate.place(z["cabecera"], anchor="right", fit="contain").duration(0.8)])
scene.render()
```

- `L.zones(plantilla)` aplica la plantilla al área segura (respeta el
  `margin` de la escena); `within="frame"`, una zona o un objeto cambian la
  región.
- `Zones.rows`, `Zones.columns` y `Zones.grid` usan las mismas pistas que los
  grids. Una plantilla se reutiliza en otra escena, en una zona
  (`zona.split(plantilla)`) o dentro de un objeto.
- `objeto.place(zona, anchor=..., fit=..., padding=..., offset=...)` lo
  coloca una vez; `objeto.animate.place(...)` lo lleva hasta ahí.
- `zona.arrange(*objetos)` los alinea en fila o columna sin adoptarlos, y
  `zonas.fill(*objetos)` pone uno en cada zona.

= Componentes

Un componente es una función de Python que devuelve una caja. Con
`@component` Gaanim comprueba sus argumentos y tu editor los autocompleta.
Cada llamada crea objetos nuevos.

```python
# output: preview.webp
from gaanim import Scene, component

scene = Scene(frame=(16, 9), theme="paper")
L = scene.layout

@component
def profile(scene, *, name: str, role: str, color: str = "#4f46e5"):
    initials = "".join(part[0] for part in name.split())
    return L.row(
        L.box(initials, width="64px", height="64px", radius="full", background=color,
              color="white", font_size="26px", weight=700, align="center", justify="center"),
        L.column(L.box(name, font_size="28px", weight=700),
                 L.box(role, font_size="20px", color="#64748b")),
        gap="18px", align="center", padding="18px", radius="20px", background="white",
        border="#e2e8f0",
    )

team = L.column(
    profile(scene, name="Ana Ruiz", role="Diseño"),
    profile(scene, name="Luis Paz", role="Datos", color="#0891b2"),
    profile(scene, name="Eva Gil", role="Producción", color="#f97316"),
    gap="16px",
)
scene.play([team.animate.fade_in().duration(0.5)])
scene.render()
```

Las plantillas de presentación (`title_slide`, `lecture`, `comparison`…) son
componentes escritos exactamente así.

= Coordenadas o cajas

Usa cajas y zonas para la composición: títulos, tarjetas, paneles, listas,
rótulos. Usa coordenadas cuando la posición tiene significado: el punto de
una gráfica, una órbita, una trayectoria.

= Animar dentro de una caja

La caja decide dónde descansa cada hijo, no cómo se mueve. Todas las
animaciones de `.animate` funcionan sobre un hijo: `fade_in_from`,
`shift_by`, `move_to`, `scale_by`, `rotate_by`, `grow_from_edge`… Parten del
sitio que le dio la caja y no empujan a sus hermanos, como un `transform` de
CSS. Si después la caja se reorganiza (un `add`, un `replace`, un texto que
cambia), el hijo conserva el desplazamiento que le diste respecto a su sitio.

```python
# output: preview.webp
from gaanim import Direction, Scene

scene = Scene(frame=(16, 9), theme="paper", margin=0.5)
L = scene.layout
title = L.box("Resultados", font_size="96px", weight=700, color="#312e81")
points = L.column(
    L.box("Menos pasos", font_size="44px", padding=("14px", "28px"), radius="16px",
          background="#e0e7ff"),
    L.box("Menos errores", font_size="44px", padding=("14px", "28px"), radius="16px",
          background="#e0e7ff"),
    gap="20px",
)
slide = L.column(title, points, gap="48px", align="center", justify="center",
                 width="fill", height="fill")

scene.play([title.animate.fade_in_from(Direction.LEFT).duration(0.5)])
scene.play([points[0].animate.fade_in_from(Direction.UP).duration(0.4),
            points[1].animate.fade_in_from(Direction.UP).duration(0.4).delay(0.15)])
scene.play([title.animate.scale_by(1.15).rotate_by(-0.05).duration(0.4)])
# La columna gana un punto: los demás se deslizan y el título sigue girado.
points.add(L.box("Más tiempo", font_size="44px", padding=("14px", "28px"),
                 radius="16px", background="#fde68a"), duration=0.5)
scene.render()
```

`move_to(x, y)` usa coordenadas de la escena también para un hijo. Lo único
reservado a la caja es colocar a un hijo _al declararlo_: `move_to`,
`next_to` o `to_edge` inmediatos sobre un hijo lanzan
`LayoutOwnershipError`. Para eso ajusta `item(offset=...)`, mueve la caja
entera o saca al hijo con `box.detach(hijo)`. Si algo no encaja,
`box.diagnostics()` y `scene.layout.check_layout()` explican qué restricción
o texto no se pudo cumplir. En el editor, `O` y después `K` activan el
overlay *Layout*: muestra cada caja y zona y, al pasar el cursor por una
caja, su padding, sus márgenes y sus gaps, como las herramientas de
desarrollo de un navegador.

Consulta #link("/referencia/layout/")[la referencia de Layout] para todas las
propiedades, firmas y errores. Los ejemplos `layout_*` y `ui_*` del
repositorio muestran interfaces completas: un panel de métricas, una app de
chat y un rótulo de vídeo.
