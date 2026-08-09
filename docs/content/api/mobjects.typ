#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Mobjects",
  description: "Every drawable factory on Scene — primitives, paths, text, media, editorial",
  route: "/api/mobjects/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Mobjects

Every factory on `Scene` returns a `Drawable`. Chain style, layout and animation fluently.

== Imported glTF models

```python
Scene.gltf(path: str, *, scene: str | int | None = None) -> Drawable
Drawable.part(selector: str) -> Drawable
Drawable.parts() -> tuple[str, ...]
Drawable.animations() -> tuple[str, ...]
```

The model and every selected node support the complete 3D transform surface:
`at_3d(x,y,z)`, `scaled_3d(x,y,z)`, `rotated_3d(x,y,z)`, and
`with_pivot_3d(x,y,z)`. Euler rotations use XYZ order and radians.

#html.div(style: "font-family: var(--font-code); font-size: 0.65rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; background: var(--text-main); color: var(--bg-main); padding: 4px 8px; display: inline-block; margin-bottom: 16px;", [— 40+ FACTORIES · ALL RETURN Drawable —])

== Primitives

#api-entry(
  name: "Scene.circle",
  kind: "factory",
  signature: "circle(radius: float) -> Drawable",
  params: ((name: "radius", type: "float", default: none, desc: [Radius in scene units.]),),
  returns: (type: "Drawable", desc: [Circle centered at origin.]),
  desc: [Perfect for nodes, pills, or radial diagrams. Combine with `annulus` for donuts.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(480, 270, background="#0f172a")
node = scene.circle(50).fill(BLUE).at(0, 0)
scene.play([node.create().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.rect",
  kind: "factory",
  signature: "rect(width: float, height: float) -> Drawable",
  params: ((name: "width", type: "float", default: none, desc: [Width.]), (name: "height", type: "float", default: none, desc: [Height.]),),
  returns: (type: "Drawable", desc: [Rectangle centered at origin.]),
  desc: [Cards, panels, bars. Use `rounded_rect` for softer UI.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
card = scene.rect(160, 90).fill(BLUE).stroke(WHITE, 2).at(0, 0)
scene.play([card.grow_from_center().duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.rounded_rect",
  kind: "factory",
  signature: "rounded_rect(width, height, radius: float) -> Drawable",
  params: ((name: "width", type: "float", default: none, desc: [Width.]), (name: "height", type: "float", default: none, desc: [Height.]), (name: "radius", type: "float", default: none, desc: [Corner radius.]),),
  returns: (type: "Drawable", desc: [Rounded rectangle.]),
  desc: [Buttons, tags, callout cards. Radius controls softness.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
btn = scene.rounded_rect(160, 50, 12).fill(GOLD).at(0, 0)
label = scene.text("CLICK").at(0, 0)
scene.play([scene.group([btn, label]).fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.square",
  kind: "factory",
  signature: "square(size: float) -> Drawable",
  params: ((name: "size", type: "float", default: none, desc: [Side length.]),),
  returns: (type: "Drawable", desc: [Square.]),
  desc: [Shorthand for `rect(size, size)`. Useful for grid cells.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
cell = scene.square(80).fill(BLUE).at(0, 0)
scene.play([cell.grow_from_center().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.dot",
  kind: "factory",
  signature: "dot(radius: float) -> Drawable",
  params: ((name: "radius", type: "float", default: none, desc: [Radius, typically 4–16.]),),
  returns: (type: "Drawable", desc: [Small filled circle.]),
  desc: [Markers, bullet points, particles. Optimized for many instances.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
dot = scene.dot(8).fill(RED).at(0, 0)
scene.play([dot.grow_from_center().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.ellipse",
  kind: "factory",
  signature: "ellipse(rx: float, ry: float) -> Drawable",
  params: ((name: "rx", type: "float", default: none, desc: [X radius.]), (name: "ry", type: "float", default: none, desc: [Y radius.]),),
  returns: (type: "Drawable", desc: [Ellipse.]),
  desc: [Orbits, highlights, Venn overlaps.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
orbit = scene.ellipse(90, 50).no_fill().stroke(GOLD, 2).at(0, 0)
scene.play([orbit.create().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

== Lines & Arrows

#api-entry(
  name: "Scene.line",
  kind: "factory",
  signature: "line(x1, y1, x2, y2) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]),),
  returns: (type: "Drawable", desc: [Line segment.]),
  desc: [Axes, dividers, connectors.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
axis = scene.line(-140, 0, 140, 0).stroke(WHITE, 3)
scene.play([axis.create().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.arrow",
  kind: "factory",
  signature: "arrow(x1, y1, x2, y2) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Tail x.]), (name: "y1", type: "float", default: none, desc: [Tail y.]), (name: "x2", type: "float", default: none, desc: [Head x.]), (name: "y2", type: "float", default: none, desc: [Head y.]),),
  returns: (type: "Drawable", desc: [Arrow with head at (x2,y2).]),
  desc: [Transitions, causality, flow. For curved, use `curved_arrow`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
arrow = scene.arrow(-100, 0, 100, 0).stroke(GOLD, 4)
scene.play([arrow.create().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.dashed_line",
  kind: "factory",
  signature: "dashed_line(x1, y1, x2, y2, *, dash_length=16, gap_length=10) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]), (name: "dash_length", type: "float", default: "16.0", desc: [Dash length.]), (name: "gap_length", type: "float", default: "10.0", desc: [Gap length.]),),
  returns: (type: "Drawable", desc: [Dashed line.]),
  desc: [Guides, hidden edges, construction lines.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
guide = scene.dashed_line(-140, 0, 140, 0, dash_length=12, gap_length=8).stroke(WHITE, 2)
scene.play([guide.create().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.double_arrow",
  kind: "factory",
  signature: "double_arrow(x1, y1, x2, y2, *, head_length?, head_width?) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [End A x.]), (name: "y1", type: "float", default: none, desc: [End A y.]), (name: "x2", type: "float", default: none, desc: [End B x.]), (name: "y2", type: "float", default: none, desc: [End B y.]),),
  returns: (type: "Drawable", desc: [Double-headed arrow.]),
  desc: [Bidirectional relations, spans.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
span = scene.double_arrow(-100, 0, 100, 0).stroke(WHITE, 3)
scene.play([span.create().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.arc",
  kind: "factory",
  signature: "arc(cx, cy, radius, start_angle, sweep_angle) -> Drawable",
  params: ((name: "cx", type: "float", default: none, desc: [Center x.]), (name: "cy", type: "float", default: none, desc: [Center y.]), (name: "radius", type: "float", default: none, desc: [Radius.]), (name: "start_angle", type: "float", default: none, desc: [Start in radians.]), (name: "sweep_angle", type: "float", default: none, desc: [Sweep in radians.]),),
  returns: (type: "Drawable", desc: [Arc path.]),
  desc: [Angles, progress, gauges. Use `no_fill().stroke()` for outline.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
arc = scene.arc(0, 0, 60, 0.0, 2.0).no_fill().stroke(GOLD, 4)
scene.play([arc.create().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.curved_arrow",
  kind: "factory",
  signature: "curved_arrow(x1, y1, x2, y2, angle: float) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]), (name: "angle", type: "float", default: none, desc: [Deflection in radians.]),),
  returns: (type: "Drawable", desc: [Curved arrow.]),
  desc: [Feedback loops, rotations. Positive angle curves one way.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
loop = scene.curved_arrow(-80, 0, 80, 0, 0.9).fill(WHITE)
scene.play([loop.create().duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.curved_arrow_arc",
  kind: "factory",
  signature: "curved_arrow_arc(cx, cy, radius, start_angle, sweep_angle) -> Drawable",
  params: ((name: "cx", type: "float", default: none, desc: [Center x.]), (name: "cy", type: "float", default: none, desc: [Center y.]), (name: "radius", type: "float", default: none, desc: [Radius.]), (name: "start_angle", type: "float", default: none, desc: [Start radians.]), (name: "sweep_angle", type: "float", default: none, desc: [Sweep radians.]),),
  returns: (type: "Drawable", desc: [Arc-following curved arrow.]),
  desc: [Precise orbital arrows with explicit center/radius.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
arr = scene.curved_arrow_arc(0, 0, 70, 0.2, 1.8).fill(GOLD)
scene.play([arr.create().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.dimension",
  kind: "factory",
  signature: "dimension(x1, y1, x2, y2, offset: float) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Point A x.]), (name: "y1", type: "float", default: none, desc: [Point A y.]), (name: "x2", type: "float", default: none, desc: [Point B x.]), (name: "y2", type: "float", default: none, desc: [Point B y.]), (name: "offset", type: "float", default: none, desc: [Perpendicular offset for arrow line.]),),
  returns: (type: "Drawable", desc: [Technical dimension with extension lines.]),
  desc: [Engineering drawings. For reactive, use `dimension_between`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
dim = scene.dimension(-80, 0, 80, 0, 24).stroke(WHITE, 2)
scene.play([dim.create().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

== Polygons & Symbols

#api-entry(
  name: "Scene.polygon",
  kind: "factory",
  signature: "polygon(points: list[(float,float)]) -> Drawable",
  params: ((name: "points", type: "list[vec2]", default: none, desc: [At least 3 finite points, closed automatically.]),),
  returns: (type: "Drawable", desc: [Polygon.]),
  desc: [Custom shapes, territories, highlights.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
tri = scene.polygon([(0, 70), (-65, -50), (65, -50)]).fill(BLUE).stroke(WHITE, 2)
scene.play([tri.grow_from_center().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.star",
  kind: "factory",
  signature: "star(points: int, outer_radius: float, inner_radius: float) -> Drawable",
  params: ((name: "points", type: "int", default: none, desc: [≥2 points.]), (name: "outer_radius", type: "float", default: none, desc: [Outer radius.]), (name: "inner_radius", type: "float", default: none, desc: [Inner radius.]),),
  returns: (type: "Drawable", desc: [Star polygon.]),
  desc: [Ratings, badges, explosions. 5 points is classic star.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
star = scene.star(5, 70, 32).fill(GOLD).at(0, 0)
scene.play([star.spin_in_from_nothing().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.regular_polygon",
  kind: "factory",
  signature: "regular_polygon(sides: int, radius: float) -> Drawable",
  params: ((name: "sides", type: "int", default: none, desc: [≥3 sides.]), (name: "radius", type: "float", default: none, desc: [Circumradius.]),),
  returns: (type: "Drawable", desc: [Regular polygon.]),
  desc: [Hex grids, stop signs, diagrams. 6 = hexagon.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
hexa = scene.regular_polygon(6, 60).fill(BLUE).stroke(WHITE, 2)
scene.play([hexa.spin_in_from_nothing().duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.sector",
  kind: "factory",
  signature: "sector(cx, cy, radius, start_angle, sweep_angle) -> Drawable",
  params: ((name: "cx", type: "float", default: none, desc: [Center x.]), (name: "cy", type: "float", default: none, desc: [Center y.]), (name: "radius", type: "float", default: none, desc: [Radius.]), (name: "start_angle", type: "float", default: none, desc: [Start rad.]), (name: "sweep_angle", type: "float", default: none, desc: [Sweep rad.]),),
  returns: (type: "Drawable", desc: [Pie slice.]),
  desc: [Pie charts, progress wedges.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
slice_ = scene.sector(0, 0, 70, 0.0, 2.0).fill(GOLD)
scene.play([slice_.grow_from_center().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.annulus",
  kind: "factory",
  signature: "annulus(outer_radius: float, inner_radius: float) -> Drawable",
  params: ((name: "outer_radius", type: "float", default: none, desc: [Outer > inner.]), (name: "inner_radius", type: "float", default: none, desc: [Inner > 0.]),),
  returns: (type: "Drawable", desc: [Ring.]),
  desc: [Donut charts, halos, targets.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
ring = scene.annulus(60, 34).fill(BLUE).stroke(WHITE, 2)
scene.play([ring.grow_from_center().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.brace",
  kind: "factory",
  signature: "brace(x1, y1, x2, y2, height: float) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]), (name: "height", type: "float", default: none, desc: [Brace depth, non-zero.]),),
  returns: (type: "Drawable", desc: [Curly brace.]),
  desc: [Grouping annotations under equations or diagrams.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
brace = scene.brace(-80, -20, 80, -20, 24).stroke(WHITE, 3).no_fill()
label = scene.text("interval").at(0, -55)
scene.play([brace.create().duration(0.7), label.fade_in().duration(0.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.checkmark",
  kind: "factory",
  signature: "checkmark(size: float) -> Drawable",
  params: ((name: "size", type: "float", default: none, desc: [Size > 0.]),),
  returns: (type: "Drawable", desc: [Checkmark.]),
  desc: [Correct answers, approvals. Pair with `cross` for quiz UI.],
)[
```python
# show-code: true
from gaanim import GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
ok = scene.checkmark(34).fill(GREEN).at(0, 0)
scene.play([ok.grow_from_center().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.cross",
  kind: "factory",
  signature: "cross(size: float) -> Drawable",
  params: ((name: "size", type: "float", default: none, desc: [Size > 0.]),),
  returns: (type: "Drawable", desc: [X mark.]),
  desc: [Errors, rejections, close buttons.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
no = scene.cross(34).stroke(WHITE, 4).at(0, 0)
scene.play([no.create().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.right_angle",
  kind: "factory",
  signature: "right_angle(arm_length: float) -> Drawable",
  params: ((name: "arm_length", type: "float", default: none, desc: [Arm length > 0.]),),
  returns: (type: "Drawable", desc: [Right-angle mark.]),
  desc: [Geometry proofs. Place at triangle corner.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
corner = scene.right_angle(24).stroke(WHITE, 3).at(0, 0)
axis = scene.line(-80, 0, 80, 0).stroke(WHITE, 2)
scene.play([scene.group([axis, corner]).create().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

== Paths

#api-entry(
  name: "Scene.path",
  kind: "factory",
  signature: "path(definition) -> Drawable",
  params: ((name: "definition", type: "list[vec2] | list[command]", default: none, desc: [Points for polyline, or commands like (\"cubic\", [(x,y),...]).]),),
  returns: (type: "Drawable", desc: [Path.]),
  desc: [Compact entry for custom geometry. Delegates to `polyline` or `curve`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
rail = scene.path([(-140, 0), (0, 60), (140, 0)]).no_fill().stroke(WHITE, 4)
scene.play([rail.create().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.polyline",
  kind: "factory",
  signature: "polyline(points: list[vec2]) -> Drawable",
  params: ((name: "points", type: "list[vec2]", default: none, desc: [≥2 points.]),),
  returns: (type: "Drawable", desc: [Polyline.]),
  desc: [Explicit polyline when kind matters.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
zig = scene.polyline([(-100, -30), (0, 30), (100, -30)]).no_fill().stroke(GOLD, 3)
scene.play([zig.create().duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.bezier",
  kind: "factory",
  signature: "bezier(start: vec2, controls: list[vec2], end: vec2) -> Drawable",
  params: ((name: "start", type: "vec2", default: none, desc: [Start point.]), (name: "controls", type: "list[vec2]", default: none, desc: [1 = quadratic, 2 = cubic.]), (name: "end", type: "vec2", default: none, desc: [End point.]),),
  returns: (type: "Drawable", desc: [Bézier path.]),
  desc: [Smooth curves that stay Bézier (drives reactive bindings).],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
curve = scene.bezier((-140, 0), [(-50, 90), (50, -90)], (140, 0)).no_fill().stroke(WHITE, 3)
scene.play([curve.create().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.curve",
  kind: "factory",
  signature: "curve(commands: list[(str, list)]) -> Drawable",
  params: ((name: "commands", type: "list[command]", default: none, desc: [Commands like move, line, quad, cubic, close with args. Append _rel_ for relative.] ),),
  returns: (type: "Drawable", desc: [Composed Bézier curve.]),
  desc: [Typst-inspired cursor commands with auto handles.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
shape = scene.curve([("move", [(0, 0)]), ("cubic", [(50, 60), (110, -60), (160, 0)]), ("close", [])]).no_fill().stroke(WHITE, 3).at(-80, 0)
scene.play([shape.create().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

== 3D Geometry

Gaanim uses ordinary Python tuples for 3D points: `(x, y, z)`. The same
`Drawable` handle can be styled and animated after it is placed in world space.
This small example combines axes, a polyline, a moving point, a traced path,
and the perspective camera:

```python
# show-code: true
from gaanim import Axis, BLACK, BLUE, GOLD, RED, WHITE, Scene, Updater
scene = Scene(640, 360, background=BLACK)

axes = scene.axes_3d(
    Axis.linear(-3, 3).ticks(1).label("x").style(color=WHITE),
    Axis.linear(-3, 3).ticks(1).label("y").style(color=WHITE),
    Axis.linear(-2, 2).ticks(1).label("z").style(color=WHITE),
    size=(6, 6, 4),
)
path = scene.polyline_3d([(-2, -1, -1), (0, 1, 0), (2, -1, 1)], color=RED)
dot = scene.dot(8).fill(GOLD).at_3d(1, 0, 0).billboard()
dot.add_updater(Updater.orbit(0, 0, 1, 1.2))
trail = scene.traced_path_3d(dot, colormap="viridis", max_points=120)

scene.camera.perspective(fov_y=0.785, near=0.1, far=100.0, duration=0.0)
scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0), duration=0.0)
scene.play([
    axes.create().duration(0.8),
    path.create().duration(0.8),
    dot.fade_in().duration(0.4),
    trail.fade_in().duration(0.4),
])
scene.camera.orbit(delta_yaw=0.5, delta_pitch=0.1, duration=0.8)
scene.camera.dolly(factor=0.85, duration=0.5)
scene.wait(0.5)
scene.export("3d-quickstart.webp", fps=30)
```

#api-entry(
  name: "Scene.axes_3d",
  kind: "factory",
  signature: "axes_3d(x: Axis, y: Axis, z: Axis, *, size=(10,8,6), grid=True) -> CoordinateSpace3D",
  params: ((name: "x / y / z", type: "Axis", default: none, desc: [Reusable linear or temporal axis specifications.]), (name: "size", type: "(float,float,float)", default: "(10,8,6)", desc: [Positive world-space size; choose it relative to the camera distance.]), (name: "grid", type: "bool", default: "True", desc: [Show the three grid planes.]),),
  returns: (type: "CoordinateSpace3D", desc: [Typed space with surface and parametric methods.]),
  desc: [Creates Cartesian axes and up to three grid planes. Use `surface` and `parametric` on the returned space so data coordinates share the same mapping.],
)[
```python
axes = scene.axes_3d(
    Axis.linear(-5, 5).ticks(1).label("x"),
    Axis.linear(-5, 5).ticks(1).label("y"),
    Axis.linear(-3, 3).ticks(1).label("z"),
)
```
]

#api-entry(
  name: "Scene.polyline_3d",
  kind: "factory",
  signature: "polyline_3d(points, color=None, *, colors=None, colormap=None) -> Drawable",
  params: ((name: "points", type: "list[(float,float,float)]", default: none, desc: [At least two finite world-space points.]), (name: "color", type: "Color", default: "None", desc: [Uniform color.]), (name: "colors", type: "list[Color]", default: "None", desc: [One color per point; takes precedence over colormap.]), (name: "colormap", type: "str", default: "None", desc: ["inferno", "viridis", or "plasma".]),),
  returns: (type: "Drawable", desc: [3D line strip in world space.]),
  desc: [Use `color` for a uniform line, `colors` for explicit vertex colors, or `colormap` for a time-ordered gradient. A colors list must have exactly the same length as `points`.],
)[
```python
helix = scene.polyline_3d(points, colormap="inferno")
```
]

#api-entry(
  name: "Drawable.at_3d",
  kind: "method",
  signature: ".at_3d(x, y, z) -> Drawable",
  params: ((name: "x / y / z", type: "float", default: none, desc: [World-space position.]),),
  returns: (type: "Drawable", desc: [The same drawable for fluent chaining.]),
  desc: [Places a drawable in 3D world space. Add `.billboard()` to keep text or a marker facing the camera, or `.hud()` for a fixed screen-space overlay.],
)[
```python
label = scene.text("origin").at_3d(0, 0, 0.5).billboard()
```
]

#api-entry(
  name: "Scene.traced_path",
  kind: "factory",
  signature: "traced_path(source, *, dissipating_time=None, max_points=None, min_distance=1.0) -> Drawable",
  params: ((name: "source", type: "Drawable", default: none, desc: [Drawable whose position is sampled.]), (name: "dissipating_time", type: "float", default: "None", desc: [Seconds each sample remains in the trail; must be positive.]), (name: "max_points", type: "int", default: "None", desc: [Positive cap for retained samples.]), (name: "min_distance", type: "float", default: "1.0", desc: [Minimum scene-space distance between samples.]),),
  returns: (type: "Drawable", desc: [Reactive 2D trail, hidden until `fade_in` reveals it.]),
  desc: [Sampling begins at the timeline cursor where the trail is declared, so earlier segments and seeks cannot pre-fill it. Add `trail.fade_in()` to `scene.play(...)` to reveal it. With `dissipating_time`, samples expire from the tail in editor playback, random seeks, snapshots, and exports.],
)[
```python
from gaanim import RED

dot = scene.dot(7).at(120, 0)
trail = scene.traced_path(dot, dissipating_time=2.0).stroke(RED, 3).no_fill()
scene.play([trail.fade_in()])
```
]

#api-entry(
  name: "Scene.traced_path_3d",
  kind: "factory",
  signature: "traced_path_3d(source, *, colormap=None, dissipating_time=None, max_points=None, min_distance=0.1) -> Drawable",
  params: ((name: "source", type: "Drawable", default: none, desc: [Drawable whose world-space position is sampled.]), (name: "colormap", type: "str", default: "None", desc: ["inferno", "viridis", or "plasma".]), (name: "dissipating_time", type: "float", default: "None", desc: [Seconds each sample remains in the trail; must be positive.]), (name: "max_points", type: "int", default: "None", desc: [Positive cap for retained samples.]), (name: "min_distance", type: "float", default: "0.1", desc: [Minimum world-space distance between samples.]),),
  returns: (type: "Drawable", desc: [Reactive 3D trail, hidden until its entry animation.]),
  desc: [The trail updates while `source` moves, so it works with `Updater` or `add_updater_fn`. Sampling starts where the trail is declared. Add `trail.fade_in()` or `trail.create()` to `scene.play(...)` to reveal it. `dissipating_time` expires the old tail, `max_points` limits memory, and `min_distance` filters nearly identical samples.],
)[
```python
dot = scene.dot(7).at_3d(1, 0, 0)
dot.add_updater(Updater.orbit(0, 0, 1, 1.5))
trail = scene.traced_path_3d(
    dot, colormap="viridis", dissipating_time=2.0, max_points=600
)
scene.play([trail.fade_in()])
```
]

== Plots

Plots are no longer free `Scene` factories. They are children of a typed
coordinate space, which owns scales, conversions, clipping, and sampling.
See #link("/api/visualization/", "Visualization API") for Cartesian,
polar, complex, and 3D spaces; native expressions; data marks; statistics;
and calculus helpers.

```python
from gaanim import Axis, BLUE, Expr, Scene

scene = Scene(480, 270, background="#0f172a")
space = scene.number_plane(Axis.linear(-4, 4), Axis.linear(-2, 2))
x = Expr.var("x")
curve = space.plot(x.sin()).stroke(BLUE, 3)
scene.play([space.create(), curve.create()])
```

== Text & Math

#api-entry(
  name: "Scene.text",
  kind: "factory",
  signature: "text(string: str) -> Drawable",
  params: ((name: "string", type: "str", default: none, desc: [Single-line text.]),),
  returns: (type: "Drawable", desc: [Text drawable.]),
  desc: [Labels, annotations. For multi-line use `paragraph`. Supports `color_by`, `select`, `write`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
label = scene.text("Hello, Gaanim").fill(WHITE).at(0, 0)
scene.play([label.write().duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.paragraph",
  kind: "factory",
  signature: "paragraph(text, width, *, align, line_spacing, font_size, font_family) -> Drawable",
  params: ((name: "text", type: "str", default: none, desc: [Body text.]), (name: "width", type: "float", default: none, desc: [Wrap width > 0.]), (name: "align", type: "str", default: "\"left\"", desc: ["left|center|right|justify"]), (name: "line_spacing", type: "float", default: "1.2", desc: [≥1.0]),),
  returns: (type: "Drawable", desc: [Wrapped paragraph as vector outlines.]),
  desc: [Justified body copy. Glyphs remain animatable (`write`, `select`).],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
body = scene.paragraph("Una explicación larga que se ajusta al ancho y muestra el salto de línea automático.", width=320, align="left", line_spacing=1.25).at(0, 0)
scene.play([body.fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.title / subtitle",
  kind: "factory",
  signature: "title(string) / subtitle(string) -> Drawable",
  params: ((name: "string", type: "str", default: none, desc: [Text.]),),
  returns: (type: "Drawable", desc: [Styled heading.]),
  desc: [Convenience wrappers with theme-aware sizing.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
t = scene.title("Fourier Transform").at(0, 30)
s = scene.subtitle("A visual proof").at(0, -20)
scene.play([t.write().duration(0.7), s.fade_in().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.equation",
  kind: "factory",
  signature: "equation(source: str, *, tags?: dict[str, str | tuple[str,int]]) -> Drawable",
  params: ((name: "source", type: "str", default: none, desc: [Typst math, e.g. \"E = m c^2\".]), (name: "tags", type: "dict", default: "None", desc: [Ordered semantic names → fragments. Use `(fragment, occurrence)` to select one zero-based repeated occurrence.]),),
  returns: (type: "Drawable", desc: [Math drawable with optional named tags.]),
  desc: [Compiled via Typst. A string selector keeps all matching occurrences; a tuple isolates one. Tag order controls `write_by_term`. Invalid selectors raise `TypeError` or `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
eq = scene.equation(
    "x + x = 2x",
    tags={"left_x": ("x", 0), "right_x": ("x", 1)},
).at(0, 0)
eq.tag("right_x").fill(GOLD)
scene.play([eq.write().duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.typst",
  kind: "factory",
  signature: "typst(source: str, *, width?) -> Drawable",
  params: ((name: "source", type: "str", default: none, desc: [Full Typst markup.]), (name: "width", type: "str|float", default: "None", desc: ["Page width, e.g. \"16cm\" or 800."]),),
  returns: (type: "Drawable", desc: [Compiled Typst drawable.]),
  desc: [Tables with spans, custom math structures. `@preview/...` imports resolved via Typst Universe cache.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
tbl = scene.typst('#table(columns: 2, [*Method*], [*Error*], [Baseline], [0.18], [GPU], [0.04])')
scene.play([tbl.fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.code",
  kind: "factory",
  signature: "code(source: str, *, language=\"text\", width, height) -> Drawable",
  params: ((name: "source", type: "str", default: none, desc: [Code string.]), (name: "language", type: "str", default: "\"text\"", desc: [For future highlighting.]), (name: "width", type: "float", default: "760.0", desc: [Block width.]),),
  returns: (type: "Drawable", desc: [Monospaced code block.]),
  desc: [Framed code for reveals. Token highlighting planned.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
snippet = scene.code("result = mass * acceleration", language="python").at(0, 0)
scene.play([snippet.fade_in().duration(0.5)])
scene.export("preview.webp", fps=30)
```
]

== Media

#api-entry(
  name: "Scene.image",
  kind: "factory",
  signature: "image(path: str, *, width?, height?, fit=\"contain\", crop?) -> Drawable",
  params: ((name: "path", type: "str", default: none, desc: [PNG/JPEG/WebP path.]), (name: "fit", type: "str", default: "\"contain\"", desc: ["contain|cover|stretch"]), (name: "width", type: "float", default: "None", desc: [Target width.]), (name: "crop", type: "(x,y,w,h)", default: "None", desc: [Source crop in pixels, top-left origin.]),),
  returns: (type: "Drawable", desc: [Textured drawable.]),
  desc: [Shares decoded texture across same path. Use `scaled`, `at` as usual.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
# Usa cualquier PNG/JPG/WebP local — se comparte textura si repites path
logo = scene.rect(120, 70).fill(WHITE).at(0, 0) # placeholder de imagen
caption = scene.text("scene.image(\"assets/logo.webp\")").at(0, -70)
scene.play([scene.group([logo, caption]).fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.svg",
  kind: "factory",
  signature: "svg(path: str) -> Drawable",
  params: ((name: "path", type: "str", default: none, desc: [SVG file path.]),),
  returns: (type: "Drawable", desc: [Group of vector paths.]),
  desc: [Imports geometry, gradients, transforms, clipPath, feGaussianBlur. Use `part(id)` to grab named group/path (case-sensitive, duplicate IDs error).],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
# Importa SVG real con scene.svg("assets/robot.svg") y accede con .part("id")
placeholder = scene.regular_polygon(6, 50).fill(BLUE).at(0, 0)
label = scene.text("scene.svg(\"assets/robot.svg\")").at(0, -80)
scene.play([scene.group([placeholder, label]).fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.part",
  kind: "method",
  signature: ".part(id: str) -> Drawable",
  params: ((name: "id", type: "str", default: none, desc: [Source element id, case-sensitive.]),),
  returns: (type: "Drawable", desc: [Named sub-drawable.]),
  desc: [Access SVG hierarchy. Unknown id raises `KeyError` listing available names.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
robot = scene.regular_polygon(5, 60).fill(BLUE).at(0, 0)
arm = robot # en SVG real: robot.part("arm")
scene.play([arm.rotate(0.4).duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.group",
  kind: "factory",
  signature: "group(members: list[Drawable]) -> Drawable",
  params: ((name: "members", type: "list[Drawable]", default: none, desc: [Members to group.]),),
  returns: (type: "Drawable", desc: [Group drawable.]),
  desc: [Move/rotate/scale as one. Stack with `vstack`/`hstack` before placing.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, BLACK, Scene
from gaanim import Direction
scene = Scene(480, 270, background="#0f172a")
row = scene.group([scene.dot(10).fill(BLUE), scene.text("Label").at(20, 0)]).at(0, 0)
scene.play([row.fade_in_from(Direction.DOWN, distance=24).duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

== Editorial

#api-entry(
  name: "Scene.callout",
  kind: "factory",
  signature: "callout(text, target, *, offset=(160,96), width=240, height=72) -> Drawable",
  params: ((name: "text", type: "str", default: none, desc: [Label text.]), (name: "target", type: "Drawable", default: none, desc: [Drawable to point at.]), (name: "offset", type: "(float,float)", default: "(160,96)", desc: [Card offset from target.]),),
  returns: (type: "Drawable", desc: [Group: card + text + connector, all follow target natively.]),
  desc: [Reusable editorial label without Python callback each frame.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
mass = scene.dot(12).fill(GOLD).at(-40, 0)
note = scene.callout("Moving mass", mass, offset=(130, 70))
scene.play([mass.move(80, 0).duration(1.0), note.fade_in().duration(0.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.caption",
  kind: "factory",
  signature: "caption(text, *, position=\"bottom\", width, height, margin) -> Drawable",
  params: ((name: "text", type: "str", default: none, desc: [Caption text.]), (name: "position", type: "str", default: "\"bottom\"", desc: ["bottom|top"]),),
  returns: (type: "Drawable", desc: [Lower-third card respecting safe area.]),
  desc: [Readable overlay for narration.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
dot = scene.dot(10).fill(GOLD).at(0, 30)
cap = scene.caption("The caption respects the safe area.", position="bottom")
scene.play([dot.fade_in().duration(0.4), cap.fade_in().duration(0.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.title_card",
  kind: "factory",
  signature: "title_card(title, subtitle?, *, width, height, panel=False) -> Drawable",
  params: ((name: "title", type: "str", default: none, desc: [Main title.]), (name: "subtitle", type: "str", default: "None", desc: [Optional subtitle.]), (name: "panel", type: "bool", default: "False", desc: [Framed version.]),),
  returns: (type: "Drawable", desc: [Centered opening with title + rule + optional subtitle.]),
  desc: [Conference opener. Single animatable group.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
opening = scene.title_card("Vector Motion", "A technical explanation", panel=True)
scene.play([opening.fade_in().duration(0.7)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.bullets",
  kind: "factory",
  signature: "bullets(items: list[str], *, width, gap, bullet_radius, bullet_color) -> Drawable",
  params: ((name: "items", type: "list[str]", default: none, desc: [Bullet strings, ≥1 non-empty.]), (name: "gap", type: "float", default: "68.0", desc: [Vertical gap.]),),
  returns: (type: "Drawable", desc: [Bulleted list as one drawable.]),
  desc: [Presentation agenda. Tune `width`, `bullet_radius`, `bullet_color`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
agenda = scene.bullets(["Setup", "Motion", "Export"], gap=48, bullet_color=GOLD).at(0, 40)
scene.play([agenda.fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "CoordinateSpace.bars",
  kind: "factory",
  signature: "bars(source: DataSource, x: str, y: str, *, width=0.8, baseline=0) -> Drawable",
  params: ((name: "source", type: "DataSource", default: none, desc: [Replaceable tabular data.]), (name: "x / y", type: "str", default: none, desc: [Numeric column names.]),),
  returns: (type: "Drawable", desc: [Batched bars parented to the coordinate space.]),
  desc: [Use a categorical `Axis` for labels. The mark regenerates natively after data replacement or append.],
)[
```python
from gaanim import Axis, BLUE, DataSource
data = DataSource({"x": [0, 1, 2], "value": [18, 42, 31]})
space = scene.axes(Axis.category(["Q1", "Q2", "Q3"]), Axis.linear(0, 50))
chart = space.bars(data, "x", "value").fill(BLUE)
```
]

#api-entry(
  name: "Scene.table",
  kind: "factory",
  signature: "table(headers, rows, *, width, row_height) -> Drawable",
  params: ((name: "headers", type: "list[str]", default: none, desc: [Column headers, ≥1.]), (name: "rows", type: "list[list[str]]", default: none, desc: [One cell per header, non-empty.]),),
  returns: (type: "Drawable", desc: [Table with blue header + rules.]),
  desc: [Compact technical table. All rows must match header count.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
tbl = scene.table(["Method","Error","Time"], [["Baseline","0.18","48 ms"],["GPU","0.04","15 ms"]]).at(0, 0)
scene.play([tbl.fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

== Reactive Geometry

#api-entry(
  name: "Scene.value_tracker",
  kind: "factory",
  signature: "value_tracker(initial: float) -> ValueTracker",
  params: ((name: "initial", type: "float", default: none, desc: [Starting value.]),),
  returns: (type: "ValueTracker", desc: [Scalar animated independently.]),
  desc: [Drive `always_redraw_arc`, `point_on_curve`, etc. Use `tracker.animate_to(v).duration(t)`. Reactive visuals need their own entry animation in `scene.play(...)`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
theta = scene.value_tracker(0.2)
arc = scene.always_redraw_arc(theta, 0, 0, 55, 0.0).fill(WHITE)
scene.play([arc.fade_in().duration(0.3), theta.animate_to(4.5).duration(1.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.point_on_curve",
  kind: "factory",
  signature: "point_on_curve(curve: Drawable, tracker: ValueTracker) -> Drawable",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Sampled polyline/bezier.]), (name: "tracker", type: "ValueTracker", default: none, desc: [0..1 clamped, arc-length.] ),),
  returns: (type: "Drawable", desc: [Dot following curve.]),
  desc: [Position by arc length, no Python callback during playback.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
from math import cos, sin, pi
scene = Scene(480, 270, background="#0f172a")
t = scene.value_tracker(0.0)
curve = scene.polyline([(110*cos(u), 60*sin(2*u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 2)
dot = scene.point_on_curve(curve, t).fill(GOLD)
scene.play([dot.fade_in().duration(0.3), t.animate_to(1.0).duration(1.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.tangent_on_curve / normal_on_curve",
  kind: "factory",
  signature: "tangent_on_curve(curve, tracker, length=80) / normal_on_curve(...) -> Drawable",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Curve.]), (name: "tracker", type: "ValueTracker", default: none, desc: [0..1.] ), (name: "length", type: "float", default: "80", desc: [Line length.]),),
  returns: (type: "Drawable", desc: [Line centered on curve point, rotated to tangent/normal.]),
  desc: [Normal is 90° CCW from tangent. Same arc-length sampling.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, BLACK, Scene
from math import cos, sin, pi
scene = Scene(480, 270, background="#0f172a")
t = scene.value_tracker(0.35)
curve = scene.polyline([(110*cos(u), 60*sin(u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 2)
tangent = scene.tangent_on_curve(curve, t, length=70).stroke(GOLD, 3)
scene.play([tangent.fade_in().duration(0.3), t.animate_to(0.9).duration(1.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.curvature_on_curve",
  kind: "factory",
  signature: "curvature_on_curve(curve, tracker, window=0.02) -> Drawable",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Curve.]), (name: "tracker", type: "ValueTracker", default: none, desc: [0..1.]),),
  returns: (type: "Drawable", desc: [Osculating circle.]),
  desc: [Estimated from neighboring arc-length samples. Style with `no_fill().stroke()`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
from math import cos, sin, pi
scene = Scene(480, 270, background="#0f172a")
t = scene.value_tracker(0.25)
curve = scene.polyline([(110*cos(u), 60*sin(u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 2)
circle = scene.curvature_on_curve(curve, t).no_fill().stroke(RED, 2)
scene.play([circle.fade_in().duration(0.3), t.animate_to(0.7).duration(1.4)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.always_redraw_arc",
  kind: "factory",
  signature: "always_redraw_arc(tracker, cx, cy, radius, start_angle) -> Drawable",
  params: ((name: "tracker", type: "ValueTracker", default: none, desc: [Drives sweep angle.]),),
  returns: (type: "Drawable", desc: [Regenerated arc each frame.]),
  desc: [For `ValueTracker`-driven rotations without Python callback.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
theta = scene.value_tracker(0.3)
rot = scene.always_redraw_arc(theta, 0, 0, 55, 0.0).fill(WHITE)
scene.play([rot.fade_in().duration(0.3), theta.animate_to(5.0).duration(1.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.spring_between",
  kind: "factory",
  signature: "spring_between(from, to, *, coils=8, amplitude=12, crossing=0) -> Drawable",
  params: ((name: "from", type: "Drawable|vec2", default: none, desc: [Endpoint A.]), (name: "to", type: "Drawable|vec2", default: none, desc: [Endpoint B.]), (name: "coils", type: "int", default: "8", desc: [Number of turns.]), (name: "amplitude", type: "float", default: "12", desc: [Radius perpendicular to the endpoint axis, in scene units.]), (name: "crossing", type: "float", default: "0", desc: [Normalized e-like interlacing amount from 0 to 1.]),),
  returns: (type: "Drawable", desc: [Reactive helical spring path.]),
  desc: [Endpoints can be drawables or (x,y). The spring is a smooth projected helix: its radius stays stable while its pitch deforms automatically as an endpoint moves. Set `crossing` above 0 to fold parts of each turn back and create e-like crossings.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
mass = scene.dot(10).fill(GOLD).at(70, 0)
spring = scene.spring_between(( -70, 0), mass, coils=6, amplitude=14, crossing=1.0).no_fill().stroke(WHITE, 3)
scene.play([spring.fade_in().duration(0.3), mass.move(40, 0).duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Scene.dimension_between",
  kind: "factory",
  signature: "dimension_between(from, to, offset: float) -> Drawable",
  params: ((name: "from", type: "Drawable|vec2", default: none, desc: [Endpoint A.]), (name: "to", type: "Drawable|vec2", default: none, desc: [Endpoint B.]),),
  returns: (type: "Drawable", desc: [Reactive dimension.]),
  desc: [Keeps measurement synchronized with moving endpoints.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
a = scene.dot(8).at(-60, 0)
b = scene.dot(8).at(60, 0)
dim = scene.dimension_between(a, b, 22).stroke(WHITE, 2)
scene.play([dim.fade_in().duration(0.3), b.move(30, 0).duration(0.9)])
scene.export("preview.webp", fps=30)
```
]

== Drawable Styling & Layout

Reference for the fluent handle returned by every factory. All return `Drawable` for chaining unless noted.

#api-entry(
  name: "Drawable.fill / stroke / opacity / effects",
  kind: "method",
  signature: ".fill(color) .stroke(color,width) .opacity(0..1) .glow / .blur / .shadow",
  params: ((name: "color", type: "Color|Brush", default: none, desc: [Fill or stroke paint.]),),
  returns: (type: "Drawable", desc: [Self.]),
  desc: [Style fluently. `no_fill()`, `no_stroke()`, `no_effects()` clear. `glow(color,radius,intensity)`, `blur(sigma)`, `shadow(color,x,y,blur)`.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, Scene
scene = Scene(480, 270, background="#0f172a")
obj = scene.circle(45).fill(BLUE).stroke(GOLD, 3).at(0, 0)
obj.glow(GOLD, radius=18)
scene.play([obj.grow_from_center().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable.at / scaled / rotated / z_index",
  kind: "method",
  signature: ".at(x,y) .scaled(factor) .rotated(radians) .z_index(int) .with_pivot(x,y)",
  params: ((name: "x", type: "float", default: none, desc: [Position / pivot.] ),),
  returns: (type: "Drawable", desc: [Self.]),
  desc: [Transforms in scene space. `with_pivot`/`pivot` sets rotation/scale origin.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
from math import pi
scene = Scene(480, 270, background="#0f172a")
arm = scene.rect(90, 18).fill(BLUE).at(45, 0).with_pivot(0, 0)
scene.play([arm.rotate(pi/2.5).duration(1.0)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable layout helpers",
  kind: "method",
  signature: ".next_to(ref, dir) .align_to(ref, anchor) .to_edge(dir, buff) .vstack / .hstack",
  params: ((name: "reference", type: "Drawable", default: none, desc: [Anchor target.]),),
  returns: (type: "Drawable", desc: [Self positioned relatively.]),
  desc: [`Direction`/`Anchor` enums. `vstack(gap, align)` and `hstack` for stacks.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
from gaanim import Anchor, Direction
scene = Scene(480, 270, background="#0f172a")
a = scene.circle(18).fill(BLUE).at(-60, 0)
b = scene.circle(18).fill(WHITE).at(0, 0)
c = scene.circle(18).fill(BLUE).at(60, 0)
row = scene.group([a, b, c]).hstack(gap=18)
scene.play([row.fade_in().duration(0.6)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable reactive bindings",
  kind: "method",
  signature: ".attach_to(src) .follow_to(src, offset) .bind_x/y/position_from(src)",
  params: ((name: "source", type: "Drawable", default: none, desc: [Drawable to follow.]),),
  returns: (type: "Drawable", desc: [Self with updater registered.]),
  desc: [Native follow without Python callback. `bind_position_from(src, axes=\"xy\")`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(480, 270, background="#0f172a")
mass = scene.dot(12).fill(GOLD).at(-60, 0)
label = scene.text("follower").at(0, 45)
label.attach_to(mass)
scene.play([label.fade_in().duration(0.3), mass.move(120, 0).duration(1.2)])
scene.export("preview.webp", fps=30)
```
]

#api-entry(
  name: "Drawable text selection",
  kind: "method",
  signature: ".color_by(fragment, color) .select(fragment, occurrence?) -> FragmentSelection .tag(name) -> FragmentSelection",
  params: ((name: "fragment", type: "str", default: none, desc: [Case-insensitive, ignores math spacing.] ),),
  returns: (type: "Drawable | FragmentSelection", desc: [Selection for chained animation.]),
  desc: [Grapheme-level control for `text`/`equation`. Tags retain their optional occurrence selector. `FragmentSelection` offers `fill`, `indicate`, `color_to`, `transform_to`, `reveal`, and `cancel`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, Scene
scene = Scene(480, 270, background="#0f172a")
eq = scene.equation("E = m c^2").at(0, 0)
eq.color_by("m", GOLD)
scene.play([eq.write().duration(0.8)])
scene.export("preview.webp", fps=30)
```
]
