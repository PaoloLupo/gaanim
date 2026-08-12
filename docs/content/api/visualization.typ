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

The visualization API separates tabular storytelling from scientific geometry:

- `ChartSpec` is an immutable grammar for data, encodings, marks, scales, axes,
  guides, and stable-key transitions.
- `Cartesian2D`, `Cartesian3D`, `PolarSpace`, and `ComplexSpace` are typed spaces
  for functions, calculus, vector fields, and custom geometry.

Constructing `ChartSpec` immediately snapshots mappings, dataframes,
`DataTable`, or `DataSource`. Later external mutation cannot alter an earlier
timeline seek.

```python
from gaanim import Axis, ChartSpec, Field, Guide, Scale, Scene, Value, BLUE

scene = Scene(1280, 720)
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
chart = scene.chart(spec)
scene.play(chart.create())
```

== ChartSpec

#api-entry(
  name: "ChartSpec",
  kind: "builder",
  signature: "ChartSpec(data, *, key=None) -> ChartSpec",
  params: (
    (name: "data", type: "mapping | dataframe | DataTable | DataSource", default: none, desc: [Input captured immediately as an owned immutable snapshot.]),
    (name: "key", type: "str | None", default: "None", desc: [Stable identity column. Null or duplicate values fail eagerly.]),
  ),
  returns: (type: "ChartSpec", desc: [Immutable declarative chart specification.]),
  desc: [`mark`, `encode`, `axes`, and `guides` return new specifications and never mutate an existing one.],
)[
Channels are `x`, `y`, `z`, `color`, `size`, `opacity`, and `label`. A channel
accepts a column name, `Field(column, scale=...)`, or `Value(constant)`.

Marks are `point`, `line`, `step`, `area`, `bar`, `histogram`, `box`, `violin`,
`error_bar`, `heatmap`, and `surface`. Point, line, and bar have native 2D/3D
morph compatibility; heatmap and surface are compatible on a shared grid.
]

== Escalas, ejes y guías

`Scale.linear`, `log`, `symlog`, `power`, `time`, and `category` configure an
encoding. The same scale drives normalized positions, colors, legends, and
colorbars. `Axis` remains the immutable visual scale builder and supports the
same numeric, temporal, and categorical families.

```python
color = Field("temperature", scale=Scale.symlog((-100, 100), threshold=1))
x = Axis.log(0.1, 1000, base=10).ticks(10).label("frequency")
guide = Guide.colorbar(title="temperature")
```

== Gráfico materializado y transiciones

`scene.chart(spec)` returns a `Chart`. Its stable layers are `marks`, `axes`,
`grid`, and `guides`; each is a regular drawable. Marks are materialized as a
constant number of retained vector or mesh batches rather than one ECS entity
per record.

Chart opacity propagates through both vector and native 3D mesh layers, so
`fade_in`, `fade_out`, and parent opacity remain consistent in mixed scenes.
For inferred bar axes, the numeric baseline is included automatically and the
outer domain reserves enough space to keep the first and last bars away from
the plot boundary. An explicitly authored axis domain is never changed.

```python
target = spec.encode(z="height").axes(z=Axis.linear(-2, 2))
scene.play([
  chart.to(target).duration(1.4),
  scene.camera.look_at(eye=(8, 6, 8), target=(0, 0, 0)).duration(1.4),
])
```

Key matching is the default and requires both specs to define the same valid
key column. Without a key, request `match_="index"` explicitly. Incompatible
mark families raise an error unless `fallback="crossfade"` is explicit.
`Chart.to` never moves the global camera implicitly.

`chart.inspect(fields=(...), format="...")` opts into preview metadata. The
inspection flag and fields are excluded from snapshots and exports.

== Espacios científicos tipados

```python
from gaanim import Axis, Scene, math as gm

scene = Scene()
plane = scene.cartesian_2d(Axis.linear(-6, 6), Axis.linear(-3, 3))
a = scene.parameter(1.0)
curve = plane.function(lambda x: a * gm.sin(x))

world = scene.cartesian_3d(
  Axis.log(0.1, 1000),
  Axis.symlog(-100, 100),
  Axis.power(0, 16, 0.5),
)
surface = world.surface(lambda x, y: x * y)
```

`Cartesian2D` exposes `function`, `parametric`, `implicit`, `contour`, and
`vector_field`, plus calculus constructions. `Cartesian3D` exposes `surface`,
`parametric`, and `vector_field`. Its scale-aware `grid`, `axes`, `ticks`,
`numbers`, and billboard `labels` are independent layers. `scene.polar(...)`,
`scene.complex(...)`, and `scene.number_line(...)` cover the other typed spaces.

`Expr` and `Parameter` remain the per-frame reactive path. Python lambdas for
traced scalar functions execute once; sampling and reactive evaluation stay in
Rust.

== Migración desde la API anterior

#table(
  columns: (1fr, 1fr),
  table.header([Legacy], [Current]),
  [`scene.axes(x, y)`], [`scene.cartesian_2d(x, y, grid=False)`],
  [`scene.number_plane(x, y)`], [`scene.cartesian_2d(x, y, grid=True)`],
  [`scene.axes_3d(x, y, z)`], [`scene.cartesian_3d(x, y, z)`],
  [`scene.polar_plane(axis)`], [`scene.polar(axis)`],
  [`scene.complex_plane()`], [`scene.complex()`],
  [`space.plot(f)`], [`space.function(f)`],
  [`space.scatter/bars/...`], [`ChartSpec(...).mark(...).encode(...)`],
)

The legacy entry points in this table are not aliases in the primary Python
API. Migrate them explicitly so a scene has one unambiguous coordinate-space
contract.

== Límites y responsabilidades

Interaction is preview-only. Interactive export, dashboards, facets,
streamlines and ODE solvers, volumes, and isosurfaces are deferred to a later
milestone. Camera animation remains global, explicit, and composable.
