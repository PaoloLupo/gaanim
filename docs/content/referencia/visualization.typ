#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry
#import "../../components/tutorial.typ": experimental

#show: docs-chapter.with(
  title: "Visualización",
  description: "scene.viz: espacios de coordenadas, funciones, cálculo, datos, valores reactivos, gráficos declarativos y campos vectoriales",
  route: "/referencia/visualization/",
  nav: "Visualización",
)

= Visualización

`scene.viz` reúne lo que convierte números en imágenes: espacios de coordenadas
tipados, funciones y construcciones de cálculo, series de datos, valores
reactivos, gráficos declarativos y campos vectoriales. Todo trabaja en
coordenadas de datos: al mover o escalar el espacio, curvas, puntos y etiquetas
lo acompañan. Las matrices están en #link("/referencia/matrices/")[Matrices].

```python
# show-code: true
import math
from gaanim import Axis, BLUE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
plane = scene.viz.cartesian_2d(Axis.linear(-6, 6).ticks(1), Axis.linear(-3, 3).ticks(1))
amplitude = scene.viz.parameter(1.0)
wave = plane.plot(lambda x, a: a * math.sin(x), inputs=[amplitude]).stroke(BLUE, 0.05)
scene.play([plane.animate.create(), wave.animate.create()], duration=1.0)
scene.play([amplitude.animate.set(2.5).duration(1.0)])
# output: preview.webp
scene.render()
```

Elige la herramienta según los datos:

- Un *espacio tipado* (`cartesian_2d`, `polar`, `complex`, `number_line`) cuando
  las coordenadas tienen significado matemático continuo: funciones, cálculo,
  campos y geometría propia.
- `ChartSpec` cuando cada fila de una tabla es una observación: marcas,
  codificaciones, escalas y transiciones con identidad estable.

== Espacios cartesianos

Un plano con ejes, rejilla, marcas y números que conoce su escala. Todas las
curvas y marcas que crea viven en sus coordenadas de datos.

#api-entry(
  name: "Visualization.cartesian_2d",
  kind: "factory",
  params: ((name: "x / y", type: "Axis", default: none, desc: [Ejes con dominio, marcas y títulos (ver «Ejes»).]), (name: "width / height", type: "float | None", default: "None", desc: [Tamaño en unidades de escena; por defecto, relativo al área segura.]), (name: "grid / axes / ticks / numbers / labels", type: "bool", default: "True", desc: [Interruptores globales de cada capa.]), (name: "x_* / y_*", type: "bool | None", default: "None", desc: [Sustituyen el interruptor global en un eje; `None` lo hereda.])),
  desc: [Devuelve un `Cartesian2D` (un `CoordinateSpace`). `numbers` controla el texto de las marcas y `labels`, los títulos de `Axis.label`. Una capa desactivada sigue disponible como un `Drawable` vacío, así que el conjunto de capas es estable para componer y animar. Los números negativos usan el signo menos tipográfico (`−2`). Los colores salen del tema (`axes/axis`, `axes/grid`, `axes/numbers`…).],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(
  Axis.linear(-4, 4).ticks(1).label("x"),
  Axis.linear(-2, 2).ticks(1).label("y"),
  grid=False, x_grid=True,
  numbers=False, y_numbers=True,
)
scene.play([plane.animate.write().duration(1.2)])
```
]

#api-entry(
  name: "CoordinateSpace.plot",
  kind: "method",
  params: ((name: "function", type: "Callable[..., float]", default: none, desc: [Recibe la coordenada x y después los valores de `inputs`.]), (name: "domain", type: "(float, float) | None", default: "None", desc: [Intervalo de x; sin él cubre la unión de todas las ventanas de `view_to` del plano.]), (name: "samples / tolerance", type: "int | None / float", default: "None / 0.0075", desc: [Muestras fijas, o muestreo adaptativo con esa tolerancia.]), (name: "derivative", type: "Callable | None", default: "None", desc: [Derivada explícita con la misma firma; no hay diferenciación implícita.]), (name: "inputs", type: "Sequence[Parameter | Variable | Computed | TimeInput]", default: "()", desc: [Dependencias reactivas en orden.])),
  desc: [Gráfica de `y = f(x)`. Con `inputs`, la curva se regenera cuando cambia el valor resuelto por la línea de tiempo, también al hacer seek. Se muestrea con más detalle si el plano tiene vistas ampliadas con `view_to`.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
curve = plane.plot(math.sin, domain=(-math.pi, math.pi)).stroke(BLUE, 0.04)
```
]

#api-entry(
  name: "CoordinateSpace.parametric",
  kind: "method",
  desc: [Curva paramétrica `t → (x, y)` sobre `domain`, con el mismo muestreo y las mismas `inputs` que `plot`.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-3, 3), Axis.linear(-2, 2))
lissajous = plane.parametric(lambda t: (2 * math.sin(3 * t), 1.5 * math.sin(2 * t)), (0, math.tau))
```
]

#api-entry(
  name: "CoordinateSpace.implicit",
  kind: "method",
  desc: [Curva de nivel cero de `f(x, y)`, muestreada en una rejilla de `resolution` celdas.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-3, 3), Axis.linear(-2, 2))
ellipse = plane.implicit(lambda x, y: x**2 / 4 + y**2 - 1)
```
]

#api-entry(
  name: "CoordinateSpace.contour",
  kind: "method",
  desc: [Curvas de nivel de `f(x, y)` para cada valor de `levels`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-3, 3), Axis.linear(-2, 2))
rings = plane.contour(lambda x, y: x * x + y * y, levels=[0.5, 1, 2, 3])
```
]

#api-entry(
  name: "CoordinateSpace.layer",
  kind: "method",
  desc: [Devuelve una capa semántica como `Drawable` para darle estilo o animarla por separado: `grid`, `major_grid`, `minor_grid`, `axis`, `axes`, `ticks`, `numbers` o `labels`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-4, 4).ticks(1), Axis.linear(-2, 2).ticks(1))
plane.layer("grid").opacity(0.3)
scene.play([plane.layer("axes").animate.create(), plane.layer("numbers").animate.fade_in()])
```
]

