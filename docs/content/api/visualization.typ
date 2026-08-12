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

== NumberLine reactivo

#api-entry(
  name: "NumberLine.point_ref",
  kind: "method",
  signature: "point_ref(value, *, normal_offset=None) -> PointRef",
  params: (
    (name: "value", type: "float | Parameter | Expr", default: none, desc: [Value mapped through the line's continuous scale.]),
    (name: "normal_offset", type: "float | Parameter | Expr | None", default: "None", desc: [Perpendicular displacement in local canvas units; `None` means zero.]),
  ),
  returns: (type: "PointRef", desc: [A non-rendered reactive endpoint that follows the line's transforms.]),
  desc: [The point remains attached when the number line moves, rotates, or scales. Categorical scales reject reactive scalar values with `ValueError`.],
)[]

#api-entry(
  name: "NumberLine.function",
  kind: "method",
  signature: "function(function, domain=None, *, normal_scale=120.0, reveal=None, samples=None, tolerance=0.75) -> Drawable",
  params: (
    (name: "function", type: "Callable[[float], scalar]", default: none, desc: [Callable traced once with `gaanim.math`.]),
    (name: "domain", type: "(float, float) | None", default: "None", desc: [Sampling interval; defaults to the axis domain.]),
    (name: "normal_scale", type: "float", default: "120.0", desc: [Positive local distance assigned to a function output of one.]),
    (name: "reveal", type: "float | Parameter | Expr | None", default: "None", desc: [Exact data-space end of the visible curve.]),
    (name: "samples", type: "int | None", default: "None", desc: [Fixed sample count, or adaptive sampling when omitted.]),
    (name: "tolerance", type: "float", default: "0.75", desc: [Positive adaptive error tolerance in local canvas units.]),
  ),
  returns: (type: "Drawable", desc: [Retained vector curve parented to the number line.]),
  desc: [Sampling and reactive parameter updates run in Rust without Python callbacks per frame. A reactive `reveal` shares the same scalar value with moving points, avoiding arc-length drift. Invalid domains, sampling settings, or non-positive scales raise `ValueError`.],
)[
```python
import math
from gaanim import Axis, Scene, math as gm

scene = Scene()
theta = scene.parameter(0.0)
line = scene.number_line(
  Axis.linear(0, 3 * math.pi).ticks(math.pi).numbers("pi", denominator=1),
  length=760,
)
curve = line.function(lambda t: gm.sin(t), normal_scale=120, reveal=theta)
point = scene.dot(8).follow(
  line.point_ref(theta, normal_offset=120 * gm.sin(theta))
)
scene.play([line.create(), curve.fade_in(duration=0.01), point.fade_in()])
scene.play([theta.animate_to(3 * math.pi, duration=4)])
```
]

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
