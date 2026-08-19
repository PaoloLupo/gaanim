#import "../../components/section.typ": docs-chapter
#import "../../components/api.typ": api-entry

#show: docs-chapter.with(
  title: "API de Scene",
  description: "API pública canónica para construir animaciones con Gaanim",
  route: "/api/scene/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Scene

`Scene` es el punto de entrada público de una animación. Es propietaria de los
objetos, la línea de tiempo, el renderizado, la exportación y los segmentos con
nombre. Crea una `Scene` para cada animación.

== Constructor y viewport

#api-entry(
  name: "Scene",
  kind: "constructor",
  signature: "Scene(width: int = 1280, height: int = 720, background: BackgroundLike | None = None, margin: float | None = None, theme: str | Theme | None = None)",
  params: ((name: "width", type: "int", default: "1280", desc: [Ancho del viewport en píxeles.]), (name: "height", type: "int", default: "720", desc: [Alto del viewport en píxeles.]), (name: "background", type: "BackgroundLike | None", default: "None", desc: [`ColorLike`, `Brush` o `Background`; tiene prioridad sobre el fondo del tema.]), (name: "margin", type: "float | None", default: "None", desc: [Margen uniforme del marco seguro.]), (name: "theme", type: "str | Theme | None", default: "None", desc: [Nombre incluido o tema centralizado reutilizable.]),),
  returns: (type: "Scene", desc: [Una escena nueva para autoría.]),
  desc: [Instala el tema antes de crear objetos. Nombres desconocidos o valores inválidos producen `ValueError` o `TypeError`.],
)[

```python
from gaanim import BLACK, Scene

scene = Scene(width=1920, height=1080, background=BLACK, margin=48)

# Viewport configuration remains available from the scene.
scene.canvas.width = 1280
scene.canvas.height = 720
scene.canvas.set_margin(32)
```
]

La forma tardía equivalente es `scene.canvas.set_theme(theme)`. Los objetos
compatibles conservan metadatos semánticos hasta la compilación, así que las
reglas también alcanzan objetos creados antes de esa llamada. Consulta
#link("/api/themes/", "Temas y colores").

`scene.canvas.set_preset(...)` configura un formato de salida estándar y un área
segura que respetan todas las operaciones de layout y colocación en bordes:

```python
scene.canvas.set_preset("vertical")  # 1080×1920, safe around mobile UI
safe = scene.canvas.safe_area()
title = safe.place(scene.text("Vertical video", role="title"), Anchor.TOP)
```

Los presets disponibles son `"widescreen"` (1920×1080 / 16:9), `"vertical"`
(1080×1920 / 9:16) y `"square"` (1080×1080 / 1:1). Usa
`set_safe_area(top=..., right=..., bottom=..., left=...)` cuando una marca o
plataforma requiera márgenes internos personalizados.

== Iluminación 3D y cámara del editor

`scene.lighting_3d(preset="studio", intensity=1.0, shadows=True)` installs one
friendly ambient/key/fill rig for native PBR primitives and glTF content. Use
`preset="none"` for emissive-only or externally lit work. The rig is scene
level, so several models never create duplicate automatic lights.

The editor keeps its inspection camera separate from `scene.camera`. Both 2D
and 3D scenes open with interactive mode disabled. Every activation through
`I` or the *Interactive: ON/OFF* indicator in Overlays (`O`) starts from a fresh
copy of the current timeline-authored camera; the prior inspection position is
never reused. *Camera View* continues to show the authored camera, and neither
mode changes snapshots, Presenter View, or export. Picking retains selection
for framing without drawing a bounding box over the selected object.

- `Num0`: switch between *Free 3D* and *Camera View*.
- Right drag: orbit; middle drag or Shift+left drag: pan; wheel: dolly.
- `F`: frame the selection, or the complete scene when nothing is selected.
- `R`: reset and frame; `I`: toggle inspection mode.

The visible output frame keeps the scene's resolution and aspect ratio fixed.
Picking and ray casting are limited to that frame. Purely 2D scenes remain
orthographic unless inspection is enabled manually.

Timeline and compact seek-bar snapping are disabled while the compiled scene
contains 3D content. Purely 2D scenes retain the regular snapping behavior.

