#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Layout",
  description: "Árboles responsive, tracks de grid, constraints relacionales y reflow animado",
  route: "/api/layout/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Layout

Layout v2 es el modelo público de composición de Gaanim. Un `Layout` también es
un `Drawable` y es propietario de la traslación de sus hijos directos. Construye
árboles adaptables con `row`, `column`, `grid` y `stack`; usa `item` para definir
reglas particulares de un hijo.

El modelo mental tiene tres niveles:

1. El contenedor decide el flujo, el tamaño exterior, el espacio y la alineación.
2. `scene.item(...)` explica cómo un hijo consume la caja que recibe.
3. El contenido se mide dentro de esa caja; el texto puede recomponer líneas y
   los medios pueden ajustar su geometría.

Una vez que un objeto pertenece a Layout, deja que el árbol determine su
posición. Mezclar `at()` o `move()` con Layout introduce dos autoridades sobre
la misma coordenada y Gaanim lo rechaza explícitamente.

```python
page = scene.column(
    [
        scene.text("Resultado", role="title"),
        scene.row([
            scene.item(copy, grow=2),
            scene.item(diagram, grow=3, fit="contain"),
        ], gap=40, align="center"),
        footer,
    ],
    within="safe",
    width="fill",
    height="fill",
    padding=48,
    gap=32,
    align="stretch",
    justify="between",
)
```

`width` y `height` aceptan un número fijo, `"hug"` o `"fill"`. `padding`
acepta un valor, `(vertical, horizontal)` o `(top, right, bottom, left)`. La
alineación puede ser `start`, `center`, `end` o `stretch`; la distribución
también admite `between`, `around` y `evenly`.

Los constructores públicos son:

```python
scene.row(children, *, gap=24, padding=0, width="hug", height="hug",
          align="center", justify="start", wrap=False, within=None)
scene.column(children, *, gap=24, padding=0, width="hug", height="hug",
             align="start", justify="start", wrap=False, within=None)
scene.grid(children, *, rows=1, columns=1, gap=0, row_gap=None,
           column_gap=None, padding=0, width="hug", height="hug",
           align="stretch", justify="start", auto_flow="row", within=None)
scene.stack(children, *, padding=0, width="hug", height="hug",
            align="center", within=None)
```

`within=None` crea un contenedor anidado e intrínseco; las raíces suelen elegir
`"safe"` o `"frame"`. Los números no negativos se expresan en unidades del
lienzo. Tamaños, padding, tracks, alineaciones o bloques contenedores inválidos
producen `ValueError`. El texto adaptable se compone con el ancho ofrecido por
su fila, columna, track o caja final. Los límites visuales más estrechos de los
glifos no se convierten después en un nuevo límite de ajuste.

== Posicionamiento mediante anchors

Fuera de un árbol `Layout`, `at()` también acepta el `AnchorPoint` de otro
objeto. El centro del objeto receptor se coloca sobre el anchor después de
aplicar la traslación, rotación y escala iniciales de la referencia. El offset
opcional pertenece al espacio local de la referencia.

```python
from gaanim import Anchor

card = scene.rect(240, 120).at(80, 20).rotated(0.15)
label = scene.text("Detalle").at(
    card.anchor_point(Anchor.TOP_RIGHT, offset=(-12, -12))
)
```

La firma relevante es
`drawable.at(reference.anchor_point(anchor, offset=(dx, dy))) -> Drawable`.
No se puede combinar un `AnchorPoint` con los argumentos `y` o `anchor` de la
variante numérica. La relación se resuelve durante el layout inicial; para
seguir una referencia mientras se anima, usa `follow` o `attach_to`.

`move_to()` admite las mismas referencias y devuelve un `Anim`: `obj.move_to(card)`
mueve centro con centro, mientras
`obj.move_to(card.anchor_point(Anchor.BOTTOM_LEFT))` anima el centro de `obj`
hasta ese anchor. El destino se calcula con el estado de layout que tiene la
referencia al programar la animación.

== Atlas de posibilidades de layout

Esta sección resume el espacio de diseño de Layout v2. Las capacidades son
ortogonales: un grid puede anidarse dentro de una columna, un stack puede ocupar
un track fraccional, el texto adaptable puede crecer dentro de cualquiera de
ellos y el árbol resultante aún puede recibir restricciones y animarse.