#api-entry(
  name: "CoordinateSpace.coord / CoordinateRef.place",
  kind: "method",
  desc: [Coordenada simbólica `(x, y)` del espacio. Pásala a `Drawable.at_coordinate` (o a `CoordinateRef.place`) para colocar un objeto como hijo del espacio, que seguirá sus cambios de vista.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
peak = scene.geometry.dot(0.1).fill(GOLD).at_coordinate(plane.coord(1, 1.5))
label = plane.coord(1, 1.8).place(scene.text("máximo", role="label"))
```
]

#api-entry(
  name: "CoordinateSpace.data_to_scene",
  kind: "method",
  desc: [Devuelve un `PointRef` de escena en el dato `(x, y)` para objetos que no son hijos del espacio (anotaciones, flechas, conectores). Se resuelve en cada fotograma con la ventana de `view_to` vigente y con la posición, escala y rotación del plano. Datos no finitos o fuera de escala lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>plane = scene.viz.cartesian_2d(Axis.linear(-6, 6), Axis.linear(-3, 3))
peak = plane.data_to_scene(2, 2)
note = scene.text("máximo").follow(peak, offset=(0.6, 0.4))
arrow = scene.geometry.connector(note, peak)
scene.play([plane.animate.view_to((0, 4), (0, 3)).duration(1.2)])
```
]

#api-entry(
  name: "CoordinateSpace.data_to_local / local_to_data",
  kind: "method",
  signature: "data_to_local(x, y) -> tuple[float, float] · local_to_data(x, y) -> tuple[float, float]",
  desc: [Convierten entre datos y coordenadas locales del espacio con la vista vigente en el cursor.],
  none,
)

#api-entry(
  name: "CoordinateSpace.view_to",
  kind: "method",
  params: ((name: "x_domain / y_domain", type: "(float, float)", default: none, desc: [Nueva ventana de datos; finita y creciente.]),),
  desc: [Cambia la ventana de datos en el cursor; `plane.animate.view_to(...)` la anima. El área de trazado no se mueve: ejes, rejillas, marcas y datos se recortan a ella, los títulos quedan fijos y los números y los marcadores de `scatter_data` conservan su tamaño. Los trazos mantienen su grosor. Los ejes con marcas automáticas regeneran marcas y rejilla con un fundido cruzado; los de paso fijo (`.ticks(1)`) lo conservan. Aplica los estilos de `layer(...)` antes del primer cambio de vista. Solo ejes lineales o temporales; otros lanzan `ValueError`.],
)[
```python
# show-code: true
import math
from gaanim import Axis, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
plane = scene.viz.cartesian_2d(Axis.linear(-6, 6), Axis.linear(-3, 3))
curve = plane.plot(lambda x: math.sin(2 * x)).stroke(GOLD, 0.05)
scene.play([plane.animate.create(), curve.animate.create()], duration=0.6)
scene.play([plane.animate.view_to((-1.5, 1.5), (-1.2, 1.2)).duration(1.2)])
# output: preview.webp
scene.render()
```
]

Las marcas de datos de un espacio cartesiano (`plot`, `parametric`,
`scatter_data`, campos…) se recortan siempre al área de trazado, haya o no
`view_to`. `mark.no_clip()` deja que una marca concreta sobresalga. No se
recortan los objetos colocados con `at_coordinate` ni las marcas de
`scene.viz.chart`.

#api-entry(
  name: "CoordinateSpace.animate / drawable / move_to / scale_to / rotate_to / CoordinateSpaceAnimation.create / write / fade_in / fade_out / move_to / scale_to / rotate_to / view_to",
  kind: "property",
  signature: "space.animate -> CoordinateSpaceAnimation · space.drawable() -> Drawable · space.move_to(x, y) · space.scale_to(factor) · space.rotate_to(radians) -> CoordinateSpace",
  desc: [Proxy `CoordinateSpaceAnimation`: `create()` traza las capas con trazos independientes de la resolución, `write()` construye a la vez ejes, rejillas, marcas, números y títulos, y también ofrece `fade_in`, `fade_out`, `move_to`, `scale_to`, `rotate_to` y `view_to`. `move_to`, `scale_to` y `rotate_to` del propio espacio devuelven el espacio para encadenar, y `drawable()` da el objeto raíz para layout y estilo.],
  none,
)

== Ejes

`Axis` describe dominio, escala, marcas, números, título, cruce y estilo de un
eje. Es inmutable: cada método devuelve una copia.

#api-entry(
  name: "Axis.linear / log / symlog / power / time / category",
  kind: "method",
  signature: "linear(min, max) · log(min, max, base=10) · symlog(min, max, *, base=10, threshold=1) · power(min, max, exponent) · time(min_ts, max_ts) · category(values)",
  desc: [Constructores estáticos. Elige según el significado: `linear` para diferencias uniformes, `log` para órdenes de magnitud positivos, `symlog` para datos con signo alrededor de cero, `power` para potencias, `time` para marcas de tiempo en segundos y `category` para grupos discretos.],
)[
```python
>>>from gaanim import *
frequency = Axis.log(0.1, 1000, base=10).ticks(10).label("frecuencia")
signed = Axis.symlog(-100, 100, threshold=1)
groups = Axis.category(["A", "B", "C"])
```
]

#api-entry(
  name: "Axis.ticks",
  kind: "method",
  desc: [Marcas principales cada `step` unidades de datos. Un paso fijo se conserva en cualquier vista.],
  none,
)

#api-entry(
  name: "Axis.auto_ticks",
  kind: "method",
  desc: [Vuelve al paso automático, que se regenera con cada `view_to`.],
  none,
)

#api-entry(
  name: "Axis.minor_ticks",
  kind: "method",
  desc: [Divide cada intervalo principal en `subdivisions` partes con marcas menores.],
  none,
)

#api-entry(
  name: "Axis.numbers",
  kind: "method",
  desc: [Formato de los números: `auto`, `fixed`, `scientific`, `percent`, `fraction`, `pi` o `datetime`. `precision` se aplica a `fixed`, `scientific` y `percent`; `denominator`, a `fraction` y `pi`; `pattern`, a `datetime`. `fraction` y `pi` redondean al múltiplo de `1/denominator` más cercano y simplifican la fracción: con `denominator=2`, `pi` rotula `π/2`, `π`, `3π/2` y `2π`, y `fraction` rotula `1/2`, `1` y `3/2`.],
)[
```python
>>>from gaanim import *
>>>import math
angle = Axis.linear(0, 2 * math.pi).ticks(math.pi / 2).numbers("pi", denominator=2)
share = Axis.linear(0, 1).ticks(0.25).numbers("percent", precision=0)
```
]

#api-entry(
  name: "Axis.label",
  kind: "method",
  desc: [Título del eje, separado de las marcas. `position="end"` (predeterminado) lo pone tras el extremo positivo y `"start"` tras el negativo; `"center"` (o `"middle"`, `"mid"`) centra x bajo sus números e y a la izquierda, girado 90°. En un eje vertical, `"top"` y `"bottom"` son alias de los extremos.],
)[
```python
>>>from gaanim import *
y = Axis.linear(0, 1).label("valor relativo", position="center")
```
]

#api-entry(
  name: "Axis.crossing",
  kind: "method",
  desc: [Dónde cruza este eje al otro: `"auto"`, `"zero"`, `"min"`, `"max"` o un valor de datos.],
  none,
)