== Creación de objetos

Cada fábrica devuelve un `Drawable` con métodos fluidos de estilo y layout.
El `Text` unificado usa de forma predeterminada la familia científica New
Computer Modern incluida; los fragmentos `$...$` usan su pareja New Computer
Modern Math. Usa `scene.text("$a + b = 2$")` para matemáticas en línea y
`scene.equation("a + b = 2")` para una ecuación independiente compilada como
`$ a + b = 2 $`.

```python
from gaanim import Axis, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720)

circle = scene.circle(80).fill(BLUE).stroke(WHITE, 4).at(-160, 0)
rect = scene.rect(180, 100).fill(GOLD).at(160, 0)
triangle = scene.polygon([(0, 100), (-90, -70), (90, -70)])
star = scene.star(5, 90, 42)
hexagon = scene.regular_polygon(6, 84)
slice = scene.sector(0, 0, 100, 0.0, 1.8)
ring = scene.annulus(100, 56)
underbrace = scene.brace(-120, -100, 120, -100, 36)
approved = scene.checkmark(32).fill(GREEN)
rejected = scene.cross(32).stroke(WHITE, 4)
corner = scene.right_angle(40)
label = scene.text("Gaanim", role="title").at(0, 220)
formula = scene.text("$E = m c^2$").at(0, -180)
arrow = scene.arrow(-80, 0, 80, 0)
angle = scene.arc(0, 0, 64, 0.0, 1.2).no_fill().stroke(WHITE, 3)
rotation = scene.curved_arrow(-90, -80, 90, -80, 0.9).fill(WHITE)
rotation_arc = scene.curved_arrow_arc(0, -80, 90, 0.2, 1.4).fill(WHITE)
guide = scene.dashed_line(-180, -120, 180, -120, dash_length=18, gap_length=10)
measure_arrow = scene.double_arrow(-140, -160, 140, -160)
measure = scene.dimension(-80, 80, 80, 80, 24)
spring = scene.path([(-80, 0), (-50, 24), (-20, -24), (10, 24), (40, -24), (80, 0)]).no_fill().stroke(WHITE, 4)
axes = scene.cartesian_2d(
    Axis.linear(-5, 5).ticks(1).label("x").style(color=WHITE),
    Axis.linear(-3, 3).ticks(1).label("f(x)").style(color=WHITE),
)
logo = scene.image("assets/logo.webp").scaled(0.25).at(360, 180)
icon = scene.svg("assets/icon.svg").scaled(0.5).at(-360, 180)
```

Available factories are `circle`, `rect`, `rounded_rect`, `square`, `dot`,
`ellipse`, `line`, `arrow`, `dashed_line`, `double_arrow`, `polygon`, `star`, `regular_polygon`, `sector`, `annulus`, `brace`, `checkmark`, `cross`, `right_angle`, `arc`, `curved_arrow`, `dimension`, `path`, `axes`, `text`, and
`group`. `image(path, width=..., height=..., fit="contain")` loads PNG, JPEG,
and WebP files. `contain` preserves aspect ratio inside the target, `cover`
fills and clips it, and `stretch` fills it without preserving aspect ratio.
Pass `crop=(x, y, width, height)` in source pixels (top-left origin) to select
a source rectangle. The regular `Drawable` methods such as `scaled`, `rotated`,
`opacity`, and `at` remain available. Reusing the same path shares its decoded
texture for the process.

`svg(path)` imports SVG geometry as a real hierarchy of regular vector paths
and source groups. Named groups and paths are available through `part(id)`:

```python
robot = scene.svg("assets/robot.svg")
arm = robot.part("left-arm")
joint = robot.part("elbow")

arm.fill(BLUE)  # group styles reach every descendant path
scene.play([joint.rotate(0.6)])
```

Part IDs are case-sensitive. Duplicate source IDs fail during import, while an
unknown ID raises `KeyError` and lists the available names. The importer
resolves paths and basic shapes, solid or linear/radial gradient fills and
strokes, CSS, `viewBox`, transforms, `<use>`, outlined text, `clipPath`,
`feGaussianBlur`, and `feDropShadow`. Patterns, masks, embedded raster images,
and arbitrary filter graphs are intentionally omitted.

