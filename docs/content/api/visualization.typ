#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "API de visualización",
  description: "Gráficos inmutables y espacios científicos tipados en 2D y 3D",
  route: "/api/visualization/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Visualización

La API de visualización separa dos problemas que se parecen en pantalla, pero
se construyen de forma distinta:

- `ChartSpec` es una gramática inmutable para contar historias con tablas de
  datos: define codificaciones, marcas, escalas, ejes, guías y transiciones con
  identidad estable.
- `Cartesian2D`, `Cartesian3D`, `PolarSpace` y `ComplexSpace` son espacios
  científicos tipados para funciones, cálculo, campos vectoriales y geometría
  personalizada.

La regla práctica es sencilla: usa `ChartSpec` cuando cada fila representa una
observación; usa un espacio tipado cuando las coordenadas tienen significado
matemático continuo. Ambos producen objetos normales que pueden recibir temas,
participar en Layout y animarse en la misma línea de tiempo.

Al construir un `ChartSpec`, Gaanim captura inmediatamente una copia de mappings,
dataframes, `DataTable` o `DataSource`. Una mutación externa posterior no puede
cambiar el resultado de una búsqueda anterior en la línea de tiempo.

```python
from gaanim import Axis, ChartSpec, Field, Guide, Scale, Scene, Value, BLUE

scene = Scene(frame=(16, 9))
spec = (
  ChartSpec({
    "id": ["a", "b", "c"],
    "x": [-2, 0, 2],
    "y": [1, 3, 2],
    "group": ["A", "B", "A"],
  }, key="id")
  .mark("point")
  .encode(
    x="x",
    y="y",
    color=Field("group", scale=Scale.category()),
    size=Value(8),
  )
  .axes(
    x=Axis.linear(-3, 3).ticks(1).label("x"),
    y=Axis.linear(0, 4).ticks(1).label("y"),
  )
  .guides(color=Guide.legend(title="Grupo"))
)
chart = scene.viz.chart(spec)
scene.play([chart.drawable().animate.create()])
```

== ChartSpec

Piensa en `ChartSpec` como una receta, no como el gráfico materializado. Cada
método devuelve una receta nueva. Esto permite conservar un estado inicial y
derivar de él el siguiente estado sin mutaciones ocultas:

```python
bars = base.mark("bar").encode(x="category", y="value")
points = base.mark("point").encode(x="time", y="value")
```

Las barras admiten categorías de texto directamente: `x="method"` centra cada
barra en su propia banda, incluyendo la primera y la última. Su `width` es una
fracción de esa banda para ejes categóricos. Añade `label="value"` sólo cuando
quieras rótulos; se materializan en la capa opcional `labels`. Las opciones de
barra `label_position="outside"` (predeterminado) o `"inside"`,
`label_offset=16` en unidades locales y `label_color=...` controlan esos
rótulos. Los valores negativos se colocan respecto a la línea base.

```python
from gaanim import BLUE, GOLD, ChartSpec, Field, Scale

bars = (
  ChartSpec({"method": ["one", "two"], "elapsed": [40, -10], "kind": ["new", "old"]})
  .mark("bar", width=0.72, label_position="outside", label_offset=16)
  .encode(
    x="method", y="elapsed", label="elapsed",
    color=Field("kind", scale=Scale.category().colors([BLUE, GOLD])),
  )
)
```

Un `Value(BLUE)` conserva un único color técnico para toda la serie. Un
`Field(..., scale=Scale.category(...).colors(...))` resuelve un color por fila
y agrupa internamente las barras que comparten ese color.

