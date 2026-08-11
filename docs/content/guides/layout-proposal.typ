#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Guía de Layout v2",
  description: "Cómo migrar y componer escenas responsive sin coordenadas manuales",
  route: "/guides/layout-proposal/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Modelo mental

Layout v2 usa un árbol híbrido de filas, columnas, grids y stacks. Cada cambio
ejecuta el ciclo determinista `measure → solve → place`; no existe un sistema
ECS de layout por frame. El resultado se materializa como una operación del
timeline.

Un layout raíz puede usar `within="safe"` o `within="frame"`. Los layouts
anidados reciben el espacio que ofrece su padre. `hug` prefiere el tamaño
intrínseco, `fill` consume el espacio ofrecido y un número fija la dimensión.

```python
page = scene.column(
    [title, scene.item(content, grow=1), footer],
    within="safe", width="fill", height="fill",
    padding=48, gap=32, align="stretch", justify="between",
)
```

== Migración directa

#table(
  columns: (1fr, 1.5fr),
  inset: 8pt,
  [Patrón anterior], [Layout v2],
  [`scene.frame_layout(...).content.place(x, ...)`], [`scene.column([x], within="safe", width="fill", height="fill")`],
  [`scene.layout(kind="row")`], [`scene.row([...])`],
  [`region.grid(...)` / `GridTrack`], [`scene.grid(..., rows=[...], columns=[...])`],
  [`group.vstack(...)`], [`scene.column([...])`],
  [`group.hstack(...)`], [`scene.row([...])`],
  [`Flow`], [`layout.add(...)`],
  [`layout.drawable`], [`layout`],
  [`scene.layout_preset("comparison")`], [`scene.template(comparison, ...)`],
  [`segment.region("left")`], [`segment.bind(left=..., right=...)`],
  [`scene.paragraph(copy, width=None)`], [`scene.text(copy, flow=TextFlow(wrap="auto"))`],
)

No hay adaptadores de compatibilidad: la ruptura evita que dos modelos de
ownership y medición produzcan resultados distintos.

== Posición y constraints

El layout controla la traslación de sus hijos. Usa `offset` para ajustes
editoriales y constraints para relaciones entre ramas:

```python
scene.constrain(
    label.left == chart.right + 24,
    label.center_y == chart.center_y,
    (label.width <= page.width * 0.3).weak(),
)
```

Las relaciones requeridas incompatibles fallan antes del render. Las débiles
pueden consultarse mediante diagnósticos de layout.

== Responsive 16:9 y 9:16

Conserva el mismo árbol y cambia sólo el viewport. Usa `fill`, crecimiento,
tracks fr, wrapping y templates para que el contenido se redistribuya. Reserva
`absolute=True` para overlays y `stack` para capas; no uses `at()` dentro del
árbol.

Los cambios de estructura o configuración disparan reflow automáticamente.
`Text` también invalida su medida al cambiar contenido, fuente, tamaño, peso,
spacing o wrapping. `become`, `morph_to`, `step_to` y `expand_to` propagan el
reflow a layouts padres con la misma duración; no hace falta duplicar esa
lógica con `layout.reflow`. Efectos transitorios como `wiggle`, `pulse` y
`wave` no cambian la medida.
