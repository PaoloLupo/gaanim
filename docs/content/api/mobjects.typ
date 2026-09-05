#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "Objetos",
  description: "Fábricas de objetos de Scene: primitivas, trayectorias, texto, medios y composición",
  route: "/api/mobjects/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Objetos

Cada fábrica de `Scene` devuelve un `Drawable`. Encadena estilo, layout y
animación con la API fluida.

== Booleanas vectoriales

```python
cut = scene.geometry.difference(shape, hole, live=True)
merged = scene.geometry.union(left, right, tolerance=0.25)
shared = scene.geometry.intersection(left, right)
either = scene.geometry.xor(left, right)
```

Las operaciones trabajan sobre áreas vectoriales cerradas y conservan los
operandos. El resultado hereda el estilo del primer operando. Con `live=True`
se reconstruye después de propagación cuando cambian paths o transformaciones;
las fuentes siguen siendo drawables independientes. `tolerance` debe ser finita
y positiva; `rule` acepta `"nonzero"` o `"evenodd"`.

== Relleno porcentual de siluetas

`scene.geometry.fill_level(mask, paint, level=0.0, direction="up", keep_outline=True)`
genera un interior vectorial que se intersecta con la silueta en cada frame.
`level` pertenece a `[0, 1]`; las direcciones disponibles son `up`, `down`,
`left` y `right`. El SVG o drawable usado como `mask` conserva visibilidad
independiente. `keep_outline=True` añade una copia reactiva que usa únicamente
su stroke; oculta el original si su fill no debe cubrir el nivel.

```python
drop = scene.media.svg("drop.svg").no_fill().stroke("#dbeafe", 4).opacity(0)
water = scene.geometry.fill_level(drop, "#38bdf8", 0.0)
scene.play([water.animate.fill_level(0.72).duration(1.4)])
```

== Clases de tema y trazos completos