#api-entry(
  name: "ChartSpec",
  kind: "builder",
  signature: "ChartSpec(data, *, key=None) -> ChartSpec",
  params: (
    (name: "data", type: "mapping | dataframe | DataTable | DataSource", default: none, desc: [Entrada capturada inmediatamente como una instantánea inmutable propia.]),
    (name: "key", type: "str | None", default: "None", desc: [Columna de identidad estable. Los valores nulos o duplicados producen un error inmediato.]),
  ),
  returns: (type: "ChartSpec", desc: [Especificación declarativa e inmutable de un gráfico.]),
  desc: [`mark`, `encode`, `axes` y `guides` devuelven especificaciones nuevas; nunca modifican la existente.],
)[
Los canales disponibles son `x`, `y`, `z`, `color`, `size`, `opacity` y
`label`. Un canal acepta el nombre de una columna, `Field(column, scale=...)`
o `Value(constant)`. Usa `Field` cuando el valor cambia por fila y `Value`
cuando todos los elementos comparten el mismo valor.

Las marcas son `point`, `line`, `step`, `area`, `bar`, `histogram`, `box`,
`violin`, `error_bar`, `heatmap` y `surface`. `point`, `line` y `bar` pueden
transformarse entre representaciones 2D y 3D; `heatmap` y `surface` pueden
compartir una misma cuadrícula.
]

== Escalas, ejes y guías

`Scale.linear`, `log`, `symlog`, `power`, `time` y `category` configuran una
codificación. La misma escala gobierna posiciones normalizadas, colores,
leyendas y barras de color. `Axis` es el constructor visual e inmutable del eje
y admite las mismas familias numéricas, temporales y categóricas.

Elige la escala según el significado, no según la apariencia: `linear` para
diferencias uniformes, `log` para órdenes de magnitud positivos, `symlog` para
datos con signo alrededor de cero, `time` para fechas y `category` para grupos
discretos.

La tipografía visual predeterminada usa 32 unidades para números de ticks y 36
para títulos de eje, de modo que siga siendo legible al reducir un vídeo 1080p.
Los selectores de tema `axes/numbers` y `axes/labels` permiten sustituir estos
valores globalmente.

En espacios cartesianos 2D, la posición predeterminada `position="end"` coloca
el título horizontal fuera del extremo positivo de su propio eje, incluso si
usa varias líneas; `"start"` hace lo mismo en el extremo negativo. Usa
`position="center"` (o sus alias `"middle"` y `"mid"`) para obtener títulos
centrados en la disposición convencional: x debajo de sus ticks e y a la
izquierda, girado 90 grados. En un eje vertical, `"top"` y `"bottom"` son
alias de los extremos.

```python
color = Field("temperature", scale=Scale.symlog((-100, 100), threshold=1))
x = Axis.log(0.1, 1000, base=10).ticks(10).label("frequency")
y = Axis.linear(0, 1).label("relative value", position="top")
guide = Guide.colorbar(title="temperature")
```

== Gráfico materializado y transiciones

`scene.viz.chart(spec)` materializa la receta y devuelve un `Chart`. Sus capas
estables son `marks`, `axes`, `grid`, `guides` y la opcional `labels`; cada una
se comporta como un objeto dibujable normal. Las marcas se agrupan por estilo
resuelto (por ejemplo, un lote por color de barra repetido), en lugar de crear
una entidad ECS por registro. La capa `axes` reúne la estructura completa del
sistema de coordenadas —rejilla, ejes, marcas, números y títulos—, por lo que
`chart.layer("axes").animate.create()` la mantiene oculta hasta que comienza su
entrada en `scene.play`.

La opacidad del gráfico se propaga por las capas vectoriales y las mallas 3D
nativas. Por eso `fade_in`, `fade_out` y la opacidad de un padre mantienen el
mismo comportamiento en escenas mixtas. Si Gaanim infiere los ejes de un gráfico
de barras, incluye automáticamente la línea base numérica y reserva espacio en
los extremos. Un dominio definido explícitamente nunca se modifica.

```python
target = spec.encode(z="height").axes(z=Axis.linear(-2, 2))
scene.play([
  chart.animate.to(target).duration(1.4),
  scene.camera.animate.look_at(eye=(8, 6, 8), target=(0, 0, 0)).duration(1.4),
])
```