Coordinate systems, plots, data marks, and calculus constructions use the
typed visualization API. Build immutable `Axis` specifications, create a
`CoordinateSpace`, then call methods on that space:

```python
from gaanim import Axis, BLUE, Expr

space = scene.cartesian_2d(
    Axis.linear(-3, 3).ticks(1).label("x"),
    Axis.linear(-2, 2).ticks(1).label("f(x)"),
    width=900,
    height=480,
)
x = Expr.var("x")
curve = space.function(x.sin()).stroke(BLUE, 3)
marker = scene.dot(6).at_coordinate(space.coord(1, 1))
```

Consulta #link("/api/visualization/", "la API de visualización") para conocer
las escalas, los espacios polares, complejos y 3D, las expresiones nativas, los
datos y estadísticas, y las herramientas educativas.

`bezier(start, controls, end)` creates a native quadratic Bézier with one
control point or a cubic Bézier with two. It remains a real Bézier path, so it
can drive the reactive curve bindings directly.

```python
curve = scene.bezier((-180, 0), [(-80, 180), (80, -180)], (180, 0))
```

`path(definition)` is the compact entry point for custom technical geometry.
Pass a sequence of `(x, y)` points for an open polyline, or cursor commands for
a composed path. The explicit `polyline` and `curve` factories remain available
for code that benefits from stating the exact path kind.

```python
rail = scene.path([(-180, 0), (0, 80), (180, 0)])
profile = scene.path([
    ("move", [(-180, -40)]),
    ("cubic", [(-80, 100), (80, -100), (180, 40)]),
])
```

`arc(cx, cy, radius, start_angle, sweep_angle)` uses radians. `curved_arrow`
connects two points with an angular deflection; `curved_arrow_arc` follows an
explicit center/radius arc. Both use radians, while
`dimension(x1, y1, x2, y2, offset)` draws extension lines and a perpendicular
double-headed measurement arrow.

== Geometría reactiva

`Parameter` anima un escalar independientemente de los objetos visibles. Usa
`always_redraw_arc` to regenerate a curved arrow from that value each frame.
Reactive visual helpers are hidden when declared. Add their entry animation to
`scene.play(...)`—for example `arc.fade_in()`, `trail.fade_in()`, or
`spring.create()`—before or alongside the animation that drives them. The
`Parameter` es una señal no visual y no necesita una animación de entrada.

```python
theta = scene.parameter(0.2)
rotation = scene.always_redraw_arc(theta, 0, 0, 140, 0.0).fill(WHITE)
scene.play([
    rotation.fade_in().duration(0.3),
    theta.animate_to(4.5).duration(2.0),
])
```

For simple spatial relationships, `attach_to` keeps a drawable centered on
another drawable after its updaters run. `bind_x_from`, `bind_y_from`, and
`bind_position_from(source, axes="xy")` provide axis-level control.

```python
label = scene.text("moving label")
label.attach_to(marker)
marker.add_updater(Updater.orbit(0, 0, 120, 1.2))
scene.play([label.fade_in().duration(0.3)])
```

Groups and drawables can rotate or scale around a scene-space point through
`with_pivot(x, y)` (also available as `pivot`). This is useful for a mechanism
with a physical hinge:

```python
mechanism = scene.group([rail, spring, mass]).with_pivot(0, 0)
scene.play([mechanism.rotate(PI / 3).duration(1.0)])
```

`spring_between(from, to, coils=8, amplitude=12, crossing=0)` creates a native
reactive helical spring. Each endpoint can be a drawable or an `(x, y)` tuple,
so it follows a moving mass without a Python callback every frame. Set
`crossing` from `0` to `1` to fold parts of each turn back into e-like visual
crossings.

`callout(text, target, offset=(160, 96), width=240, height=72)` creates a
reusable editorial label: its card, text, and connector follow the target
natively. It returns a regular `Drawable` group, so it can be animated like any
other mobject.

```python
mass = scene.dot(20).fill(GOLD)
note = scene.callout("Moving mass", mass, offset=(180, 100))
scene.play([mass.move(240, 0).duration(1.2), note.fade_in().duration(0.4)])
```