#api-entry(
  name: "Axis.style",
  kind: "method",
  desc: [Sustituye solo las propiedades indicadas (`color`, `width`, `tick_length`, `tick_width`, `tick_color`, `number_color`, `label_color`); el resto se hereda del tema. Así, cambiar solo `width` conserva el color del tema.],
)[
```python
>>>from gaanim import *
x = Axis.linear(-3, 3).ticks(1).style(color=WHITE, width=0.03)
```
]

#api-entry(
  name: "Axis.domain",
  kind: "property",
  desc: [Tupla `(mínimo, máximo)` del eje.],
  none,
)

== Construcciones de cálculo

Tangentes, secantes, áreas y sumas de Riemann sobre funciones del espacio.

#api-entry(
  name: "CoordinateSpace.tangent",
  kind: "method",
  desc: [Recta tangente a `function` en `x`. `length` fija la longitud visible y `dx` el paso de la derivada numérica.],
)[
```python
# show-code: true
from gaanim import Axis, BLUE, GOLD, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
plane = scene.viz.cartesian_2d(Axis.linear(-1, 5).ticks(1), Axis.linear(-1, 3).ticks(1))
f = lambda x: 0.15 * x * x + 0.3
curve = plane.plot(f).stroke(BLUE, 0.04)
area = plane.riemann_sum(f, (0, 4), rectangles=8).fill(GOLD).opacity(0.6)
touch = plane.tangent(f, 3, length=3).stroke(GOLD, 0.04)
scene.play([plane.animate.create(), curve.animate.create()], duration=0.6)
scene.play([area.animate.fade_in(), touch.animate.create()], duration=0.8)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "CoordinateSpace.normal",
  kind: "method",
  desc: [Recta normal a `function` en `x`, con las mismas opciones que `tangent`.],
  none,
)

#api-entry(
  name: "CoordinateSpace.secant",
  kind: "method",
  desc: [Recta secante que une los puntos de `function` en `x0` y `x1`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>plane = scene.viz.cartesian_2d(Axis.linear(-1, 5), Axis.linear(-1, 3))
chord = plane.secant(lambda x: 0.15 * x * x, 1, 4)
```
]

#api-entry(
  name: "CoordinateSpace.area_under",
  kind: "method",
  desc: [Región rellena entre `function` y `baseline` sobre `domain`.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
>>>plane = scene.viz.cartesian_2d(Axis.linear(0, 4), Axis.linear(-1, 1.5))
area = plane.area_under(math.sin, (0, math.pi)).fill(BLUE).opacity(0.5)
```
]

#api-entry(
  name: "CoordinateSpace.riemann_sum",
  kind: "method",
  desc: [Rectángulos de una suma de Riemann: `rectangles` de ancho igual y altura evaluada según `method` (`left`, `midpoint`, `middle` o `right`).],
  none,
)

#api-entry(
  name: "CoordinateSpace.projections",
  kind: "method",
  desc: [Líneas de proyección desde el punto `(x, y)` hasta los dos ejes.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>plane = scene.viz.cartesian_2d(Axis.linear(0, 4), Axis.linear(0, 3))
guides = plane.projections(2.5, 1.8)
```
]

== Series de datos

`plot_data` y `scatter_data` dibujan series `(xs, ys)` directamente en las
coordenadas del espacio: si el plano se mueve o escala, la serie lo acompaña.

#api-entry(
  name: "CoordinateSpace.plot_data",
  kind: "method",
  params: ((name: "xs, ys", type: "Sequence[float | None]", default: none, desc: [Series de igual longitud; `None` es una muestra ausente.]), (name: "step", type: "bool", default: "False", desc: [Gráfica escalonada.]), (name: "baseline", type: "float | None", default: "None", desc: [Con un valor, rellena el área hasta esa línea base.]), (name: "policy", type: "str", default: "\"gap\"", desc: [Muestras no finitas: `gap` corta la línea, `drop` las omite y `error` lanza un error.]), (name: "color / width", type: "Color | None / float | None", default: "None", desc: [Sustituyen el trazo del tema.])),
  desc: [Curva de datos medidos. Para recorrer muestras con el tiempo de la escena usa `Parameter.drive_from_samples`.],
)[
```python
>>>import math
>>>times = [i * 0.1 for i in range(301)]
>>>accel = [0.3 * math.sin(2 * t) * math.exp(-t / 12) for t in times]
from gaanim import CYAN, Axis, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
plane = scene.viz.cartesian_2d(
  Axis.linear(0, 30).ticks(5).label("tiempo (s)"),
  Axis.linear(-0.4, 0.4).ticks(0.2).label("aceleración (g)"),
  width=12,
  height=4.75,
)
curve = plane.plot_data(times, accel, color=CYAN, width=0.05)
scene.play([plane.animate.create().duration(0.85), curve.animate.create().duration(2.2)])
```
]

#api-entry(
  name: "CoordinateSpace.scatter_data",
  kind: "method",
  desc: [Puntos de radio `radius` (unidades de escena) en las coordenadas de datos; `view_to` mueve los centros sin deformarlos. Úsalo para destacar muestras sobre una curva de `plot_data`.],
)[
```python
# continue
>>>from gaanim import GOLD
>>>peak_times = [0.8, 3.9, 7.1]
>>>peak_values = [0.28, -0.22, 0.17]
peaks = plane.scatter_data(peak_times, peak_values, radius=0.07, color=GOLD)
scene.play([peaks.animate.fade_in()])
```
]

== Valores reactivos

Magnitudes que conducen otras propiedades. Rust resuelve un snapshot numérico
estable para cada instante y llama después a las funciones Python declaradas,
así que el seek es exacto.

Elige el tipo según su papel:

- `Parameter`: magnitud invisible que conduce otras propiedades.
- `Variable`: magnitud que además se muestra como `etiqueta = valor`.
- `Readout`: muestra una cantidad derivada sin ser su fuente.
- `computed(...)`: escalar calculado a partir de otros.

Las funciones reciben primero las coordenadas y después los valores de
`inputs=[...]`, en el orden declarado. Pueden usar `math`, funciones auxiliares y
control de flujo, pero deben ser puras y devolver un escalar finito. Los
setters absolutos (`move_to`, `rotate_to`, `scale_to`, `opacity` y sus
variantes 3D) aceptan estas fuentes directamente.

#api-entry(
  name: "Visualization.parameter",
  kind: "factory",
  desc: [Escalar invisible y animable. Conduce curvas reactivas, geometría (`point_on_curve`, `always_redraw_arc`…) y setters; cada objeto conducido necesita su propia animación de entrada. Valores no finitos lanzan `ValueError`.],
)[
```python
import math
from gaanim import Axis, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
amplitude = scene.viz.parameter(1.0)
axes = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
curve = axes.plot(lambda x, a: a * math.sin(x), inputs=[amplitude])
scene.play([axes.animate.create(), curve.animate.write(), amplitude.animate.set(2.0).duration(1.2)])
```
]