Por defecto, una transición relaciona elementos mediante `key`; ambas
especificaciones deben declarar la misma columna de identidad válida. Sin una
clave, solicita explícitamente `match_="index"`. Las familias de marcas
incompatibles producen un error, salvo que indiques
`fallback="crossfade"`. `Chart.to` nunca mueve implícitamente la cámara global.

`chart.inspect(fields=(...), format="...")` activa metadatos de inspección en
la vista previa. Esa configuración no aparece en capturas ni exportaciones.

== Espacios científicos tipados

Un espacio tipado conserva la relación entre datos y lienzo. En vez de convertir
manualmente cada valor a píxeles, describes los dominios mediante `Axis` y
trabajas siempre en coordenadas científicas. Al mover o escalar el espacio,
curvas, puntos, etiquetas y construcciones de cálculo permanecen unidos.

```python
import math
from gaanim import Axis, Scene

scene = Scene()
plane = scene.viz.cartesian_2d(Axis.linear(-6, 6), Axis.linear(-3, 3))
a = scene.viz.parameter(1.0)
curve = plane.plot(lambda x, amplitude: amplitude * math.sin(x), inputs=[a])

world = scene.viz.cartesian_3d(
  Axis.log(0.1, 1000),
  Axis.symlog(-100, 100),
  Axis.power(0, 16, 0.5),
)
surface = world.surface(lambda x, y: x * y)
```

`Cartesian2D` ofrece `plot`, `parametric`, `implicit`, `contour` y
`field`, además de construcciones de cálculo. `Cartesian3D` ofrece
`surface`, `parametric` y `field`. Sus capas `grid`, `axes`, `ticks`,
`numbers` y etiquetas billboard conocen la escala y pueden estilizarse por
separado. `scene.viz.polar(...)`, `scene.viz.complex(...)` y
`scene.viz.number_line(...)` cubren los demás espacios tipados.

`Cartesian2D.plot(function, ..., derivative=None, inputs=())` recibe primero
la coordenada y después los valores declarados. Si
se solicita una derivada, `derivative` debe ser otro callable con la misma firma;
no hay diferenciación simbólica ni aproximación numérica implícita.
`Cartesian2D.parametric(..., inputs=())`, `Cartesian3D.parametric(...,
inputs=())` y `Cartesian3D.surface(..., inputs=())` usan el mismo orden y se
regeneran cuando cambia el snapshot.

Todos esos espacios heredan por defecto los colores semánticos
`axes/axis`, `axes/ticks`, `axes/grid`, `axes/numbers` y `axes/labels` del tema
activo. `Axis.style(...)` solo sustituye las propiedades proporcionadas; las
omitidas continúan heredándose del tema. Así, cambiar únicamente `width` no
pierde el negro de los ejes en `paper`. Usa `color`, `tick_color`,
`number_color` o `label_color` para sustituir explícitamente cada color.

`Cartesian2D.animate.write().duration(seconds)` construye en paralelo los ejes, las guías, los
ticks, los números y los títulos. Durante el trazado, las guías asociadas a X
avanzan de arriba hacia abajo y las asociadas a Y de izquierda a derecha.

=== Visibilidad de componentes

Los constructores de espacios tipados aceptan switches globales para sus capas
semánticas. En cartesianos, un override por eje distinto de `None` sustituye el
switch global correspondiente. Por ejemplo, `grid=False, x_grid=True` conserva
únicamente las guías verticales generadas por los ticks de X. En 3D las grillas
se seleccionan geométricamente mediante `xy_grid`, `xz_grid` y `yz_grid`.

`numbers` controla el texto de los ticks; `labels` controla exclusivamente los
títulos definidos con `Axis.label(...)`. Ocultar una capa no altera dominios ni
conversiones de coordenadas. `space.layer(...)` sigue devolviendo un `Drawable`
vacío, de modo que el conjunto de capas permanece estable para composición y
animación.

