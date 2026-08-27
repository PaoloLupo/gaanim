#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Layout",
  description: "Composición didáctica de escenas adaptables sin coordenadas manuales",
  route: "/guides/layout/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Qué resuelve Layout

Layout organiza contenido editorial: títulos, columnas, tarjetas, leyendas y
paneles. Usa un árbol de filas, columnas, grids y stacks. Cada cambio ejecuta
el ciclo determinista `measure → solve → place`; el resultado se materializa
como una operación de la línea de tiempo.

Para movimiento geométrico deliberado sigue usando transformaciones y
coordenadas. Para relaciones espaciales entre bloques usa Layout.

== Primera columna

```python
page = scene.layout.column(
    [title, scene.layout.item(content, grow=1), footer],
    within="safe",
    width="fill",
    height="fill",
    padding=48,
    gap=32,
    align="stretch",
    justify="between",
)
```

El área `safe` respeta el margen de la escena. `hug` prefiere el tamaño
intrínseco, `fill` consume el espacio ofrecido y un número fija la dimensión.

== Filas, grids y capas

Usa `scene.layout.row` para distribuir elementos horizontalmente, `scene.layout.grid` para
tracks bidimensionales y `scene.layout.stack` para overlays. Los layouts anidados
reciben el espacio que ofrece su padre.

== Posición y constraints

El layout controla la traslación de sus hijos. Usa `offset` para ajustes
editoriales pequeños y constraints para relaciones entre ramas:

```python
scene.layout.constrain(
    label.left == chart.right + 24,
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.3).weak(),
)
```

Las relaciones obligatorias incompatibles fallan antes del render. Las débiles
pueden consultarse mediante los diagnósticos de layout.

== Responsive 16:9 y 9:16

Conserva el mismo árbol y cambia solo el viewport. Usa `fill`, crecimiento,
tracks fr, wrapping y templates para que el contenido se redistribuya. Reserva
`absolute=True` para overlays y evita `move_to()` dentro del árbol.

Los cambios de estructura disparan reflow. `Text` también invalida su medida
al cambiar contenido, fuente, tamaño, spacing o wrapping. Transiciones
estructurales como `become` y `text.animate.transform_to(target)` propagan el
reflow al layout padre con la misma duración.

== Referencia

Consulta #link("/api/layout/")[la API de Layout] para todas las firmas, reglas
por elemento, constraints y plantillas disponibles.
