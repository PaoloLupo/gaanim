#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Temas avanzados",
  description: "Reactividad, visualización, 3D, presentaciones y pruebas visuales",
  route: "/manual/avanzado/",
  updated: datetime.today().display(),
  code-langs: (),
)

= Reactividad

La reactividad describe dependencias que deben seguir siendo correctas al
reproducir, pausar, exportar o saltar directamente a cualquier instante. No es
un sinónimo de «ejecutar Python en cada fotograma».

Elige la herramienta más declarativa que pueda expresar la relación:

1. Usa `Parameter` para una magnitud invisible y animable.
2. Usa `computed()` con funciones Python puras e `inputs=` explícitos para
   derivar otras magnitudes.
3. Usa `PointRef`, `AnchorPoint` y fábricas como `bar_between` para geometría
   que depende de posiciones.
4. Usa bindings cuando una propiedad de un objeto copie o transforme otra.
5. Usa `drive_from_samples` para registros medidos.
6. Reserva `add_updater_fn` para simulaciones o comportamientos que necesitan
   estado incremental.

```python
from math import pi
from gaanim import GOLD, Scene

scene = Scene(frame=(16, 9))
theta = scene.viz.parameter(0.0)
tip = scene.geometry.polar_point((0, 0), 180, theta)
radius = scene.mechanics.bar_between((0, 0), tip).stroke(GOLD, 6)
label = scene.viz.readout(lambda value: value, inputs=[theta], label="$theta$", format=".2f")
label.follow(tip, offset=(0, 28))

scene.play([radius.animate.create().duration(0.5), label.animate.fade_in().duration(0.3)])
scene.play([theta.animate.set(2 * pi).duration(4.0)])
scene.render()
```

Aquí `theta` es la única fuente de verdad. El punto, la barra y la lectura se
evalúan a partir de ella. Al buscar el segundo 2.7 no es necesario reproducir
los 2.7 segundos anteriores.

El ejemplo canónico es `examples/sine_curve_unit_circle.py`. Combina
`Updater.orbit`, `tracking_line`, `bind_y_from` y `traced_path`, e incluye
capturas deterministas para regresión visual.

=== Migración desde expresiones simbólicas

`gaanim_expr`, `_Expr` y `gaanim.math` fueron eliminados sin adaptadores. Usa
estas equivalencias:

- `gm.sin(parameter)` pasa a `computed(lambda value: math.sin(value), inputs=[parameter])`.
- `lambda x: amplitude * gm.sin(x)` pasa a
  `lambda x, amplitude: amplitude * math.sin(x)` con `inputs=[amplitude]`.
- Un readout derivado declara sus entradas:
  `scene.viz.readout(lambda radius: math.pi * radius**2, inputs=[radius])`.
- Endpoints, cámara y slots escalares aceptan `float`, `Parameter`, `Variable`
  o `Computed`; pasar un parámetro directamente conserva la identidad.
- `plot(function, derivative=2)` pasa a
  `plot(function, derivative=second_derivative)`. La derivada debe tener la
  misma firma y los mismos `inputs`; no existe diferenciación ni aproximación
  numérica implícita.
- El tiempo se declara con `inputs=[scene.viz.time]`.

El contrato determinista exige callbacks síncronos y puros. Con el mismo script,
entradas y entorno Python, reproducción, rewind, seeks directos y exportación
producen el mismo resultado. El reloj del sistema, I/O, azar global y estado
mutable quedan fuera del contrato; deriva cualquier azar de una semilla
explícita recibida por `inputs`. No se promete igualdad bit a bit entre
plataformas distintas.

Gaanim valida la aridad y pertenencia a la escena, resuelve un snapshot numérico
en Rust y cachea por los bits exactos de las entradas. Excepciones, formas de
salida incorrectas y valores no finitos no reutilizan el último valor válido:
producen gaps geométricos, `invalid` en readouts o ausencia de binding en
endpoints y cámara.

=== Cuándo necesitas un updater con estado

Una simulación incremental —por ejemplo, velocidad integrada bajo gravedad— no
puede reducirse siempre a una expresión del tiempo. En ese caso proporciona
`reset` y un `fixed_dt` positivo junto con `add_updater_fn`. Gaanim podrá
restaurar el estado y repetir subpasos constantes durante seeks y exportación.
Sin ese par, usa el updater solo para comportamiento ligero basado en el tiempo
absoluto.

== Visualización de datos

Las escalas, ejes, espacios tipados y gráficos convierten datos en drawables
normales. Esto permite aplicarles Layout, temas y animaciones con claves
estables. Consulta #link("/api/visualization/")[Visualización] para elegir entre
una composición científica y un `ChartSpec` declarativo.

La pregunta decisiva es qué representa cada dato. Si cada fila es una
observación y quieres asignar columnas a posición, color o tamaño, empieza con
`ChartSpec`. Si trabajas con dominios continuos, funciones, integrales o campos,
empieza con un espacio tipado como `Cartesian2D`.

== Escenas 3D

Gaanim dispone de geometría, cámara, iluminación y modelos glTF 3D. Mantén las
unidades y la orientación coherentes; valida primero una escena estática y
añade movimientos de cámara después.

== Producción

- #link("/guides/projects/")[Proyectos] explica manifiesto, recursos y salidas.
- #link("/guides/slides/")[Presentaciones] cubre segmentos, notas y paradas.
- #link("/guides/visual-regression/")[Regresión visual] enseña capturas,
  comparación y aprobación de cambios.
- #link("/api/")[Referencia de la API] contiene firmas y contratos técnicos.

== Criterio de dominio

Una escena avanzada sigue siendo mantenible cuando separa contenido, estilo,
composición y tiempo; usa claves estables para transformaciones; evita
coordenadas manuales en estructuras editoriales; y tiene al menos una forma
repetible de validar su salida.
