#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Propuesta de layout",
  description: "Modelo mental, FrameLayout y evolución futura",
  route: "/guides/layout-proposal/",
  updated: datetime.today().display(),
  code-langs: (),
)

El layout debe permitir construir composiciones de vídeo consistentes sin convertir la API en un clon de CSS. La unidad fundamental es una región rectangular en coordenadas de escena. Una región puede colocar objetos, aplicar un área segura o subdividirse en un grid.

La primera versión implementada cubre:

- frame seguro que respeta los márgenes del `Scene`;
- regiones editoriales `header`, `content` y `footer`;
- regiones anidadas con `inset`;
- grids con filas, columnas, gaps y spans;
- colocación mediante los nueve `Anchor` existentes;
- párrafos vectoriales con ancho, alineación, justificación, interlineado, fuente y tamaño;
- compatibilidad con las animaciones actuales, porque el resultado sigue siendo un `Drawable`.

= Modelo mental

```text
Scene
└── FrameLayout
    ├── frame: LayoutRegion
    ├── header: LayoutRegion
    ├── content: LayoutRegion
    │   └── GridLayout
    │       ├── cell(row, column)
    │       └── area(row, column, row_span, column_span)
    └── footer: LayoutRegion
```

`FrameLayout` es un preset editorial. `LayoutRegion` y `GridLayout` son las primitivas generales. Esto permite añadir otros presets —por ejemplo, comparación, lower third o presentación— sin crear otro motor.

Los presets actuales escalan sus bandas respecto al safe frame:

```python
lecture = scene.layout_preset("lecture")
comparison = scene.layout_preset("comparison")
vertical = scene.layout_preset("vertical_short")
minimal = scene.layout_preset("minimal")
```

= API Python actual

```python
from gaanim import Anchor, Scene

scene = Scene(1920, 1080, margin=72)
layout = scene.frame_layout(header=180, footer=72, gap=32)

title = layout.header.place(
    scene.title("Transformada de Fourier"),
    Anchor.TOP_LEFT,
)

grid = layout.content.grid(
    rows=2,
    columns=12,
    row_gap=24,
    column_gap=24,
)

copy_region = grid.area(0, 0, row_span=2, column_span=5).inset(16)
visual_region = grid.area(0, 5, row_span=2, column_span=7).inset(16)

copy = scene.paragraph(
    "Una explicación larga que se ajusta automáticamente al ancho disponible.",
    width=copy_region.width,
    align="justify",
    line_spacing=1.25,
)
copy = copy_region.place(copy, Anchor.TOP_LEFT)

visual = visual_region.place(scene.circle(140), Anchor.CENTER)
scene.play([title.write(), copy.fade_in(), visual.create()])
```

== Regiones

```python
safe = layout.content.inset(24)
position = safe.point(Anchor.BOTTOM_RIGHT)
object = safe.place(object, Anchor.CENTER)
```

`inset(value)` aplica el mismo valor a los cuatro lados. También acepta la forma CSS `inset(top, right, bottom, left)`.

== Grid

```python
grid = region.grid(rows=3, columns=12, row_gap=20, column_gap=24)
cell = grid.cell(0, 0)
hero = grid.area(0, 0, row_span=2, column_span=8)
sidebar = grid.area(0, 8, row_span=3, column_span=4)
```

Las filas se numeran de arriba hacia abajo y las columnas de izquierda a derecha. Un área fuera del grid genera `IndexError` en Python y `None` en Rust.

Para tracks fijos + espacio disponible, usa `grid_tracks`. Número = unidades fijas; `"fr"` reparte el resto.

```python
grid = region.grid_tracks(
    rows=["1fr"],
    columns=[260, "1fr", "2fr"],
    column_gap=24,
)
```

En este ejemplo la primera columna ocupa 260 unidades; las dos restantes se reparten 1:2. Esta versión no implementa `auto`.

== Párrafos