#api-entry(
  name: "Visualization.cartesian_2d",
  kind: "method",
  signature: "cartesian_2d(x, y, *, width=None, height=None, grid=True, axes=True, ticks=True, numbers=True, labels=True, x_axis=None, y_axis=None, x_grid=None, y_grid=None, x_ticks=None, y_ticks=None, x_numbers=None, y_numbers=None, x_labels=None, y_labels=None) -> Cartesian2D",
  params: (
    (name: "grid / axes / ticks / numbers / labels", type: "bool", default: "True", desc: [Valores globales para las capas semánticas.]),
    (name: "x_* / y_*", type: "bool | None", default: "None", desc: [Overrides por eje; `None` hereda el valor global.]),
  ),
  returns: (type: "Cartesian2D", desc: [Espacio tipado con todas sus capas direccionables, incluso cuando están vacías.]),
  desc: [El ancho y alto conservan sus defaults actuales basados en `safe_frame`.],
)[]

#api-entry(
  name: "Visualization.cartesian_3d",
  kind: "method",
  signature: "cartesian_3d(x, y, z, *, size=(10.0, 8.0, 6.0), grid=True, axes=True, ticks=True, numbers=True, labels=True, x_axis=None, y_axis=None, z_axis=None, xy_grid=None, xz_grid=None, yz_grid=None, x_ticks=None, y_ticks=None, z_ticks=None, x_numbers=None, y_numbers=None, z_numbers=None, x_labels=None, y_labels=None, z_labels=None) -> Cartesian3D",
  params: (
    (name: "xy_grid / xz_grid / yz_grid", type: "bool | None", default: "None", desc: [Visibilidad de cada plano; `None` hereda `grid`.]),
    (name: "x_* / y_* / z_*", type: "bool | None", default: "None", desc: [Overrides de ejes, ticks, números y títulos.]),
  ),
  returns: (type: "Cartesian3D", desc: [Espacio 3D con capas estables y escala consciente de los ejes.]),
  desc: [Las grillas desactivadas no emiten segmentos ni duplican las aristas que pertenecen a los ejes.],
)[]

#api-entry(
  name: "Visualization.polar",
  kind: "method",
  signature: "polar(radial, *, radius=220.0, angle_divisions=12, grid=True, axes=True, numbers=True, labels=True, rings=None, spokes=None) -> PolarSpace",
  params: (
    (name: "rings / spokes", type: "bool | None", default: "None", desc: [Anillos y radios; `None` hereda `grid`.]),
    (name: "labels", type: "bool", default: "True", desc: [Muestra el título del eje radial cuando existe.]),
  ),
  returns: (type: "PolarSpace", desc: [Espacio polar con capas `grid`, `axes`, `numbers` y `labels`.]),
  desc: [`numbers` afecta sólo a los valores radiales y `labels` al título radial.],
)[]

#api-entry(
  name: "Visualization.number_line",
  kind: "method",
  signature: "number_line(axis, *, length=None, axis_visible=True, ticks=True, numbers=True, labels=True) -> NumberLine",
  params: (
    (name: "axis_visible / ticks / numbers / labels", type: "bool", default: "True", desc: [Visibilidad independiente de cada componente.]),
  ),
  returns: (type: "NumberLine", desc: [Recta tipada cuya transformación de datos permanece activa aunque no dibuje componentes.]),
  desc: [`axis_visible` evita colisionar con el argumento existente `axis`.],
)[]

