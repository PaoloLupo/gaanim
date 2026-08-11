#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Visualization API",
  description: "Coordinate spaces, functions, data, statistics, and calculus",
  route: "/api/visualization/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Visualization

The visualization API is intentionally centered on a typed coordinate space.
An `Axis` describes a reusable scale; `Scene` creates a space; plots and data
marks are then created by that space. This keeps coordinates, layout,
transformations, clipping, and sampling in one native hierarchy.

```python
from gaanim import Axis, BLUE, Scene, math as gm

scene = Scene(1280, 720)
x_axis = Axis.linear(-6, 6).ticks(1).minor_ticks(2).label("x")
y_axis = Axis.linear(-3, 3).ticks(1).label("f(x)")
plane = scene.number_plane(x_axis, y_axis, width=1000, height=520)

amplitude = scene.parameter(1.0)
curve = plane.plot(lambda x: amplitude * gm.sin(x)).stroke(BLUE, 3)

scene.play([plane.create(), curve.create()])
scene.play([amplitude.animate_to(2.0).duration(1.2)])
```

Lambdas are traced once with `gaanim.math` and then evaluated and
differentiated in Rust. A traced expression that references a `Parameter` is
resampled while that parameter animates without a Python callback per frame.

== Axis

#api-entry(
  name: "Axis",
  kind: "builder",
  signature: "Axis.linear/log/symlog/power/time/category(...) -> Axis",
  params: (
    (name: "domain", type: "float pair or categories", default: none, desc: [Valid values represented by the axis.]),
    (name: "ticks", type: "float", default: "automatic", desc: [Major tick step; `minor_ticks(n)` adds subdivisions.]),
    (name: "numbers", type: "format", default: "auto", desc: [Fixed, scientific, percent, fraction, pi, or datetime labels.]),
  ),
  returns: (type: "Axis", desc: [Immutable reusable axis specification.]),
  desc: [`label`, `crossing`, `numbers`, `ticks`, `minor_ticks`, and `style` return a new configured axis. Numeric domains must be finite and increasing; logarithmic domains must be positive.],
)[
```python
linear = Axis.linear(-10, 10).ticks(2).label("x")
log_y = Axis.log(0.01, 1000, base=10).numbers("scientific")
signed = Axis.symlog(-100, 100, threshold=1)
classes = Axis.category(["control", "pilot", "final"])
```
]

== Coordinate spaces

`scene.number_line(axis)`, `scene.axes(x, y)`, `scene.number_plane(x, y)`,
`scene.polar_plane(radial)`, `scene.complex_plane(...)`, and
`scene.axes_3d(x, y, z)` return typed handles rather than generic axes
drawables. Cartesian and complex spaces expose:

- `coord(x, y) -> CoordinateRef` and `drawable.at_coordinate(ref)`;
- immediate `data_to_local` and `local_to_data` conversions;
- `layer("grid" | "minor_grid" | "axes" | "ticks" | "numbers" | "labels")`;
- root `at`, `scaled`, `rotated`, `create`, `fade_in`, and `fade_out` operations.
- `animate_view(x_domain, y_domain, duration=...)` for an affine pan/zoom of
  linear or temporal views.

A `CoordinateRef` is resolved as a child of the space, so the placed drawable
continues to follow later moves, scaling, rotation, and layout of the space.

```python
space = scene.axes(Axis.linear(-4, 4), Axis.linear(-2, 6))
marker = scene.dot(6).at_coordinate(space.coord(1.5, 3.0))
scene.play([space.create(), marker.fade_in()])
scene.play(space.animate_view((-2, 2), (-1, 4), duration=1.2))
```

Polar plots use `polar.plot(lambda angle: radius, domain=(0, 2*pi))`. A 3D
space exposes `surface(function, resolution=(64, 48))`,
`parametric(function, domain, samples=320)`, and the batched static
`vector_field(function, resolution=(8, 8, 6))`. Surfaces receive a native
height colormap. Static Python callbacks are never kept as per-frame updaters.

== Functions and fields

#api-entry(
  name: "CoordinateSpace.plot",
  kind: "factory",
  signature: "plot(callable, domain=None, *, samples=None, tolerance=0.75, derivative=0) -> Drawable",
  params: (
    (name: "function", type: "callable", default: none, desc: [Scalar function y=f(x), traced once with `gaanim.math`.]),
    (name: "domain", type: "(float,float)", default: "x axis domain", desc: [Sampling interval.]),
    (name: "samples", type: "int", default: "adaptive", desc: [Set for fixed sampling.]),
    (name: "tolerance", type: "float", default: "0.75", desc: [Maximum visual error for adaptive sampling.]),
  ),
  returns: (type: "Drawable", desc: [A retained vector path parented to the space.]),
  desc: [Adaptive sampling separates invalid regions and visual discontinuities.],
)[
```python
curve = space.plot(lambda x: 1 / x, domain=(-4, 4)).stroke(BLUE, 3)
orbit = space.parametric(lambda t: (2*cos(t), sin(t)), (0, 2*pi))
level = space.implicit(lambda x, y: x*x + y*y - 1)
levels = space.contour(lambda x, y: x*x - y*y, [-2, -1, 0, 1, 2])
field = space.vector_field(lambda x, y: (-y, x), resolution=(20, 12))
```
]

== Data and statistics

`DataTable` accepts Python columns, NumPy-like columns with `tolist`, and
dataframe objects with `to_dict("list")`; neither NumPy nor pandas is a required
dependency. `DataSource` owns replaceable/appendable data. Its marks regenerate
natively after `replace` or `append`; an optional stable `key` validates the
identity column for future keyed transitions.

```python
from gaanim import DataSource

data = DataSource({
  "id": ["a", "b", "c"],
  "x": [0, 1, 2],
  "value": [18, 42, 31],
}, key="id")

bars = space.bars(data, "x", "value", width=0.8)
points = space.scatter(data, "x", "value")
data.replace({"id": ["a", "b", "c"], "x": [0, 1, 2], "value": [24, 35, 48]})
```

Available marks are `line`, `step`, `area`, `scatter`, `bars`, `histogram`,
`box_plot`, `violin`, `error_bars`, and quantized `heatmap`. Line-like marks
default to `gap` for missing/non-finite values; policies are `gap`, `drop`, and
`error`. Aggregated statistics ignore missing values.

== Educational constructions

Coordinate spaces provide `projections`, `secant`, `tangent`, `normal`,
`area_under`, and `riemann_sum`. `Expr.derivative(variable)` supplies a native
symbolic derivative for a second plot.

```python
tangent = space.tangent(lambda x: sin(x), 1.0, length=3.0)
normal = space.normal(lambda x: sin(x), 1.0, length=2.0)
area = space.area_under(lambda x: sin(x) + 1, (0, pi), baseline=0)
rects = space.riemann_sum(lambda x: x*x, (0, 3), rectangles=16)

x = Expr.var("x")
derivative = space.plot((x.sin() * x).derivative("x"))
```

== Current boundaries

This API intentionally does not integrate trajectories or implement ODEs,
phase portraits, PDEs, volume rendering, dashboards, faceting, or symbolic
algebra beyond expression differentiation. Three-dimensional axes currently
accept linear and temporal scales. Python callbacks are static; use `Expr` and
`Parameter` for per-frame reactivity. `animate_view` currently performs an
affine view animation and therefore preserves alignment but does not regenerate
tick labels during the tween.