#table(
  columns: (1.05fr, 1.15fr, 2.4fr),
  inset: 7pt,
  [*Objetivo*], [*API*], [*Qué controla*],
  [Flujo horizontal], [`scene.row(...)`], [El eje principal avanza de izquierda a derecha y puede continuar en filas nuevas.],
  [Flujo vertical], [`scene.column(...)`], [El eje principal avanza de arriba abajo y puede continuar en columnas nuevas.],
  [Distribución por tracks], [`scene.grid(...)`], [Filas y columnas fijas, intrínsecas `auto` o ponderadas `fr`, spans y colocación automática.],
  [Superposición], [`scene.stack(...)`], [Una caja compartida para fondos, medios, captions, badges e hijos absolutos.],
  [Tamaño exterior], [`width` / `height`], [Un número fijo en unidades del lienzo, el tamaño intrínseco `"hug"` o el espacio disponible `"fill"`.],
  [Hijos flexibles], [`scene.item(...)`], [`grow`, `shrink`, alineación individual, coordenadas de grid, spans, offsets, anchors y ajuste.],
  [Espaciado], [`padding`, `gap`], [Inset único, vertical/horizontal o por los cuatro lados, y separaciones independientes de filas y columnas.],
  [Alineación], [`align`, `justify`], [Colocación en el eje transversal y distribución en el eje principal.],
  [Límites de la raíz], [`within="safe"` / `"frame"`], [Uso del área segura que respeta márgenes o del viewport completo.],
  [Contenido adaptable], [`TextFlow(wrap="auto")`], [Vuelve a medir el texto con el ancho ofrecido por su caja final.],
  [Ajuste de medios], [`fit=...`], [`none`, `contain`, `cover`, `stretch` o `scale_down`; `cover` también recorta.],
  [Relaciones], [`scene.constrain(...)`], [Ecuaciones e inecuaciones lineales entre geometría de ramas distintas.],
  [Estructura viva], [`add` / `remove` / `detach` / `replace` / `configure`], [Crea una instantánea determinista del reflow, inmediata o animada.],
  [Páginas reutilizables], [`scene.template(...)` / `segment.bind(...)`], [Slots tipados, patrones de presentación y tokens de espaciado del tema.],
)

=== Escena completa sin coordenadas

La escena siguiente usa un árbol `column -> row -> stack -> column`. Ningún hijo
llama a `at()`: el árbol exterior decide todas las traslaciones y el texto se
vuelve a medir con el ancho de la tarjeta que lo contiene.

```python
from gaanim import BLUE, GOLD, WHITE, Scene, TextFlow

scene = Scene(1280, 720, background="#0b1020", margin=48)

copy = scene.text(
    "El mismo árbol puede componer una diapositiva, un panel o un video vertical.",
    role="body",
    color=WHITE,
    flow=TextFlow(wrap="auto", line_spacing=1.2),
)

card = scene.stack(
    [
        scene.item(scene.rounded_rect(360, 220, 18).fill(BLUE), fit="stretch"),
        scene.column(
            [
                scene.text("Contenido medido", role="heading", color=GOLD),
                copy,
            ],
            width="fill",
            height="fill",
            padding=28,
            gap=18,
            align="stretch",
            justify="center",
        ),
    ],
    width=360,
    height=220,
    align="stretch",
)

body = scene.row(
    [
        scene.item(card, grow=2),
        scene.item(scene.circle(96).fill(GOLD), grow=1, align="center"),
    ],
    width="fill",
    gap=40,
    align="center",
)

page = scene.column(
    [
        scene.text("Atlas de Layout v2", role="title", color=GOLD),
        scene.item(body, grow=1, align="stretch"),
        scene.text("Sin coordenadas manuales", role="caption", color=WHITE),
    ],
    within="safe",
    width="fill",
    height="fill",
    padding=(24, 40),
    gap=32,
    align="stretch",
    justify="between",
)

scene.play([page.fade_in().duration(0.6)])
scene.render()
```

=== Tamaños, padding y espacio flexible

`"hug"` mide el contenido intrínseco, un número fija la caja exterior en
unidades del lienzo y `"fill"` consume la restricción disponible. `grow`
distribuye entre hermanos el espacio sobrante del eje principal; `shrink`
decide cuáles pueden contraerse cuando la fila o columna es más estrecha que su
tamaño preferido.

Una forma útil de decidir es comenzar con `"hug"`, cambiar a `"fill"` solo en
el eje que deba consumir espacio y usar `grow` únicamente para repartir el
sobrante entre hermanos. Demasiados `fill` anidados suelen indicar que no está
claro qué contenedor debe controlar el tamaño.

