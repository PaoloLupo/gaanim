#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Migrar de 0.1 a 0.2",
  description: "Separación de Scene en capacidades enfocadas",
  route: "/guides/migration-0-2/",
  updated: datetime.today().display(),
  code-langs: (),
)

Gaanim 0.2 convierte `Scene` en el orquestador del tiempo y la presentación.
Las fábricas conservan sus firmas y comportamiento, pero se acceden mediante
una capacidad ligada a la misma escena. No existen aliases para los métodos
planos retirados.

= Equivalencias

#table(
  columns: (1fr, 1fr),
  [*0.1*], [*0.2*],
  [`scene.circle(r)`], [`scene.geometry.circle(r)`],
  [`scene.equation(...)`], [`scene.text.equation(...)`],
  [`scene.row(children)`], [`scene.layout.row(children)`],
  [`scene.image(path)`], [`scene.media.image(path)`],
  [`scene.parameter(value)`], [`scene.viz.parameter(value)`],
  [`scene.badge(text)`], [`scene.slides.badge(text)`],
  [`scene.force_at(...)`], [`scene.mechanics.force_at(...)`],
  [`scene.assets_dir(path)`], [`scene.assets.assets_dir(path)`],
)

`scene.text("Hola")` no cambia. Desde 0.2, `scene.text` es una capacidad
invocable `Typography` que también expone `equation`, `typst`, `measure` y
`code`.

= Qué permanece en Scene

`play`, `wait`, `stop`, `fade_out_all`, `segment`, `link`, `reuse`, `persist`,
`release`, `render` y `snapshots` continúan directamente en `Scene`. `canvas`
y `camera` mantienen sus contratos anteriores.

= Ejemplo completo

```python
from gaanim import BLUE, Scene

scene = Scene(1280, 720)
circle = scene.geometry.circle(96).fill(BLUE)
title = scene.text("Capacidades", role="title")
page = scene.layout.column([title, circle], within="safe", gap=24)
scene.play([page.fade_in()])
scene.render()
```
