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
  params: ((name: "x / y", type: "Axis", default: none, desc: [Ejes con dominio, marcas y títulos (ver «Ejes»).]), (name: "width / height", type: "float | None", default: "None", desc: [Tamaño en unidades de escena; por defecto, relativo al área segura.]), (name: "grid / axes / ticks / numbers / labels", type: "bool", default: "True", desc: [Interruptores globales de cada capa.]), (name: "x_axis / y_axis, x_grid / y_grid, x_ticks / y_ticks, x_numbers / y_numbers, x_labels / y_labels", type: "bool | None", default: "None", desc: [Sustituyen el interruptor global en un eje; `None` lo hereda. El global es `axes` y el de cada eje `x_axis` y `y_axis`, en singular.])),
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
  signature: "coord(x, y) -> CoordinateRef · place(drawable, *, anchor=None, offset=None) -> Drawable",
  desc: [Coordenada simbólica `(x, y)` del espacio. Pásala a `Drawable.at_coordinate` (o a `CoordinateRef.place`) para colocar un objeto como hijo del espacio, que seguirá sus cambios de vista. Sin `anchor`, el objeto queda centrado en el dato (su origen va ahí); con un `anchor` (`"left"`, `Anchor.BOTTOM`…), ese punto de su caja va al dato, y `offset` lo mueve luego en unidades de escena, como en `Drawable.place`: `anchor="left", offset=(0.15, 0)` deja una etiqueta justo a la derecha de un punto, sea cual sea su ancho o la escala de los ejes. `Drawable.at_coordinate` admite los mismos `anchor` y `offset`. Un `offset` no finito lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
peak = scene.geometry.dot(0.1).fill(GOLD).at_coordinate(plane.coord(1, 1.5))
label = plane.coord(1, 1.5).place(scene.text("máximo", role="label"), anchor="left", offset=(0.15, 0))
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
  name: "CoordinateSpace.point_ref",
  kind: "method",
  signature: "point_ref(x, y) -> PointRef",
  params: ((name: "x, y", type: "float | Parameter | Computed | TimeInput", default: none, desc: [Coordenadas en unidades de dato; pueden ser reactivas.]),),
  returns: (type: "PointRef", desc: [Un punto de escena que se recalcula en cada fotograma.]),
  desc: [Como `data_to_scene`, pero sigue valores reactivos: un `Parameter` movido por `drive_from_samples` o animado, un `Computed` o el tiempo. El punto acompaña a los valores, a la ventana de `view_to` y a la posición, escala y rotación del plano. Un valor que el eje no puede representar (un negativo en un eje logarítmico) deja el punto donde estaba.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(0, 240), Axis.linear(0, 5000), width=5.6, height=2.5)
times = [0.0, 1.0, 2.0]
rho = scene.viz.parameter(0.0).drive_from_samples(times, [20, 120, 200])
q = scene.viz.parameter(0.0).drive_from_samples(times, [1500, 4200, 2600])
car = scene.geometry.dot(0.08).follow(plane.point_ref(rho, q))
scene.play([plane.animate.create(), car.animate.fade_in()])
scene.wait(2.0)
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
  desc: [Constructores estáticos. Elige según el significado: `linear` para diferencias uniformes, `log` para órdenes de magnitud positivos, `symlog` para datos con signo alrededor de cero, `power` para potencias, `time` para marcas de tiempo en segundos y `category` para grupos discretos. Un eje `log` marca cada potencia de la base entre sus extremos, incluidos ambos. Un eje `symlog` marca el 0 y las potencias de la base desde `threshold`, donde acaba la región lineal, a cada lado: con `threshold=1`, `Axis.symlog(-100, 100)` marca −100, −10, −1, 0, 1, 10 y 100. Con `ticks(step)` usa ese paso lineal, y si el dominio no sale de la región lineal, pasos lineales automáticos.],
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
  desc: [El valor de este eje por el que pasa el otro: `"auto"`, `"zero"`, `"min"`, `"max"` o un valor de datos. Así, `crossing("min")` en el eje y baja el eje x al borde inferior del plano, y en el eje x lleva el eje y al borde izquierdo. `"auto"`, el predeterminado, es el cero si el dominio lo contiene y, si no, el extremo más cercano; un número fuera del dominio se acota a él.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
plane = scene.viz.cartesian_2d(Axis.linear(0, 10), Axis.linear(-60, 60).crossing("min"))
```
]

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
  desc: [Conduce el escalar con `callback(actual, dt, transcurrido) -> valor`. Sin `reset` ni `fixed_dt`, calcula el valor a partir de `transcurrido`, como en el ejemplo: un valor que acumula `dt` vuelve al inicial al hacer seek o capturar fotogramas sueltos. Para simulaciones con estado, pasa `reset` y un `fixed_dt` positivo: Gaanim llama a `reset()` y repite los pasos fijos al hacer seek y al exportar. Las simulaciones de objetos de paso fijo se ejecutan antes, así que el callback ve su estado del mismo fotograma. Resultados no finitos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
phase = scene.viz.parameter(0.0)
phase.add_updater_fn(lambda value, dt, elapsed: 2.0 * elapsed)
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
  params: ((name: "initial", type: "float", default: none, desc: [Valor inicial.]), (name: "label", type: "str", default: none, desc: [Etiqueta antes del signo igual; admite matemática `$…$`.]), (name: "format", type: "str", default: "\".2f\"", desc: [Especificación de formato de Python para un `float`, como en `format(valor, spec)`: relleno y alineación, signo, `z`, `#`, `0`, ancho, agrupación `,` o `_`, precisión y tipo `f`, `F`, `e`, `E`, `g`, `G` o `%`. `"05.2f"` muestra `00.07`. Los tipos de enteros o texto (`d`, `x`, `s`…) y las especificaciones mal formadas lanzan `ValueError`; para enteros usa `".0f"`. El relleno con espacios no se ve, porque el número se alinea a la derecha por su tinta: para reservar sitio usa `reserve`.]), (name: "prefix / suffix / unit", type: "str / str / str | None", default: "\"\" / \"\" / None", desc: [Afijos y unidad visibles.]), (name: "decimal_separator", type: "str", default: "\".\"", desc: [`","` muestra `3,14` y convierte la agrupación `,` en `.`.]), (name: "font_size", type: "float | None", default: "None", desc: [Tamaño de todos los términos (etiqueta, `=`, número y unidad) en unidades de escena; 0.48 por defecto. Úsalo en lugar de `scale_to` para un indicador más pequeño dentro de un panel.]), (name: "color", type: "ColorLike | None", default: "None", desc: [Color de todos los términos, también del número al actualizarse; sin él, el color de texto del tema.]), (name: "font / weight", type: "str | None / int | None", default: "None / None", desc: [Familia y peso (1 a 1000) del número y de la etiqueta y la unidad en texto plano; una etiqueta `$…$` conserva la fuente matemática. Sin ellos, el número usa la fuente matemática del tema. Un peso fuera de rango lanza `ValueError`.])),
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
  params: ((name: "source", type: "número | Parameter | Variable | Computed | callable", default: none, desc: [Escalar o función pura cuyos argumentos son `inputs`.]), (name: "inputs", type: "Sequence", default: "()", desc: [Dependencias en orden.]), (name: "invalid", type: "str", default: "\"invalid\"", desc: [Texto para evaluaciones inválidas o no finitas.]), (name: "label / format / prefix / suffix / unit / decimal_separator / font_size / color / font / weight", type: "—", default: "—", desc: [Igual que en `variable`; un separador que sea dígito, signo, espacio, `e` o `%` lanza `ValueError`.]), (name: "spacing", type: "float | None", default: "None", desc: [Espacio entre etiqueta, `=`, número y unidad, en unidades de escena, contado como se compone el texto: desde el avance de cada término, no desde su tinta. Por defecto, un espacio fino que crece con `font_size` (0.1 con el tamaño por defecto). `prefix` y `suffix` van pegados al número: `suffix=" m"` conserva su espacio.]), (name: "reserve", type: "float | None", default: "None", desc: [Valor cuyo texto cabe en la celda de un layout, además de los valores que toman sus parámetros. Úsalo con conteos en vivo sin máximo conocido, como `votes` (`reserve=999`). Un valor no finito lanza `ValueError`.])),
  desc: [Lectura numérica reactiva con términos equiespaciados y alineados en la línea base. Como en una ecuación compuesta, cada término conserva el blanco que la fuente da a sus glifos a los lados: `T = 1.62 s` se ve parejo aunque la `T` y el `1` dejen aire distinto junto a su tinta. El número solo se regenera si cambia el texto formateado. Sin `color` usa el color de texto del tema. Dentro de un layout, su celda mide lo que el texto más ancho que mostrará: el de los valores a los que se fijan o animan sus parámetros (el `percent` de una encuesta llega a 100) y el de `reserve`. El número queda alineado a la derecha en ese espacio, así que sus vecinos no se mueven mientras cuenta. `variable` también acepta `reserve`.],
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

#api-entry(
  name: "Readout.move_to / Variable.move_to",
  kind: "method",
  signature: "move_to(x, y=None, anchor: Anchor | TextAnchor | None = None) -> Readout",
  desc: [Coloca la lectura como cualquier objeto: sin `anchor`, su centro va a `(x, y)`; con un `Anchor`, ese punto de su caja. Con un `TextAnchor` la coloca por su línea base, alineada con un `Text`: `TextAnchor.BASELINE_LEFT` deja fijo su comienzo aunque el número cambie de ancho. Con un `TextAnchor` hacen falta `x` e `y`, que pueden ser escalares reactivos.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
scene.text("Carga").move_to(-4, 0, TextAnchor.BASELINE_LEFT)
scene.viz.readout(12.5, label="$P$", unit="kN").move_to(-2, 0, TextAnchor.BASELINE_LEFT)
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
    (name: "decimals / min_digits", type: "int", default: "0 / 1", desc: [De 0 a 6 decimales y de 1 a 15 posiciones enteras reservadas; la suma no supera 15.]),
    (name: "pad", type: "str", default: "\"0\"", desc: [Relleno de las posiciones reservadas sin usar: `"0"` dibuja ceros iniciales y `" "` deja celdas vacías del mismo ancho, sin dibujar sus separadores de miles.]),
    (name: "group_separator / decimal_separator", type: "str", default: "\"\" / \".\"", desc: [Cero o un carácter, y un carácter distinto.]),
    (name: "prefix / suffix / show_plus", type: "str / str / bool", default: "\"\" / \"\" / False", desc: [Afijos de una línea (256 bytes UTF-8 en total) y signo positivo visible.]),
    (name: "font_family / weight", type: "str | None / int | None", default: "None", desc: [Familia y peso (1 a 1000), resueltos como en `scene.text`; sin familia hereda la de cuerpo.]),
    (name: "font_size / digit_spacing / line_height", type: "float", default: "0.75 / 0.02 / 1.25", desc: [Tamaño, espacio entre celdas y altura de la ventana como múltiplo de la altura de las cifras (al menos 1).]),
    (name: "mode", type: "str", default: "\"odometer\"", desc: [`odometer` arrastra las ruedas superiores durante la última unidad antes del acarreo; `continuous` gira cada rueda a la velocidad de su posición.]),
    (name: "direction", type: "str", default: "\"up\"", desc: [`up` o `down` para magnitudes crecientes; al disminuir se invierte.]),
    (name: "color", type: "Color | None", default: "None", desc: [Color de cifras, signo y afijos; sin él, el del tema.]),
  ),
  desc: [Las cifras son contornos vectoriales de ancho fijo recortados en su celda. La geometría depende solo del valor actual, así que reproducción, seeks y exportación coinciden. El borde derecho queda anclado; `min_digits` reserva posiciones para que no crezca durante un acarreo. Con `pad=" "` esas posiciones quedan vacías: el contador ocupa siempre lo mismo, el prefijo no se desplaza y una frase con el valor final queda centrada para todos los valores. En modo `continuous`, las ruedas se asientan durante el primer y el último 15 % de cada animación del valor. Opciones inválidas lanzan `ValueError`; si un driver sale del rango, se muestra una raya.],
)[
```python
from gaanim import Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
counter = scene.viz.rolling_number(
    98, min_digits=4, prefix="$ ", group_separator=",",
).move_to(0, 0)
scene.play([counter.count_to(1250, duration=3).easing(Easing.SMOOTH)])
scene.play([counter.animate.set(500).duration(2)])
designs = scene.viz.rolling_number(0, min_digits=8, pad=" ", group_separator=" ").move_to(0, -1.5)
scene.play([designs.count_to(14_000_605, duration=3)])
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

== Anillos de progreso y temporizadores

Un arco que se llena en el sentido del reloj desde las 12, con una pista tenue
detrás y un contador rodante en el centro. Todo sigue a un único `Parameter`.

#api-entry(
  name: "Visualization.progress_ring",
  kind: "factory",
  params: (
    (name: "value", type: "float", default: "0.0", desc: [Progreso inicial; el anillo muestra `value` recortado a `[0, 1]`.]),
    (name: "radius / width", type: "float", default: "1.0 / 0.12", desc: [Radio del anillo y grosor del trazo, en unidades de escena.]),
    (name: "color", type: "Color | None", default: "None", desc: [Color del arco; sin él, el acento del tema.]),
    (name: "track / track_color", type: "bool / Color | None", default: "True / None", desc: [Dibuja un círculo tenue completo detrás del arco, con otro color si se indica.]),
    (name: "label / decimals", type: "bool / int", default: "True / 0", desc: [Muestra el porcentaje con un contador rodante de 0 a 6 decimales.]),
    (name: "label_color / font_size", type: "Color | None / float", default: "None / 0.5", desc: [Color y tamaño del porcentaje; sin color, el del texto.]),
  ),
  desc: [El arco tiene extremos redondeados y su caja es el anillo completo, así que la maquetación no sigue al barrido. Anima el valor con `animate.set(...)`; la geometría depende solo del valor actual, así que reproducción, seeks y exportación coinciden. `arc`, `track` y `label` dan acceso a cada parte. Tamaños no positivos o un valor no finito lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
ring = scene.viz.progress_ring(0.0, radius=1.6, width=0.18)
scene.play([ring.animate.set(0.75).duration(1.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Visualization.countdown",
  kind: "factory",
  params: (
    (name: "seconds", type: "float", default: none, desc: [Duración de la cuenta; también es el máximo del anillo.]),
  ),
  desc: [Un `ProgressRing` que empieza lleno, con los segundos restantes rodando en el centro. `count_down()` lo vacía a velocidad constante. El resto de opciones son las de `progress_ring`. Unos `seconds` no positivos lanzan `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
timer = scene.viz.countdown(3, radius=1.6, width=0.18)
scene.play([timer.count_down()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "ProgressRing.count_down",
  kind: "method",
  params: (
    (name: "duration", type: "float | None", default: "None", desc: [Segundos de la animación; por defecto, el valor actual.]),
  ),
  returns: (type: "Anim", desc: [El valor bajando hasta cero con easing lineal.]),
  desc: [Con `countdown(10)` dura diez segundos reales. Una duración negativa lanza `ValueError`.],
  none,
)

#api-entry(
  name: "ProgressRing.parameter / visual / arc / track / label / maximum / current / set / animate",
  kind: "property",
  signature: "parameter: Parameter · visual: Drawable · arc: Drawable · track: Drawable | None · label: Drawable | None · maximum: float · current: float · set(value) · animate",
  desc: [`parameter` es el escalar que mueve el arco y la etiqueta, reutilizable en `computed` o lecturas. `maximum` es `1.0` en un anillo y los segundos en una cuenta atrás. `set` fija el valor al instante (un corte reversible tras la declaración) y `animate.set(value)` lo anima. `visual` es el grupo de las tres partes: usa `visual.animate` para moverlo o desvanecerlo. `move_to`, `shift_by` y `opacity` devuelven el anillo. La etiqueta se asienta en cifras enteras: redondea el porcentaje (la cuenta atrás redondea los segundos hacia arriba) y gira a la cifra siguiente cuando el valor la cruza, así que un valor como `1/3` muestra `33%` sin quedarse entre dos cifras.],
  none,
)

== Carrera de barras

Una _bar chart race_: barras ordenadas por valor que crecen, se adelantan unas a
otras y salen del ranking. Todo es función pura de una única posición de
fotograma clave, así que un seek muestra lo mismo que la reproducción continua.

#api-entry(
  name: "Visualization.bar_race",
  kind: "factory",
  params: (
    (name: "frames", type: "Mapping[Any, Mapping[str, float]] | Sequence[tuple[Any, Mapping[str, float]]]", default: none, desc: [Cada etiqueta de fotograma clave (un año, una fecha…) con los `{nombre: valor}` de ese instante, en orden. Un nombre ausente vale cero en ese fotograma.]),
    (name: "top", type: "int", default: "10", desc: [Puestos visibles; el líder arriba.]),
    (name: "rank_smoothing", type: "float", default: "0.3", desc: [Fotogramas clave (de 0 a 2) que dura el intercambio de dos barras, centrado en el instante en que se cruzan y acortado para que termine antes de que esas dos barras vuelvan a cruzarse; con `0` cambian de puesto al instante.]),
    (name: "value_format", type: "str", default: "\"{:,.0f}\"", desc: [Campo de formato de Python con texto opcional alrededor: `"{}"`, `"{:,.0f}"`, `"{:_.2f}"` o `"${:,.1f} M"`, hasta 6 decimales.]),
    (name: "width / height", type: "float", default: "10.0 / 6.0", desc: [Tamaño de toda la carrera, centrada en su origen, en unidades de escena.]),
    (name: "label_width / bar_gap", type: "float | None / float", default: "None / 0.18", desc: [Ancho de la columna de nombres (por defecto el 22% de `width`) y fracción vacía de cada puesto. Un nombre más ancho que la columna se dibuja más pequeño, hasta que cabe.]),
    (name: "colors", type: "Mapping[str, Color] | Sequence[Color] | None", default: "None", desc: [Colores por nombre o una lista que vuelve a empezar como una paleta; el resto toma la paleta del tema (`Theme(palette=...)`).]),
    (name: "label_color / font_size", type: "Color | None / float | None", default: "None / None", desc: [Color y tamaño de nombres y valores; por defecto el color de texto del tema y el 55% del grosor de la barra, sin pasar del 12% de `height` para que las carreras de pocos puestos sigan legibles.]),
    (name: "ticker / ticker_size / ticker_color", type: "bool / float | None / Color | None", default: "True / None / None", desc: [Etiqueta del fotograma clave abajo a la derecha, por defecto al 15% de `height` y con el color atenuado del tema.]),
  ),
  desc: [Los valores se interpolan linealmente entre fotogramas clave. Cuando dos barras se cruzan, intercambian sus puestos con un movimiento suavizado en vez de saltar; entre cruces, cada barra ocupa su propio puesto. Las barras que salen del `top` se desvanecen por debajo del último puesto y la barra más larga ocupa siempre todo el ancho disponible. Los nombres van en una columna a la izquierda y un contador rodante (`rolling_number`) sigue el extremo de cada barra. Si todas las etiquetas son números enteros, como años, el ticker rueda entre ellos; si no, cambia de texto con un fundido. Datos vacíos o no finitos, nombres repetidos, un `value_format` no admitido o tamaños no positivos lanzan `ValueError`; otra forma de `frames`, `TypeError`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
frames = {
    2020: {"Ana": 12, "Luis": 30, "Marta": 18, "Noé": 5},
    2021: {"Ana": 35, "Luis": 34, "Marta": 22, "Noé": 20},
    2022: {"Ana": 48, "Luis": 40, "Marta": 51, "Noé": 38},
    2023: {"Ana": 60, "Luis": 44, "Marta": 58, "Noé": 71},
}
race = scene.viz.bar_race(frames, top=3, width=12, height=6)
scene.play([race.animate.play().duration(4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "BarRaceAnimation.play / to",
  kind: "method",
  signature: "play() -> Anim · to(position: float) -> Anim",
  desc: [`race.animate.play()` avanza desde la posición actual hasta el último fotograma clave a ritmo constante, un segundo por fotograma pendiente salvo que encadenes `.duration(segundos)`. `to(position)` va a una posición concreta: `0` es el primer fotograma clave y las posiciones fraccionarias quedan entre dos. Una posición fuera de `[0, frame_count - 1]` lanza `ValueError`.],
  none,
)

#api-entry(
  name: "BarRace.parameter / position / frame_count / names / ticker / bar / set / visual",
  kind: "property",
  signature: "parameter: Parameter · position: float · frame_count: int · names: list[str] · ticker: Drawable | None · bar(name) -> Drawable · set(position) · visual: Drawable",
  desc: [`parameter` es la posición de fotograma clave, reutilizable en `computed` o lecturas. `bar(name)` devuelve la fila de una barra (barra, nombre y valor), que se mueve entre puestos y se desvanece al salir del top; su propia opacidad queda libre, así que `race.bar(name).animate.opacity(0.3)` la atenúa además de ese desvanecimiento. Un nombre desconocido lanza `KeyError`. `set` salta a una posición (un corte reversible tras la declaración). `visual` es el grupo de todas las filas y el ticker; `move_to`, `shift_by` y `opacity` devuelven la carrera.],
  none,
)

== Treemaps y diagramas de Sankey

Dos diagramas de proporciones a partir de tablas: un treemap anida rectángulos
cuyas áreas siguen los valores, y un diagrama de Sankey dibuja flujos entre
nodos como cintas tan anchas como su valor. Cada pieza es un drawable normal
con nombre: la celda `"Europe/France"`, el nodo `"power"` o la cinta
`"coal→power"`. `animate.to(datos)` construye la nueva versión y hace un
`magic_move` hacia ella: lo que conserva su nombre se mueve y cambia de forma,
y el resto se desvanece. Los colores se conservan por nombre entre versiones.

#api-entry(
  name: "Visualization.treemap",
  kind: "factory",
  params: (
    (name: "data", type: "DataTable | DataSource | Mapping | DataFrame", default: none, desc: [Una fila por hoja.]),
    (name: "path", type: "str | Sequence[str]", default: none, desc: [Columnas del grupo exterior a la hoja; los números se leen como nombres.]),
    (name: "value", type: "str", default: none, desc: [Columna numérica; las filas con la misma ruta se suman y un valor ausente cuenta como 0.]),
    (name: "within", type: "\"safe\" | \"frame\" | Zone | Drawable | None", default: "None", desc: [Región que llena; por defecto, el área segura.]),
    (name: "labels", type: "bool", default: "True", desc: [Nombres de las hojas en el centro y de los grupos arriba, más pequeños si no caben y omitidos si ni así.]),
    (name: "gap / padding", type: "float", default: "0.04 / 0.06", desc: [Separación entre celdas hermanas y margen dentro de cada grupo.]),
    (name: "colors", type: "Sequence[ColorLike] | None", default: "None", desc: [Colores de los grupos exteriores por orden de aparición; por defecto, la paleta categórica del tema.]),
  ),
  desc: [El área de cada grupo es la suma de sus filas, y cada nivel se reparte con el algoritmo _squarified_, de mayor a menor, para que las celdas queden cerca del cuadrado; los valores iguales conservan el orden de su primera fila. Las hojas toman el color de su grupo exterior y los grupos son paneles con su nombre arriba. `keys` da las claves de las celdas (los padres antes que sus hijos) y `cell(key)` o `treemap[key]`, su rectángulo. Una ruta vacía, columnas que faltan, valores negativos, no finitos o todos nulos, una ruta que es a la vez fila y grupo, o un `gap`/`padding` negativo lanzan `ValueError`; una clave desconocida, `KeyError`.],
)[
```python
# show-code: true
# player
from gaanim import Scene
scene = Scene(frame=(16, 9))
gdp = {
    "region": ["Asia", "Asia", "Asia", "Europe", "Europe", "America"],
    "country": ["China", "Japan", "India", "Germany", "France", "USA"],
    "gdp": [18, 4, 4, 4, 3, 27],
}
later = dict(gdp, gdp=[20, 4, 6, 4, 3, 28])
treemap = scene.viz.treemap(gdp, path=["region", "country"], value="gdp")
scene.play([treemap.animate.fade_in().duration(0.5)])
scene.play([treemap.animate.to(later).duration(1.2)])
scene.wait(0.5)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Visualization.sankey",
  kind: "factory",
  params: (
    (name: "flows", type: "DataTable | DataSource | Mapping | DataFrame", default: none, desc: [Un flujo por fila.]),
    (name: "source / target / value", type: "str", default: "\"source\" / \"target\" / \"value\"", desc: [Columnas con los nombres de los extremos (los números se leen como nombres) y el valor numérico; un valor ausente cuenta como 0.]),
    (name: "within", type: "\"safe\" | \"frame\" | Zone | Drawable | None", default: "None", desc: [Región que llenan las barras.]),
    (name: "node_width / node_gap", type: "float", default: "0.24 / 0.3", desc: [Ancho de cada barra y separación entre las barras de una capa.]),
    (name: "align", type: "\"justify\" | \"left\" | \"right\"", default: "\"justify\"", desc: [Con `"justify"` los nodos de los que no sale ningún flujo van a la última capa; `"left"` deja cada nodo en su capa y `"right"` lo empuja tan a la derecha como permiten sus flujos.]),
    (name: "labels", type: "bool", default: "True", desc: [Nombres junto a las barras, hacia el centro del diagrama.]),
    (name: "colors", type: "Sequence[ColorLike] | None", default: "None", desc: [Colores de los nodos por orden de aparición.]),
    (name: "flow_opacity", type: "float", default: "0.45", desc: [Opacidad de las cintas, que toman el color de su origen.]),
  ),
  desc: [Cada nodo va en la capa de la cadena de flujos más larga que llega a él y es tan alto como lo mayor entre lo que entra y lo que sale, en una sola escala para todo el diagrama. Las capas se relajan un número fijo de veces para que las cintas vayan lo más rectas posible, y los mismos flujos dan siempre la misma disposición. `nodes` da los nombres, `node(name)` la barra y `flow(source, target)` la cinta. Un flujo de un nodo a sí mismo, flujos en círculo, valores negativos, no finitos o todos nulos, columnas que faltan, un `align` desconocido, un `node_width` no positivo, un `node_gap` negativo, una región que no es más ancha que `node_width` o un `flow_opacity` fuera de `[0, 1]` lanzan `ValueError`; un nombre desconocido, `KeyError`.],
)[
```python
# show-code: true
# player
from gaanim import Scene
scene = Scene(frame=(16, 9))
energy = {
    "source": ["coal", "gas", "solar", "gas", "power", "power"],
    "target": ["power", "power", "power", "homes", "homes", "industry"],
    "value": [30, 20, 10, 10, 25, 35],
}
cleaner = dict(energy, value=[10, 20, 30, 10, 25, 35])
diagram = scene.viz.sankey(energy)
scene.play([diagram.animate.fade_in().duration(0.5)])
scene.play([diagram.animate.to(cleaner).duration(1.2)])
scene.wait(0.5)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "TreemapAnimation.to / SankeyAnimation.to",
  kind: "method",
  signature: "to(data) -> Anim",
  desc: [Cambia a `data`, leída con las mismas columnas. Las piezas que conservan su nombre se deslizan y cambian de tamaño o de forma; las demás se desvanecen y aparecen, y la nueva versión toma el relevo al final. Los cambios siguientes, `cell`, `node` y `flow` parten de `data`, aunque la animación aún no se haya reproducido. `animate` conserva además todo el vocabulario de `Drawable.animate`. Datos inválidos lanzan `ValueError`.],
  none,
)

== Línea de tiempo

Un eje de fechas con hitos rotulados. Cada rótulo va al primer carril libre de
su lado, así que los vecinos no se pisan, y crece hacia el lado en que cabe
dentro del eje. Las fechas son números, como años.

#api-entry(
  name: "Visualization.timeline",
  kind: "factory",
  params: (
    (name: "start / end", type: "float", default: none, desc: [Primera y última fecha del eje.]),
    (name: "x_range / y", type: "tuple[float, float] / float", default: "(-6.5, 6.5) / 0", desc: [Dónde va el eje, en unidades de escena.]),
    (name: "breaks", type: "Sequence[tuple[float, float]]", default: "()", desc: [Cortes de escala `(fecha, fracción)`: esa fecha cae en esa fracción del eje y las fechas se reparten linealmente entre cortes. Cada corte se dibuja como un hueco con dos barras inclinadas.]),
    (name: "lane_height / gap", type: "float", default: "0.6 / 0.2", desc: [Distancia entre carriles y separación mínima entre rótulos.]),
    (name: "label_width", type: "float | None", default: "None", desc: [Ancho al que se parten títulos y cuerpos; sin él van en una línea.]),
    (name: "color", type: "ColorLike | None", default: "None", desc: [Color de puntos y tallos; por defecto el `accent` del tema.]),
  ),
  returns: (type: "Timeline", desc: [El eje (`axis`), `x(fecha)`, `event(...)`, `reserve(...)` y la lista `events`.]),
  desc: [Un final que no va después del inicio, cortes que no avanzan, una fecha fuera del eje, un lado desconocido o tamaños no positivos lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
line = scene.viz.timeline(1980, 2026, breaks=[(2016, 0.55)])
for year, title in [(1985, "Primeros ensayos"), (1992, "Norma E.070"), (2004, "Muros confinados")]:
    line.event(year, title, "albañilería", side="above")
line.event(2019, "Visión artificial", "automatización", side="below")
scene.play([line.axis.animate.create()])
```
]

#api-entry(
  name: "timeline.event",
  kind: "method",
  params: (
    (name: "date / title / body", type: "float / str / str | None", default: "— / — / None", desc: [La fecha y el rótulo: el título en negrita sobre el cuerpo, alineados con el tallo.]),
    (name: "side", type: "str", default: "\"auto\"", desc: [`"above"`, `"below"` o `"auto"`, que toma el lado con el carril libre más cercano al eje y alterna cuando están igual de cerca.]),
    (name: "color", type: "ColorLike | None", default: "None", desc: [Color del punto y del tallo.]),
    (name: "knockout", type: "bool", default: "True", desc: [Una placa del color del fondo detrás del rótulo tapa los tallos que pasan por debajo.]),
  ),
  returns: (type: "TimelineEvent", desc: [`dot`, `stem` y `label` (y `title`, `body`, `side`, `lane`), objetos normales que se animan por separado; `parts` los da en orden para revelarlos.]),
  desc: [El rótulo crece a la derecha del tallo, o a la izquierda cuando pasaría del final del eje, y ocupa el primer carril de su lado donde guarda `gap` con los rótulos y reservas que ya hay; uno alto ocupa los carriles que necesite. `timeline.reserve(desde, hasta, side=, lane=None)` aparta esas fechas en un carril o en todos, para lo que se dibuja aparte: un rótulo que choca con una reserva de todos los carriles crece hacia el otro lado, y si tampoco cabe así, `"auto"` lo lleva al otro lado del eje y un lado fijo lanza `ValueError`. `timeline.x(fecha)` da la x de una fecha.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
line = scene.viz.timeline(2000, 2030)
line.reserve(2010, 2018, side="above")
hito = line.event(2014, "Reserva arriba: va abajo")
scene.play([part.animate.fade_in() for part in hito.parts])
```
]

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

== Mapas

Un mapa pone longitud y latitud sobre un rectángulo de la escena. No es un
dibujable: dibuja polígonos, líneas, archivos GeoJSON y retículas como
dibujables comunes en coordenadas de escena, con su estilo y sus animaciones de
siempre, y coloca otros objetos en una longitud y una latitud. Lo que se sale
de su marco se recorta.

```python
# show-code: true
from gaanim import Scene

scene = Scene(frame=(16, 9))
atlas = scene.viz.map(lat=(-19, 1), center_lon=-72)
grid = atlas.graticule(5, every=(10, 5))
countries = atlas.geojson("andes.geojson", key="name")
trench = atlas.polyline([(-81.2, 1.0), (-82.0, -5.0), (-78.2, -12.3), (-71.2, -19.0)])
lima = atlas.place(scene.geometry.dot(0.08), -77.03, -12.05)
scene.play([grid.animate.create(), countries.animate.create()], duration=1.0)
scene.play([countries["Peru"].animate.fill(scene.colors.accent), trench.animate.create(), lima.animate.fade_in()])
# output: preview.webp
scene.render()
```

Las dos proyecciones separan longitud y latitud (un meridiano es una línea
vertical y un paralelo, una horizontal) y usan una sola escala para ambas, así
que el mapa conserva sus proporciones. Mercator mantiene la forma de las áreas
pequeñas y las estira hacia los polos; la equirectangular espacia los
paralelos por igual.

Las longitudes valen en cualquier vuelta: cada punto toma la copia más cercana
al centro del mapa (`-170` y `190` son el mismo meridiano), y un anillo o una
línea que cruza el antimeridiano sigue entero y aparece en los dos bordes de un
mapa que lo alcanza.

Sin un estilo propio, las áreas se rellenan con el color `header` del tema y un
borde fino `rule`, las líneas toman su color de primer plano, la retícula el
color `rule` y sus rótulos el apagado a tamaño de pie de foto. Los selectores
de tema `map/area`, `map/line`, `map/graticule`, `map/labels` y `map/frame` los
cambian.

#api-entry(
  name: "Visualization.map",
  kind: "factory",
  params: (
    (name: "lat", type: "tuple[float, float]", default: none, desc: [Bordes sur y norte en grados, en cualquier orden.]),
    (name: "lon", type: "tuple[float, float] | None", default: "None", desc: [Bordes oeste y este. El este puede pasar de 180 para un mapa que cruza el Pacífico.]),
    (name: "center_lon", type: "float | None", default: "None", desc: [Longitud del centro cuando no se da `lon`; 0 si se omiten ambos.]),
    (name: "projection", type: "\"mercator\" | \"equirectangular\"", default: "\"mercator\"", desc: [Proyección.]),
    (name: "within", type: "\"safe\" | \"frame\" | Zone | Drawable | None", default: "None", desc: [Región del mapa; el área segura por defecto.]),
  ),
  desc: [Mapa de las latitudes `lat` en la región `within`. Con `center_lon`, el mapa llena la región: las latitudes fijan la escala y el ancho de la región fija los bordes oeste y este alrededor de esa longitud. Con `lon`, el mapa es tan grande como cabe en la región, centrado. El marco de Mercator llega a 85,05° como mucho; el equirectangular, hasta los polos. Latitudes iguales o fuera de esos límites, un `lon` cuyo oeste no queda antes de su este o que abarca más de 360°, un mapa que abarcaría más de 360° (una franja estrecha de latitudes en una región ancha: da `lon`), `lon` junto con `center_lon` u otra proyección lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
zones = scene.layout.zones(Zones.columns(2, padding="page_padding"))
andes = scene.viz.map(lat=(-19, 1), center_lon=-72, within=zones[0])
world = scene.viz.map(lat=(-60, 75), lon=(-180, 180), projection="equirectangular", within=zones[1])
print(andes.lon, world.scale)
```
]

#api-entry(
  name: "GeoMap.point / place",
  kind: "method",
  signature: "point(lon, lat) -> tuple[float, float] · place(drawable, lon, lat, *, anchor=None, offset=None) -> Drawable",
  desc: [`point` da dónde se dibuja `(lon, lat)`, en unidades de escena. `place` mueve el dibujable allí y lo devuelve: su centro, o el punto `anchor` de su caja, y luego `offset` más allá en unidades de escena. Una coordenada no finita lanza `ValueError`; un dibujable que posee un layout, `LayoutOwnershipError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>atlas = scene.viz.map(lat=(-19, 1), center_lon=-72)
x, y = atlas.point(-77.03, -12.05)
label = atlas.place(scene.text("Lima", role="label"), -77.03, -12.05, anchor="left", offset=(0.12, 0))
```
]

#api-entry(
  name: "GeoMap.polygon / polyline",
  kind: "method",
  signature: "polygon(rings) -> Drawable · polyline(points, *, closed=False) -> Drawable",
  desc: [`polygon` rellena un anillo de puntos `(lon, lat)`, o varios: el primero es el contorno, uno posterior dentro de él abre un agujero y uno aparte añade una parte (una isla), giren como giren. Un anillo puede repetir su primer punto al final, como en GeoJSON. Los vecinos comparten el borde exacto: el trazo va centrado en él. `polyline` traza una línea por puntos `(lon, lat)` con tramos rectos en el mapa (en Mercator, de rumbo constante); `closed=True` une el último con el primero. Un anillo de menos de tres puntos, una línea de menos de dos o una coordenada no finita lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>atlas = scene.viz.map(lat=(-19, 1), center_lon=-72)
outline = [(-80.0, -4.0), (-70.0, -4.0), (-70.0, -18.0), (-80.0, -18.0)]
lake = [(-70.5, -14.5), (-69.0, -14.5), (-69.0, -16.5), (-70.5, -16.5)]
region = atlas.polygon([outline, lake]).fill(GOLD).opacity(0.5)
route = atlas.polyline([(-77.0, -12.0), (-72.0, -13.5), (-68.2, -16.5)]).stroke(WHITE, 0.03)
```
]

#api-entry(
  name: "GeoMap.geojson",
  kind: "method",
  params: (
    (name: "source", type: "str | os.PathLike | Mapping | __geo_interface__", default: none, desc: [Ruta de un archivo, que se busca como los demás recursos, un mapeo GeoJSON o un objeto con `__geo_interface__` (una geometría de Shapely, un marco de GeoPandas).]),
    (name: "key", type: "str | None", default: "None", desc: [Propiedad que encuentra cada entidad por nombre; `"id"` también lee el `id` de cada entidad.]),
  ),
  desc: [Dibuja las entidades de un documento GeoJSON (una colección, una entidad o una geometría sola) y devuelve un `GeoFeatures`, un grupo con un dibujable por entidad en el orden del documento. Los polígonos y multipolígonos se rellenan como `polygon`, las líneas se trazan como `polyline`, los puntos son círculos, una colección de geometrías es un grupo y una entidad sin geometría, un grupo vacío. Las posiciones son `[longitud, latitud]` y se ignora una elevación detrás. Se quitan los puntos que mueven la línea dibujada menos de un cuarto de píxel y las partes que el mapa nunca muestra. Con `key`, cada entidad necesita esa propiedad como texto o número y ninguna puede repetirla. Un archivo que falta lanza `FileNotFoundError`; un GeoJSON inválido, un anillo o una clave mal dados, `ValueError`, y no se dibuja nada.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>atlas = scene.viz.map(lat=(-19, 1), center_lon=-72)
countries = atlas.geojson("andes.geojson", key="name")
countries["Peru"].fill(scene.colors.accent)
print(len(countries), countries.keys[:3], countries.properties[0]["iso"])
```
]

#api-entry(
  name: "GeoFeatures.keys / properties",
  kind: "property",
  signature: "keys -> list[str] | None · properties -> list[dict]",
  desc: [`GeoFeatures` es un `Drawable`: sus setters y animaciones actúan sobre todas las entidades. Se indexa por posición o, con `key=`, por clave, y también se recorre y admite `in`. `keys` da la clave de cada entidad, en orden, o `None` sin `key=`; `properties`, las propiedades de cada una como diccionarios nuevos. Una clave desconocida lanza `KeyError` y una posición fuera de rango, `IndexError`.],
  none,
)

#api-entry(
  name: "GeoMap.graticule",
  kind: "method",
  params: (
    (name: "step", type: "float | tuple[float, float] | None", default: "None", desc: [Grados entre líneas, o un par (longitud, latitud). Por defecto, un paso redondo que traza unas ocho líneas a lo ancho del mapa.]),
    (name: "every", type: "float | tuple[float, float] | None", default: "None", desc: [Rotula solo las líneas en sus múltiplos; por defecto, todas.]),
    (name: "labels", type: "str | Sequence[str] | None", default: "None", desc: [Lados con rótulos, como nombres o un texto separado por comas: meridianos en `"top"` y `"bottom"`, paralelos en `"left"` y `"right"`. `None` rotula la izquierda y abajo; `()`, ninguno.]),
    (name: "hemispheres", type: "str", default: "\"NSEW\"", desc: [Letras de norte, sur, este y oeste: `"NSEO"` en español.]),
  ),
  desc: [Traza meridianos y paralelos y los rotula. Devuelve un `Graticule`, un grupo con `lines` (todas las líneas en un trazado) y `labels` (un grupo de textos). Se omiten las líneas sobre los bordes del marco. Los rótulos van dentro del marco junto a su línea y se leen `"15°S"`, `"75°W"`, `"0°"` y `"180°"`; se omite el que taparía a otro o saldría del marco. Pasos no positivos, lados desconocidos o un `hemispheres` que no sean cuatro letras lanzan `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>atlas = scene.viz.map(lat=(-19, 1), center_lon=-72)
grid = atlas.graticule(5, every=(10, 5), labels="left,bottom", hemispheres="NSEO")
grid.lines.opacity(0.5)
```
]

#api-entry(
  name: "GeoMap.frame / projection / lon / lat / scale",
  kind: "property",
  signature: "frame -> Drawable · projection -> str · lon -> tuple[float, float] · lat -> tuple[float, float] · scale -> float",
  desc: [`frame` es el rectángulo donde está el mapa y que recorta sus dibujos: no dibuja nada hasta darle relleno (el mar) o trazo (un marco), y queda debajo de los dibujos del mapa. `lon` y `lat` son los bordes `(oeste, este)` y `(sur, norte)` en grados, y `scale`, las unidades de escena por grado de longitud.],
  none,
)

== Grafos y árboles

`scene.viz.graph` dibuja redes, árboles y diagramas de nodos a partir de sus
nodos y aristas, con la disposición calculada. Cada nodo es un grupo con un
círculo y su nombre, y cada arista una línea reactiva (una flecha si el grafo
es dirigido) que sale un poco fuera del borde de cada círculo y sigue a sus
nodos cuando se mueven, escalan o giran. Son drawables normales con nombre: el
nodo `"A"` o la arista `"A→B"`. Los cambios de estructura, como
`animate.add_node`, vuelven a disponer el grafo: los nodos que siguen se
mueven a su nuevo lugar y lo que entra o sale aparece o se desvanece.

#api-entry(
  name: "Visualization.graph",
  kind: "factory",
  params: (
    (name: "nodes", type: "Sequence[str] | None", default: "None", desc: [Nombres de los nodos, en orden; sin ellos, los que nombran las aristas, en el orden en que aparecen.]),
    (name: "edges", type: "Sequence[tuple[str, str]] | None", default: "None", desc: [Pares `(origen, destino)` de nombres.]),
    (name: "layout", type: "\"tree\" | \"layered\" | \"circular\" | \"force\"", default: "\"layered\"", desc: [Árbol ordenado, capas de Sugiyama, círculo o fuerzas.]),
    (name: "directed", type: "bool", default: "True", desc: [Flechas del primer nodo de cada arista al segundo; decide también las raíces de un árbol y el sentido de las capas.]),
    (name: "seed", type: "int", default: "0", desc: [Lugares de partida de `"force"`.]),
    (name: "node_radius", type: "float", default: "0.3", desc: [Radio mínimo de los nodos, que crecen para que quepa su nombre.]),
    (name: "labels", type: "bool", default: "True", desc: [Nombres dentro de los nodos.]),
    (name: "within", type: "\"safe\" | \"frame\" | Zone | Drawable | None", default: "None", desc: [Región que llena; por defecto, el área segura.]),
  ),
  desc: [Cada disposición se calcula solo a partir de los nodos y las aristas, así que el mismo grafo cae en los mismos lugares en cada ejecución y en cada `seek`. `"tree"` es un árbol ordenado que crece hacia abajo: las raíces son los nodos a los que no llega ninguna arista (el primero de cada parte si no es dirigido), cada nodo cuelga del primero que lo alcanza a lo ancho, las hojas ocupan columnas consecutivas y cada padre queda sobre el centro de sus hijos. En un grafo dirigido, `"layered"` pone cada nodo en la capa de la cadena de aristas más larga que llega a él y cuenta al revés las aristas que cierran un ciclo; sin dirección, en la capa de cuántas aristas lo separan del primer nodo de su parte. Una arista que salta capas reserva un lugar en cada capa que cruza, para que ningún nodo quede en su camino, y cada capa se reordena según sus vecinos para que se crucen menos aristas. `"circular"` reparte los nodos en un círculo, el primero arriba y en sentido horario, y `"force"` deja que las aristas atraigan a sus nodos y que todos se repelan, un número fijo de pasos. La disposición llena la región sin pasar de 2,4 unidades entre vecinos; árboles y capas se estiran a la región, y círculos y fuerzas conservan sus proporciones. `nodes` y `edges` dan la estructura actual, `node(name)` o `graph[name]` el nodo y `edge(origen, destino)` la arista. Sin nodos, un nombre repetido, una arista a un nodo desconocido o de un nodo a sí mismo, un `node_radius` no positivo o otro `layout` lanzan `ValueError`; `nodes` o `edges` con otra forma, `TypeError`; un nombre o una arista desconocidos, `KeyError`.],
)[
```python
# show-code: true
# player
from gaanim import Scene
scene = Scene(frame=(16, 9))
tree = scene.viz.graph(
    nodes=["raíz", "a", "b", "a1", "a2"],
    edges=[("raíz", "a"), ("raíz", "b"), ("a", "a1"), ("a", "a2")],
    layout="tree",
)
scene.play([tree.animate.fade_in().duration(0.5)])
scene.play([tree.animate.add_node("b1", edges=[("b", "b1")])])
scene.wait(0.5)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "GraphAnimation.to / add_node / remove_node / add_edge / remove_edge",
  kind: "method",
  signature: "to(nodes, edges=None, *, duration=None) -> Composition · add_node(name, edges=None, *, duration=None) -> Composition · remove_node(name, *, duration=None) -> Composition · add_edge(source, target, *, duration=None) -> Composition · remove_edge(source, target, *, duration=None) -> Composition",
  desc: [Cambian la estructura y vuelven a disponer el grafo entero. Los nodos que siguen se mueven a su lugar nuevo; los nodos y aristas nuevos aparecen en el suyo y los que se quitan se desvanecen, todo a la vez, mientras las aristas siguen a sus nodos. Los lugares están en el marco del propio grafo: si el grafo se movió, escaló o giró, las piezas quedan dentro de él, y un nodo movido a mano se mueve lo mismo que su lugar y conserva su desplazamiento. Las piezas nuevas quedan ocultas hasta que el cambio se reproduce, y los cambios siguientes, `nodes`, `edges`, `node` y `edge` parten de la estructura nueva enseguida. En `to`, una arista que las dos estructuras tienen se conserva, en orden. `duration` fija la duración de cada parte; sin ella, manda la de `Scene.play`. `animate` conserva además todo el vocabulario de `Drawable.animate`. Quitar un nodo o una arista que no existe lanza `KeyError`; una estructura inválida o una `duration` no positiva, `ValueError`.],
)[
```python
# show-code: true
# player
from gaanim import Scene
scene = Scene(frame=(16, 9))
flow = scene.viz.graph(edges=[("leer", "validar"), ("validar", "guardar"), ("validar", "error")])
scene.play([flow.animate.fade_in().duration(0.5)])
scene.play([flow.animate.add_edge("error", "leer")])
scene.play([flow.animate.remove_node("error")], duration=0.8)
scene.play([flow.edge("validar", "guardar").animate.fill(scene.canvas.color("accent"))])
scene.wait(0.5)
# output: preview.webp
scene.render()
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
  desc: [Tipo de marca: `point`, `line`, `step`, `area`, `bar`, `histogram`, `box`, `violin`, `error_bar`, `heatmap`, `surface` o `arc`. Las opciones dependen de la marca: las barras aceptan `width` (fracción de la banda en ejes categóricos), `label_position` (`"outside"` o `"inside"`), `label_offset` y `label_color`; los puntos, `radius`; `heatmap`, `cell_width` y `cell_height` (tamaño de celda en unidades de dato; por defecto, el paso más pequeño entre valores distintos de `x` y de `y`, así que las celdas cubren el área sin huecos, y 1 con una sola columna o fila) y `bands` (niveles de color, 12 por defecto). `arc` dibuja una tarta sin ejes, una porción por fila según su parte del total de `theta`, en el sentido del reloj desde las 12; acepta `inner_radius` (fracción del radio que queda vacía, en `[0, 1)`: un donut si es mayor que 0), `pad_angle` (radianes vacíos entre porciones), `start_angle` (radianes en sentido antihorario desde el eje x; `math.pi / 2` por defecto), `radius` (unidades de escena) y `label_color`. Las marcas rellenas (`bar`, `histogram`, `area`, `point`, `box`, `violin` y las celdas de `heatmap`) se dibujan sin borde; para trazarlo, usa `chart.layer("marks").stroke(...)`.],
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
  desc: [Asigna canales (`x`, `y`, `z`, `color`, `size`, `opacity`, `label`, `theta`, `radius`). Cada canal acepta un nombre de columna, `Field(columna, scale=...)` o `Value(constante)`. En `point`, `size` es el radio en unidades de escena (0.06 por defecto); un `Field` numérico lo reparte entre 0.03 y 0.12 con área proporcional. `color` y `opacity` con `Field` valen por fila; en `line`, `step` y `area`, un `color` categórico dibuja una serie por categoría. `theta` y `radius` son de `arc`: `theta` (obligatorio, no negativo; un valor ausente no ocupa sitio) da a cada fila su parte del círculo, y un campo `radius` dibuja un gráfico de rosa, con cada porción desde el radio interior hacia el exterior en proporción a su valor.],
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
  desc: [Copia de la escala con un rango ordenado de colores: una `Palette`, el nombre de una paleta (`"okabe_ito"`) o colores (`Color`, cadenas CSS/hex o tuplas RGB(A), incluso mezclados). Colores inválidos, nombres desconocidos o una lista vacía lanzan `ValueError`. En un campo de texto, los colores se reparten entre las categorías por turno y vuelven a empezar como una paleta; sin ellos, las categorías toman la paleta del tema. En un campo numérico forman una rampa. En un `heatmap`, los colores y el dominio de la escala del canal `color` sustituyen a la paleta `heatmap` del tema solo en ese gráfico, y `bands=` cuantiza sobre ellos.],
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
  signature: "chart(spec, *, width=None, height=None) -> Chart",
  params: (
    (name: "spec", type: "ChartSpec", default: none, desc: [La receta del gráfico.]),
    (name: "width, height", type: "float | None", default: "None", desc: [Tamaño del área del gráfico en unidades de escena, como en `cartesian_2d`. El texto conserva los tamaños del tema, a diferencia de `scale_to` sobre el gráfico ya hecho. Un lado sin valor toma el 72 % del ancho del encuadre o el 62 % de su alto. El radio de un gráfico de arcos se ajusta al lado menor, y `chart.animate.to(...)` conserva el tamaño. Un lado no finito o no positivo lanza `ValueError`.]),
  ),
  desc: [Materializa la receta en un `Chart` con capas semánticas por lotes: las marcas se agrupan por estilo resuelto en lugar de crear una entidad por registro.],
)[
```python
>>>from gaanim import *
scene = Scene(frame=(16, 9))
spec = ChartSpec({"x": [1, 2, 3], "y": [2, 5, 3]}).mark("bar").encode(x="x", y="y")
panel = scene.viz.chart(spec, width=6.0, height=2.8).move_to(-3.5, 0)
```
]

#api-entry(
  name: "Chart.layer",
  kind: "method",
  desc: [Capa estable como `Drawable`: `marks`, `axes`, `grid`, `guides` o `labels`. La capa `axes` reúne rejilla, ejes, marcas, números y títulos, así que `chart.layer("axes").animate.create()` la mantiene oculta hasta su entrada. Las tres últimas son opcionales: `grid` existe si los ejes dibujan rejilla, `guides` si la especificación tiene leyenda o título de guía (`ChartSpec.guides`) y `labels` en gráficos de barras o de arcos con el canal `label`. La capa `axes` de un gráfico de arcos está vacía. Pedir una que el gráfico no tiene lanza `ValueError` con las capas que sí tiene.],
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
  name: "ChartAnimation.grow",
  kind: "method",
  returns: (type: "Composition", desc: [Las animaciones de la entrada, en paralelo.]),
  desc: [Entrada de un gráfico de arcos, desde `chart.animate.grow()`: una manecilla gira en el sentido del reloj desde el ángulo inicial, acelerando y frenando, y cada porción se abre en su sitio cuando la manecilla la cruza. La etiqueta de una porción aparece desde que la manecilla pasa por su centro hasta que la porción se completa; las guías aparecen a lo largo de toda la entrada. En otro tipo de gráfico lanza `ValueError`.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
sales = {"region": ["North", "South", "East"], "sales": [42, 28, 18]}
spec = ChartSpec(sales, key="region").mark("arc", inner_radius=0.55).encode(theta="sales", color="region", label="region")
chart = scene.viz.chart(spec)
scene.play([chart.animate.grow()], duration=1.5)
later = ChartSpec({"region": ["North", "South", "West"], "sales": [30, 35, 20]}, key="region")
scene.play([chart.animate.to(later.mark("arc", inner_radius=0.55).encode(theta="sales", color="region", label="region"))])
```
]

#api-entry(
  name: "ChartAnimation.to",
  kind: "method",
  params: ((name: "target", type: "ChartSpec", default: none, desc: [Receta de destino.]), (name: "match_", type: "str", default: "\"key\"", desc: [`"key"` relaciona elementos por la columna de identidad (ambas recetas deben declararla); sin clave, pide `"index"`.]), (name: "fallback", type: "str", default: "\"error\"", desc: [Con familias de marca incompatibles: `"error"` o `"crossfade"`.])),
  desc: [Transición accesible desde `chart.animate.to(...)`. Entre gráficos 2D, las marcas, ejes y etiquetas que se corresponden se transforman y el resto se funde; al terminar, el gráfico de destino sustituye al original. `point`, `line` y `bar` pueden pasar entre 2D y 3D (con un fundido), y `heatmap` y `surface` comparten rejilla. Entre dos gráficos de arcos, cada porción barre sus ángulos y radios hasta la porción con su clave (su fila con `match_="index"`); una porción sin pareja se cierra sobre su ángulo medio o se abre desde él. Nunca mueve la cámara global.],
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
  params: ((name: "x / y / z", type: "Axis", default: none, desc: [Ejes lineales o temporales reutilizables.]), (name: "size", type: "(float, float, float)", default: "(10.0, 8.0, 6.0)", desc: [Tamaño positivo en el mundo; elígelo según la distancia de la cámara.]), (name: "xy_grid / xz_grid / yz_grid", type: "bool | None", default: "None", desc: [Visibilidad de cada plano; `None` hereda `grid`.]), (name: "x_axis / y_axis / z_axis, x_ticks / …, x_numbers / …, x_labels / …", type: "bool | None", default: "None", desc: [Eje, marcas, números y título de cada eje, con el prefijo `x_`, `y_` o `z_`; `None` hereda el interruptor global.]), (name: "sides", type: "\"view\" | \"fixed\"", default: "\"view\"", desc: [Dónde van los planos y los ejes en la caja. Con `"view"` siguen a la cámara: cada plano de la rejilla va a la cara que queda detrás del contenido y cada eje a una arista del contorno de la caja. Con `"fixed"` se quedan en las caras más bajas y en las aristas que salen de la esquina más baja. Otro valor lanza `ValueError`.])),
  desc: [Devuelve un `Cartesian3D` (un `CoordinateSpace3D`) con capas estables. Las rejillas ocultas no emiten segmentos y ninguna línea de la rejilla se dibuja sobre un eje ni sobre otra; las etiquetas miran a la cámara. Con `sides="view"`, como en las bibliotecas de gráficos, la rejilla nunca cruza por delante de lo que contiene la caja: los dos ejes perpendiculares al `up` de la cámara recorren las aristas del suelo más cercanas a ella, el eje vertical sube por la arista izquierda del contorno, y sus marcas, números y títulos van con ellos. Cada título queda junto al centro de su eje, pasados sus números, y en la esquina donde se juntan dos ejes solo escribe su número el primero. Cuando la cámara cruza el plano que pasa por el centro de la caja, saltan al otro lado.],
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
  desc: [Superficie `z = f(x, y)` muestreada en una rejilla de `resolution`, con `inputs` reactivas como `plot`. Los vértices se colorean por altura, de la más baja a la más alta, con `color_map`: un `ColorMap` o el nombre de un mapa integrado, como `"viridis"` o `"plasma"`. Sin él, usa una rampa de azul, verde y rojo. Con `vectorized=True`, `function` se llama una vez para toda la rejilla, cada vez que cambian sus entradas, en lugar de una vez por vértice: `x` e `y` son arrays de numpy de solo lectura con un valor por vértice, y devuelve el array de alturas (o un número para todas); una altura `NaN` deja un hueco. Necesita numpy y es mucho más rápido en rejillas grandes o animadas. Con `material` la superficie se ilumina por las dos caras con `lighting_3d`, conserva sus colores y proyecta y recibe sombras; sin él no se ilumina.],
)[
```python
>>>from gaanim import *
>>>scene = Scene(frame=(16, 9))
>>>world = scene.viz.cartesian_3d(Axis.linear(-3, 3), Axis.linear(-3, 3), Axis.linear(-1, 1))
saddle = world.surface(lambda x, y: 0.1 * (x * x - y * y), resolution=(32, 24))
relief = world.surface(lambda x, y: 0.1 * (x * x - y * y), material=Material3D.matte())
```
]

#api-entry(
  name: "CoordinateSpace3D.parametric_surface",
  kind: "method",
  desc: [Superficie paramétrica `(u, v) → (x, y, z)` muestreada sobre `u_domain` × `v_domain` en una rejilla de `resolution`, con `inputs` reactivas como `surface`: la malla se vuelve a muestrear solo cuando cambian, así que una superficie puede doblarse en otra. `color(u, v)` da el valor con que se colorea cada vértice a través de `color_map`. Sin `color_inputs` se muestrea una sola vez, de modo que el color queda pintado sobre los mismos puntos del material aunque la forma cambie; con ellas, `color(u, v, *color_inputs)` se vuelve a muestrear cuando cambian. `color_range` fija los valores de los dos extremos del mapa (por defecto, el menor y el mayor). Sin `color`, colorea por altura. Con `vectorized=True`, `function` y `color` se llaman una vez para toda la rejilla en lugar de una vez por vértice: `u` y `v` son arrays de numpy de solo lectura con un valor por vértice, y la función devuelve los arrays `(x, y, z)` (o uno de forma `(vértices, 3)`); un `NaN` deja un hueco. Necesita numpy y es mucho más rápido en rejillas grandes. Con `material` la superficie se ilumina por las dos caras con `lighting_3d` y conserva sus colores; sin él no se ilumina.],
)[
```python
>>>from gaanim import *
>>>import math
>>>scene = Scene(frame=(16, 9))
>>>world = scene.viz.cartesian_3d(Axis.linear(-2, 2), Axis.linear(-2, 2), Axis.linear(-1, 1))
tube = world.parametric_surface(
    lambda u, v: (math.cos(u), math.sin(u), v),
    (0, math.tau),
    (-1, 1),
    color=lambda u, v: v,
    color_map="viridis",
    material=Material3D(roughness=0.5),
)
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