```python
body = scene.paragraph(
    text,
    width=520,
    align="left",       # left | center | right | justify
    line_spacing=1.2,
    font_size=34,
    font_family="New Computer Modern",
)
```

Los párrafos se componen con Typst y se extraen como contornos vectoriales. Siguen funcionando `write`, `fade_in` y transformaciones. `scene.text()` es la opción ligera para una sola línea.

== Texto flotante y overlays

Un overlay se coloca respecto al `frame`, recibe un `z_index` alto y puede conservar posición fija durante varios segmentos:

```python
badge = layout.frame.inset(32).place(
    scene.text("Capítulo 2").z_index(100),
    Anchor.TOP_RIGHT,
)
```

Para una etiqueta que sigue a un objeto animado:

```python
label = scene.text("máximo local").z_index(100)
label.follow_to(point, offset=(0, 36))
scene.play([label.fade_in().duration(0.3)])
```

== Stacks y Flow

`vstack`/`hstack` ordenan los hijos directos de un grupo antes de colocarlo:

```python
heading = scene.text("Pasos").scaled(1.25)
items = [scene.text("Preparar"), scene.text("Explicar"), scene.text("Cerrar")]

stack = scene.group([heading, *items]).vstack(gap=20)
stack = layout.content.place(stack, Anchor.TOP_LEFT)

legend = scene.group([dot, scene.text("Dato")]).hstack(gap=12, align=Anchor.CENTER)
```

`Scene.flow` elimina el grupo explícito cuando los elementos se conocen secuencialmente:

```python
flow = scene.flow(direction="vertical", gap=20, align=Anchor.LEFT)
flow.add(scene.text("Introducción").scaled(1.25))
flow.add(scene.paragraph(texto, width=480))
flow.add(scene.equation("E = m c^2"))

content = layout.content.place(flow.build(), Anchor.TOP_LEFT)
```

Un `Flow` no acepta nuevos elementos tras `build()`.

= Evolución propuesta

== Fase 2: tracks `auto`

```python
grid = region.grid_tracks(
    columns=["2fr", "3fr", 280],
    rows=["auto", "1fr"],
    gap=24,
)
```

- número: tamaño fijo (implementado);
- `fr`: reparto del resto (implementado);
- `auto`: máximo intrínseco de los objetos de la pista.

Requiere fase de medición antes de resolver posiciones.

== Fase 3: flow con medición

```python
flow = region.flow(direction="vertical", gap=20, align="left")
flow.add(title)
flow.add(paragraph)
```

Próximo nivel: `start`/`center`/`end`/`space_between`, wrapping y `auto`.

== Fase 4: constraints

```python
region.fit(image, mode="contain")
region.fit(group, mode="scale_down")
paragraph = paragraph.max_lines(4).overflow("ellipsis")
```

Modos: `contain`/`cover`/`stretch`/`scale_down`; overflow `clip`/`ellipsis`/`visible`.

== Fase 5: texto enriquecido

```python
scene.rich_text([
    Span("Velocidad: "),
    Span("42 m/s", weight="bold", color=YELLOW),
], width=500)
```

Spans, peso, itálica, color, tracking, indentación. Typst como compositor.

== Fase 6: presets de vídeo

```python
layout = scene.layout.preset("lecture")
lower_third = scene.layout.overlay("lower_third")
```

Título completo, dos columnas, picture-in-picture, lower third, créditos. Los estilos pertenecen al tema, no al layout.

= Decisiones pendientes y criterios

- Unidades tipográficas en escena vs puntos.
- Medir ancho lógico de párrafo aunque última línea sea corta.
- Recalcular layout cuando cambia texto via updater.
- Guías de safe area para 16:9, 9:16 y 1:1.
- Fase explícita `measure → resolve → animate`.

Criterios de estabilidad: posiciones deterministas en seeks/export, grids en `Pixels` y `Scene`, párrafos justificables con glifos animables, errores reportados antes del render, snapshots para 16:9 y 9:16.