The themed factories `badge`, `chip`, `card`, `banner`, `lower_third`,
`stat_card`, `quote_card`, and `section_header` are documented together under
#link("/api/mobjects/", [Mobjects — Composición editorial]). `banner` replaces
the removed `caption` helper, while `badge` is positioned through the regular
Drawable API (`scene.badge("READY").at(x, y)`).

`title_card(title, subtitle=None)` returns a restrained, centered opening with
title, optional subtitle, and an accent rule. Its elements remain a single
animatable drawable. Pass `panel=True` for a framed version.

```python
opening = scene.title_card("Vector motion", "A short technical explanation")
scene.play([opening.fade_in_from(Direction.DOWN, distance=48).duration(0.6)])
```

`bullets(items)` creates a vertically aligned bullet list as one drawable. The
default gap and colors are suitable for a technical presentation; tune
`width`, `gap`, `bullet_radius`, `bullet_color`, and `color` when needed.

```python
agenda = scene.bullets(["Setup", "Motion", "Export"], gap=72)
scene.play([agenda.fade_in_from(Direction.DOWN, distance=32).duration(0.5)])
```

Charts are immutable tabular specifications materialized into stable semantic
layers. Their marks remain batched independently from row count.

```python
from gaanim import Axis, ChartSpec

spec = ChartSpec({"x": [0, 1, 2], "value": [18, 42, 31]}) \
  .mark("bar").encode(x="x", y="value") \
  .axes(x=Axis.category(["Q1", "Q2", "Q3"]), y=Axis.linear(0, 50))
chart = scene.chart(spec)
scene.play([chart.layer("axes").create(), chart.layer("marks").grow_from_center().duration(0.6)])
```

`table(headers, rows)` creates a compact table with a restrained blue header and
thin construction rules. Each row must have exactly one non-empty cell per header.

```python
results = scene.table(
    ["Method", "Error", "Time"],
    [["Baseline", "0.18", "48 ms"], ["GPU", "0.04", "15 ms"]],
)
scene.play([results.fade_in_from(Direction.DOWN, distance=24).duration(0.5)])
```

`typst(source)` compiles full Typst document markup into a vector drawable.
Use it for publication-style layouts such as table spans or custom mathematical
structures; `text("$...$")` is the concise API for math-only content. The
embedded world resolves `@preview/...` imports through the standard Typst
Universe cache; the first use downloads the requested package.

```python
comparison = scene.typst('''
#table(
  columns: 2,
  [*Method*], [*Error*],
  [Baseline], [0.18],
  [GPU], [0.04],
)
''')
```

`code(source, language=...)` creates a monospaced vector code block with a
quiet technical frame. It is suitable for code reveals and can be animated as
one drawable; token-level highlighting and diffs are planned separately.

```python
snippet = scene.code("result = mass * acceleration", language="python")
scene.play([snippet.fade_in().duration(0.4)])
```

`point_on_curve(curve, tracker)` creates a dot whose position follows the
normalized value of a `Parameter` along a sampled `polyline` or Bézier
path. The value is clamped to
`[0, 1]` and measured by arc length, with no Python callback during playback.

```python
t = scene.parameter(0.0)
curve = scene.polyline([
  (180 * cos(u), 100 * sin(2 * u))
  for u in (2 * PI * index / 240 for index in range(241))
])
dot = scene.point_on_curve(curve, t).fill(GOLD)
scene.play([dot.fade_in().duration(0.3), t.animate_to(1.0).duration(2.0)])
```

`tangent_on_curve(curve, tracker, length=80)` returns a line centered on that
same position and rotated to the current polyline segment. It uses the same
native arc-length sampling as `point_on_curve`.

`normal_on_curve(curve, tracker, length=80)` is the perpendicular companion,
rotated 90 degrees counter-clockwise from the tangent.

`curvature_on_curve(curve, tracker, window=0.02)` returns the local osculating
circle estimated from neighboring arc-length samples. Style it as a regular
circle, usually with `no_fill().stroke(...)`.

Use `label.follow_to(mass, offset=(0, 48))` for annotations that accompany an
object without covering it. `dimension_between(from, to, offset)` similarly
keeps a technical measurement synchronized with moving endpoints. These
generated visuals, including `attach_to`, `bind_*`, curve markers, tracking
lines, springs, dimensions, and traced paths, remain hidden until their own
entry animation is included in `scene.play(...)`.