```python
badge = scene.row([icon, label], width="hug", padding=(8, 14), gap=8)

workspace = scene.row(
    [
        scene.item(sidebar, grow=0, shrink=0),
        scene.item(content, grow=3, shrink=1, align="stretch"),
        scene.item(inspector, grow=1, shrink=1),
    ],
    width="fill",
    height="fill",
    padding=(24, 40),       # vertical, horizontal
    gap=32,
    align="stretch",
)

workspace.configure(
    min_width=640,
    max_width=1180,
    min_height=360,
    aspect_ratio=16 / 9,
)
```

`padding` acepta `padding=24`, `padding=(vertical, horizontal)` o
`padding=(top, right, bottom, left)`. Las filas y columnas usan un único `gap`;
los grids pueden sustituirlo mediante `row_gap` y `column_gap`.

=== Alineación y distribución

`align` controla el eje transversal y acepta `start`, `center`, `end` o
`stretch`. `justify` controla el eje principal y acepta `start`, `center`,
`end`, `between`, `around` o `evenly`. Un hijo puede sustituir la alineación
transversal mediante `scene.item(..., align=...)`.

En una `row`, el eje principal es horizontal y `align` actúa verticalmente. En
una `column`, el eje principal es vertical y `align` actúa horizontalmente.
Esta distinción resuelve la mayoría de dudas sobre cuál de las dos propiedades
usar.

```python
toolbar = scene.row(
    [back, scene.item(search, grow=1, align="stretch"), actions],
    width="fill",
    align="center",
    justify="between",
)

steps = scene.column(
    [intro, explanation, result],
    height="fill",
    align="stretch",
    justify="evenly",
)

chips = scene.row(tags, width=620, gap=12, wrap=True, align="center")
```

Con `wrap=True`, una fila comienza otra fila cuando el siguiente hijo supera el
ancho disponible; una columna aplica la regla equivalente en vertical y abre
otra columna. Los hijos absolutos nunca consumen espacio del flujo.

=== Reglas por elemento

#table(
  columns: (0.95fr, 1.15fr, 2.7fr),
  inset: 7pt,
  [*Regla*], [*Valores*], [*Efecto*],
  [`grow`], [número no negativo], [Proporción ponderada del espacio sin usar en el eje principal.],
  [`shrink`], [número no negativo], [Permiso relativo para contraerse bajo presión; `0` protege el tamaño preferido.],
  [`align`], [`start | center | end | stretch`], [Sustituye la alineación transversal del contenedor para este hijo.],
  [`row`, `column`], [entero basado en cero o `None`], [Elige un origen explícito en el grid; sin coordenadas se usa colocación automática determinista.],
  [`row_span`, `column_span`], [entero >= 1], [Ocupa varios tracks del grid.],
  [`absolute`], [`True | False`], [Retira el hijo de la medición del flujo y lo coloca respecto a la caja contenedora.],
  [`anchor`], [`Anchor.*`], [Elige el centro, borde o esquina dentro de un stack, celda de grid o caja absoluta.],
  [`offset`], [`(x, y)`], [Añade un desplazamiento editorial intencional sin abandonar la propiedad de Layout.],
  [`fit`], [`none | contain | cover | stretch | scale_down`], [Adapta la geometría del objeto a la caja asignada.],
)

Estas reglas funcionan durante la construcción y pueden cambiarse después sin
reconstruir el árbol:

```python
page.configure_item(
    chart,
    grow=2,
    shrink=1,
    align="stretch",
    offset=(12, 0),
    fit="contain",
    animate=0.35,
)
```

=== Raíces responsive y árboles anidados

Usa `within="safe"` para contenido de presentación que respete los márgenes de
la escena y `within="frame"` para fondos a sangre completa. Los layouts anidados
suelen dejar `within=None` porque reciben sus restricciones del padre. El estado
sucio y la medición adaptable se propagan automáticamente hasta el propietario
exterior.

```python
background = scene.stack(
    [scene.item(photo, fit="cover")],
    within="frame",
    width="fill",
    height="fill",
    align="stretch",
)

content = scene.column(
    [title, scene.text(copy, flow=TextFlow(wrap="auto")), footer],
    within="safe",
    width="fill",
    height="fill",
    align="stretch",
    justify="between",
)

page = scene.stack([background, content], within="frame", width="fill", height="fill")
```