#api-entry(
  name: "Drawable.style_class",
  kind: "method",
  signature: "style_class(name: str) -> Self",
  params: ((name: "name", type: "str", default: none, desc: [Theme class without the leading dot; letters, digits, `_`, and `-` are accepted.]),),
  returns: (type: "Self", desc: [The same fluent drawable type.]),
  desc: [Adds an ordered class used by `Theme.styles`. Repeated calls cascade in order; explicit constructor and fluent styles retain higher priority. Applying a class to a group propagates it through nested visual members.],
)[
```python
# show-code: true
from gaanim import Scene, Style, Theme
theme = Theme("paper", styles={".warning": Style(fill="#e11d48")})
scene = Scene(frame=(16, 9), theme=theme)
warning = scene.geometry.square(90).style_class("warning")
scene.wait(0.1)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.tracking_line",
  kind: "reactive factory",
  signature: "tracking_line(from, to) -> Drawable",
  params: ((name: "from", type: "Endpoint", default: none, desc: [Fixed point, drawable origin, `PointRef`, or `AnchorPoint`.]), (name: "to", type: "Endpoint", default: none, desc: [Second same-frame endpoint.])),
  returns: (type: "Drawable", desc: [Hidden reactive line; reveal it with `create`, `write`, or another entry animation.]),
  desc: [Regenerates its full path whenever either endpoint moves while preserving active path-reveal progress.],
)[
```python
# show-code: true
from gaanim import GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tip = scene.geometry.dot(9).fill(GOLD).move_to(100, 30)
rod = scene.geometry.tracking_line((-100, -30), tip).no_fill().stroke(WHITE, 4)
scene.play([rod.animate.create().duration(0.8), tip.animate.shift_by(-40, 80).duration(0.8)])
scene.play([rod.animate.write().duration(0.8), tip.animate.shift_by(80, -50).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.stroke_style",
  kind: "method",
  signature: "stroke_style(style: StrokeStyle) -> Self",
  params: ((name: "style", type: "StrokeStyle", default: none, desc: [Paint, width, cap, join, miter limit, dash sequence, and dash offset.]),),
  returns: (type: "Self", desc: [The styled drawable.]),
  desc: [Applies complete native stroke geometry. Individual calls require a literal CSS color or Brush; token names are resolved when StrokeStyle is placed inside `Theme.styles`. Invalid stroke metrics raise `ValueError`.],
)[
```python
# show-code: true
from gaanim import Scene, StrokeStyle
scene = Scene(frame=(16, 9))
guide = scene.geometry.line(-160, 0, 160, 0).stroke_style(
    StrokeStyle("#2563eb", 5, cap="round", dashes=[18, 10])
)
scene.wait(0.1)
# output: preview.webp
scene.render()
```
]

== Modelos glTF importados

```python
MediaLibrary.gltf(path: str, *, scene: str | int | None = None) -> Drawable
Drawable.part(selector: str) -> Drawable
Drawable.parts() -> tuple[str, ...]
Drawable.animations() -> tuple[str, ...]
```

The model and every selected node support the complete 3D transform surface:
`at_3d(x,y,z)`, `scaled_3d(x,y,z)`, `rotated_3d(x,y,z)`, and
`with_pivot_3d(x,y,z)`. Euler rotations use XYZ order and radians.

== Primitivas PBR nativas

`Primitive3D` extends `Drawable`, so position, rotation, scale, opacity, and
the PBR material are animatable and deterministic under timeline seek. Geometry
parameters stay fixed after construction. Gaanim uses a right-handed, Y-up
world: cylinders and cones grow along Y, while planes lie on XZ.

```python
from gaanim import BLUE, GOLD, Material3D, Scene

scene = Scene(frame=(16, 9))
cube = scene.geometry.cube(2, material=Material3D.matte(BLUE))
sphere = scene.geometry.sphere(1, segments=32, rings=16,
                      material=Material3D.metal(GOLD)).move_to_3d(3, 0, 0)
floor = scene.geometry.plane(10, 8, subdivisions=(4, 4)).move_to_3d(0, -2, 0)
scene.geometry.lighting_3d("studio", intensity=1.0, shadows=True)
scene.play([cube.animate.create(), sphere.animate.create(), floor.animate.fade_in()])
scene.play([cube.animate.material(Material3D.metal(GOLD)).duration(1.0)])
```

```python
Geometry.cube(size=2.0, *, material=None) -> Primitive3D
Geometry.sphere(radius=1.0, *, segments=32, rings=16, material=None) -> Primitive3D
Geometry.cylinder(radius=1.0, height=2.0, *, segments=32, caps=True, material=None) -> Primitive3D
Geometry.cone(radius=1.0, height=2.0, *, segments=32, cap=True, material=None) -> Primitive3D
Geometry.plane(width=2.0, height=2.0, *, subdivisions=(1, 1), material=None) -> Primitive3D
```

`Material3D(color=WHITE, roughness=0.55, metallic=0.0, emissive=None,
emissive_strength=0.0)` validates all surface ranges. Presets
`Material3D.matte`, `.metal`, and `.emissive` cover common looks. Use
`Primitive3D.material(...)` for an immediate fluent change and
`material_to(...)` for interpolation. `create()` grows a mesh from its center
while fading it in; vector-only `write()` is rejected explicitly.

#html.div(style: "font-family: var(--font-code); font-size: 0.65rem; font-weight: 800; letter-spacing: 0.1em; text-transform: uppercase; background: var(--text-main); color: var(--bg-main); padding: 4px 8px; display: inline-block; margin-bottom: 16px;", [— 40+ FACTORIES · ALL RETURN Drawable —])

== Primitivas

#api-entry(
  name: "Geometry.circle",
  kind: "factory",
  signature: "circle(radius: float) -> Drawable",
  params: ((name: "radius", type: "float", default: none, desc: [Radius in scene units.]),),
  returns: (type: "Drawable", desc: [Circle centered at origin.]),
  desc: [Perfect for nodes, pills, or radial diagrams. Combine with `annulus` for donuts.],
)[
```python
# show-code: true
from gaanim import BLUE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
node = scene.geometry.circle(50).fill(BLUE).move_to(0, 0)
scene.play([node.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.rect",
  kind: "factory",
  signature: "rect(width: float, height: float) -> Drawable",
  params: ((name: "width", type: "float", default: none, desc: [Width.]), (name: "height", type: "float", default: none, desc: [Height.]),),
  returns: (type: "Drawable", desc: [Rectangle centered at origin.]),
  desc: [Cards, panels, bars. Use `rounded_rect` for softer UI.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
card = scene.geometry.rect(160, 90).fill(BLUE).stroke(WHITE, 2).move_to(0, 0)
scene.play([card.animate.grow_from_center().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.rounded_rect",
  kind: "factory",
  signature: "rounded_rect(width, height, radius: float) -> Drawable",
  params: ((name: "width", type: "float", default: none, desc: [Width.]), (name: "height", type: "float", default: none, desc: [Height.]), (name: "radius", type: "float", default: none, desc: [Corner radius.]),),
  returns: (type: "Drawable", desc: [Rounded rectangle.]),
  desc: [Buttons, tags, callout cards. Radius controls softness.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
btn = scene.geometry.rounded_rect(160, 50, 12).fill(GOLD).move_to(0, 0)
label = scene.text("CLICK").move_to(0, 0)
scene.play([scene.geometry.group([btn, label]).animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.surrounding_rect",
  kind: "factory",
  signature: "surrounding_rect(targets, *, padding=12, corner_radius=8) -> SurroundingRect",
  params: ((name: "targets", type: "Drawable | TextSelection | Sequence", default: none, desc: [One or more live object, text-part, or equation-part bounds.]), (name: "padding", type: "float | (v,h) | (t,r,b,l)", default: "12", desc: [Finite non-negative scene-unit inset around the union.]), (name: "corner_radius", type: "float", default: "8", desc: [Non-negative radius, clamped to the current frame size.])),
  returns: (type: "SurroundingRect", desc: [Theme-stroked, unfilled live frame.]),
  desc: [Uses the targets' world-space AABB and follows movement, scaling, rotation, and layout in the same frame. Empty, foreign-scene, or invalid targets and dimensions raise `TypeError` or `ValueError`. Position, scale, rotation, and Layout ownership belong to the binding; animate the target or call `retarget`.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
eq = scene.text.equation("E =", part("mass", "m"), part("light", "c^2"))
frame = scene.geometry.surrounding_rect(eq["mass"]).stroke(GOLD, 3)
scene.play([eq.animate.fade_in(), frame.animate.create()])
scene.play([frame.retarget(eq["light"]).duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.square",
  kind: "factory",
  signature: "square(size: float) -> Drawable",
  params: ((name: "size", type: "float", default: none, desc: [Side length.]),),
  returns: (type: "Drawable", desc: [Square.]),
  desc: [Shorthand for `rect(size, size)`. Useful for grid cells.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
cell = scene.geometry.square(80).fill(BLUE).move_to(0, 0)
scene.play([cell.animate.grow_from_center().duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.dot",
  kind: "factory",
  signature: "dot(radius: float) -> Drawable",
  params: ((name: "radius", type: "float", default: none, desc: [Radius, typically 4–16.]),),
  returns: (type: "Drawable", desc: [Small filled circle.]),
  desc: [Markers, bullet points, particles. Optimized for many instances.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
dot = scene.geometry.dot(8).fill(RED).move_to(0, 0)
scene.play([dot.animate.grow_from_center().duration(0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.ellipse",
  kind: "factory",
  signature: "ellipse(rx: float, ry: float) -> Drawable",
  params: ((name: "rx", type: "float", default: none, desc: [X radius.]), (name: "ry", type: "float", default: none, desc: [Y radius.]),),
  returns: (type: "Drawable", desc: [Ellipse.]),
  desc: [Orbits, highlights, Venn overlaps.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
orbit = scene.geometry.ellipse(90, 50).no_fill().stroke(GOLD, 2).move_to(0, 0)
scene.play([orbit.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

== Líneas y flechas

#api-entry(
  name: "Geometry.line",
  kind: "reactive factory",
  signature: "line(p1: Endpoint, p2: Endpoint) -> Drawable",
  params: ((name: "p1", type: "Endpoint", default: none, desc: [Fixed tuple, drawable origin, `PointRef`, or `AnchorPoint`.]), (name: "p2", type: "Endpoint", default: none, desc: [Second fixed or same-frame endpoint.])),
  returns: (type: "Drawable", desc: [Visible line segment that follows reference endpoints.]),
  desc: [Creates axes, dividers, or connectors. The compatibility form `line(x1, y1, x2, y2)` remains accepted. Invalid endpoints and mixed arities raise `TypeError`.],
)[
```python
# show-code: true
from gaanim import Anchor, GOLD, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
left = scene.geometry.dot(9).fill(GOLD).move_to(-120, -30)
card = scene.geometry.rect(120, 70).move_to(90, 35)
connector = scene.geometry.line(
    left.anchor_point(Anchor.RIGHT),
    card.anchor_point(Anchor.LEFT),
).stroke(WHITE, 3)
scene.play([left.animate.shift_by(30, 60).duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.arrow",
  kind: "factory",
  signature: "arrow(x1, y1, x2, y2) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Tail x.]), (name: "y1", type: "float", default: none, desc: [Tail y.]), (name: "x2", type: "float", default: none, desc: [Head x.]), (name: "y2", type: "float", default: none, desc: [Head y.]),),
  returns: (type: "Drawable", desc: [Arrow with head at (x2,y2).]),
  desc: [Transitions, causality, flow. For curved, use `curved_arrow`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
arrow = scene.geometry.arrow(-100, 0, 100, 0).stroke(GOLD, 4)
scene.play([arrow.animate.create().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.dashed_line",
  kind: "factory",
  signature: "dashed_line(x1, y1, x2, y2, *, dash_length=16, gap_length=10) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]), (name: "dash_length", type: "float", default: "16.0", desc: [Dash length.]), (name: "gap_length", type: "float", default: "10.0", desc: [Gap length.]),),
  returns: (type: "Drawable", desc: [Dashed line.]),
  desc: [Guides, hidden edges, construction lines.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
guide = scene.geometry.dashed_line(-140, 0, 140, 0, dash_length=12, gap_length=8).stroke(WHITE, 2)
scene.play([guide.animate.create().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.double_arrow",
  kind: "factory",
  signature: "double_arrow(x1, y1, x2, y2, *, head_length?, head_width?) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [End A x.]), (name: "y1", type: "float", default: none, desc: [End A y.]), (name: "x2", type: "float", default: none, desc: [End B x.]), (name: "y2", type: "float", default: none, desc: [End B y.]),),
  returns: (type: "Drawable", desc: [Double-headed arrow.]),
  desc: [Bidirectional relations, spans.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
span = scene.geometry.double_arrow(-100, 0, 100, 0).stroke(WHITE, 3)
scene.play([span.animate.create().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.arc",
  kind: "factory",
  signature: "arc(cx, cy, radius, start_angle, sweep_angle) -> Drawable",
  params: ((name: "cx", type: "float", default: none, desc: [Center x.]), (name: "cy", type: "float", default: none, desc: [Center y.]), (name: "radius", type: "float", default: none, desc: [Radius.]), (name: "start_angle", type: "float", default: none, desc: [Start in radians.]), (name: "sweep_angle", type: "float", default: none, desc: [Sweep in radians.]),),
  returns: (type: "Drawable", desc: [Arc path.]),
  desc: [Angles, progress, gauges. Use `no_fill().stroke()` for outline.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
arc = scene.geometry.arc(0, 0, 60, 0.0, 2.0).no_fill().stroke(GOLD, 4)
scene.play([arc.animate.create().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.curved_arrow",
  kind: "factory",
  signature: "curved_arrow(x1, y1, x2, y2, angle: float) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]), (name: "angle", type: "float", default: none, desc: [Deflection in radians.]),),
  returns: (type: "Drawable", desc: [Curved arrow.]),
  desc: [Feedback loops, rotations. Positive angle curves one way.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
loop = scene.geometry.curved_arrow(-80, 0, 80, 0, 0.9).fill(WHITE)
scene.play([loop.animate.create().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.curved_arrow_arc",
  kind: "factory",
  signature: "curved_arrow_arc(cx, cy, radius, start_angle, sweep_angle) -> Drawable",
  params: ((name: "cx", type: "float", default: none, desc: [Center x.]), (name: "cy", type: "float", default: none, desc: [Center y.]), (name: "radius", type: "float", default: none, desc: [Radius.]), (name: "start_angle", type: "float", default: none, desc: [Start radians.]), (name: "sweep_angle", type: "float", default: none, desc: [Sweep radians.]),),
  returns: (type: "Drawable", desc: [Arc-following curved arrow.]),
  desc: [Precise orbital arrows with explicit center/radius.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
arr = scene.geometry.curved_arrow_arc(0, 0, 70, 0.2, 1.8).fill(GOLD)
scene.play([arr.animate.create().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.dimension",
  kind: "factory",
  signature: "dimension(x1, y1, x2, y2, offset: float) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Point A x.]), (name: "y1", type: "float", default: none, desc: [Point A y.]), (name: "x2", type: "float", default: none, desc: [Point B x.]), (name: "y2", type: "float", default: none, desc: [Point B y.]), (name: "offset", type: "float", default: none, desc: [Perpendicular offset for arrow line.]),),
  returns: (type: "Drawable", desc: [Technical dimension with extension lines.]),
  desc: [Engineering drawings. For reactive, use `dimension_between`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
dim = scene.mechanics.dimension(-80, 0, 80, 0, 24).stroke(WHITE, 2)
scene.play([dim.animate.create().duration(0.7)])
# output: preview.webp
scene.render()
```
]

== Polígonos y símbolos

#api-entry(
  name: "Geometry.polygon",
  kind: "factory",
  signature: "polygon(points: list[(float,float)]) -> Drawable",
  params: ((name: "points", type: "list[vec2]", default: none, desc: [At least 3 finite points, closed automatically.]),),
  returns: (type: "Drawable", desc: [Polygon.]),
  desc: [Custom shapes, territories, highlights.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tri = scene.geometry.polygon([(0, 70), (-65, -50), (65, -50)]).fill(BLUE).stroke(WHITE, 2)
scene.play([tri.animate.grow_from_center().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.star",
  kind: "factory",
  signature: "star(points: int, outer_radius: float, inner_radius: float) -> Drawable",
  params: ((name: "points", type: "int", default: none, desc: [≥2 points.]), (name: "outer_radius", type: "float", default: none, desc: [Outer radius.]), (name: "inner_radius", type: "float", default: none, desc: [Inner radius.]),),
  returns: (type: "Drawable", desc: [Star polygon.]),
  desc: [Ratings, badges, explosions. 5 points is classic star.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
star = scene.geometry.star(5, 70, 32).fill(GOLD).move_to(0, 0)
scene.play([star.animate.spin_in_from_nothing().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.regular_polygon",
  kind: "factory",
  signature: "regular_polygon(sides: int, radius: float) -> Drawable",
  params: ((name: "sides", type: "int", default: none, desc: [≥3 sides.]), (name: "radius", type: "float", default: none, desc: [Circumradius.]),),
  returns: (type: "Drawable", desc: [Regular polygon.]),
  desc: [Hex grids, stop signs, diagrams. 6 = hexagon.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
hexa = scene.geometry.regular_polygon(6, 60).fill(BLUE).stroke(WHITE, 2)
scene.play([hexa.animate.spin_in_from_nothing().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.sector",
  kind: "factory",
  signature: "sector(cx, cy, radius, start_angle, sweep_angle) -> Drawable",
  params: ((name: "cx", type: "float", default: none, desc: [Center x.]), (name: "cy", type: "float", default: none, desc: [Center y.]), (name: "radius", type: "float", default: none, desc: [Radius.]), (name: "start_angle", type: "float", default: none, desc: [Start rad.]), (name: "sweep_angle", type: "float", default: none, desc: [Sweep rad.]),),
  returns: (type: "Drawable", desc: [Pie slice.]),
  desc: [Pie charts, progress wedges.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
slice_ = scene.geometry.sector(0, 0, 70, 0.0, 2.0).fill(GOLD)
scene.play([slice_.animate.grow_from_center().duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.annulus",
  kind: "factory",
  signature: "annulus(outer_radius: float, inner_radius: float) -> Drawable",
  params: ((name: "outer_radius", type: "float", default: none, desc: [Outer > inner.]), (name: "inner_radius", type: "float", default: none, desc: [Inner > 0.]),),
  returns: (type: "Drawable", desc: [Ring.]),
  desc: [Donut charts, halos, targets.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
ring = scene.geometry.annulus(60, 34).fill(BLUE).stroke(WHITE, 2)
scene.play([ring.animate.grow_from_center().duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.brace",
  kind: "factory",
  signature: "brace(x1, y1, x2, y2, height: float) -> Drawable",
  params: ((name: "x1", type: "float", default: none, desc: [Start x.]), (name: "y1", type: "float", default: none, desc: [Start y.]), (name: "x2", type: "float", default: none, desc: [End x.]), (name: "y2", type: "float", default: none, desc: [End y.]), (name: "height", type: "float", default: none, desc: [Brace depth, non-zero.]),),
  returns: (type: "Drawable", desc: [Curly brace.]),
  desc: [Grouping annotations under equations or diagrams.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
brace = scene.geometry.brace(-80, -20, 80, -20, 24).stroke(WHITE, 3).no_fill()
label = scene.text("interval").move_to(0, -55)
scene.play([brace.animate.create().duration(0.7), label.animate.fade_in().duration(0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.checkmark",
  kind: "factory",
  signature: "checkmark(size: float) -> Drawable",
  params: ((name: "size", type: "float", default: none, desc: [Size > 0.]),),
  returns: (type: "Drawable", desc: [Checkmark.]),
  desc: [Correct answers, approvals. Pair with `cross` for quiz UI.],
)[
```python
# show-code: true
from gaanim import GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
ok = scene.geometry.checkmark(34).fill(GREEN).move_to(0, 0)
scene.play([ok.animate.grow_from_center().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.cross",
  kind: "factory",
  signature: "cross(size: float) -> Drawable",
  params: ((name: "size", type: "float", default: none, desc: [Size > 0.]),),
  returns: (type: "Drawable", desc: [X mark.]),
  desc: [Errors, rejections, close buttons.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
no = scene.geometry.cross(34).stroke(WHITE, 4).move_to(0, 0)
scene.play([no.animate.create().duration(0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.right_angle",
  kind: "factory",
  signature: "right_angle(arm_length: float) -> Drawable",
  params: ((name: "arm_length", type: "float", default: none, desc: [Arm length > 0.]),),
  returns: (type: "Drawable", desc: [Right-angle mark.]),
  desc: [Geometry proofs. Place at triangle corner.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
corner = scene.geometry.right_angle(24).stroke(WHITE, 3).move_to(0, 0)
axis = scene.geometry.line(-80, 0, 80, 0).stroke(WHITE, 2)
scene.play([scene.geometry.group([axis, corner]).animate.create().duration(0.7)])
# output: preview.webp
scene.render()
```
]

== Trayectorias

#api-entry(
  name: "Geometry.path",
  kind: "factory",
  signature: "path(definition) -> Drawable",
  params: ((name: "definition", type: "list[vec2] | list[command]", default: none, desc: [Points for polyline, or commands like (\"cubic\", [(x,y),...]).]),),
  returns: (type: "Drawable", desc: [Path.]),
  desc: [Compact entry for custom geometry. Delegates to `polyline` or `curve`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
rail = scene.geometry.path([(-140, 0), (0, 60), (140, 0)]).no_fill().stroke(WHITE, 4)
scene.play([rail.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.polyline",
  kind: "factory",
  signature: "polyline(points: list[vec2]) -> Drawable",
  params: ((name: "points", type: "list[vec2]", default: none, desc: [≥2 points.]),),
  returns: (type: "Drawable", desc: [Polyline.]),
  desc: [Explicit polyline when kind matters.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
zig = scene.geometry.polyline([(-100, -30), (0, 30), (100, -30)]).no_fill().stroke(GOLD, 3)
scene.play([zig.animate.create().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.bezier",
  kind: "factory",
  signature: "bezier(start: vec2, controls: list[vec2], end: vec2) -> Drawable",
  params: ((name: "start", type: "vec2", default: none, desc: [Start point.]), (name: "controls", type: "list[vec2]", default: none, desc: [1 = quadratic, 2 = cubic.]), (name: "end", type: "vec2", default: none, desc: [End point.]),),
  returns: (type: "Drawable", desc: [Bézier path.]),
  desc: [Smooth curves that stay Bézier (drives reactive bindings).],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
curve = scene.geometry.bezier((-140, 0), [(-50, 90), (50, -90)], (140, 0)).no_fill().stroke(WHITE, 3)
scene.play([curve.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.curve",
  kind: "factory",
  signature: "curve(commands: list[(str, list)]) -> Drawable",
  params: ((name: "commands", type: "list[command]", default: none, desc: [Commands like move, line, quad, cubic, close with args. Append _rel_ for relative.] ),),
  returns: (type: "Drawable", desc: [Composed Bézier curve.]),
  desc: [Typst-inspired cursor commands with auto handles.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
shape = scene.geometry.curve([("move", [(0, 0)]), ("cubic", [(50, 60), (110, -60), (160, 0)]), ("close", [])]).no_fill().stroke(WHITE, 3).move_to(-80, 0)
scene.play([shape.animate.create().duration(1.0)])
# output: preview.webp
scene.render()
```
]

== Geometría 3D

Gaanim uses ordinary Python tuples for 3D points: `(x, y, z)`. The same
`Drawable` handle can be styled and animated after it is placed in world space.
This small example combines axes, a polyline, a moving point, a traced path,
and the perspective camera:

```python
# show-code: true
from gaanim import Axis, BLACK, BLUE, GOLD, RED, WHITE, Scene, Updater
scene = Scene(frame=(16, 9), background=BLACK)

axes = scene.viz.cartesian_3d(
    Axis.linear(-3, 3).ticks(1).label("x").style(color=WHITE),
    Axis.linear(-3, 3).ticks(1).label("y").style(color=WHITE),
    Axis.linear(-2, 2).ticks(1).label("z").style(color=WHITE),
    size=(6, 6, 4),
)
path = scene.geometry.polyline_3d([(-2, -1, -1), (0, 1, 0), (2, -1, 1)], color=RED)
dot = scene.geometry.dot(8).fill(GOLD).move_to_3d(1, 0, 0).billboard()
dot.add_updater(Updater.orbit(0, 0, 1, 1.2))
trail = scene.geometry.traced_path_3d(dot, colormap="viridis", max_points=120)

scene.camera.perspective(fov_y=0.785, near=0.1, far=100.0)
scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0))
scene.play([
    axes.animate.create().duration(0.8),
    path.animate.create().duration(0.8),
    dot.animate.fade_in().duration(0.4),
    trail.animate.fade_in().duration(0.4),
])
scene.play([scene.camera.animate.orbit(delta_yaw=0.5, delta_pitch=0.1).duration(0.8)])
scene.play([scene.camera.animate.dolly(factor=0.85).duration(0.5)])
scene.wait(0.5)
# output: 3d-quickstart.webp
scene.render()
```

#api-entry(
  name: "Visualization.cartesian_3d",
  kind: "factory",
  signature: "cartesian_3d(x: Axis, y: Axis, z: Axis, *, size=(10,8,6), grid=True) -> Cartesian3D",
  params: ((name: "x / y / z", type: "Axis", default: none, desc: [Reusable linear or temporal axis specifications.]), (name: "size", type: "(float,float,float)", default: "(10,8,6)", desc: [Positive world-space size; choose it relative to the camera distance.]), (name: "grid", type: "bool", default: "True", desc: [Show the three grid planes.]),),
  returns: (type: "CoordinateSpace3D", desc: [Typed space with surface and parametric methods.]),
  desc: [Creates Cartesian axes and up to three grid planes. Use `surface` and `parametric` on the returned space so data coordinates share the same mapping.],
)[
```python
axes = scene.viz.cartesian_3d(
    Axis.linear(-5, 5).ticks(1).label("x"),
    Axis.linear(-5, 5).ticks(1).label("y"),
    Axis.linear(-3, 3).ticks(1).label("z"),
)
```
]

#api-entry(
  name: "Geometry.polyline_3d",
  kind: "factory",
  signature: "polyline_3d(points, color=None, *, colors=None, colormap=None) -> Drawable",
  params: ((name: "points", type: "list[(float,float,float)]", default: none, desc: [At least two finite world-space points.]), (name: "color", type: "Color", default: "None", desc: [Uniform color.]), (name: "colors", type: "list[Color]", default: "None", desc: [One color per point; takes precedence over colormap.]), (name: "colormap", type: "ColorMap | str", default: "None", desc: [Any built-in Matplotlib or Scientific Colour Map, or a custom `ColorMap`.]),),
  returns: (type: "Drawable", desc: [3D line strip in world space.]),
  desc: [Use `color` for a uniform line, `colors` for explicit vertex colors, or `colormap` for a time-ordered gradient. A colors list must have exactly the same length as `points`.],
)[
```python
helix = scene.geometry.polyline_3d(points, colormap="inferno")
```
]

#api-entry(
  name: "Drawable.at_3d",
  kind: "method",
  signature: ".move_to_3d(x, y, z) -> Drawable",
  params: ((name: "x / y / z", type: "float", default: none, desc: [World-space position.]),),
  returns: (type: "Drawable", desc: [The same drawable for fluent chaining.]),
  desc: [Places a drawable in 3D world space. Add `.billboard()` to keep text or a marker facing the camera, or `.hud()` for a fixed screen-space overlay.],
)[
```python
label = scene.text("origin").move_to_3d(0, 0, 0.5).billboard()
```
]

#api-entry(
  name: "Geometry.traced_path",
  kind: "factory",
  signature: "traced_path(source, *, dissipating_time=None, max_points=None, min_distance=1.0) -> Drawable",
  params: ((name: "source", type: "Drawable", default: none, desc: [Drawable whose position is sampled.]), (name: "dissipating_time", type: "float", default: "None", desc: [Seconds each sample remains in the trail; must be positive.]), (name: "max_points", type: "int", default: "None", desc: [Positive cap for retained samples.]), (name: "min_distance", type: "float", default: "1.0", desc: [Minimum scene-space distance between samples.]),),
  returns: (type: "Drawable", desc: [Reactive 2D trail, hidden until `fade_in` reveals it.]),
  desc: [Sampling begins at the timeline cursor where the trail is declared, so earlier segments and seeks cannot pre-fill it. Add `trail.animate.fade_in()` to `scene.play(...)` to reveal it. With `dissipating_time`, samples expire from the tail in editor playback, random seeks, snapshots, and exports.],
)[
```python
from gaanim import RED

dot = scene.geometry.dot(7).move_to(120, 0)
trail = scene.geometry.traced_path(dot, dissipating_time=2.0).stroke(RED, 3).no_fill()
scene.play([trail.animate.fade_in()])
```
]

#api-entry(
  name: "Geometry.traced_path_3d",
  kind: "factory",
  signature: "traced_path_3d(source, *, colormap=None, dissipating_time=None, max_points=None, min_distance=0.1) -> Drawable",
  params: ((name: "source", type: "Drawable", default: none, desc: [Drawable whose world-space position is sampled.]), (name: "colormap", type: "str", default: "None", desc: ["inferno", "viridis", or "plasma".]), (name: "dissipating_time", type: "float", default: "None", desc: [Seconds each sample remains in the trail; must be positive.]), (name: "max_points", type: "int", default: "None", desc: [Positive cap for retained samples.]), (name: "min_distance", type: "float", default: "0.1", desc: [Minimum world-space distance between samples.]),),
  returns: (type: "Drawable", desc: [Reactive 3D trail, hidden until its entry animation.]),
  desc: [The trail updates while `source` moves, so it works with `Updater` or `add_updater_fn`. Sampling starts where the trail is declared. Add `trail.animate.fade_in()` or `trail.animate.create()` to `scene.play(...)` to reveal it. `dissipating_time` expires the old tail, `max_points` limits memory, and `min_distance` filters nearly identical samples.],
)[
```python
dot = scene.geometry.dot(7).move_to_3d(1, 0, 0)
dot.add_updater(Updater.orbit(0, 0, 1, 1.5))
trail = scene.geometry.traced_path_3d(
    dot, colormap="viridis", dissipating_time=2.0, max_points=600
)
scene.play([trail.animate.fade_in()])
```
]

== Gráficos

Plots are no longer free `Scene` factories. They are children of a typed
coordinate space, which owns scales, conversions, clipping, and sampling.
See #link("/api/visualization/", "Visualization API") for Cartesian,
polar, complex, and 3D spaces; reactive callbacks; data marks; statistics;
and calculus helpers.

```python
import math
from gaanim import Axis, BLUE, Scene

scene = Scene(frame=(16, 9), background="#0f172a")
space = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
curve = space.plot(lambda x: math.sin(x)).stroke(BLUE, 3)
scene.play([space.animate.create(), curve.animate.create()])
```

== Texto y matemáticas

This page keeps a compact factory index. The canonical, complete reference is
#link("/api/text/", "Text — content, math, style, flow, selections, Layout, and animation").

#api-entry(
  name: "Scene.text",
  kind: "factory",
  signature: "text(*content, role=None, style=None, flow=None, **overrides) -> Text",
  params: ((name: "content", type: "str | TextPart | TextParts", default: none, desc: [Composable strings and semantic parts.]), (name: "role", type: "str | None", default: "None", desc: [title|subtitle|heading|body|caption|label|code|math]),),
  returns: (type: "Text", desc: [Structured, measurable vector text.]),
  desc: [`$...$` enables math, `\$` emits a literal dollar, and unbalanced delimiters raise `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
label = scene.text("Hello, ", part("product", "Gaanim", color=GOLD)).move_to(0, 0)
scene.play([label.animate.write().duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Typography.equation",
  kind: "factory",
  signature: "equation(*content, role=None, style=None, flow=None, **overrides) -> Text",
  params: ((name: "content", type: "str | TextPart | TextParts", default: none, desc: [Equation content without surrounding `$` delimiters.]),),
  returns: (type: "Text", desc: [Standalone display equation with the complete structured-text API.]),
  desc: [Adds and preserves `$ ... $` internally so Typst composes a block equation. It otherwise shares `Scene.text` styling, flow, selections, and animations; content boundaries use ordinary Typst whitespace. Without an explicit role or size it uses the 44-unit math default.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, part, parts
scene = Scene(frame=(16, 9), background="#0f172a")
eq = scene.text.equation(part("force", "sum F_t"), "=", parts(mass="m", acceleration="a_t"))
eq["acceleration"].fill(GOLD)
scene.play([eq.animate.write(by="part").duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "TextFlow",
  kind: "value",
  signature: "TextFlow(*, wrap=\"auto\", align=\"left\", line_spacing=1.2, max_lines=None, overflow=\"clip\", direction=\"auto\", hyphenate=False)",
  params: ((name: "wrap", type: "\"auto\" | False | float", default: "\"auto\"", desc: [Uses the offered Layout width, disables wrapping, or caps typographic width.]), (name: "align", type: "str", default: "\"left\"", desc: [left|center|right|justify]), (name: "overflow", type: "str", default: "\"clip\"", desc: [visible|clip|ellipsis]),),
  returns: (type: "TextFlow", desc: [Reusable internal composition options.]),
  desc: [Box size, padding, columns, fit, growth, and vertical alignment remain Layout v2 properties.],
)[
```python
# show-code: true
from gaanim import Scene, TextFlow
scene = Scene(frame=(16, 9), background="#0f172a")
body = scene.text("Una explicación larga que se ajusta al ancho.", flow=TextFlow(wrap=320, align="left", line_spacing=1.25)).move_to(0, 0)
scene.play([body.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Text roles and TextStyle",
  kind: "value",
  signature: "TextStyle(font=None, math_font=None, size=None, weight=None, color=None, ...)",
  params: ((name: "font", type: "str | None", default: "None", desc: [Primary text font.]), (name: "math_font", type: "str | None", default: "None", desc: [Math font used by inline equations.]), (name: "size", type: "float | None", default: "None", desc: [Font size in canvas/Typst points.]), (name: "weight", type: "int | None", default: "None", desc: [Font weight from 1 through 1000.]), (name: "color", type: "Color | None", default: "None", desc: [Resolved glyph color.])),
  returns: (type: "TextStyle", desc: [Reusable typography overlay.]),
  desc: [Roles are title, subtitle, heading, body, caption, label, code, and math. The 1080p-oriented defaults are respectively 64, 48, 48, 40, 32, 36, 36, and 44 scene units. Role theme values are resolved before `TextStyle`, direct keywords, local `part` style, and persistent selection changes.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.text("Fourier Transform", role="title").move_to(0, 30)
s = scene.text("A visual proof", role="subtitle").move_to(0, -20)
scene.play([t.animate.write().duration(0.7), s.animate.fade_in().duration(0.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "TextPart, TextParts, and inline math",
  kind: "factory",
  signature: "part(name, *content, **style) -> TextPart | parts(**content: str) -> TextParts",
  params: ((name: "name", type: "str", default: none, desc: [Non-empty semantic name, unique among siblings.]), (name: "content", type: "str | TextPart | TextParts", default: none, desc: [Nested content, or ordered keyword strings for `parts()`.]), (name: "style", type: "TextStyle | keywords", default: "None", desc: [Local typography overlay for a `part()` subtree.])),
  returns: (type: "TextPart | TextParts", desc: [Composable semantic subtree or ordered plain-part group.]),
  desc: [Math and prose share structured `Text`; `scene.text.equation` supplies display delimiters. Semantic paths replace manual ranges and equation tags. Every content boundary inside math becomes ordinary Typst whitespace, while prose boundaries remain exact. Local part styles remain inside one Typst equation and never introduce synthetic `#h()` gaps.],
)[
```python
# show-code: true
from gaanim import GOLD, Scene, parts
scene = Scene(frame=(16, 9), background="#0f172a")
# Fluent placement preserves the specialized Text handle.
eq = scene.text.equation(parts(variable="x", operator="dot 5 =", result="25")).move_to(0, 0)
eq["result"].fill(GOLD)
scene.play([eq.animate.write().duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Typography.typst",
  kind: "factory",
  signature: "typst(source: str, *, width?) -> Drawable",
  params: ((name: "source", type: "str", default: none, desc: [Full Typst markup.]), (name: "width", type: "str|float", default: "None", desc: ["Page width, e.g. \"16cm\" or 800."]),),
  returns: (type: "Drawable", desc: [Compiled Typst drawable.]),
  desc: [Tables with spans, custom math structures. `@preview/...` imports resolved via Typst Universe cache.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tbl = scene.text.typst('#table(columns: 2, [*Method*], [*Error*], [Baseline], [0.18], [GPU], [0.04])')
scene.play([tbl.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Typography.code",
  kind: "factory",
  signature: "code(source: str, *, language=\"text\", width, height) -> Drawable",
  params: ((name: "source", type: "str", default: none, desc: [Code string.]), (name: "language", type: "str", default: "\"text\"", desc: [For future highlighting.]), (name: "width", type: "float", default: "760.0", desc: [Block width.]),),
  returns: (type: "Drawable", desc: [Monospaced code block.]),
  desc: [Framed code for reveals. Token highlighting planned.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
snippet = scene.text.code("result = mass * acceleration", language="python").move_to(0, 0)
scene.play([snippet.animate.fade_in().duration(0.5)])
# output: preview.webp
scene.render()
```
]

== Medios

#api-entry(
  name: "MediaLibrary.image",
  kind: "factory",
  signature: "image(path: str, *, width?, height?, fit=\"contain\", crop?) -> Drawable",
  params: ((name: "path", type: "str", default: none, desc: [PNG/JPEG/WebP path.]), (name: "fit", type: "str", default: "\"contain\"", desc: ["contain|cover|stretch"]), (name: "width", type: "float", default: "None", desc: [Target width.]), (name: "crop", type: "(x,y,w,h)", default: "None", desc: [Source crop in pixels, top-left origin.]),),
  returns: (type: "Drawable", desc: [Textured drawable.]),
  desc: [Shares decoded texture across same path. Use `scaled`, `at` as usual.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
# Usa cualquier PNG/JPG/WebP local — se comparte textura si repites path
logo = scene.geometry.rect(120, 70).fill(WHITE).move_to(0, 0) # placeholder de imagen
caption = scene.text("scene.media.image(\"assets/logo.webp\")").move_to(0, -70)
scene.play([scene.geometry.group([logo, caption]).animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "MediaLibrary.video",
  kind: "factory",
  signature: "video(path: str, *, width?, height?, fit=\"contain\", crop?, offset=0.0, duration?, loop=false, speed=1.0, audio=true, volume=1.0) -> Video",
  params: ((name: "path", type: "str", default: none, desc: [Archivo MP4 local.]), (name: "offset", type: "float", default: "0.0", desc: [Inicio dentro de la fuente, en segundos.]), (name: "duration", type: "float", default: "hasta el final", desc: [Duración seleccionada de la fuente.]), (name: "loop", type: "bool", default: "false", desc: [Repite el intervalo seleccionado.]), (name: "speed", type: "float", default: "1.0", desc: [Velocidad positiva; conserva el tono del audio.]), (name: "audio", type: "bool", default: "true", desc: [Reproduce y exporta la primera pista de audio.]), (name: "volume", type: "float", default: "1.0", desc: [Ganancia no negativa.]),),
  returns: (type: "Video", desc: [Declaracion transformable que se activa con `Scene.play`.]),
  desc: [Usa FFmpeg/ffprobe. Declarar no inicia frames ni audio: `scene.play([clip])` fija el inicio absoluto. Un video finito sin loop aporta su duración de salida al batch; un loop no alarga el timeline. `width`, `height`, `fit` y `crop` tienen la misma semántica que `MediaLibrary.image`.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9))
# Con un MP4 local: clip = scene.media.video("assets/clip.mp4", width=720, duration=4, loop=True, volume=0.8); scene.play([clip])
clip = scene.geometry.rect(720, 405) # placeholder ejecutable para la documentación
scene.play([clip.animate.fade_in().duration(0.4)])
scene.wait(7.6)
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "MediaLibrary.lottie",
  kind: "factory",
  signature: "lottie(path: str, *, width?, height?, fit=\"contain\", offset=0.0, duration?, loop=false, speed=1.0) -> Lottie",
  params: ((name: "path", type: "str", default: none, desc: [Archivo Lottie JSON local.]), (name: "fit", type: "str", default: "\"contain\"", desc: ["contain|cover|stretch"]), (name: "offset", type: "float", default: "0.0", desc: [Inicio dentro de la composición, en segundos.]), (name: "duration", type: "float", default: "hasta el final", desc: [Duración seleccionada de la fuente.]), (name: "loop", type: "bool", default: "false", desc: [Repite el intervalo seleccionado.]), (name: "speed", type: "float", default: "1.0", desc: [Velocidad positiva.]),),
  returns: (type: "Lottie", desc: [Declaración transformable que se activa con `Scene.play`.]),
  desc: [Renderiza Lottie JSON con Velato dentro de la misma escena Vello. Declarar no inicia la animación: `scene.play([clip])` fija su inicio en el timeline. Un clip finito aporta su duración al batch; un loop no lo alarga. `clip.warnings` enumera características detectadas que Velato omite o aproxima.],
)[
```python
# show-code: true
from gaanim import Scene
scene = Scene(frame=(16, 9), background="#0f172a")
# Con un JSON local: clip = scene.media.lottie("assets/pulse.json", width=180); scene.play([clip])
clip = scene.geometry.circle(70) # placeholder ejecutable para la documentación
scene.play([clip.animate.fade_in().duration(0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "MediaLibrary.svg",
  kind: "factory",
  signature: "svg(path: str) -> Drawable",
  params: ((name: "path", type: "str", default: none, desc: [SVG file path.]),),
  returns: (type: "Drawable", desc: [Group of vector paths.]),
  desc: [Imports geometry, gradients, transforms, clipPath, feGaussianBlur. Use `part(id)` to grab named group/path (case-sensitive, duplicate IDs error).],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
# Importa SVG real con scene.media.svg("assets/robot.svg") y accede con .part("id")
placeholder = scene.geometry.regular_polygon(6, 50).fill(BLUE).move_to(0, 0)
label = scene.text("scene.media.svg(\"assets/robot.svg\")").move_to(0, -80)
scene.play([scene.geometry.group([placeholder, label]).animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
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
scene = Scene(frame=(16, 9), background="#0f172a")
robot = scene.geometry.regular_polygon(5, 60).fill(BLUE).move_to(0, 0)
arm = robot # en SVG real: robot.part("arm")
scene.play([arm.animate.rotate_by(0.4).duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.group",
  kind: "factory",
  signature: "group(members: list[Drawable]) -> Drawable",
  params: ((name: "members", type: "list[Drawable]", default: none, desc: [Members to group.]),),
  returns: (type: "Drawable", desc: [Group drawable.]),
  desc: [Move/rotate/scale as one while preserving the authored coordinate frame used by member updaters. A deferred trace, force, or connector no longer hides already-visible siblings merely by joining the group. A group `move` keeps deferred members hidden until their own entry; `write`, `create`, and fades on the group are explicit entries for the complete subtree. Use `LayoutBuilder.row`, `column`, `grid`, or `stack` for layout.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, BLACK, Scene
from gaanim import Direction
scene = Scene(frame=(16, 9), background="#0f172a")
row = scene.geometry.group([scene.geometry.dot(10).fill(BLUE), scene.text("Label").move_to(20, 0)]).move_to(0, 0)
scene.play([row.animate.fade_in_from(Direction.DOWN, distance=24).duration(0.6)])
# output: preview.webp
scene.render()
```
]

== Composición editorial

#api-entry(
  name: "SlideKit.callout",
  kind: "factory",
  signature: "callout(text, target, *, offset=(160,96), width=240, height=72) -> Drawable",
  params: ((name: "text", type: "str", default: none, desc: [Label text.]), (name: "target", type: "Drawable", default: none, desc: [Drawable to point at.]), (name: "offset", type: "(float,float)", default: "(160,96)", desc: [Card offset from target.]),),
  returns: (type: "Drawable", desc: [Group: card + text + connector, all follow target natively.]),
  desc: [Reusable editorial label without Python callback each frame.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
mass = scene.geometry.dot(12).fill(GOLD).move_to(-40, 0)
note = scene.slides.callout("Moving mass", mass, offset=(130, 70))
scene.play([mass.animate.shift_by(80, 0).duration(1.0), note.animate.fade_in().duration(0.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SlideKit.badge",
  kind: "factory",
  signature: "badge(text, *, variant=\"neutral\", appearance=\"soft\", padding=(18,10), radius=None, font_size=None, min_width=None, color=None, background=None, border=None) -> Drawable",
  params: ((name: "text", type: "str", default: none, desc: [Non-empty label.]), (name: "variant", type: "str", default: "\"neutral\"", desc: [neutral, accent, success, warning, or danger.]), (name: "appearance", type: "str", default: "\"soft\"", desc: [soft, solid, or outline.])),
  returns: (type: "Drawable", desc: [Auto-sized pill group at the origin.]),
  desc: [`radius=None` derives a pill radius from measured height. Invalid text or finite geometry raises `ValueError`; position with `.move_to(...)`.],
)[
```python
tag = scene.slides.badge("READY", variant="success", appearance="solid").move_to(-240, 120)
scene.play([tag.animate.grow_from_center()])
```
]

#api-entry(
  name: "SlideKit.chip",
  kind: "factory",
  signature: "chip(text, *, dot=True, variant=\"neutral\", appearance=\"soft\", padding=(14,8), radius=None, font_size=None, color=None, background=None, border=None) -> Drawable",
  params: ((name: "text", type: "str", default: none, desc: [Non-empty label.]), (name: "dot", type: "bool", default: "True", desc: [Show the semantic tone dot.])),
  returns: (type: "Drawable", desc: [Compact auto-sized group.]),
  desc: [A smaller badge for filters, states, and metadata. Theme and validation behavior matches `badge`.],
)[
```python
live = scene.slides.chip("Live", variant="danger", appearance="outline")
```
]

#api-entry(
  name: "SlideKit.card",
  kind: "factory",
  signature: "card(title, body=None, footer=None, *, width=420, min_height=180, padding=(28,24), gap=14, radius=18, variant=\"neutral\", appearance=\"soft\", color=None, background=None, border=None) -> Drawable",
  params: ((name: "title", type: "str", default: none, desc: [Heading slot.]), (name: "body", type: "str | None", default: "None", desc: [Wrapped body slot.]), (name: "footer", type: "str | None", default: "None", desc: [Caption slot.])),
  returns: (type: "Drawable", desc: [Auto-height panel group.]),
  desc: [Semantic text roles are measured at construction. Empty supplied slots or invalid dimensions raise `ValueError`.],
)[
```python
result = scene.slides.card("Result", "The solver converged.", "12 ms", variant="accent")
```
]

#api-entry(
  name: "SlideKit.banner",
  kind: "factory",
  signature: "banner(title, subtitle=None, *, position=\"top\", width=None, margin=32, padding=(28,18), gap=8, radius=14, variant=\"neutral\", appearance=\"soft\", color=None, background=None, border=None) -> Drawable",
  params: ((name: "title", type: "str", default: none, desc: [Heading slot.]), (name: "position", type: "str", default: "\"top\"", desc: [top or bottom.]), (name: "width", type: "float | None", default: "None", desc: [None fills the safe width minus margin.])),
  returns: (type: "Drawable", desc: [Safe-edge anchored group.]),
  desc: [Replacement for the removed `caption` helper. Height follows measured title and subtitle content.],
)[
```python
notice = scene.slides.banner("Simulation complete", position="bottom", variant="success")
```
]

#api-entry(
  name: "SlideKit.lower_third",
  kind: "factory",
  signature: "lower_third(title, subtitle=None, *, kicker=None, side=\"left\", width=520, margin=32, padding=(28,20), gap=8, radius=16, variant=\"neutral\", appearance=\"soft\", color=None, background=None, border=None) -> Drawable",
  params: ((name: "title", type: "str", default: none, desc: [Primary label.]), (name: "subtitle", type: "str | None", default: "None", desc: [Secondary label.]), (name: "side", type: "str", default: "\"left\"", desc: [left or right safe corner.])),
  returns: (type: "Drawable", desc: [Safe-corner anchored group.]),
  desc: [Kicker, title, and subtitle use Theme roles and wrap within the authored width.],
)[
```python
speaker = scene.slides.lower_third("Ada Lovelace", "Mathematician", kicker="SPEAKER")
```
]

#api-entry(
  name: "SlideKit.stat_card",
  kind: "factory",
  signature: "stat_card(value, label, *, delta=None, width=280, min_height=170, padding=(24,20), gap=8, radius=18, variant=\"neutral\", appearance=\"soft\", color=None, background=None, border=None) -> Drawable",
  params: ((name: "value", type: "str", default: none, desc: [Formatted primary value.]), (name: "label", type: "str", default: none, desc: [Metric label.]), (name: "delta", type: "str | None", default: "None", desc: [Optional comparison text.])),
  returns: (type: "Drawable", desc: [Auto-height metric panel.]),
  desc: [Value and delta use the semantic tone; no numeric sign or formatting is inferred.],
)[
```python
metric = scene.slides.stat_card("98%", "Accuracy", delta="+4.2%", variant="success")
```
]

#api-entry(
  name: "SlideKit.quote_card",
  kind: "factory",
  signature: "quote_card(quote, attribution=None, *, width=620, padding=(32,28), gap=16, radius=18, variant=\"neutral\", appearance=\"soft\", color=None, background=None, border=None) -> Drawable",
  params: ((name: "quote", type: "str", default: none, desc: [Wrapped quotation.]), (name: "attribution", type: "str | None", default: "None", desc: [Optional right-aligned credit.])),
  returns: (type: "Drawable", desc: [Auto-height quotation panel.]),
  desc: [Adds typographic quotation marks and a semantic attribution treatment.],
)[
```python
quote = scene.slides.quote_card("Clarity matters.", "Gaanim", appearance="outline")
```
]

#api-entry(
  name: "SlideKit.section_header",
  kind: "factory",
  signature: "section_header(title, *, kicker=None, subtitle=None, width=720, align=\"left\", rule=False, padding=(24,18), gap=10, radius=12, variant=\"neutral\", appearance=\"soft\", color=None, background=None, border=None) -> Drawable",
  params: ((name: "title", type: "str", default: none, desc: [Section heading.]), (name: "align", type: "str", default: "\"left\"", desc: [left, center, or right.]), (name: "rule", type: "bool", default: "False", desc: [Opt in to the horizontal semantic accent rule.])),
  returns: (type: "Drawable", desc: [Measured section heading group.]),
  desc: [Kicker, title, and subtitle share alignment and Theme roles.],
)[
```python
heading = scene.slides.section_header("Method", kicker="02", align="center")
```
]

#api-entry(
  name: "SlideKit.title_card",
  kind: "factory",
  signature: "title_card(title, subtitle?, *, width, height, panel=False) -> Drawable",
  params: ((name: "title", type: "str", default: none, desc: [Main title.]), (name: "subtitle", type: "str", default: "None", desc: [Optional subtitle.]), (name: "panel", type: "bool", default: "False", desc: [Framed version.]),),
  returns: (type: "Drawable", desc: [Centered opening with title + rule + optional subtitle.]),
  desc: [Conference opener. Single animatable group.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
opening = scene.slides.title_card("Vector Motion", "A technical explanation", panel=True)
scene.play([opening.animate.fade_in().duration(0.7)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "SlideKit.bullets",
  kind: "factory",
  signature: "bullets(items: list[str], *, width, gap, bullet_radius, bullet_color) -> Drawable",
  params: ((name: "items", type: "list[str]", default: none, desc: [Bullet strings, ≥1 non-empty.]), (name: "gap", type: "float", default: "68.0", desc: [Vertical gap.]),),
  returns: (type: "Drawable", desc: [Bulleted list as one drawable.]),
  desc: [Presentation agenda. Tune `width`, `bullet_radius`, `bullet_color`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
agenda = scene.slides.bullets(["Setup", "Motion", "Export"], gap=48, bullet_color=GOLD).move_to(0, 40)
scene.play([agenda.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "ChartSpec.mark('bar')",
  kind: "factory",
  signature: "ChartSpec(data).mark('bar').encode(x=..., y=...) -> ChartSpec",
  params: ((name: "source", type: "DataSource", default: none, desc: [Replaceable tabular data.]), (name: "x / y", type: "str", default: none, desc: [Numeric column names.]),),
  returns: (type: "ChartSpec", desc: [Immutable batched bar-chart specification.]),
  desc: [Materialize with `scene.viz.chart(spec)`; build a new spec after replacing a `DataSource` to capture its new immutable version.],
)[
```python
from gaanim import Axis, ChartSpec
spec = ChartSpec({"x": [0, 1, 2], "value": [18, 42, 31]}) \
  .mark("bar").encode(x="x", y="value") \
  .axes(x=Axis.category(["Q1", "Q2", "Q3"]), y=Axis.linear(0, 50))
chart = scene.viz.chart(spec)
```
]

#api-entry(
  name: "SlideKit.table",
  kind: "factory",
  signature: "table(headers, rows, *, width, row_height) -> Drawable",
  params: ((name: "headers", type: "list[str]", default: none, desc: [Column headers, ≥1.]), (name: "rows", type: "list[list[str]]", default: none, desc: [One cell per header, non-empty.]),),
  returns: (type: "Drawable", desc: [Table with blue header + rules.]),
  desc: [Compact technical table. All rows must match header count.],
)[
```python
# show-code: true
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
tbl = scene.slides.table(["Method","Error","Time"], [["Baseline","0.18","48 ms"],["GPU","0.04","15 ms"]]).move_to(0, 0)
scene.play([tbl.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
```
]

== Reactividad nativa

Las dependencias reactivas se declaran al construir la escena. Durante la
reproducción, Gaanim resuelve primero un snapshot numérico estable en Rust y
llama después a las funciones Python declaradas.

Elige el tipo según la responsabilidad:

- `Parameter` es una magnitud invisible que conduce otras propiedades.
- `Variable` es una magnitud que también debe aparecer como anotación numérica.
- `Readout` muestra una cantidad derivada sin convertirse en su fuente.
- `PointRef` y `AnchorPoint` expresan posiciones dependientes sin crear marcas
  visibles adicionales.
- Los bindings conectan propiedades existentes; los updaters quedan para
  comportamiento temporal que no puede expresarse como una relación pura.

Usa funciones Python puras, incluido el módulo estándar `math`, helpers y control
de flujo. Declara las dependencias con `inputs=[...]`: las coordenadas llegan
primero y los valores reactivos después, exactamente en el orden declarado.
`scene.viz.time` permite depender explícitamente del tiempo.

`scene.time` expone la misma fuente. `Computed` también puede aparecer dentro
de `inputs`, tanto en otro `computed` como en gráficas, campos y readouts:

```python
from gaanim import computed
import math

radius = scene.viz.parameter(1.0)
area = computed(lambda r: math.pi*r*r, inputs=[radius])
doubled = computed(lambda value: 2*value, inputs=[area])
scene.viz.readout(doubled, label="2A")
```

Las dependencias compartidas conservan su caché para el mismo snapshot de
entradas. La validación de escena sigue todas las dependencias transitivas;
envolver un parámetro ajeno en `Computed` no permite usarlo en otra escena.
`Computed` sigue siendo escalar: no devuelve colores, texto ni geometría.

Los setters absolutos `move_to`, `rotate_to`, `scale_to` y `opacity`, incluidas
sus variantes 3D, aceptan estas fuentes. Un setter reactivo enlaza el canal
desde el cursor; otro lo reemplaza y uno numérico lo termina mediante un corte
reversible. La posición, rotación y escala se tratan como canales completos,
incluso al mezclar constantes y fuentes por eje. Mientras el canal esté
enlazado, anima el parámetro; las escrituras relativas y animaciones directas
conflictivas se rechazan. Consulta la página de animaciones para destinos
reactivos congelados al inicio de un clip.

La ventaja principal aparece al hacer seek: una relación pura se evalúa para el
instante solicitado y no depende de haber reproducido todos los fotogramas
anteriores.

#api-entry(
  name: "Visualization.parameter",
  kind: "factory",
  signature: "parameter(initial: float) -> Parameter",
  params: ((name: "initial", type: "float", default: none, desc: [Valor escalar inicial y finito.]),),
  returns: (type: "Parameter", desc: [Escalar invisible utilizable directamente o como entrada explícita de un callback.]),
  desc: [`current` lee el espejo autoral, `set(value)` registra un corte inmediato en el cursor y `animate.set(value).duration(...)` construye un `Anim` puro. Los valores no finitos producen `ValueError`.],
)[
```python
import math
from gaanim import Axis, Scene

scene = Scene(frame=(16, 9))
amplitude = scene.viz.parameter(1.0)
axes = scene.viz.cartesian_2d(Axis.linear(-4, 4), Axis.linear(-2, 2))
curve = axes.plot(lambda x, a: a * math.sin(x), inputs=[amplitude])
scene.play([axes.animate.create(), curve.animate.write(), amplitude.animate.set(2.0).duration(1.2)])
```
]

#api-entry(
  name: "Visualization.variable",
  kind: "factory",
  signature: "variable(initial, *, label, format='.2f', prefix='', suffix='', unit=None, font_size=None, color=None, invalid='invalid') -> Variable",
  params: ((name: "label", type: "str", default: none, desc: [Etiqueta visible colocada antes del signo igual.]), (name: "format", type: "str", default: "'.2f'", desc: [Formato numérico: ancho, signo, agrupación, precisión y `f`, `e`, `g` o `%`.]), (name: "unit", type: "str | None", default: none, desc: [Unidad visible opcional.]),),
  returns: (type: "Variable", desc: [Objeto dibujable y escalar reactivo al mismo tiempo.]),
  desc: [Variables accept the same scalar operations and animation methods as `Parameter`. Their `label`, `equals`, `number`, and `unit` properties expose stylable `Drawable` parts. All terms use `font_size`, defaulting together to the 48-unit reactive annotation size. The parts keep equal equation-style spacing; the label, number, and unit share a visual baseline while the equality sign stays centered on the numeric axis. `color` paints every visible term, including the value after updates and seeks. The returned group retains normal create, write, fade, layout, and style operations.],
)[
```python
from gaanim import RED, Scene

scene = Scene(frame=(16, 9))
k = scene.viz.variable(10, label="$k$", format=".0f", color=RED)
scene.play([k.animate.create(), k.animate.set(100).duration(1.5)])
```
]

#api-entry(
  name: "Visualization.readout",
  kind: "factory",
  signature: "readout(source, *, inputs=(), label=None, format='.2f', prefix='', suffix='', unit=None, font_size=None, color=None, invalid='invalid') -> Readout",
  params: ((name: "source", type: "number | Parameter | Variable | Computed | callable", default: none, desc: [Escalar o función Python pura cuyos argumentos corresponden a `inputs`.]), (name: "inputs", type: "Sequence[Parameter | Variable | Computed | TimeInput]", default: "()", desc: [Dependencias explícitas en orden.]), (name: "invalid", type: "str", default: "'invalid'", desc: [Texto usado cuando la evaluación es inválida o no finita.]),),
  returns: (type: "Readout", desc: [Grupo dibujable reactivo.]),
  desc: [The numeric path is regenerated only if the formatted text changes, avoiding work for sub-precision animation steps. `label`, `equals`, `number`, and `unit` are available as drawable parts; every part uses `font_size`, defaulting together to 48 scene units. They keep equal equation-style spacing and a shared visual baseline for textual terms. `color` paints the complete row and remains applied to regenerated numeric glyphs and timeline seeks.],
)[
```python
import math
from gaanim import Scene

scene = Scene(frame=(16, 9))
radius = scene.viz.parameter(1.0)
area = scene.viz.readout(lambda r: math.pi * r**2, inputs=[radius], label="$A$", format=".2f", unit="m²")
scene.play([area.animate.create(), radius.animate.set(3.0).duration(1.5)])
```
]

== Geometría reactiva

La geometría reactiva conecta extremos, magnitudes y referencias espaciales en
el mismo fotograma. En lugar de recalcular una línea desde Python, construyes una
relación: «este extremo es la esquina del rectángulo» o «este punto está al 35 %
de la curva». El motor regenera solo la geometría afectada después de aplicar
las transformaciones de sus fuentes.

Un objeto reactivo suele empezar oculto igual que cualquier objeto recién
creado. Anima su entrada (`fade_in`, `create` o `write`) junto con el parámetro o
el objeto que lo conduce.

#api-entry(
  name: "Drawable.anchor_point",
  kind: "method",
  signature: "anchor_point(anchor=Anchor.CENTER, *, offset=(0, 0)) -> AnchorPoint",
  params: ((name: "anchor", type: "Anchor", default: "Anchor.CENTER", desc: [One of the nine local-bounds anchors.]), (name: "offset", type: "vec2", default: "(0, 0)", desc: [Additional local-space displacement.]),),
  returns: (type: "AnchorPoint", desc: [Non-rendered endpoint that follows the full transformed hierarchy.]),
  desc: [Use anchored points with tracking lines, bars, springs, and dimensions. The local offset rotates and scales with the drawable; non-finite values raise `ValueError`.],
)[
```python
from gaanim import Anchor, Scene
scene = Scene(frame=(16, 9))
frame = scene.geometry.rect(180, 90)
corner = frame.anchor_point(Anchor.TOP_RIGHT, offset=(8, 0))
```
]

#api-entry(
  name: "Visualization.parameter",
  kind: "factory",
  signature: "parameter(initial: float) -> Parameter",
  params: ((name: "initial", type: "float", default: none, desc: [Starting value.]),),
  returns: (type: "Parameter", desc: [Scalar animated independently.]),
  desc: [Drive `always_redraw_arc`, `point_on_curve`, etc. Use `tracker.animate.set(v).duration(t)`. Reactive visuals need their own entry animation in `scene.play(...)`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
theta = scene.viz.parameter(0.2)
arc = scene.geometry.always_redraw_arc(theta, 0, 0, 55, 0.0).fill(WHITE)
scene.play([arc.animate.fade_in().duration(0.3), theta.animate.set(4.5).duration(1.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.point_on_curve",
  kind: "factory",
  signature: "point_on_curve(curve: Drawable, tracker: Parameter) -> Drawable",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Sampled polyline/bezier.]), (name: "tracker", type: "Parameter", default: none, desc: [0..1 clamped, arc-length.] ),),
  returns: (type: "Drawable", desc: [Dot following curve.]),
  desc: [Position by arc length, no Python callback during playback.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
from math import cos, sin, pi
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.0)
curve = scene.geometry.polyline([(110*cos(u), 60*sin(2*u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 2)
dot = scene.geometry.point_on_curve(curve, t).fill(GOLD)
scene.play([dot.animate.fade_in().duration(0.3), t.animate.set(1.0).duration(1.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.tangent_on_curve / normal_on_curve",
  kind: "factory",
  signature: "tangent_on_curve(curve, tracker, length=80) / normal_on_curve(...) -> Drawable",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Curve.]), (name: "tracker", type: "Parameter", default: none, desc: [0..1.] ), (name: "length", type: "float", default: "80", desc: [Line length.]),),
  returns: (type: "Drawable", desc: [Line centered on curve point, rotated to tangent/normal.]),
  desc: [Normal is 90° CCW from tangent. Same arc-length sampling.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, BLACK, Scene
from math import cos, sin, pi
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.35)
curve = scene.geometry.polyline([(110*cos(u), 60*sin(u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 2)
tangent = scene.geometry.tangent_on_curve(curve, t, length=70).stroke(GOLD, 3)
scene.play([tangent.animate.fade_in().duration(0.3), t.animate.set(0.9).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.curvature_on_curve",
  kind: "factory",
  signature: "curvature_on_curve(curve, tracker, window=0.02) -> Drawable",
  params: ((name: "curve", type: "Drawable", default: none, desc: [Curve.]), (name: "tracker", type: "Parameter", default: none, desc: [0..1.]),),
  returns: (type: "Drawable", desc: [Osculating circle.]),
  desc: [Estimated from neighboring arc-length samples. Style with `no_fill().stroke()`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
from math import cos, sin, pi
scene = Scene(frame=(16, 9), background="#0f172a")
t = scene.viz.parameter(0.25)
curve = scene.geometry.polyline([(110*cos(u), 60*sin(u)) for u in (2*pi*i/240 for i in range(241))]).no_fill().stroke(WHITE, 2)
circle = scene.geometry.curvature_on_curve(curve, t).no_fill().stroke(RED, 2)
scene.play([circle.animate.fade_in().duration(0.3), t.animate.set(0.7).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Geometry.always_redraw_arc",
  kind: "factory",
  signature: "always_redraw_arc(tracker, cx, cy, radius, start_angle) -> Drawable",
  params: ((name: "tracker", type: "Parameter", default: none, desc: [Drives sweep angle.]),),
  returns: (type: "Drawable", desc: [Regenerated arc each frame.]),
  desc: [For `Parameter`-driven rotations without Python callback.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
theta = scene.viz.parameter(0.3)
rot = scene.geometry.always_redraw_arc(theta, 0, 0, 55, 0.0).fill(WHITE)
scene.play([rot.animate.fade_in().duration(0.3), theta.animate.set(5.0).duration(1.6)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.bar_between",
  kind: "factory",
  signature: "bar_between(from, to, *, width=8) -> Drawable",
  params: ((name: "from", type: "Endpoint", default: none, desc: [First fixed, drawable-origin, or anchored endpoint.]), (name: "to", type: "Endpoint", default: none, desc: [Second endpoint.]), (name: "width", type: "float", default: "8", desc: [Positive scene-unit thickness.]),),
  returns: (type: "Drawable", desc: [Round-capped bar with reactive length and angle.]),
  desc: [The bar is regenerated in the same frame as endpoint animation or updaters. Draw articulation circles separately when required.],
)[
```python
# show-code: true
from gaanim import Anchor, BLACK, Scene
scene = Scene(frame=(16, 9))
body = scene.geometry.rect(150, 70).move_to(40, -20)
bar = scene.mechanics.bar_between((-150, 100), body.anchor_point(Anchor.TOP_LEFT), width=9).stroke(BLACK, 9)
scene.play([bar.animate.fade_in(), body.animate.shift_by(80, 0).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.spring_between",
  kind: "factory",
  signature: "spring_between(from, to, coils=8, amplitude=12, crossing=0, start_straight=12, end_straight=12) -> Drawable",
  params: ((name: "from", type: "Endpoint", default: none, desc: [Endpoint A.]), (name: "to", type: "Endpoint", default: none, desc: [Endpoint B.]), (name: "coils", type: "int", default: "8", desc: [Number of turns.]), (name: "amplitude", type: "float", default: "12", desc: [Radius perpendicular to the endpoint axis, in scene units.]), (name: "crossing", type: "float", default: "0", desc: [Normalized e-like interlacing amount from 0 to 1.]), (name: "start_straight", type: "float", default: "12", desc: [Non-negative straight length before the first coil.]), (name: "end_straight", type: "float", default: "12", desc: [Non-negative straight length after the final coil.]),),
  returns: (type: "Drawable", desc: [Reactive helical spring path.]),
  desc: [Endpoints can be fixed tuples, drawable origins, or AnchorPoint references inside transformed groups. By default, it has 12 scene-unit straight segments at both ends, as in a mechanical spring. The helix radius stays stable while its pitch deforms automatically as an endpoint moves. Set either straight length to `0` to coil directly from that endpoint; close endpoints shorten both segments proportionally. Negative or non-finite straight lengths raise `ValueError`. Set `crossing` above 0 to fold parts of each turn back and create e-like crossings.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, WHITE, RED, GREEN, Scene
scene = Scene(frame=(16, 9), background="#0f172a")
mass = scene.geometry.dot(10).fill(GOLD).move_to(70, 0)
spring = scene.mechanics.spring_between(( -70, 0), mass, coils=6, amplitude=14, crossing=1.0, start_straight=18, end_straight=18).no_fill().stroke(WHITE, 3)
scene.play([spring.animate.fade_in().duration(0.3), mass.animate.shift_by(40, 0).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.dimension_between",
  kind: "factory",
  signature: "dimension_between(from, to, offset, *, label=None, show_value=False, value=None, format=\".2f\", unit=None, scale=1, label_gap=10, label_orientation=\"upright\", font_size=None, color=None, line_width=3, extension_style=\"solid\", dash_length=12, gap_length=8) -> Dimension",
  params: ((name: "from", type: "Endpoint", default: none, desc: [Endpoint A.]), (name: "to", type: "Endpoint", default: none, desc: [Endpoint B.]), (name: "offset", type: "float", default: none, desc: [Signed perpendicular displacement.]), (name: "label", type: "str|None", default: "None", desc: [Optional symbolic text or inline math.]), (name: "show_value", type: "bool", default: "False", desc: [Show current XY distance.]), (name: "value", type: "float|Parameter|Variable|Computed|None", default: "None", desc: [Semantic numeric readout. Implies `show_value` and overrides measured distance and `scale`.]), (name: "format", type: "str", default: "\".2f\"", desc: [Reactive number format.]), (name: "unit", type: "str|None", default: "None", desc: [Optional unit text.]), (name: "scale", type: "float", default: "1", desc: [Positive multiplier from scene units to displayed units when `value` is omitted.]), (name: "label_gap", type: "float", default: "10", desc: [Non-negative outward annotation gap.]), (name: "label_orientation", type: "str", default: "\"upright\"", desc: [`upright` or readable `aligned`.]), (name: "line_width", type: "float", default: "3", desc: [Positive filled-line width.]), (name: "extension_style", type: "str", default: "\"solid\"", desc: [`solid` or `dashed`.]), (name: "dash_length", type: "float", default: "12", desc: [Positive dash length.]), (name: "gap_length", type: "float", default: "8", desc: [Positive dash gap.])),
  returns: (type: "Dimension", desc: [Reactive drawable exposing compatible `line`, independent `extensions`, `label`, `number`, and `unit`.]),
  desc: [Keeps all geometry and annotations synchronized with moving endpoints. `value` accepts a number, Parameter, Variable, or Computed; it controls only the number, so changing it never changes the line length. Without `value`, `show_value=True` displays endpoint distance multiplied by `scale`. Labels, values, and units default to 48 scene units for 1080p readability. `color` initializes the complete silhouette and annotation, including the changing number after updates and seeks; `extensions` remains independently styleable. Invalid scalar types, metrics, extension styles, and orientation raise `TypeError` or `ValueError`; non-finite reactive results display the configured invalid-value marker.],
)[
```python
# show-code: true
from gaanim import Anchor, BLACK, WHITE, Scene
scene = Scene(frame=(16, 9), background=WHITE)
frame = scene.geometry.rect(180, 80).move_to(0, 0)
physical_width = scene.viz.parameter(2.5)
dim = scene.mechanics.dimension_between(
  frame.anchor_point(Anchor.TOP_LEFT),
  frame.anchor_point(Anchor.TOP_RIGHT),
  35, label="$W_f$", value=physical_width, unit="m", color=BLACK,
  extension_style="dashed", line_width=3, dash_length=12, gap_length=8,
)
scene.play([dim.animate.fade_in().duration(0.3), physical_width.animate.set(4.0).duration(0.9)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "PointRef and Drawable.follow",
  kind: "reactive geometry",
  signature: "point_ref(x,y) · point_between(from,to,alpha=.5,offset=(0,0)) · polar_point(origin,radius,angle) · drawable.follow(endpoint,offset=(0,0),offset_space=\"world\")",
  returns: (type: "PointRef | Drawable", desc: [Non-rendered points and a fluent same-frame follower.]),
  desc: [`PointRef` is accepted anywhere an `Endpoint` is accepted. Scalars can be `float`, `Parameter`, `Variable`, or `Computed`. `offset_space="local"` rotates and scales offsets with drawable and anchor sources; invalid values raise `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLACK, GOLD, Scene
scene = Scene(frame=(16, 9))
theta = scene.viz.parameter(0.2)
tip = scene.geometry.polar_point((0, 0), 85, theta)
bar = scene.mechanics.bar_between((0, 0), tip).stroke(BLACK, 7)
label = scene.text("tip").fill(GOLD).follow(tip, offset=(0, 18))
scene.play([bar.animate.fade_in(), label.animate.write(), theta.animate.set(2.2).duration(1.4)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanics.angle_between",
  kind: "factory",
  signature: "angle_between(vertex, from, to, *, radius=64, label=None, show_value=False, format=\".1f\", unit=\"deg\", sweep=\"minor\", arrowheads=\"both\", label_gap=12, label_orientation=\"upright\", show_extensions=True, font_size=None, color=None) -> AngleDimension",
  returns: (type: "AngleDimension", desc: [Reactive `arc`, `arrows`, `extensions`, `label`, `number`, and `unit`.]),
  desc: [`from` and `to` accept fixed `Direction` values or endpoints. Sweep is `minor`, `major`, `cw`, or `ccw`; arrowheads are solid triangles. The label, value, and unit share a 48-unit default. `color` paints the arc, arrows, label, reactive value, and unit, including after updates and seeks. Zero-length rays hide the affected geometry rather than emitting invalid paths.],
)[
```python
# show-code: true
from gaanim import Direction, GOLD, Scene
scene = Scene(frame=(16, 9))
bob = scene.geometry.dot(10).move_to(80, -90)
theta = scene.mechanics.angle_between((0,0), Direction.DOWN, bob, label="$theta$", show_value=True, color=GOLD)
scene.play([theta.animate.fade_in(), bob.animate.shift_by(90, 35).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanism annotations",
  kind: "factories",
  signature: "vector_between(...) / moment_about(...) / coordinate_frame_at(...) / contact_on_curve(...) ",
  returns: (type: "Drawable", desc: [Composable reactive technical annotations.]),
  desc: [`vector_between` provides a solid head and optional formatted magnitude; `moment_about` follows a center; `coordinate_frame_at` builds orthogonal labeled axes; `contact_on_curve` groups the existing point, tangent, and normal helpers. Technical annotation text uses a uniform 48-unit default unless `font_size` is supplied.],
)[
```python
# show-code: true
from gaanim import Direction, GOLD, Scene
scene = Scene(frame=(16, 9))
force = scene.mechanics.vector_between((-100, 0), (70, 45), label="$F$", color=GOLD)
moment = scene.mechanics.moment_about((120, -40), radius=42, label="$M$")
frame = scene.mechanics.coordinate_frame_at((0, -80), Direction.RIGHT, labels=("$e_1$", "$e_2$"))
scene.play([force.animate.fade_in(), moment.animate.fade_in(), frame.animate.fade_in()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Reactive force vectors",
  kind: "factories",
  signature: "offset_point(origin, dx, dy) / force_at(origin, magnitude, direction=0, visual_scale=1, ...) / force_from_components(origin, fx, fy, visual_scale=1, ...) -> ForceVector",
  returns: (type: "PointRef | ForceVector", desc: [Reactive relative geometry or a drawable exposing `shaft`, `head`, `label`, `number`, and `unit`.]),
  desc: [`force_at` accepts physical magnitude and a radian direction; `force_from_components` accepts physical X/Y components. `visual_scale` converts physical units to scene units while the optional readout remains in physical units. Its label, value, and unit share a 48-unit default. `color` paints the force, label, changing numeric value, and unit. All scalar inputs accept floats, Parameters, Variables, and Computed values. `Parameter.add_updater_fn(callback)` drives a scalar directly as `callback(current, dt, elapsed) -> value`; pair `reset` with `fixed_dt` for deterministic stateful simulations. Fixed-step drawable simulations are rebuilt before ordinary parameter callbacks, so a force magnitude or direction derived from the simulated body observes the same-frame state during playback, seeks, and export. Non-positive scales, invalid label metrics, non-finite callback results, or incomplete deterministic-updater pairs raise `ValueError`.],
)[
```python
# show-code: true
from gaanim import GREEN, Scene
scene = Scene(frame=(16, 9))
body = scene.geometry.circle(24)
magnitude = scene.viz.parameter(30)
force = scene.mechanics.force_at(
  body.anchor_point(), magnitude, direction=0.5, visual_scale=2,
  label="$F$", show_value=True, unit="N", color=GREEN,
)
scene.play([body.animate.fade_in(), force.animate.fade_in(), magnitude.animate.set(80).duration(1.5)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Mechanical supports and joints",
  kind: "factories",
  signature: "support_at(point,kind=\"pin\",direction=UP,size=48,ground_length=70,color=None) -> Support",
  returns: (type: "Support", desc: [Theme-aware vector symbol exposing `joint`, `body`, `ground`, `rollers`, `guides`, and `hatching`.]),
  desc: [Kinds are `fixed`, `pin`, `roller`, `simple`, `guided`, `prismatic`, `cable`, and `spring`. Direction runs from base toward connection, so `UP` places ground below and `DOWN` creates ceiling supports. Convenience methods are `fixed_support`, `pin_support`, `roller_support`, and `guided_support`; `joint_at` creates standalone revolute/prismatic joints.],
)[
```python
# show-code: true
from gaanim import Direction, Scene
scene = Scene(frame=(16, 9))
pin = scene.mechanics.pin_support((-100, 0), direction=Direction.UP)
roller = scene.mechanics.roller_support((100, 0), direction=Direction.UP)
scene.play([pin.animate.fade_in(), roller.animate.fade_in()])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Transmission primitives",
  kind: "factories and bindings",
  signature: "gear(radius,teeth,bore_radius=8) / rack(length,teeth) / cam_profile(samples,bore_radius=8)",
  returns: (type: "Drawable", desc: [Editorial, styleable mechanism geometry.]),
  desc: [Use `bind_rotation_from(source,ratio,phase)` for gear coupling and `bind_translation_from_rotation(source,axis,scale)` for rack motion. These are visual relationships, not a kinematic/contact solver. Gear teeth are schematic rather than manufacturing involutes.],
)[
```python
# show-code: true
from gaanim import Direction, Scene
scene = Scene(frame=(16, 9))
driver = scene.mechanics.gear(55, 20).move_to(-55, 20)
driven = scene.mechanics.gear(33, 12).move_to(33, 20).bind_rotation_from(driver, ratio=-5/3)
rack = scene.mechanics.rack(220, 18).move_to(0, -65).bind_translation_from_rotation(
  driver, axis=Direction.RIGHT, scale=55,
)
scene.play([driver.animate.fade_in(), driven.animate.fade_in(), rack.animate.fade_in(), driver.animate.rotate_by(2.0)])
# output: preview.webp
scene.render()
```
]

== Estilo y layout de Drawable

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
scene = Scene(frame=(16, 9), background="#0f172a")
obj = scene.geometry.circle(45).fill(BLUE).stroke(GOLD, 3).move_to(0, 0)
obj.glow(GOLD, radius=18)
scene.play([obj.animate.grow_from_center().duration(0.8)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Drawable.at / scaled / rotated / z_index",
  kind: "method",
  signature: ".move_to(x, y, anchor=None) | .move_to(reference) .scale_to(factor) .rotate_to(radians) .z_index(int) .with_pivot(x,y)",
  params: ((name: "x / y", type: "float", default: none, desc: [Target scene-space position.]), (name: "reference", type: "Drawable", default: none, desc: [Alternative single argument for deferred center-to-center placement.]), (name: "anchor", type: "Anchor", default: "Anchor.CENTER", desc: [Local point placed at `(x, y)`; coordinates only.]),),
  returns: (type: "Drawable", desc: [Self.]),
  desc: [Transforms in scene space. Generic drawables use center-based `.move_to(x, y)` positioning, while coordinate-system roots preserve their authored mathematical origin so labels do not displace an axis; pass an `Anchor` to select an explicit geometric edge, corner, or center instead. `.move_to(reference)` creates a deferred center-to-center layout relation equivalent to `align_to(reference, Anchor.CENTER)`; it does not follow later animations, so use `follow` or `attach_to` for reactive placement. A reference cannot be combined with `y` or `anchor`. The specialized `Text` subtype additionally accepts `TextAnchor` for coordinates and defaults single-line coordinate placement to its baseline center; see the Text API. `with_pivot`/`pivot` sets rotation/scale origin.],
)[
```python
# show-code: true
from gaanim import Anchor, BLUE, GOLD, WHITE, RED, GREEN, Scene
from math import pi
scene = Scene(frame=(16, 9), background="#0f172a")
hinge = scene.geometry.dot(7).fill(GOLD).move_to(-200, 100)
arm = scene.geometry.rect(90, 18).fill(BLUE).move_to(hinge).with_pivot(-200, 100)
scene.play([arm.animate.rotate_by(pi/2.5).duration(1.0)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "Free drawable positioning",
  kind: "method",
  signature: ".next_to(ref, dir) .align_to(ref, anchor) .to_edge(dir, buff)",
  params: ((name: "reference", type: "Drawable", default: none, desc: [Anchor target.]),),
  returns: (type: "Drawable", desc: [Self positioned relatively.]),
  desc: [`Direction`/`Anchor` helpers are only for free drawables. Layout children use `offset` and constraints.],
)[
```python
# show-code: true
from gaanim import BLUE, WHITE, Scene
from gaanim import Anchor, Direction
scene = Scene(frame=(16, 9), background="#0f172a")
a = scene.geometry.circle(18).fill(BLUE)
b = scene.geometry.circle(18).fill(WHITE)
c = scene.geometry.circle(18).fill(BLUE)
row = scene.layout.row([a, b, c], gap=18, within="safe")
scene.play([row.animate.fade_in().duration(0.6)])
# output: preview.webp
scene.render()
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
scene = Scene(frame=(16, 9), background="#0f172a")
mass = scene.geometry.dot(12).fill(GOLD).move_to(-60, 0)
label = scene.text("follower").move_to(0, 45)
label.attach_to(mass)
scene.play([label.animate.fade_in().duration(0.3), mass.animate.shift_by(120, 0).duration(1.2)])
# output: preview.webp
scene.render()
```
]

#api-entry(
  name: "TextSelection and TextQuery",
  kind: "method",
  signature: "text[name] / text[index_or_slice] / text.graphemes|words|lines|parts[index] -> TextSelection; name in text.parts -> bool",
  params: ((name: "name", type: "str", default: none, desc: [Nested semantic path from `part()`.]), (name: "index", type: "int | slice", default: none, desc: [Rendered unit selection, Unicode-grapheme safe.])),
  returns: (type: "TextSelection", desc: [Deferred local selection; never a Layout leaf.]),
  desc: [Selections support persistent `fill`, compound `animate.fill(...).opacity(...)`, typed emphasis effects, and structural morph/copy transitions. Use `"mass" in text.parts` before optional styling; membership recognizes both leaf names and dotted nested paths such as `"formula.mass"`. The proxy rejects non-local transform and stroke channels.],
)[
```python
# show-code: true
from gaanim import GOLD, RED, Scene, part
scene = Scene(frame=(16, 9), background="#0f172a")
# TextSelection animations compose with complete-Text animations.
eq = scene.text("$E = ", part("mass", "m"), " c^2$").move_to(0, 0)
if "mass" in eq.parts:
  eq["mass"].fill(GOLD)
scene.play([
    eq.animate.write().duration(0.8),
    eq["mass"].animate.fill(RED).opacity(0.7).duration(0.8),
])
# output: preview.webp
scene.render()
```
]