== Línea de tiempo

`play` receives a list of animations; calls are sequential and animations in a
single list run in parallel.

```python
scene.play([
    circle.create().duration(1.0).smooth(),
    rect.grow_from_center().duration(1.0).spring(),
    label.write().duration(0.8),
])
scene.wait(0.5)
scene.play([circle.move(200, 0).duration(1.0)])
scene.play([rect.fade_out().duration(0.5)])
```

`segment` is the single structural unit for visibility, transitions, notes,
and presentation layouts. Segment boundaries remain continuous; use `stop()`
only where interactive playback must wait for input.

#api-entry(
  name: "Scene.segment",
  kind: "method",
  signature: "segment(name, transition=None, *, notes=None, template=None) -> Segment",
  params: ((name: "name", type: "str", default: none, desc: [Non-empty name, unique within the Scene without regard to ASCII case.]), (name: "transition", type: "Transition | None", default: "None", desc: [Incoming transition from the preceding segment.]), (name: "notes", type: "str | None", default: "None", desc: [Speaker notes shown by Presenter View.]), (name: "template", type: "LayoutTemplate | None", default: "None", desc: [Typed Python template instantiated later with `Segment.bind`.]),),
  returns: (type: "Segment", desc: [Stable handle accepted by `link`; `bind(**slots)` returns its root Layout.]),
  desc: [Creates and activates a structural segment. The first call replaces the untouched implicit segment. An incoming transition on that first segment, or an empty or duplicate name, raises `ValueError`.],
)[
```python
intro = scene.segment("Introduction", notes="State the goal.", template=title_slide)
intro.bind(title=scene.text("One clear idea", role="title"))
scene.wait(0.5)
details = scene.segment("Details", Transition.cross_fade(0.4), template=lecture)
scene.link(intro, details, Transition.cross_fade(0.4))
```
]

#api-entry(
  name: "Scene.stop",
  kind: "method",
  signature: "stop(name=None) -> None",
  params: ((name: "name", type: "str | None", default: "None", desc: [Optional Presenter View label.]),),
  returns: (type: "None", desc: [Adds no duration and creates no visual change.]),
  desc: [Pauses real-time playback when the playhead reaches this exact position. At a segment boundary, the completed outgoing segment remains visible until playback advances, so no trailing `wait()` is required. Export, snapshots, and explicit seeks ignore stops and traverse the timeline continuously. Empty names and a second stop at the same segment-local timestamp raise `ValueError`.],
)[
```python
scene.play([result.write().duration(0.6)])
scene.stop("result-ready")
```
]

#api-entry(
  name: "Scene.reuse / persist / release",
  kind: "method",
  signature: "reuse(object, *others) / persist(object, *others) / release(object, *others) -> None",
  params: ((name: "object", type: "Drawable", default: none, desc: [First drawable to reuse or change lifetime.]), (name: "others", type: "Drawable...", default: "()", desc: [Additional drawables from the same Scene.]),),
  returns: (type: "None", desc: [Queues a scene-membership change at the current timeline cursor.]),
  desc: [`reuse` adopts existing drawables into the active segment. `persist` keeps them available across future segments and outside automatic transitions. `release` returns persistent drawables to the active segment. Visual state is preserved; drawables from another Scene raise `ValueError`.],
)[
```python
# show-code: true
from gaanim import BLUE, GOLD, Scene, Transition

scene = Scene(480, 270, background="#0f172a")
title = scene.text("Shared context", role="title").fill(GOLD).at(0, 70)
scene.play([title.write().duration(0.5)])

scene.segment("content", Transition.cross_fade(0.35))
scene.reuse(title)
dot = scene.dot(18).fill(BLUE).at(0, -30)
scene.play([dot.grow_from_center().duration(0.4)])
scene.persist(title)

scene.segment("closing", Transition.slide(0.35, "left"))
scene.release(title)
scene.wait(0.5)
# output: preview.webp
scene.render()
```
]

== Segmentos de presentación e identidad visual

Configure the deck identity once before declaring presentation segments:

```python
scene.canvas.set_theme("presentation")
scene.brand(
    logo="assets/university.svg",
    footer="UNIVERSITY · MASTER THESIS · 2026",
    slide_numbers=True,
    rule=True,
    show_on_cover=False,
    logo_scale=0.75,
)
```

The logo, theme-colored rule, footer, and current slide number are generated
inside every explicit segment, so navigation and independent segment export
remain correct. Cover layouts omit the chrome by default.

`segment(template=...)` accepts a built-in or project-defined typed Python
template. `bind(**slots)` validates required, optional, and extra slots and
returns the segment's root `Layout`.

```python
segment = scene.segment("Results", template=comparison, notes="Compare both models.")
segment.bind(title=scene.text("Results", role="title"), left=baseline, right=proposed)
scene.stop("comparison-ready")
```

`Transition.zoom_through(duration, center=(0, 0), max_zoom=4)` zooms into a
scene-space point before revealing the next segment. It is useful when a detail
of the outgoing scene introduces the following section.

== Cámara

```python
scene.camera.pan_to(-160, 40, duration=0.8)
scene.camera.zoom_to(1.5, duration=0.6)
scene.camera.frame_to([circle, label], margin=(32, 48), duration=0.9, dynamic=True)
scene.camera.rotate_to(0.15, duration=0.5)
scene.camera.follow(circle.anchor(Anchor.TOP), offset=(0, 24), lag=0.2, duration=2.0)
scene.camera.shake(amplitude=12, frequency=8, duration=0.4)

# Camera animations return Anim and can run beside drawable animations.
scene.play([
    circle.move_to(180, 0).duration(1.2),
    scene.camera.orbit(delta_yaw=0.5, delta_pitch=0.1, duration=1.2),
])
```

`camera.frame_to` accepts one drawable or a sequence, with scalar, two-side, or
CSS-order four-side margins. With `dynamic=True`, it recomputes the union after
updaters and layout in the same frame. `camera.pan_to`, `camera.follow`, and
`camera.look_at` accept a `Drawable`, `AnchorPoint`, `PointRef`, or a 2D/3D
tuple. Zoom and rotation also accept `Parameter`, `Variable`, and `_Expr`.
`camera.follow` supports world/local offsets and deterministic absolute-time
lag. `camera.orthographic(...)` changes projection explicitly, while
`camera.reset()` restores pose, target, up, and default orthographic projection.
`camera.shake` is deterministic so previews, seeks, and exports match. The
camera methods return `Anim`: discarded results retain the existing sequential
behavior, while passing them to `scene.play` regroups them with drawable or
glTF Action animations at the same timeline start. Fluent `Anim` controls such
as `.duration()`, `.delay()`, `.smooth()`, and `.linear()` also apply. The
old flat `scene.camera_*` methods are removed; `scene.camera.*` is the sole
public camera surface.

=== Bindings reactivos persistentes

Bindings are non-rendered ECS constraints with stable creation order. They are
active from creation unless `enabled=False`, remain active across segments,
and record `enable()` / `disable()` at the current timeline cursor. Each
constraint owns only declared channels. Influence is a scalar source in
`0..1`; later constraints compose over earlier ones. Temporary follow/dynamic
framing runs after bindings, and shake is always an additive final modifier.

```python
theta = scene.parameter(0.0)
focus = scene.point_ref(theta * 180, (theta * 2).sin() * 80)
rig2d = scene.camera.bind_2d(center=focus, zoom=1 + theta * 0.3)
scene.play([theta.animate_to(1.0, duration=2.0)])
rig2d.disable()

rig3d = scene.camera.bind_3d(
    eye=(6, 4, 8), target=focus, fov_y=0.8, influence=0.75,
)
rig3d.disable()
```

`bind_2d` requires `center`, `zoom`, or `rotation` and selects orthographic
projection. `bind_3d` requires `eye`, `target`, or `fov_y` and selects
perspective. Camera APIs reject non-finite values, invalid zoom/FOV/clipping,
degenerate look-at poses, and influence outside `0..1`; there are no silent
clamps.

=== Cámara 3D

The 3D camera uses world-space `(x, y, z)` coordinates. Camera operations are
queued on the scene timeline; use `duration=0.0` for an immediate setup and a
positive duration for an animated camera move. Angles are in radians.