El mismo árbol puede servir para 16:9 y 9:16. El wrap del flujo, los tracks
fraccionales, `grow` y el texto adaptable responden al viewport ofrecido. Crea
árboles distintos solo cuando cambie la jerarquía editorial, no únicamente el
tamaño.

== Grid y overlays

Los tracks de un grid aceptan valores fijos, `"auto"` y fracciones como
`"2fr"`. Los elementos pueden elegir fila y columna, además de abarcar varios
tracks. Las posiciones omitidas usan colocación automática y determinista por
filas o columnas.

Usa `auto` para contenido cuyo tamaño intrínseco debe mandar, unidades fijas
para requisitos editoriales rígidos y `fr` para repartir el resto. Un grid es
preferible a filas anidadas cuando varias regiones deben compartir líneas de
alineación.

```python
cards = scene.grid(
    [hero, scene.item(chart, column_span=2), notes],
    columns=[240, "1fr", "2fr"],
    rows=["auto", "1fr"],
    gap=24,
    width="fill",
)

overlay = scene.stack([
    scene.item(photo, fit="cover"),
    scene.item(caption, absolute=True, offset=(0, -180)),
], within="frame", width="fill", height="fill")
```

El ajuste de imágenes y SVG admite `contain`, `cover`, `stretch` y
`scale_down`. `cover` recorta el renderizado a la caja asignada.

Las celdas explícitas y sus spans se reservan antes de la colocación automática,
independientemente del orden de los hijos. Una colisión explícita, un span fuera
de rango o un grid sin suficientes celdas libres detiene la resolución e indica
el ID del nodo implicado.

== Propiedad y reflow

Después de adjuntar un objeto, las llamadas posicionales como `at`, `next_to`,
`align_to`, `to_edge` o las animaciones manuales de movimiento producen
`LayoutOwnershipError`. La rotación y la escala siguen siendo válidas. Expresa
un desplazamiento intencional mediante `offset`.

El contenedor raíz sí es un `Drawable` posicionable. Operaciones como
`page.at(400, 200)`, `at_anchor`, `next_to`, `align_to`, `to_edge`, `to_corner`,
rotación y escala se reaplican sobre la caja final después de cada reflow y
transforman el árbol completo. La restricción anterior corresponde únicamente
a los hijos cuya traslación pertenece al contenedor.

```python
panel = scene.column([formula, explanation], gap=50, align="center")
panel.at(400, 200)
```

Este error no es una limitación accidental: protege la búsqueda temporal. Si
Layout y una animación escribieran la misma posición, recorrer la línea de
tiempo podría resolver resultados diferentes según el orden de evaluación.

```python
page.add(extra, at=1, animate=0.35)
page.replace(old, new, animate=0.35)
page.detach(title, animate=0.35)
scene.play([title.move_to(0, 200)])
page.configure(gap=40, padding=56, animate=0.4)
page.configure(min_width=480, max_width=960, aspect_ratio=16 / 9)
page.configure_item(chart, grow=2, offset=(12, 0), animate=0.3)
page.reflow(animate=0.25)
```

Las mutaciones estructurales se propagan por los layouts anidados. Las
operaciones de la línea de tiempo almacenan instantáneas versionadas del árbol;
una búsqueda directa y una reproducción secuencial resuelven la misma geometría.

`add`, `remove`, `detach` y `replace` actúan sobre hijos directos. `remove`
realiza una salida visual; `detach` conserva la posición global, la opacidad y
la pertenencia a la escena mientras libera la propiedad de Layout. Un objeto
separado puede usar inmediatamente `at`, `move_to`, `next_to` y las demás
operaciones posicionales. Esto es útil al llevar un hijo administrado a un
segmento nuevo:

```python
scene.segment("detail", Transition.cross_fade(0.4))
scene.reuse(title)
page.detach(title)
scene.play([title.move_to(0, 200).duration(0.35)])
```

El valor `animate` de estas operaciones, así como el de `configure`,
`configure_item` y `reflow`, es una duración en segundos. En `detach` solo se
anima el reflow de los hijos restantes; el hijo separado permanece visible y
fijo hasta recibir una animación explícita. Con `None`, la línea de tiempo
registra una transición instantánea y determinista.

== Constraints lineales

Cada objeto expone `left`, `right`, `top`, `bottom`, `center_x`, `center_y`,
`width` y `height`. Las expresiones son lineales: solo admiten suma, resta y
multiplicación o división por escalares.