#api-entry(
  name: "Visualization.complex",
  kind: "method",
  signature: "complex(x=None, y=None, *, width=None, height=None, grid=True, axes=True, ticks=True, numbers=True, labels=True, x_axis=None, y_axis=None, x_grid=None, y_grid=None, x_ticks=None, y_ticks=None, x_numbers=None, y_numbers=None, x_labels=None, y_labels=None) -> ComplexSpace",
  params: (
    (name: "x / y", type: "Axis | None", default: "None", desc: [Ejes opcionales; al omitirse se crean los ejes `Re` e `Im`.]),
    (name: "switches y overrides", type: "bool | None", default: "True / None", desc: [Misma precedencia que `cartesian_2d`.]),
  ),
  returns: (type: "ComplexSpace", desc: [Plano complejo con conversión y capas cartesianas tipadas.]),
  desc: [Los títulos predeterminados pueden ocultarse mediante `labels=False` o los overrides X/Y.],
)[]

```python
plane = scene.viz.cartesian_2d(
  Axis.linear(-4, 4).ticks(1).label("x"),
  Axis.linear(-2, 2).ticks(1).label("y"),
  grid=False, x_grid=True,
  numbers=False, y_numbers=True,
  labels=False, x_labels=True,
)

polar = scene.viz.polar(
  Axis.linear(0, 4).ticks(1).label("r"),
  grid=False, rings=True, axes=False, numbers=False,
)
```

```python
plane = scene.viz.cartesian_2d(
  Axis.linear(-6, 6).label("x"),
  Axis.linear(-3, 3).label("y"),
)
scene.play([plane.animate.write().duration(1.2)])
```

`Parameter`, `Variable`, `Computed` y `scene.viz.time` forman la ruta reactiva. Rust
resuelve sus valores en un snapshot estable y llama la función Python con las
coordenadas primero y los valores de `inputs=` después.

== Campos vectoriales y líneas de corriente

`space.field(function, inputs=())` separa la función matemática de sus representaciones.
El mismo `VectorField` puede producir flechas y líneas de corriente sin volver
a definir el campo. La función puede usar `math`, helpers y control de flujo, y
recibe las coordenadas antes de las entradas explícitas. Las flechas y líneas se
regeneran desde el snapshot resuelto por el timeline; `field.evaluation` vale
`"python"` para esta ruta.

```python
from gaanim import Axis, Scene

scene = Scene()
plane = scene.viz.cartesian_2d(
  Axis.linear(-4, 4).ticks(1),
  Axis.linear(-3, 3).ticks(1),
)
field = plane.field(lambda x, y: (-y, x))
arrows = field.arrows(resolution=(18, 12), colormap="batlow")
streams = field.streamlines(
  seeds=(16, 10), direction="both", tolerance=1e-5,
  max_time=3.5, separation=0.045, colormap="vik",
)
scene.play([plane.animate.create(), arrows.animate.write(), streams.animate.write()])
scene.play(streams.flow(3.0, time_width=0.12))
```

#api-entry(
  name: "Cartesian2D.field / Cartesian3D.field",
  kind: "method",
  signature: "field(function, *, inputs=()) -> VectorField",
  params: (
    (name: "function", type: "Callable", default: none, desc: [Devuelve dos componentes en 2D o tres en 3D; recibe coordenadas y luego valores de `inputs`.]),
    (name: "inputs", type: "Sequence[Parameter | Variable | Computed | TimeInput]", default: "()", desc: [Entradas reactivas explícitas en orden.]),
  ),
  returns: (type: "VectorField", desc: [Evaluador reutilizable asociado al dominio y la transformación del espacio.]),
  desc: [No dibuja por sí solo. Usa `arrows`, `streamlines` o las operaciones de advección del campo. La propiedad `evaluation` vale `"python"`.],
)[]

