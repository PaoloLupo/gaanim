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
  [`scene.circle(r)`], [`scene.geometry.circle(radius)`],
  [`scene.equation(...)`], [`scene.text.equation(...)`],
  [`scene.row(children)`], [`scene.layout.row(children)`],
  [`scene.image(path)`], [`scene.media.image(path)`],
  [`scene.parameter(value)`], [`scene.viz.parameter(value)`],
  [`scene.badge(text)`], [`scene.slides.badge(text)`],
  [`scene.force_at(...)`], [`scene.mechanics.force_at(...)`],
  [`scene.assets_dir(path)`], [`scene.assets.assets_dir(path)`],
  [`dot.at(x, y)`], [`dot.move_to(x, y)`],
  [`dot.move_to(x, y)` (animado)], [`dot.animate.move_to(x, y)`],
  [`tracker.animate_to(v)`], [`tracker.animate.set(v)`],
  [`space.animate_view(x, y)`], [`space.animate.view_to(x, y)`],
  [`matrix.scale_by(k)` (álgebra)], [`matrix.scalar_multiply(k)`],
  [`AnimationGroup(a, b)`], [`parallel(a, b)`],
  [`Succession(a, b)`], [`sequence(a, b)`],
  [`LaggedStart(a, b, lag=t)`], [`stagger(a, b, each=t)`],
  [`scene.play(items, lag=t)`], [`scene.play(stagger(*items, each=t))`],
  [`.smooth()`], [`.easing(Easing.SMOOTH)`],
  [`.linear()`], [`.easing(Easing.LINEAR)`],
  [`.spring()`], [`.easing(Easing.spring(stiffness=90, damping=12))`],
  [`.ease("spring")`], [`.easing(Easing.spring(stiffness=300, damping=20))`],
  [`.steps(n)`], [`.easing(Easing.steps(n))`],
  [`scene.play(items, rate="linear")`], [`scene.play(items, easing=Easing.LINEAR)`],
  [`obj.animate.write(0.8)`], [`obj.animate.write().duration(seconds=0.8)`],
  [`obj.at_anchor(x, y, anchor)`], [`obj.move_to(x, y, anchor=anchor)`],
  [`obj.color(value)`], [`obj.fill(value)`],
)

Los setters directos son cortes reversibles en el cursor actual y no consumen
tiempo. `animate` nunca se invoca: es una propiedad de solo lectura. El `Anim`
resultante es una descripción pura y `Scene.play` valida el lote completo antes
de incorporarlo al timeline.

`scene.text("Hola")` no cambia. Desde 0.2, `scene.text` es una capacidad
invocable `Typography` que también expone `equation`, `typst`, `measure` y
`code`.

`Easing` y `EasingCurve` se importan desde `gaanim`. Son objetos tipados e
inmutables, por lo que el autocompletado enumera presets y familias y los
nombres desconocidos fallan en vez de degradarse a otra curva.

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
scene.play([page.animate.fade_in()])
scene.render()
```
