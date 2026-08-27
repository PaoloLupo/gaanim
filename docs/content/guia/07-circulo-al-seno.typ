#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Del círculo a la curva seno",
  description: "Una recta numérica y un parámetro compartido para explicar la periodicidad",
  route: "/guia/circulo-al-seno/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= Una fase, dos representaciones

El círculo y la onda no necesitan velocidades independientes. Ambos pueden
derivarse del mismo parámetro `theta`, de modo que la animación sea exacta y
reproducible incluso después de mover el cabezal de tiempo.

== Preparar una recta numerada con pi

```python
import math
from gaanim import Axis

axis = (
  Axis.linear(0, 3 * math.pi)
  .ticks(math.pi)
  .numbers("pi", denominator=1)
)
timeline = scene.number_line(axis, length=600)
timeline.drawable().at(180, -20)
```

La primera coordenada está exactamente al inicio de la recta. Las marcas se
generan como `0`, `π`, `2π` y `3π`; no hay que colocar textos manualmente.

== Dibujar el seno respecto a la recta

`NumberLine.function` interpreta el resultado de la función como una distancia
perpendicular. Un valor de uno ocupa `normal_scale` unidades locales.

```python
import math
from gaanim import computed

radius = 120
theta = scene.parameter(0.0)
sine_curve = timeline.function(
  lambda value: math.sin(value),
  normal_scale=radius,
  reveal=theta,
)
sine_curve.stroke(PRIMARY, 3).no_fill()
```

La función usa Python normal. Rust resuelve sus entradas y cachea cada snapshot
numérico exacto para que reproducción y seek produzcan la misma geometría.

== Compartir el ángulo

```python
circle_ref = scene.polar_point(circle_center, radius, theta)
circle_dot = scene.dot(10).fill(ACCENT).follow(circle_ref)

wave_ref = timeline.point_ref(
  theta,
  normal_offset=computed(lambda angle: radius * math.sin(angle), inputs=[theta]),
)
wave_dot = scene.dot(8).fill(ACCENT).follow(wave_ref)
```

`point_ref` devuelve un punto lógico, no una entidad visible adicional. Sus
coordenadas se evalúan en el marco local de la recta; si la recta se traslada,
rota o escala, el punto y la curva la acompañan.

== Mostrar la correspondencia

```python
radius_line = scene.tracking_line(circle_center, circle_ref)
projection = scene.tracking_line(circle_ref, wave_ref)

scene.play([
  timeline.create().duration(0.8),
  sine_curve.fade_in().duration(0.01),
  wave_dot.fade_in().duration(0.3),
  projection.fade_in().duration(0.3),
])
scene.play([
  theta.animate_to(3 * math.pi, duration=8),
])
```

Ahora el radio, el punto circular, la posición horizontal y la altura de la
onda proceden del mismo valor. No existe deriva entre una velocidad angular y
otra medida en píxeles por segundo. `reveal=theta` usa el ángulo como extremo
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

La versión ejecutable completa está en `examples/manual_movimiento_circular.py`.
]
