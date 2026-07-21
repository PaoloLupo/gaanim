# Propuesta de layout y tipografía para Gaanim

## Objetivo

El layout debe permitir construir composiciones de vídeo consistentes sin convertir la API en un clon de CSS. La unidad fundamental es una región rectangular en coordenadas de escena. Una región puede colocar objetos, aplicar un área segura o subdividirse en un grid.

La primera versión implementada cubre:

- frame seguro que respeta los márgenes del `Scene`;
- regiones editoriales `header`, `content` y `footer`;
- regiones anidadas con `inset`;
- grids con filas, columnas, gaps y spans;
- colocación mediante los nueve `Anchor` existentes;
- párrafos vectoriales con ancho, alineación, justificación, interlineado, fuente y tamaño;
- compatibilidad con las animaciones actuales, porque el resultado sigue siendo un `Drawable`.

## Modelo mental

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

`FrameLayout` es un preset editorial. `LayoutRegion` y `GridLayout` son las primitivas generales. Esto permite añadir otros presets más adelante —por ejemplo, comparación, lower third o presentación— sin crear otro motor de layout.

## API Python actual

```python
from gaanim import Anchor, Scene

scene = Scene(1920, 1080, margin=72)
layout = scene.layout(header=180, footer=72, gap=32)

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

### Regiones

```python
safe = layout.content.inset(24)
position = safe.point(Anchor.BOTTOM_RIGHT)
object = safe.place(object, Anchor.CENTER)
```

`inset(value)` aplica el mismo valor a los cuatro lados. También acepta la forma CSS `inset(top, right, bottom, left)`.

### Grid

```python
grid = region.grid(rows=3, columns=12, row_gap=20, column_gap=24)
cell = grid.cell(0, 0)
hero = grid.area(0, 0, row_span=2, column_span=8)
sidebar = grid.area(0, 8, row_span=3, column_span=4)
```

Las filas se numeran de arriba hacia abajo y las columnas de izquierda a derecha. Un área fuera del grid genera `IndexError` en Python y `None` en Rust.

### Párrafos

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

Los párrafos se componen con Typst y se extraen como contornos vectoriales individuales. Por ello siguen funcionando `write`, `fade_in`, selecciones de glifos y transformaciones. `scene.text()` permanece como la opción ligera para etiquetas de una sola línea.

### Texto flotante y overlays

Un overlay se coloca respecto al `frame`, recibe un `z_index` alto y puede conservar una posición fija durante varios segmentos:

```python
badge = layout.frame.inset(32).place(
    scene.text("Capítulo 2").z_index(100),
    Anchor.TOP_RIGHT,
)
```

Para una etiqueta que sigue a un objeto animado se reutilizan los bindings existentes:

```python
label = scene.text("máximo local").z_index(100)
label.follow_to(point, offset=(0, 36))
```

### Stacks

`vstack` y `hstack` ordenan los hijos directos de un grupo antes de colocar
el grupo dentro de una región. Por defecto, el stack vertical alinea a la
izquierda y el horizontal alinea por abajo.

```python
heading = scene.text("Pasos").scaled(1.25)
items = [scene.text("Preparar"), scene.text("Explicar"), scene.text("Cerrar")]

stack = scene.group([heading, *items]).vstack(gap=20)
stack = layout.content.place(stack, Anchor.TOP_LEFT)

legend = scene.group([dot, scene.text("Dato")]).hstack(gap=12, align=Anchor.CENTER)
```

Los stacks conservan los `Drawable` originales; se pueden animar como grupo o
animar cada hijo por separado.

## Separación entre layout y contenido

El grid calcula espacios; no crea rectángulos visibles ni es propietario de los objetos. `place` solo agrega una operación de layout al `Drawable`. Esta separación permite reutilizar regiones entre segmentos, conservar toda la API de animación y evitar entidades ECS auxiliares.

El layout inicial se resuelve al compilar la escena. Una animación posterior parte de esa posición resuelta.

## Evolución propuesta

### Fase 2: tracks y tamaños flexibles

```python
grid = region.grid(
    columns=["2fr", "3fr", 280],
    rows=["auto", "1fr"],
    gap=24,
)
```

- número: tamaño fijo;
- `fr`: reparto del espacio restante;
- `auto`: máximo tamaño intrínseco de los objetos de esa pista.

Esto requiere una fase de medición antes de resolver posiciones. No debe incorporarse hasta tener límites claros para grupos, imágenes y texto.

### Fase 3: flow y wrapping automático

```python
flow = region.flow(direction="vertical", gap=20, align="left")
flow.add(title)
flow.add(paragraph)
flow.add(equation)
```

`vstack` y `hstack` ya resuelven la secuencia explícita de hijos. El siguiente nivel es un flow que acepte elementos añadidos incrementalmente y ofrezca `start`, `center`, `end`, `space_between` y wrapping horizontal. Necesita medir objetos diferidos antes de compilar.

### Fase 4: constraints y ajuste

```python
region.fit(image, mode="contain")
region.fit(group, mode="scale_down")
paragraph = paragraph.max_lines(4).overflow("ellipsis")
```

Modos sugeridos: `contain`, `cover`, `stretch`, `scale_down`. El overflow de texto debería ser `clip`, `ellipsis` o `visible`.

### Fase 5: texto enriquecido

```python
scene.rich_text([
    Span("Velocidad: "),
    Span("42 m/s", weight="bold", color=YELLOW),
], width=500)
```

Debe soportar spans, peso, itálica, color, tracking, indentación de primera línea y espacio entre párrafos. Conviene conservar Typst como compositor y mapear los spans de vuelta a rangos fuente para animarlos.

### Fase 6: presets de vídeo

Los presets solo deben producir regiones:

```python
layout = scene.layout.preset("lecture")
layout = scene.layout.preset("comparison")
lower_third = scene.layout.overlay("lower_third")
```

Posibles presets: título completo, título + contenido, dos columnas, comparación, picture-in-picture, lower third y créditos. Los estilos visuales pertenecen a un tema, no al layout.

## Decisiones pendientes

- Definir si los anchos tipográficos se expresan siempre en unidades de escena o en puntos independientes del canvas.
- Medir correctamente el ancho lógico de un párrafo aunque la última línea sea corta.
- Decidir cómo recalcular un layout cuando cambia el texto mediante un updater.
- Añadir guías de safe area para formatos 16:9, 9:16 y 1:1.
- Diseñar una fase explícita `measure → resolve → animate` antes de implementar `auto` y `fr`.

## Criterios de estabilidad

- El mismo layout produce posiciones deterministas en seeks y exportación.
- Grid, spans e insets funcionan en coordenadas `Pixels` y `Scene`.
- Un párrafo justificado mantiene glifos seleccionables y animables.
- Los errores de filas, columnas, ancho y alineación se reportan antes del render.
- Existen snapshots visuales para 16:9 y 9:16.
