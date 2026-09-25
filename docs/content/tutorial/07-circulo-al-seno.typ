#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Del círculo a la curva seno",
  description: "Una recta numérica y un parámetro compartido para explicar la periodicidad",
  route: "/tutorial/circulo-al-seno/",
)[

= Una fase, dos representaciones

El círculo y la onda no necesitan velocidades independientes. Ambos pueden
derivarse del mismo parámetro `theta`, de modo que la animación sea exacta y
reproducible incluso después de mover el cabezal de tiempo.

== Preparar el archivo

Este capítulo sustituye el mecanismo del anterior: el punto y el radio pasarán
a depender de `theta`. Para que `main.py` conserve un solo punto y un solo
radio, elimina primero:

- `point`, su `add_updater(...)`, `scene.wait(5.3)`, `point.remove_updater()` y
  el import de `Updater`;
- la `tracking_line` llamada `radius` y el `scene.play` del capítulo anterior que
  revelaba `point` y `radius`;
- `radius` y `point` dentro de `scene.geometry.group([...])` y en la segunda
  entrada del capítulo 4, que queda solo con `orbit.animate.create()`.

Después define el centro del círculo junto a la paleta:

```python
>>>from gaanim import BLUE, WHITE, YELLOW, Color, Scene
>>>from gaanim import Easing, stagger
>>>BACKGROUND = Color(15, 23, 42)
>>>PRIMARY = BLUE
>>>ACCENT = YELLOW
>>>MUTED = Color(148, 163, 184)
>>>scene = Scene(frame=(16, 9), background=BACKGROUND, margin=0.6)
>>>title = scene.text("Movimiento circular", role="title").fill(WHITE).move_to(0, 3.25)
>>>caption = scene.text("Un punto, un radio constante", role="subtitle").fill(MUTED).move_to(0, 2.69)
>>>orbit = scene.geometry.circle(1.5).stroke(PRIMARY, 0.05).no_fill().move_to(-4, 0)
>>>system = scene.geometry.group([orbit])
>>>formula = scene.text("$y(t) = r sin(omega t)$", role="subtitle").fill(WHITE)
>>>explanation = scene.text("La altura del punto se convertirá en una curva.", role="body").fill(MUTED)
>>>panel = scene.layout.column([formula, explanation], gap=0.225, align="start")
>>>panel.move_to(3.5, 1.875)
>>>scene.play(stagger(
>>>    title.animate.write().duration(0.8),
>>>    caption.animate.fade_in().duration(0.6),
>>>    each=0.12,
>>>))
>>>scene.play(stagger(
>>>    orbit.animate.create().duration(1.0),
>>>    each=0.15,
>>>))
>>>scene.play(stagger(
>>>    formula.animate.write().duration(0.8),
>>>    explanation.animate.fade_in().duration(0.6),
>>>    each=0.1,
>>>))
>>>scene.wait(0.8)
>>>scene.wait(0.8)
>>>scene.play([system.animate.shift_by(0.5, 0).duration(0.6).easing(Easing.SMOOTH)])
>>>scene.play([system.animate.shift_by(-0.5, 0).duration(0.6).easing(Easing.SMOOTH)])
circle_center = (-4, 0)
```

== Preparar una recta numerada con pi

```python
# continue
import math
from gaanim import Axis

axis = (
  Axis.linear(0, 3 * math.pi)
  .ticks(math.pi)
  .numbers("pi", denominator=1)
)
timeline = scene.viz.number_line(axis, length=7.5)
timeline.drawable().move_to(2.25, 0)
```

La primera coordenada está exactamente al inicio de la recta. Las marcas se
generan como `0`, `π`, `2π` y `3π`; no hay que colocar textos manualmente.

== Dibujar el seno respecto a la recta

`NumberLine.function` interpreta el resultado de la función como una distancia
perpendicular. Un valor de uno ocupa `normal_scale` unidades locales.

```python
# continue
import math
from gaanim import computed

radius = 1.5
theta = scene.viz.parameter(0.0)
sine_curve = timeline.function(
  lambda value: math.sin(value),
  normal_scale=radius,
  reveal=theta,
)
sine_curve.stroke(PRIMARY, 0.04).no_fill()
```

La función usa Python normal. Rust resuelve sus entradas y cachea cada snapshot
numérico exacto para que reproducción y seek produzcan la misma geometría.

== Compartir el ángulo

```python
# continue
circle_ref = scene.geometry.polar_point(circle_center, radius, theta)
circle_dot = scene.geometry.dot(0.125).fill(ACCENT).follow(circle_ref)

wave_ref = timeline.point_ref(
  theta,
  normal_offset=computed(lambda angle: radius * math.sin(angle), inputs=[theta]),
)
wave_dot = scene.geometry.dot(0.1).fill(ACCENT).follow(wave_ref)
```

`point_ref` devuelve un punto lógico, no una entidad visible adicional. Sus
coordenadas se evalúan en el marco local de la recta; si la recta se traslada,
rota o escala, el punto y la curva la acompañan.

== Mostrar la correspondencia

```python
# continue
radius_line = scene.geometry.tracking_line(circle_center, circle_ref)
radius_line.stroke(MUTED, 0.025).no_fill()
projection = scene.geometry.tracking_line(circle_ref, wave_ref)
projection.stroke(ACCENT, 0.025).no_fill()

scene.play([
  circle_dot.animate.fade_in().duration(0.3),
  radius_line.animate.fade_in().duration(0.3),
  timeline.animate.create().duration(0.8),
  sine_curve.animate.fade_in().duration(0.01),
  wave_dot.animate.fade_in().duration(0.3),
  projection.animate.fade_in().duration(0.3),
])
scene.play([
  theta.animate.set(3 * math.pi).duration(8).easing(Easing.LINEAR),
])
```

Ahora el radio, el punto circular, la posición horizontal y la altura de la
onda proceden del mismo valor. No existe deriva entre una velocidad angular y
otra medida en unidades de la escena por segundo. `reveal=theta` usa el ángulo como extremo
exacto del dominio visible: la onda se forma delante del espectador y su último
punto coincide con el punto proyectado, sin aproximar por longitud de arco.

#idea[
La misma construcción sirve para señales, fase, Fourier y diagramas temporales:
una escala unidimensional aporta la coordenada principal y `normal_offset`
expresa la magnitud perpendicular.
]

#checkpoint[
Comprueba los estados `theta = 0`, `π/2`, `π`, `2π` y `3π`. El punto de la onda
debe coincidir con la recta en los múltiplos de pi y alcanzar `±radius` en los
cuartos de vuelta correspondientes.
]

== El ejemplo canónico

El repositorio de Gaanim incluye una versión ejecutable completa, con medidas
ligeramente distintas, en `examples/manual_movimiento_circular.py`.
]