#api-entry(
  name: "Parameter.set",
  kind: "method",
  desc: [Fija el valor al instante: el valor inicial al declarar o un corte reversible en el cursor después.],
  none,
)

#api-entry(
  name: "Parameter.animate",
  kind: "property",
  desc: [Proxy puro: `parameter.animate.set(value)` devuelve un `Anim` con duración y easing configurables para `scene.play`.],
  none,
)

#api-entry(
  name: "Parameter.current",
  kind: "property",
  desc: [Valor en el cursor de autoría (el espejo de lo último que fijaste o animaste), no el valor en un instante reproducido.],
  none,
)

#api-entry(
  name: "Parameter.add_updater_fn",
  kind: "method",
  desc: [Conduce el escalar con `callback(actual, dt, transcurrido) -> valor`. Para simulaciones con estado, pasa `reset` y un `fixed_dt` positivo: Gaanim llama a `reset()` y repite los pasos fijos al hacer seek y al exportar. Las simulaciones de objetos de paso fijo se ejecutan antes, así que el callback ve su estado del mismo fotograma. Resultados no finitos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
phase = scene.viz.parameter(0.0)
phase.add_updater_fn(lambda value, dt, elapsed: value + 2.0 * dt)
hand = scene.geometry.line((0, 0), scene.geometry.polar_point((0, 0), 1.5, phase))
scene.wait(1)
```
]

#api-entry(
  name: "Parameter.drive_from_samples",
  kind: "method",
  desc: [Recorre una serie `(times, values)` de forma nativa: el valor es `offset + scale * muestra` como función pura del tiempo, sin callbacks. `times` es relativo al cursor donde haces la llamada; `interpolation` es `"linear"` o `"step"`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
level = scene.viz.parameter(0.0)
level.drive_from_samples([0, 0.5, 1.0, 1.5], [0, 1, 0.4, 0.8], scale=2.0)
readout = scene.viz.readout(level, label="$h$", unit="m")
scene.wait(1.5)
```
]

#api-entry(
  name: "Parameter.remove_updater",
  kind: "method",
  desc: [Quita el callback o la serie que conduce el parámetro.],
  none,
)

#api-entry(
  name: "computed",
  kind: "function",
  signature: "computed(callback, *, inputs=()) -> Computed",
  desc: [Escalar determinista calculado a partir de `inputs`, que pueden incluir otros `Computed`. Las dependencias compartidas reutilizan su caché para el mismo snapshot. Sigue siendo escalar: no devuelve colores, texto ni geometría. Envolver un parámetro de otra escena no permite usarlo aquí.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
radius = scene.viz.parameter(1.0)
area = computed(lambda r: math.pi * r * r, inputs=[radius])
doubled = computed(lambda value: 2 * value, inputs=[area])
scene.viz.readout(doubled, label="2A")
```
]

#api-entry(
  name: "Visualization.time",
  kind: "property",
  desc: [Tiempo de la línea de tiempo como entrada reactiva explícita; es la misma fuente que `scene.time`.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
bob = scene.geometry.dot(0.15).fill(GOLD)
bob.move_to(0, computed(lambda t: math.sin(3 * t), inputs=[scene.viz.time]))
scene.wait(2)
```
]

#api-entry(
  name: "Visualization.variable",
  kind: "factory",
  params: ((name: "initial", type: "float", default: none, desc: [Valor inicial.]), (name: "label", type: "str", default: none, desc: [Etiqueta antes del signo igual; admite matemática `$…$`.]), (name: "format", type: "str", default: "\".2f\"", desc: [Formato numérico de Python: ancho, signo, agrupación, precisión y `f`, `e`, `g` o `%`.]), (name: "prefix / suffix / unit", type: "str / str / str | None", default: "\"\" / \"\" / None", desc: [Afijos y unidad visibles.]), (name: "decimal_separator", type: "str", default: "\".\"", desc: [`","` muestra `3,14` y convierte la agrupación `,` en `.`.])),
  desc: [Escalar animable que también se dibuja como `etiqueta = valor unidad`. Acepta las mismas operaciones que `Parameter`. Sus partes `label`, `equals`, `number` y `unit` son `Drawable` que se estilizan por separado; todos los términos usan `font_size` (0.48 por defecto) y comparten línea base. `color` pinta todo, también el número al actualizarse.],
)[
```python
from gaanim import RED, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
k = scene.viz.variable(10, label="$k$", format=".0f", color=RED)
scene.play([k.animate.create(), k.animate.set(100).duration(1.5)])
```
]

#api-entry(
  name: "Visualization.readout",
  kind: "factory",
  params: ((name: "source", type: "número | Parameter | Variable | Computed | callable", default: none, desc: [Escalar o función pura cuyos argumentos son `inputs`.]), (name: "inputs", type: "Sequence", default: "()", desc: [Dependencias en orden.]), (name: "invalid", type: "str", default: "\"invalid\"", desc: [Texto para evaluaciones inválidas o no finitas.]), (name: "label / format / prefix / suffix / unit / decimal_separator", type: "—", default: "—", desc: [Igual que en `variable`; un separador que sea dígito, signo, espacio, `e` o `%` lanza `ValueError`.])),
  desc: [Lectura numérica reactiva con términos equiespaciados y alineados en la línea base. El número solo se regenera si cambia el texto formateado. Sin `color` usa el color de texto del tema.],
)[
```python
import math
from gaanim import Scene

scene = Scene(frame=(16, 9), background="#0f172a")
radius = scene.viz.parameter(1.0)
area = scene.viz.readout(lambda r: math.pi * r**2, inputs=[radius], label="$A$", format=".2f", unit="m²")
scene.play([area.animate.create(), radius.animate.set(3.0).duration(1.5)])
```
]