Las restricciones complementan el flujo; no deberían reconstruirlo. Úsalas
cuando dos ramas separadas deban compartir una relación geométrica, por ejemplo
alinear una etiqueta externa con el centro de un gráfico.

```python
relations = scene.constrain(
    (label.left == chart.right + 24).strong(),
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.30).weak(),
)
```

Las relaciones son `required` por defecto y ofrecen las alternativas `strong`,
`medium` y `weak`. Los conflictos obligatorios fallan antes del renderizado. Los
IDs estables, el orden canónico y las permanencias débiles explícitas hacen
reproducibles las soluciones equivalentes.

`scene.check_layout()` devuelve diagnósticos de restricciones blandas justo
después del registro y fallos de composición intrínseca encontrados durante la
reproducción. El contenido Typst adaptable inválido no termina el hot reload del
editor. `layout.diagnostics()` filtra los diagnósticos de una raíz. Un mensaje de
conflicto incluye su etiqueta o índice canónico y los IDs implicados. Una
expresión no puede mezclar objetos de escenas diferentes.

== Texto responsive y plantillas

`scene.text(..., flow=TextFlow(wrap="auto"))` es la única hoja de texto
adaptable. El texto libre recibe el ancho del área segura; el texto administrado
consume el ancho ofrecido por sus `BoxConstraints`. `wrap=False` conserva una
sola línea y un valor numérico limita el ancho tipográfico sin crear un segundo
modelo de cajas.

```python
from gaanim import TextFlow

copy = scene.text(
    "Layout v2 mide este texto con el ancho de su tarjeta.",
    flow=TextFlow(wrap="auto", align="justify", line_spacing=1.25),
)
page = scene.row([
    scene.item(copy, grow=2),
    scene.item(diagram, grow=3, fit="contain"),
], width="fill", gap=32)
```

El adaptador `CompiledTextMeasure` reutiliza la medición intrínseca, la
convergencia de ocho pasadas, clips, diagnósticos y `ResolvedLayout`; no existe
un solucionador separado para texto. La clave de caché incluye el contenido
estructurado, el estilo resuelto, el flujo y las restricciones ofrecidas. Los
cambios métricos y `become`, `morph_to`, `step_to` o `expand_to` invalidan la
instantánea versionada compartida y provocan el reflow de los padres con la
misma duración de transición. Los efectos transitorios `wiggle`, `pulse` y
`wave` no invalidan la medición.

Un `Text` administrado rechaza `at`, `move`, `next_to` y las animaciones
posicionales manuales igual que cualquier otro hijo de Layout. Una
`TextSelection` nunca se convierte en una hoja independiente, pero sus métodos
devuelven valores `Anim` normales; por eso sus selecciones se componen en
`scene.play([...])` con cualquier otra animación. Los destinos de transición de
otra escena o con propietario incompatible producen `LayoutOwnershipError`.

Las plantillas son funciones tipadas de Python:

```python
from gaanim import comparison, layout_template

@layout_template
def two_columns(scene, *, title, left, right, footer=None):
    return scene.column([
        title,
        scene.row([scene.item(left, grow=1), scene.item(right, grow=1)]),
        footer,
    ], within="safe", width="fill", height="fill")

page = scene.template(two_columns, title=title, left=copy, right=diagram)
slide = scene.segment("Comparison", template=comparison)
page = slide.bind(title=title, left=copy, right=diagram)
```

Las plantillas incluidas son `title_slide`, `lecture`, `comparison`,
`vertical_short`, `minimal`, `lower_third` y `credits`.

Las plantillas incluidas consumen tokens de Layout del tema en lugar de
dimensiones aisladas. Léelos con `scene.canvas.layout_token(name)` y
sustitúyelos mediante
`Theme(..., layout={"page_padding": 56, "column_gap": 48})`.

== Errores frecuentes

- *Usar `at()` después de adjuntar un objeto.* Expresa la intención mediante
  `align`, `anchor`, `offset` o una restricción.
- *Aplicar `fill` en todos los niveles.* Decide qué contenedor posee el espacio
  disponible y deja que los descendientes usen `hug` cuando corresponda.
- *Confundir `align` con `justify`.* Identifica primero el eje principal del
  contenedor.
- *Usar constraints para construir una cuadrícula completa.* Prefiere `grid` y
  reserva las restricciones para relaciones entre ramas.
- *Crear árboles distintos para cada resolución.* Intenta primero tracks
  fraccionales, wrap, `grow` y texto adaptable; duplica la estructura solo si
  cambia la jerarquía narrativa.