#api-entry(
  name: "VectorField.arrows",
  kind: "method",
  signature: "arrows(*, resolution=None, min_length=0, max_length=None, length_scale=1, width=2, tip_length=None, tip_width=None, color=None, colormap=None, color_range=None) -> ArrowVectorField",
  params: (
    (name: "resolution", type: "(int,int) | (int,int,int) | None", default: "None", desc: [Muestras regulares por eje; los valores predeterminados dependen de la dimensión.]),
    (name: "min_length / max_length", type: "float", default: "0 / automático", desc: [Límites en unidades locales después de transformar el vector desde coordenadas de datos.]),
    (name: "color / colormap", type: "ColorLike | ColorMapLike | None", default: "None / viridis", desc: [Opciones mutuamente excluyentes. El mapa usa la magnitud del campo.]),
    (name: "color_range", type: "(float,float) | None", default: "None", desc: [Dominio explícito de magnitudes; si se omite se obtiene de las muestras finitas.]),
  ),
  returns: (type: "ArrowVectorField", desc: [Grupo retenido con astas y puntas explícitas en 2D o 3D.]),
  desc: [Valida resolución, longitudes y rangos antes de crear geometría. El grupo ofrece `create`, `write`, `fade_in`, `fade_out`, `uncreate`, `unwrite`, `grow_from_center` y `shrink_to_center`.],
)[]

#api-entry(
  name: "VectorField.streamlines",
  kind: "method",
  signature: "streamlines(*, seeds=None, direction=\"both\", tolerance=1e-4, min_step=1e-5, max_step=.1, max_time=3, max_length=None, max_steps=10000, stagnation=1e-10, padding=.05, separation=.035, width=2, opacity=1, color=None, colormap=None, color_range=None) -> StreamLines",
  params: (
    (name: "seeds", type: "(int,int) | (int,int,int) | None", default: "None", desc: [Resolución determinista de candidatos de semilla.]),
    (name: "direction", type: "forward | backward | both", default: "both", desc: [Sentido temporal de integración desde cada semilla.]),
    (name: "tolerance / min_step / max_step", type: "float", default: "1e-4 / 1e-5 / .1", desc: [Control adaptativo Dormand–Prince RK45.]),
    (name: "max_time / max_length / max_steps", type: "float | int", default: "3 / None / 10000", desc: [Límites finitos que hacen reproducible cada trayectoria.]),
    (name: "padding / separation", type: "float", default: ".05 / .035", desc: [Margen normalizado del dominio y distancia mínima de cobertura.]),
  ),
  returns: (type: "StreamLines", desc: [Curvas integrales retenidas con color por velocidad.]),
  desc: [`create`, `write`, `fade_in`, `fade_out`, `uncreate`, `unwrite`, `grow_from_center` y `shrink_to_center` animan las curvas base. `flow(duration, time_width=...)` mueve resaltados más claros sobre ellas sin recortar la geometría base; sus clips son finitos y seekables.],
)[]

La integración es determinista: las semillas se recorren en orden regular, las
trayectorias se filtran por cobertura y todos los límites son explícitos. En
3D, los colores se conservan por vértice y la ventana móvil de `flow` recorta
la línea nativa también durante seeks y capturas exactas.

`field.advect(drawable, seed, ...)` mueve el centro de cualquier `Drawable`
por una trayectoria 2D o 3D calculada con el mismo integrador. La semilla es
explícita y está en coordenadas de datos; el clip resultante es finito, usa
longitud de arco y puede buscarse exactamente. `field.particles(count, ...)`
genera semillas de Halton deterministas, crea puntos 2D o esferas 3D, y devuelve
un `FlowParticles`. Las partículas permanecen ocultas hasta su primera animación:
pueden entrar con `create`, `write`, `fade_in` o `grow_from_center`, y luego
advectarse con `scene.play(particles.flow())`. También ofrece `fade_out`,
`uncreate`, `unwrite` y `shrink_to_center`; `particles.drawable()` conserva el
acceso al grupo para layout o estilo.

Los colores o colormaps indicados explícitamente en flechas y streamlines tienen
prioridad sobre las reglas `plot` de `set_theme`; el tema todavía controla el
fondo, los ejes y cualquier estilo no sobrescrito por el usuario.

