#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Scene API",
  description: "The canonical public API for Gaanim animations",
  route: "/api/scene/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Scene

`Scene` is the public entry point for an animation. It owns mobjects, the
timeline, rendering, export, and named segments. Create a `Scene` for every
animation; `Canvas` is retained only as a deprecated compatibility constructor.

== Constructor and viewport

```python
from gaanim import BLACK, Scene

scene = Scene(width=1920, height=1080, background=BLACK, margin=48)

# Viewport configuration remains available from the scene.
scene.canvas.width = 1280
scene.canvas.height = 720
scene.canvas.set_margin(32)
```

== Spawning mobjects

Every factory returns a `Drawable` handle with fluent style and layout methods.

```python
from gaanim import BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720)

circle = scene.circle(80).fill(BLUE).stroke(WHITE, 4).at(-160, 0)
rect = scene.rect(180, 100).fill(GOLD).at(160, 0)
label = scene.title("Gaanim").at(0, 220)
formula = scene.equation("E = m c^2").at(0, -180)
arrow = scene.arrow(-80, 0, 80, 0)
logo = scene.image("assets/logo.webp").scaled(0.25).at(360, 180)
icon = scene.svg("assets/icon.svg").scaled(0.5).at(-360, 180)
```

Available factories are `circle`, `rect`, `rounded_rect`, `square`, `dot`,
`ellipse`, `line`, `arrow`, `text`, `title`, `subtitle`, `equation`, and
`group`. `image(path, width=..., height=..., fit="contain")` loads PNG, JPEG,
and WebP files. `contain` preserves aspect ratio inside the target, `cover`
fills and clips it, and `stretch` fills it without preserving aspect ratio.
Pass `crop=(x, y, width, height)` in source pixels (top-left origin) to select
a source rectangle. The regular `Drawable` methods such as `scaled`, `rotated`,
`opacity`, and `at` remain available. Reusing the same path shares its decoded
texture for the process.

`svg(path)` imports SVG geometry as a group of regular vector paths, so the
imported vector paths can be styled individually by their SVG source. It resolves
paths and basic shapes, solid fills/strokes, CSS,
`viewBox`, transforms, and `<use>`. Raster SVG images, text, gradients,
patterns, filters, masks, and individual source-group handles are not yet
preserved.

== Timeline

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

Use `scene.segment(name, transition)` for named sections, `scene.link(...)` to
connect them, and `scene.slide()` to add a presentation breakpoint.

== Camera

```python
scene.camera_pan_to(-160, 40, duration=0.8)
scene.camera_zoom_to(1.5, duration=0.6)
scene.camera_frame_to(circle, margin=48, duration=0.9)
scene.camera_rotate_to(0.15, duration=0.5)
scene.camera_follow(circle, duration=2.0)
scene.camera_shake(amplitude=12, frequency=8, duration=0.4)
```

`camera_frame_to` derives a pan and orthographic zoom from the target's current
bounds, keeping it inside the viewport with the requested margin.
`camera_follow` follows a mobject while reactive updaters are active, and
`camera_shake` is deterministic so previews, seeks, and exports match.

== Output

```python
scene.render()                         # Interactive Gaanim viewer
scene.export("output.webp", fps=30)   # Format follows the file extension
scene.snapshots("snapshots", [0.0, 1.0])
```

Run a script through the Gaanim application:

```bash
gaanim my_animation.py
```
