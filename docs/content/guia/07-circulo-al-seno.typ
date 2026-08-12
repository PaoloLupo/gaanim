#import "../../components/section.typ": docs-chapter
#import "../../components/tutorial.typ": checkpoint, idea

#docs-chapter(
  title: "Del círculo a la curva seno",
  description: "Bindings, líneas reactivas y trazado de una trayectoria",
  route: "/guia/circulo-al-seno/",
  updated: datetime.today().display(),
  code-langs: (),
)[

= Convertir altura en curva

El punto que gira contiene dos señales: su coordenada X y su coordenada Y. Para
dibujar el seno conservaremos Y y haremos que una copia avance hacia la derecha.

== Preparar el eje

Añade un eje horizontal desde el comienzo de la zona de dibujo:

```python
curve_start_x = -160.0
x_axis = scene.line(curve_start_x, 0, 480, 0).stroke(MUTED, 2)

for i, label in enumerate(["pi", "2 pi", "3 pi", "4 pi"]):
    scene.text(f"${label}$").fill(MUTED).scaled(0.55).at(-40 + 120 * i, -32)
```

Las etiquetas usan el mismo sistema matemático que la fórmula.

== Crear el punto proyectado

```python
projection = scene.dot(5).fill(ACCENT).at(curve_start_x, 0)
projection.bind_y_from(point)
projection.add_updater(Updater.advance_x(speed=65.0))
```

`bind_y_from(point)` copia la altura del punto orbitante. El updater solo hace
avanzar X. Al combinar ambas reglas, `projection` recorre una onda.

== Mostrar la correspondencia

Una línea reactiva une ambos puntos:

```python
projection_line = scene.tracking_line(point, projection)
projection_line.stroke(ACCENT, 2).no_fill()
```

La línea hace visible que cada altura del círculo se transfiere al gráfico.

== Conservar el recorrido

```python
sine_curve = scene.traced_path(
    projection,
    max_points=1200,
    min_distance=1.0,
)
sine_curve.stroke(PRIMARY, 3).no_fill()
```

`traced_path` acumula muestras de la posición. `min_distance` evita registrar
puntos casi idénticos; `max_points` limita el crecimiento de la trayectoria.

Revela todas las piezas antes de dejar avanzar el tiempo:

```python
scene.play([
    x_axis.create().duration(0.6),
    projection.fade_in().duration(0.3),
    projection_line.fade_in().duration(0.3),
    sine_curve.fade_in().duration(0.3),
], lag=0.08)

scene.wait(8.0)
point.remove_updater()
projection.remove_updater()
```

#idea[
La curva no se calcula como una lista de valores preparada en Python. Surge de
relaciones visuales que Gaanim evalúa juntas: órbita, binding, avance, línea y
traza.
]

== Leer la escena terminada

La animación tiene ahora una gramática clara:

1. El círculo introduce el movimiento periódico.
2. El radio muestra la fase actual.
3. La línea horizontal transfiere la altura.
4. La traza convierte esa altura en historia.
5. La fórmula nombra la relación que el espectador acaba de observar.

#checkpoint[
La proyección debe conservar exactamente la altura del punto, avanzar a
velocidad constante y dejar una curva suave. Si se sale del viewport, reduce
`speed` o la duración de `wait`.
]

== El ejemplo canónico

La versión ejecutable completa queda en `examples/manual_movimiento_circular.py`.
El siguiente capítulo la convertirá en un proyecto reproducible y exportable.
]
