#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Scene API",
  description: "The Scene class — core of gaanim",
  route: "/api/scene/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Scene

The `Scene` class is the main entry point for creating animations. It manages the timeline, spawns mobjects, and handles rendering/export.

== Constructor

```python
Scene(
    width: int = 1920,
    height: int = 1080,
    title: str = "Gaanim",
    theme: Theme = Theme.DARK,
)
```

Creates a new scene with the given dimensions and theme.

== Spawning Mobjects

All mobject-spawning methods return a `Mobject` handle that supports fluent chaining:

```python
scene = Scene(1280, 720)

# Shapes
circle = scene.circle(80)
rect = scene.rectangle(200, 120)
square = scene.square(100)
dot = scene.dot(12)
ellipse = scene.ellipse(100, 60)

# Lines
line = scene.line(-200, 0, 200, 0)
arrow = scene.arrow(0, 0, 100, 100)
dashed = scene.dashed_line(-100, 0, 100, 0)

# Polygons
triangle = scene.regular_polygon(3, 80)
star = scene.star(5, 80, 40)
polygon = scene.polygon([(0,0), (100,0), (50,86)])

# Text
title = scene.title("My Title")
body = scene.body("Some body text")
text = scene.text("Custom text")

# Math
eq = scene.equation("E = m c^2")
```

See #link("/api/mobjects/", "Mobjects") for the full list.

== Timeline Control

```python
# Play animations (parallel if multiple args)
scene.play(
    circle.animate().write(duration=2.0),
    rect.animate().fade_in_anim().duration(1.0),
)

# Wait
scene.wait(1.0)

# Sequential: each play() call is sequential
scene.play(circle.animate().shift(100, 0).duration(1.0))
scene.play(circle.animate().fade_out_anim().duration(0.5))
```

== Terminal Methods

```python
scene.render()   # Open Vulkan GPU preview (blocking)
scene.edit()     # Open interactive editor (blocking)
scene.export(    # Headless offline render (blocking)
    "output.mp4",
    fps=60,
    width=None,
    height=None,
    transparent=None,
    aspect_ratio=None,  # "youtube", "tiktok", "instagram"
    quality=None,       # "draft", "standard", "production"
)
```

== Theme

```python
scene = Scene(theme=Theme.DRACULA)
scene.set_theme(Theme.GRUVBOX)  # Switch mid-scene
```

See #link("/api/themes/", "Themes") for available themes.