#api-entry(
  name: "Readout.label / equals / number / unit / Variable.label / equals / number / unit / set / current / animate / add_updater_fn / remove_updater",
  kind: "property",
  signature: "label · equals · number · unit -> Drawable | None",
  desc: [Partes de `Readout` y `Variable` que se estilizan por separado (`number` siempre existe; las demás valen `None` si no se pidieron). `Variable` añade `current`, `set`, `animate`, `add_updater_fn` y `remove_updater`, con el mismo comportamiento que en `Parameter`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
speed = scene.viz.readout(3.2, label="$v$", unit="m/s")
speed.number.fill(GOLD)
```
]

== Contador rodante

Cifras que giran como un cuentakilómetros, para dinero, marcadores y cuentas
regresivas.

#api-entry(
  name: "Visualization.rolling_number",
  kind: "factory",
  params: (
    (name: "value", type: "float", default: "0.0", desc: [Valor inicial finito; su magnitud por `10**decimals` debe ser menor que `1e15`.]),
    (name: "decimals / min_digits", type: "int", default: "0 / 1", desc: [De 0 a 6 decimales y de 1 a 15 posiciones enteras con ceros iniciales; la suma no supera 15.]),
    (name: "group_separator / decimal_separator", type: "str", default: "\"\" / \".\"", desc: [Cero o un carácter, y un carácter distinto.]),
    (name: "prefix / suffix / show_plus", type: "str / str / bool", default: "\"\" / \"\" / False", desc: [Afijos de una línea (256 bytes UTF-8 en total) y signo positivo visible.]),
    (name: "font_family / weight", type: "str | None / int | None", default: "None", desc: [Familia y peso (1 a 1000), resueltos como en `scene.text`; sin familia hereda la de cuerpo.]),
    (name: "font_size / digit_spacing / line_height", type: "float", default: "0.75 / 0.02 / 1.25", desc: [Tamaño, espacio entre celdas y altura de la ventana como múltiplo de la altura de las cifras (al menos 1).]),
    (name: "mode", type: "str", default: "\"odometer\"", desc: [`odometer` arrastra las ruedas superiores durante la última unidad antes del acarreo; `continuous` gira cada rueda a la velocidad de su posición.]),
    (name: "direction", type: "str", default: "\"up\"", desc: [`up` o `down` para magnitudes crecientes; al disminuir se invierte.]),
    (name: "color", type: "Color | None", default: "None", desc: [Color de cifras, signo y afijos; sin él, el del tema.]),
  ),
  desc: [Las cifras son contornos vectoriales de ancho fijo recortados en su celda. La geometría depende solo del valor actual, así que reproducción, seeks y exportación coinciden. El borde derecho queda anclado; `min_digits` reserva posiciones para que no crezca durante un acarreo. En modo `continuous`, las ruedas se asientan durante el primer y el último 15 % de cada animación del valor. Opciones inválidas lanzan `ValueError`; si un driver sale del rango, se muestra una raya.],
)[
```python
from gaanim import Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
counter = scene.viz.rolling_number(
    98, min_digits=4, prefix="$ ", group_separator=",",
).move_to(0, 0)
scene.play([counter.count_to(1250, duration=3).easing(Easing.SMOOTH)])
scene.play([counter.animate.set(500).duration(2)])
counter.set(0)  # Corte reversible en el cursor actual.
scene.wait(0.5)
scene.render()
```
]

#api-entry(
  name: "RollingNumber.count_to",
  kind: "method",
  desc: [Animación de cuenta para `scene.play`, compatible con easing y retardo. El destino se usa tal cual: una fracción de la unidad mínima deja la rueda entre dos cifras. `snap=True` redondea al valor representable más cercano, así que termina asentado y `current` coincide con lo visible.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
mean = scene.viz.rolling_number(0, decimals=1)
scene.play([mean.count_to(61.7956, snap=True)])  # Termina en 61.8.
```
]

#api-entry(
  name: "RollingNumber.set",
  kind: "method",
  desc: [Fija el valor al instante (un corte reversible tras la declaración) y devuelve el contador. Acepta `snap` como `count_to`.],
  none,
)

#api-entry(
  name: "RollingNumber.move_to",
  kind: "method",
  desc: [Coloca el contador. Con `TextAnchor.BASELINE_LEFT`, `BASELINE_CENTER` o `BASELINE_RIGHT`, la línea base de las cifras queda en `y`, para alinearlo con texto; un `Anchor` usa la ventana completa, con el margen de `line_height`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
number = scene.viz.rolling_number(2, min_digits=2, font_size=1)
number.move_to(-5, 1.12, TextAnchor.BASELINE_LEFT)
scene.text("Objetivos", size=1).move_to(-3.5, 1.12, TextAnchor.BASELINE_LEFT)
```
]

#api-entry(
  name: "RollingNumber.visual / parameter / current / animate / fill / opacity",
  kind: "property",
  signature: "visual: Drawable · parameter: Parameter · current: float · animate",
  desc: [`visual` es el objeto dibujado: usa `counter.visual.animate` para posición, opacidad o entradas (sin entrada, el contador es visible desde su declaración). `parameter` es el escalar subyacente, reutilizable en `computed`, lecturas o `drive_from_samples`. `animate.set(value)` anima el valor como en un `Parameter`. `move_to`, `fill`, `opacity` y `set` devuelven el contador; otros métodos heredados pueden devolver su `Drawable`.],
  none,
)

== Recta numérica, plano polar y plano complejo

Espacios tipados de una dimensión, polares y complejos.

#api-entry(
  name: "Visualization.number_line",
  kind: "factory",
  desc: [Recta numérica tipada. `axis_visible`, `ticks`, `numbers` y `labels` controlan cada componente; los componentes ocultos siguen disponibles como capas vacías y la conversión de datos sigue activa.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
line = scene.viz.number_line(Axis.linear(0, 10).ticks(1), length=10)
```
]

#api-entry(
  name: "NumberLine.point_ref",
  kind: "method",
  params: ((name: "value", type: "float | Parameter | Variable | Computed", default: none, desc: [Valor convertido con la escala de la recta.]), (name: "normal_offset", type: "float | fuente reactiva | None", default: "None", desc: [Desplazamiento perpendicular en unidades locales.])),
  desc: [`PointRef` reactivo que sigue a la recta cuando se mueve, rota o escala. Las escalas categóricas rechazan valores reactivos con `ValueError`.],
  none,
)

#api-entry(
  name: "NumberLine.function",
  kind: "method",
  params: ((name: "function", type: "Callable[..., float]", default: none, desc: [Recibe la coordenada y después `inputs`.]), (name: "normal_scale", type: "float", default: "1.2", desc: [Distancia local para una salida igual a 1.]), (name: "reveal", type: "float | fuente reactiva | None", default: "None", desc: [Extremo visible exacto, en coordenadas de datos.])),
  desc: [Grafica una función perpendicular a la recta, con muestreo y actualización en Rust. Un `reveal` reactivo puede compartir el mismo escalar que un punto móvil sin desfases de longitud de arco.],
)[
```python
import math
from gaanim import Axis, Scene, computed

scene = Scene(frame=(16, 9), background="#0f172a")
theta = scene.viz.parameter(0.0)
line = scene.viz.number_line(
  Axis.linear(0, 3 * math.pi).ticks(math.pi).numbers("pi", denominator=1),
  length=9.5,
)
curve = line.function(lambda t: math.sin(t), normal_scale=1.2, reveal=theta)
point = scene.geometry.dot(0.1).follow(
  line.point_ref(theta, normal_offset=computed(lambda t: 1.2 * math.sin(t), inputs=[theta]))
)
scene.play([line.animate.create(), curve.animate.fade_in().duration(0.01), point.animate.fade_in()])
scene.play([theta.animate.set(3 * math.pi).duration(4)])
```
]

