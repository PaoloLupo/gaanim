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
```

Available factories are `circle`, `rect`, `rounded_rect`, `square`, `dot`,
`ellipse`, `line`, `arrow`, `text`, `title`, `subtitle`, `equation`, and
`group`. `image(path)` loads PNG, JPEG, and WebP files at their native pixel
dimensions; use the regular `Drawable` methods such as `scaled`, `rotated`,
`opacity`, and `at` to compose them. Reusing the same path shares its decoded
texture for the process.

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