#api-entry(
  name: "VectorField.advect / VectorField.particles",
  kind: "method",
  signature: "advect(target, seed, *, duration=3, ...) -> Anim; particles(count=32, *, radius=None, duration=3, ...) -> FlowParticles",
  params: (
    (name: "target / seed", type: "Drawable / tuple", default: none, desc: [Objeto cuyo centro se mueve y posición inicial en coordenadas de datos.]),
    (name: "count / radius", type: "int / float | None", default: "32 / automático", desc: [Cantidad de partículas y radio local; el predeterminado depende de 2D o 3D.]),
    (name: "duration", type: "float", default: "3", desc: [Duración finita de los clips seekables.]),
  ),
  returns: (type: "Anim | FlowParticles", desc: [Una trayectoria individual o un grupo con sus clips de flujo.]),
  desc: [La advección transforma el centro; no deforma punto a punto la geometría del objeto. Las partículas usan bases 2, 3 y 5 para una distribución reproducible.],
)[]

== Series de datos muestreadas

`plot_data` y `scatter_data` dibujan series `(xs, ys)` directamente en las
coordenadas de datos del espacio. El resultado queda emparentado con el plano:
si el espacio se mueve o escala, la serie lo acompaña. No existe una conversión
manual entre datos y píxeles que pueda desincronizarse.

#api-entry(
  name: "Cartesian2D.plot_data",
  kind: "method",
  signature: "plot_data(xs, ys, *, step=False, baseline=None, policy=\"gap\", color=None, width=None) -> Drawable",
  params: (
    (name: "xs, ys", type: "sequence[float | None]", default: none, desc: [Series de igual longitud en coordenadas de datos; `None` representa una muestra ausente.]),
    (name: "step", type: "bool", default: "False", desc: [Dibuja una gráfica escalonada en lugar de una línea continua.]),
    (name: "baseline", type: "float | None", default: "None", desc: [Línea base en el espacio de datos; un valor rellena el área bajo la curva.]),
    (name: "policy", type: "\"gap\" | \"drop\" | \"error\"", default: "\"gap\"", desc: [Tratamiento de muestras no finitas: separar la línea, omitir la muestra manteniendo la conexión o producir un error.]),
    (name: "color, width", type: "Color | None, float | None", default: "None", desc: [Color y ancho opcionales para sustituir el trazo predeterminado de la serie.]),
  ),
  returns: (type: "Drawable", desc: [Curva vectorial retenida y emparentada con el espacio; admite estilo y todas las animaciones de un objeto dibujable.]),
  desc: [Valida que las series no estén vacías y tengan la misma longitud. Es la ruta estática para datos medidos; usa `Parameter.drive_from_samples` cuando el tiempo de la escena debe recorrer las muestras.],
)[
```python
from gaanim import CYAN, Axis, Scene

scene = Scene()
plane = scene.viz.cartesian_2d(
  Axis.linear(0, 30).ticks(5).label("tiempo (s)"),
  Axis.linear(-0.4, 0.4).ticks(0.2).label("aceleración (g)"),
  width=1460,
  height=570,
)
curve = plane.plot_data(times, accel, color=CYAN, width=4)
scene.play([plane.animate.create().duration(0.85), curve.animate.create().duration(2.2)])
```
]

#api-entry(
  name: "Cartesian2D.scatter_data",
  kind: "method",
  signature: "scatter_data(xs, ys, *, radius=6.0, policy=\"gap\", color=None) -> Drawable",
  params: (
    (name: "xs, ys", type: "sequence[float | None]", default: none, desc: [Series de igual longitud en coordenadas de datos.]),
    (name: "radius", type: "float", default: "6.0", desc: [Radio positivo de cada punto en unidades locales del lienzo.]),
    (name: "policy", type: "\"gap\" | \"drop\" | \"error\"", default: "\"gap\"", desc: [Tratamiento de muestras no finitas.]),
    (name: "color", type: "Color | None", default: "None", desc: [Relleno opcional que sustituye el color de serie proporcionado por el tema.]),
  ),
  returns: (type: "Drawable", desc: [Grupo de puntos emparentado con el espacio.]),
  desc: [Úsalo para destacar muestras sobre una curva de `plot_data`; ambos elementos siguen el mismo plano.],
)[
```python
peaks = plane.scatter_data(peak_times, peak_values, radius=7, color=GOLD)
scene.play([peaks.animate.fade_in()])
```
]