#api-entry(
  name: "NumberLine.coord / data_to_local / layer / drawable / animate",
  kind: "method",
  signature: "coord(value) -> CoordinateRef · data_to_local(value) -> float · layer(name) -> Drawable · drawable() -> Drawable · animate",
  desc: [Coordenada simbólica, conversión a coordenada local y capas `axis`, `ticks`, `numbers` y `labels`.],
  none,
)

#api-entry(
  name: "Visualization.polar",
  kind: "factory",
  desc: [Espacio polar de radio `radius` con `angle_divisions` radios. `rings` y `spokes` heredan `grid` si se omiten; `numbers` afecta a los valores radiales y `labels`, al título radial.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
polar = scene.viz.polar(Axis.linear(0, 2).ticks(0.5).label("r"), radius=3)
rose = polar.plot(lambda angle: 2 * abs(math.cos(3 * angle))).stroke(GOLD, 0.04)
```
]

#api-entry(
  name: "PolarSpace.plot",
  kind: "method",
  desc: [Curva polar `r = f(θ)` sobre `domain` (una vuelta completa por defecto).],
  none,
)

#api-entry(
  name: "PolarSpace.coord / layer / drawable / animate",
  kind: "method",
  signature: "coord(radius, angle) -> CoordinateRef · layer(name) -> Drawable · drawable() -> Drawable · animate",
  desc: [Coordenada simbólica polar y capas `grid`, `axes`, `numbers` y `labels`.],
  none,
)

#api-entry(
  name: "Visualization.complex",
  kind: "factory",
  desc: [Plano complejo cartesiano. Sin ejes, crea `Re` e `Im`; los interruptores y su precedencia son los de `cartesian_2d`, y devuelve el mismo tipo de espacio.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.complex(labels=False)
z = scene.geometry.dot(0.1).fill(GOLD).at_coordinate(plane.coord(1, 1))
```
]

== Gráficos declarativos

`ChartSpec` es una receta inmutable: datos, marca, codificaciones, ejes y guías.
`scene.viz.chart(spec)` la materializa en capas y `chart.animate.to(otra)` anima
el paso de una receta a otra.

```python
# show-code: true
from gaanim import Axis, ChartSpec, Field, Guide, Scale, Scene, Value

scene = Scene(frame=(16, 9), background="#0f172a")
spec = (
  ChartSpec({
    "id": ["a", "b", "c", "d"],
    "x": [-2, -0.5, 1, 2.5],
    "y": [1, 3, 2, 3.5],
    "group": ["A", "B", "A", "B"],
  }, key="id")
  .mark("point")
  .encode(x="x", y="y", color=Field("group", scale=Scale.category()), size=Value(0.12))
  .axes(x=Axis.linear(-3, 3).ticks(1).label("x"), y=Axis.linear(0, 4).ticks(1).label("y"))
  .guides(color=Guide.legend(title="Grupo"))
)
chart = scene.viz.chart(spec)
scene.play([chart.layer("axes").animate.create().duration(0.8)])
scene.play([chart.layer("marks").animate.fade_in(), chart.layer("guides").animate.fade_in()], duration=0.6)
# output: preview.webp
scene.render()
```

#api-entry(
  name: "ChartSpec",
  kind: "class",
  signature: "ChartSpec(data, *, key=None)",
  params: ((name: "data", type: "mapping | dataframe | DataTable | DataSource", default: none, desc: [Se copia al instante como instantánea inmutable propia.]), (name: "key", type: "str | None", default: "None", desc: [Columna de identidad estable para las transiciones; nulos o duplicados lanzan un error.])),
  desc: [Receta de gráfico. Una mutación externa posterior de los datos no cambia el resultado: los seeks anteriores siguen siendo iguales. Cada método devuelve una receta nueva, así que puedes derivar varios estados de una base.],
)[
```python
>>>from gaanim import *
base = ChartSpec({"category": ["a", "b", "c"], "time": [0, 1, 2], "value": [3, 1, 2]})
bars = base.mark("bar").encode(x="category", y="value")
points = base.mark("point").encode(x="time", y="value")
```
]

#api-entry(
  name: "ChartSpec.mark",
  kind: "method",
  desc: [Tipo de marca: `point`, `line`, `step`, `area`, `bar`, `histogram`, `box`, `violin`, `error_bar`, `heatmap` o `surface`. Las opciones dependen de la marca: las barras aceptan `width` (fracción de la banda en ejes categóricos), `label_position` (`"outside"` o `"inside"`), `label_offset` y `label_color`; los puntos, `radius`.],
)[
```python
>>>from gaanim import *
bars = (
  ChartSpec({"method": ["one", "two"], "elapsed": [40, -10], "kind": ["new", "old"]})
  .mark("bar", width=0.72, label_position="outside", label_offset=0.2)
  .encode(
    x="method", y="elapsed", label="elapsed",
    color=Field("kind", scale=Scale.category().colors([BLUE, GOLD])),
  )
)
```
]

#api-entry(
  name: "ChartSpec.encode",
  kind: "method",
  desc: [Asigna canales (`x`, `y`, `z`, `color`, `size`, `opacity`, `label`). Cada canal acepta un nombre de columna, `Field(columna, scale=...)` o `Value(constante)`. En `point`, `size` es el radio en unidades de escena (0.06 por defecto); un `Field` numérico lo reparte entre 0.03 y 0.12 con área proporcional. `color` y `opacity` con `Field` valen por fila; en `line`, `step` y `area`, un `color` categórico dibuja una serie por categoría.],
  none,
)

#api-entry(
  name: "ChartSpec.axes",
  kind: "method",
  desc: [Ejes explícitos; los omitidos se infieren. Un dominio explícito se respeta tal cual. Al inferirlos en 2D, el dominio se amplía para que quepan las marcas: las barras incluyen la línea base y media barra de margen, los mapas de calor media celda, el área su línea base, las barras de error su rango más un 5 % y los puntos y líneas un 5 % del rango.],
  none,
)

#api-entry(
  name: "ChartSpec.guides",
  kind: "method",
  desc: [Leyendas y barras de color para los canales `color`, `size` y `opacity`, derivadas de sus escalas.],
  none,
)

#api-entry(
  name: "ChartSpec.validate",
  kind: "method",
  desc: [Comprueba canales obligatorios, columnas, clave, escalas y opciones, y lanza un error antes de materializar.],
  none,
)

#api-entry(
  name: "ChartSpec.key",
  kind: "property",
  desc: [Columna de identidad de las transiciones, o `None`.],
  none,
)