#api-entry(
  name: "Camera.perspective",
  kind: "method",
  signature: "perspective(fov_y, near=0.1, far=1000.0, duration=1.0) -> Anim",
  params: ((name: "fov_y", type: "float", default: none, desc: [Vertical field of view in radians.]), (name: "near", type: "float", default: "0.1", desc: [Positive near clipping plane.]), (name: "far", type: "float", default: "1000.0", desc: [Far clipping plane, greater than near.]), (name: "duration", type: "float", default: "1.0", desc: [Animation duration in seconds.]),),
  returns: (type: "Anim", desc: [A composable perspective projection animation.]),
  desc: [Switches the scene to perspective projection. Requires `0 < near < far` and `0 < fov_y < pi`.],
)[
```python
scene.camera.perspective(fov_y=0.785, near=0.1, far=1000.0, duration=0.0)
```
]

#api-entry(
  name: "Camera.look_at",
  kind: "method",
  signature: "look_at(eye, target, up=None, duration=1.0) -> Anim",
  params: ((name: "eye", type: "Endpoint", default: none, desc: [Reactive camera position in world space.]), (name: "target", type: "Endpoint", default: none, desc: [Reactive point the camera looks at.]), (name: "up", type: "(float,float,float)", default: "None", desc: [World up direction; defaults to (0,1,0).]), (name: "duration", type: "float", default: "1.0", desc: [Animation duration in seconds.]),),
  returns: (type: "Anim", desc: [A composable camera orientation and position animation.]),
  desc: [Positions the camera at `eye` and aims it at `target`. Endpoints resolve after reactive layout; eye and target must differ and up must be non-zero and non-collinear.],
)[
```python
scene.camera.look_at(eye=(7, 5, 6), target=(0, 0, 0), duration=0.0)
```
]

#api-entry(
  name: "Camera.orbit",
  kind: "method",
  signature: "orbit(delta_yaw, delta_pitch, duration=1.0) -> Anim",
  params: ((name: "delta_yaw", type: "float", default: none, desc: [Horizontal orbit angle in radians.]), (name: "delta_pitch", type: "float", default: none, desc: [Vertical orbit angle in radians.]), (name: "duration", type: "float", default: "1.0", desc: [Animation duration in seconds.]),),
  returns: (type: "Anim", desc: [A composable orbit around the current look-at target.]),
  desc: [Use small yaw and pitch deltas for a smooth turn around the current target.],
)[
```python
marker = scene.dot(6)
scene.play([
    marker.fade_in(1.0),
    scene.camera.orbit(delta_yaw=0.5, delta_pitch=0.1, duration=1.0),
])
```
]

#api-entry(
  name: "Camera.dolly",
  kind: "method",
  signature: "dolly(factor, duration=1.0) -> Anim",
  params: ((name: "factor", type: "float", default: none, desc: [Positive distance multiplier.]), (name: "duration", type: "float", default: "1.0", desc: [Animation duration in seconds.]),),
  returns: (type: "Anim", desc: [A composable camera move toward or away from its target.]),
  desc: [`factor < 1` moves closer; `factor > 1` moves farther. The factor must be finite and positive.],
)[
```python
scene.camera.dolly(factor=0.85, duration=0.6)
```
]

== Recorte y máscaras

Use any vector drawable as clipping geometry for another drawable or a nested
group. The mask keeps its own visibility; make it transparent when it should
only constrain content:

```python
mask = scene.rounded_rect(420, 220, 28).no_fill().no_stroke()
chart_group.clip(mask)
```

Mask and target transforms are resolved in world space, so they can be placed,
scaled, rotated, or nested independently. `rule="evenodd"` supports paths with
holes, and `drawable.no_clip()` removes a previously assigned mask.

== Salida

```python
scene.render()  # Submit the authored timeline to the Gaanim host
```

Preview, export and visual capture are host responsibilities. Export a script
without changing it:

```bash
gaanim export my_animation.py --output output.webp --quality standard
```

Run a script through the Gaanim application:

```bash
gaanim my_animation.py
```

For visual regression, the executable injects the authoritative directory.
The script only declares exact timeline times:

```python
import os

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 1.0])
```