== NumberLine reactivo

`NumberLine` es útil cuando una sola magnitud conduce varias representaciones.
Un mismo `Parameter` puede determinar la posición de un punto, el final visible
de una función y cualquier desplazamiento normal. Compartir la magnitud evita
que dos animaciones aparentemente equivalentes acumulen desfase.

#api-entry(
  name: "NumberLine.point_ref",
  kind: "method",
  signature: "point_ref(value, *, normal_offset=None) -> PointRef",
  params: (
    (name: "value", type: "float | Parameter | Variable | Computed", default: none, desc: [Valor convertido mediante la escala continua de la recta.]),
    (name: "normal_offset", type: "float | Parameter | Variable | Computed | None", default: "None", desc: [Desplazamiento perpendicular en unidades locales; `None` equivale a cero.]),
  ),
  returns: (type: "PointRef", desc: [Extremo reactivo no renderizado que sigue las transformaciones de la recta.]),
  desc: [El punto permanece unido cuando la recta se mueve, rota o escala. Las escalas categóricas rechazan valores escalares reactivos con `ValueError`.],
)[]

#api-entry(
  name: "NumberLine.function",
  kind: "method",
  signature: "function(function, domain=None, *, normal_scale=120.0, reveal=None, samples=None, tolerance=0.75, inputs=()) -> Drawable",
  params: (
    (name: "function", type: "Callable[..., float]", default: none, desc: [Recibe la coordenada y después los valores declarados en `inputs`.]),
    (name: "domain", type: "(float, float) | None", default: "None", desc: [Intervalo de muestreo; usa el dominio del eje si se omite.]),
    (name: "normal_scale", type: "float", default: "120.0", desc: [Distancia local positiva asignada a una salida de función igual a uno.]),
    (name: "reveal", type: "float | Parameter | Variable | Computed | None", default: "None", desc: [Extremo exacto de la curva visible, expresado en coordenadas de datos.]),
    (name: "samples", type: "int | None", default: "None", desc: [Cantidad fija de muestras; al omitirse se usa muestreo adaptativo.]),
    (name: "tolerance", type: "float", default: "0.75", desc: [Tolerancia positiva del error adaptativo en unidades locales.]),
  ),
  returns: (type: "Drawable", desc: [Curva vectorial retenida y emparentada con la recta numérica.]),
  desc: [El muestreo y las actualizaciones reactivas se ejecutan en Rust, sin callbacks de Python por fotograma. Un `reveal` reactivo puede compartir el mismo escalar que los puntos móviles y evitar desfases de longitud de arco. Dominios inválidos, ajustes de muestreo incorrectos o escalas no positivas producen `ValueError`.],
)[
```python
import math
from gaanim import Axis, Scene, computed

scene = Scene()
theta = scene.viz.parameter(0.0)
line = scene.viz.number_line(
  Axis.linear(0, 3 * math.pi).ticks(math.pi).numbers("pi", denominator=1),
  length=760,
)
curve = line.function(lambda t: math.sin(t), normal_scale=120, reveal=theta)
point = scene.geometry.dot(8).follow(
  line.point_ref(theta, normal_offset=computed(lambda t: 120 * math.sin(t), inputs=[theta]))
)
scene.play([line.animate.create(), curve.animate.fade_in().duration(0.01), point.animate.fade_in()])
scene.play([theta.animate.set(3 * math.pi).duration(4)])
```
]

== Límites y responsabilidades

La inspección interactiva solo existe en la vista previa. La exportación
interactiva, los dashboards, facets, volúmenes e isosuperficies no forman parte
todavía de esta superficie. La
animación de cámara sigue siendo global, explícita y componible.