#api-entry(
  name: "Scale.linear / log / symlog / power / time / category",
  kind: "method",
  signature: "linear(domain=None, *, clamp=False) · log(domain=None, *, base=10, clamp=False) · symlog(domain=None, *, base=10, threshold=1, clamp=False) · power(domain=None, *, exponent=1, clamp=False) · time(domain=None, *, clamp=False) · category(values=None)",
  desc: [Escala de un canal codificado con `Field`. La misma escala gobierna posiciones, colores, leyendas y barras de color. `category` sin valores los infiere en orden de aparición.],
)[
```python
>>>from gaanim import *
color = Field("temperature", scale=Scale.symlog((-100, 100), threshold=1))
kind = Field("kind", scale=Scale.category(["new", "old"]).colors(["#38bdf8", "#f97316"]))
```
]

#api-entry(
  name: "Scale.colors",
  kind: "method",
  desc: [Copia de la escala con un rango ordenado de colores: `Color`, cadenas CSS/hex o tuplas RGB(A), incluso mezclados. Colores inválidos lanzan `ValueError`.],
  none,
)

#api-entry(
  name: "Field",
  kind: "class",
  signature: "Field(column, *, scale=None)",
  desc: [Codifica un canal con una columna, opcionalmente con su escala. Úsalo cuando el valor cambia por fila.],
  none,
)

#api-entry(
  name: "Value",
  kind: "class",
  signature: "Value(value)",
  desc: [Constante para todo un canal: un número finito, una cadena o un color. Por ejemplo, `Value(BLUE)` da un único color a toda la serie.],
  none,
)

#api-entry(
  name: "Guide.legend / colorbar / disabled",
  kind: "method",
  signature: "legend(*, title=None) · colorbar(*, title=None) · disabled()",
  desc: [`legend` lista cada categoría con una muestra de su color, en el orden de `Scale.category` o de aparición; `colorbar` es la guía continua; `disabled` oculta la guía de un canal.],
  none,
)

#api-entry(
  name: "DataSource.replace / append / version / DataTable.columns",
  kind: "class",
  signature: "DataSource(data, *, key=None) · replace(data) · append(data) · version",
  desc: [Tabla reemplazable. Tras `replace` o `append`, construye un `ChartSpec` nuevo para capturar la nueva versión inmutable; `version` cuenta los cambios. `DataTable(columns)` es la tabla inmutable equivalente, con `columns` y `len()`.],
)[
```python
>>>from gaanim import *
source = DataSource({"x": [0, 1], "y": [2, 3]})
before = ChartSpec(source).mark("line").encode(x="x", y="y")
source.append({"x": [2], "y": [5]})
after = ChartSpec(source).mark("line").encode(x="x", y="y")
```
]

#api-entry(
  name: "Visualization.chart",
  kind: "factory",
  desc: [Materializa la receta en un `Chart` con capas semánticas por lotes: las marcas se agrupan por estilo resuelto en lugar de crear una entidad por registro.],
  none,
)

#api-entry(
  name: "Chart.layer",
  kind: "method",
  desc: [Capa estable como `Drawable`: `marks`, `axes`, `grid`, `guides` o la opcional `labels`. La capa `axes` reúne rejilla, ejes, marcas, números y títulos, así que `chart.layer("axes").animate.create()` la mantiene oculta hasta su entrada.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
chart = scene.viz.chart(ChartSpec({"x": [0, 1, 2], "value": [18, 42, 31]}).mark("bar").encode(x="x", y="value"))
scene.play([chart.layer("axes").animate.create(), chart.layer("marks").animate.fade_in()])
```
]

#api-entry(
  name: "Chart.drawable",
  kind: "method",
  desc: [Objeto raíz del gráfico, para layout, estilo y animaciones genéricas. La opacidad se propaga por todas las capas, incluidas las mallas 3D.],
  none,
)

#api-entry(
  name: "ChartAnimation.to",
  kind: "method",
  params: ((name: "target", type: "ChartSpec", default: none, desc: [Receta de destino.]), (name: "match_", type: "str", default: "\"key\"", desc: [`"key"` relaciona elementos por la columna de identidad (ambas recetas deben declararla); sin clave, pide `"index"`.]), (name: "fallback", type: "str", default: "\"error\"", desc: [Con familias de marca incompatibles: `"error"` o `"crossfade"`.])),
  desc: [Transición accesible desde `chart.animate.to(...)`. Entre gráficos 2D, las marcas, ejes y etiquetas que se corresponden se transforman y el resto se funde; al terminar, el gráfico de destino sustituye al original. `point`, `line` y `bar` pueden pasar entre 2D y 3D (con un fundido), y `heatmap` y `surface` comparten rejilla. Nunca mueve la cámara global.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>spec = ChartSpec({"id": ["a", "b", "c"], "x": [-1, 0, 1], "y": [1, 2, 3], "height": [-1, 0.5, 1.5]}, key="id").mark("point").encode(x="x", y="y")
>>>chart = scene.viz.chart(spec)
target = spec.encode(z="height").axes(z=Axis.linear(-2, 2))
scene.play([
  chart.animate.to(target).duration(1.4),
  scene.camera.animate.look_at(eye=(8, 6, 8), target=(0, 0, 0)).duration(1.4),
])
```
]

#api-entry(
  name: "Chart.inspect / inspection_enabled / move_to / move_to_3d / scale_to / animate",
  kind: "method",
  desc: [Activa metadatos de inspección de los campos indicados en la vista previa. No aparece en capturas ni exportaciones; `inspection_enabled` indica si está activa. `move_to`, `move_to_3d` y `scale_to` devuelven el gráfico.],
  none,
)

== Campos vectoriales

Un `VectorField` separa la función de sus representaciones: el mismo campo
produce flechas, líneas de corriente, advección y partículas.

#api-entry(
  name: "CoordinateSpace.field / VectorField.dimensions / evaluation",
  kind: "method",
  desc: [Evaluador reutilizable de una función que devuelve dos componentes (tres en 3D) y recibe las coordenadas y después `inputs`. No dibuja por sí solo. `dimensions` vale 2 o 3 y `evaluation`, `"python"` en esta ruta.],
)[
```python
# show-code: true
from gaanim import Axis, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
plane = scene.viz.cartesian_2d(Axis.linear(-4, 4).ticks(1), Axis.linear(-3, 3).ticks(1))
field = plane.field(lambda x, y: (-y, x))
arrows = field.arrows(resolution=(18, 12), colormap="batlow")
streams = field.streamlines(seeds=(12, 8), max_time=3.5, colormap="vik")
scene.play([plane.animate.create(), arrows.animate.write()], duration=0.8)
scene.play([streams.animate.write().duration(0.8)])
scene.play(streams.flow(1.5, time_width=0.12))
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "VectorField.arrows",
  kind: "method",
  params: ((name: "resolution", type: "(int, int) | (int, int, int) | None", default: "None", desc: [Muestras regulares por eje.]), (name: "min_length / max_length / length_scale", type: "float", default: "0 / 0.28 (0.24 en 3D) / 1", desc: [Límites y escala de longitud en unidades locales.]), (name: "width / tip_length / tip_width", type: "float", default: "0.02 / auto / auto", desc: [Grosor del asta y tamaño de la punta.]), (name: "color / colormap", type: "ColorLike | ColorMapLike | None", default: "None / viridis", desc: [Excluyentes; el mapa usa la magnitud.]), (name: "color_range", type: "(float, float) | None", default: "None", desc: [Rango explícito de magnitudes.])),
  desc: [Devuelve un `ArrowVectorField` (grupo retenido de astas y puntas) con `drawable()` y `animate`. Se valida todo antes de crear geometría.],
  none,
)

#api-entry(
  name: "VectorField.streamlines",
  kind: "method",
  params: ((name: "seeds", type: "(int, int) | (int, int, int) | None", default: "None", desc: [Resolución determinista de semillas candidatas.]), (name: "direction", type: "str", default: "\"both\"", desc: [`forward`, `backward` o `both`.]), (name: "tolerance / min_step / max_step", type: "float", default: "1e-4 / 1e-5 / 0.1", desc: [Control adaptativo Dormand–Prince RK45.]), (name: "max_time / max_length / max_steps", type: "float | int", default: "3 / None / 10000", desc: [Límites que hacen reproducible cada trayectoria.]), (name: "padding / separation", type: "float", default: "0.05 / 0.035", desc: [Margen del dominio y distancia mínima entre líneas.])),
  desc: [Líneas de corriente deterministas coloreadas por velocidad. Devuelve `StreamLines` con `drawable()` y `animate`.],
  none,
)

#api-entry(
  name: "StreamLines.flow / drawable / animate / ArrowVectorField.drawable / animate / FlowParticles.flow / drawable / animate",
  kind: "method",
  signature: "StreamLines.flow(duration=2.0, *, time_width=0.15) -> list[Anim] · FlowParticles.flow() -> list[Anim] · drawable() -> Drawable · animate",
  desc: [`StreamLines.flow` devuelve animaciones que desplazan resaltados más claros por las líneas sin recortar la geometría base; `FlowParticles.flow` devuelve un clip de advección por partícula. Son finitas y exactas al hacer seek: pasa la lista directamente a `scene.play`. Los tres grupos (`StreamLines`, `ArrowVectorField` y `FlowParticles`) ofrecen `drawable()` para layout y estilo y `animate` para sus entradas y salidas.],
  none,
)

#api-entry(
  name: "VectorField.advect",
  kind: "method",
  desc: [Mueve el centro de un objeto por la trayectoria que parte de `seed` (coordenadas de datos), con el mismo integrador. Devuelve un `Anim` finito por longitud de arco; no deforma la geometría del objeto.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>plane = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-3, 3))
>>>field = plane.field(lambda x, y: (-y, x))
boat = scene.geometry.dot(0.12).fill(GOLD)
scene.play([field.advect(boat, (2, 0), duration=2)])
```
]

#api-entry(
  name: "VectorField.particles",
  kind: "method",
  desc: [Crea `count` partículas con semillas de Halton deterministas (puntos en 2D, esferas en 3D). El `FlowParticles` resultante queda oculto hasta su primera entrada (`create`, `write`, `fade_in`, `grow_from_center`) y `flow()` devuelve un clip de advección por partícula.],
)[
```python
# continue
particles = field.particles(24, duration=2)
scene.play([particles.animate.fade_in()])
scene.play(particles.flow())
```
]

Los colores y mapas explícitos de flechas y líneas tienen prioridad sobre las
reglas `plot` del tema, que sigue controlando el fondo y los ejes.

== Espacios 3D

#experimental()

Ejes 3D con rejillas por plano, superficies y curvas paramétricas. Combínalos
con la cámara en perspectiva de #link("/referencia/scene/")[Escena].

#api-entry(
  name: "Visualization.cartesian_3d",
  kind: "factory",
  params: ((name: "x / y / z", type: "Axis", default: none, desc: [Ejes lineales o temporales reutilizables.]), (name: "size", type: "(float, float, float)", default: "(10.0, 8.0, 6.0)", desc: [Tamaño positivo en el mundo; elígelo según la distancia de la cámara.]), (name: "xy_grid / xz_grid / yz_grid", type: "bool | None", default: "None", desc: [Visibilidad de cada plano; `None` hereda `grid`.]), (name: "x_* / y_* / z_*", type: "bool | None", default: "None", desc: [Ejes, marcas, números y títulos por eje.])),
  desc: [Devuelve un `Cartesian3D` (un `CoordinateSpace3D`) con capas estables. Las rejillas ocultas no emiten segmentos ni duplican las aristas de los ejes; las etiquetas miran a la cámara.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
world = scene.viz.cartesian_3d(
    Axis.linear(-3, 3).ticks(1).label("x"),
    Axis.linear(-3, 3).ticks(1).label("y"),
    Axis.linear(-2, 2).ticks(1).label("z"),
    size=(6, 6, 4), xz_grid=False, yz_grid=False,
)
```
]

#api-entry(
  name: "CoordinateSpace3D.surface",
  kind: "method",
  desc: [Superficie `z = f(x, y)` muestreada en una rejilla de `resolution`, con `inputs` reactivas como `plot`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>world = scene.viz.cartesian_3d(Axis.linear(-3, 3), Axis.linear(-3, 3), Axis.linear(-1, 1))
saddle = world.surface(lambda x, y: 0.1 * (x * x - y * y), resolution=(32, 24))
```
]

#api-entry(
  name: "CoordinateSpace3D.parametric",
  kind: "method",
  desc: [Curva 3D `t → (x, y, z)` sobre `domain` con `samples` muestras e `inputs` reactivas.],
  none,
)

#api-entry(
  name: "CoordinateSpace3D.field",
  kind: "method",
  desc: [Campo vectorial 3D; sus flechas, líneas de corriente y partículas funcionan como en 2D, con color por vértice.],
  none,
)

#api-entry(
  name: "CoordinateSpace3D.layer / move_to_3d / scale_to / data_to_local / local_to_data / drawable / animate",
  kind: "method",
  desc: [Capa 3D animable por separado: `grid`, `axes`, `ticks`, `numbers` o `labels`. `move_to_3d`, `scale_to`, `data_to_local` y `local_to_data` completan el espacio.],
  none,
)

== Límites

La inspección interactiva solo existe en la vista previa. La exportación
interactiva, los dashboards, las facetas, los volúmenes y las isosuperficies
aún no forman parte de esta API. La animación de cámara es global, explícita y
componible.
