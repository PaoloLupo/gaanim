#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Basic examples",
  description: "Small runnable scenes using the public API",
  route: "/examples/basic/",
  updated: datetime.today().display(),
)

= Basic examples

Each example starts with `Scene`; animations are built on drawables and passed to
`scene.play` as a list.

== Circle and rectangle

```python
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK)
circle = scene.circle(80).fill(BLUE).stroke(WHITE, 4).at(-120, 0)
rect = scene.rect(160, 100).fill(GOLD).at(120, 0)

scene.play([
    circle.create().duration(1.0).smooth(),
    rect.grow_from_center().duration(1.0).spring(),
])
scene.wait(0.5)
scene.play([circle.move(180, 0).duration(1.0), rect.fade_out().duration(0.5)])
# Run this file with: gaanim basic_circle.py
```

== Text and math

```python
from gaanim import BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(1280, 720, background=BLACK)
title = scene.text("Mass-energy equivalence", role="title").fill(WHITE).at(0, 180)
equation = scene.text("$E = m c^2$").fill(GOLD).at(0, 0)
caption = scene.text("Energy and mass are related", role="subtitle").fill(BLUE).at(0, -150)

scene.play([title.write().duration(1.0), equation.write().duration(1.5)])
scene.play([caption.fade_in().duration(0.8)])
scene.wait(1.0)
# Run this file with: gaanim text_and_math.py
```

== Shapes gallery

```python
from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene

scene = Scene(1280, 720, background=BLACK)
circle = scene.circle(60).fill(BLUE).at(-240, 80)
rect = scene.rect(130, 80).fill(RED).at(0, 80)
square = scene.square(90).fill(GREEN).at(240, 80)
ellipse = scene.ellipse(90, 55).fill(GOLD).at(-120, -120)
arrow = scene.arrow(0, -120, 220, -120).stroke(WHITE, 4)

scene.play([
    circle.grow_from_center().duration(0.8),
    rect.grow_from_center().duration(0.8),
    square.grow_from_center().duration(0.8),
    ellipse.create().duration(0.8),
    arrow.create().duration(0.8),
], lag=0.1)
scene.wait(1.0)
# Run this file with: gaanim shapes_gallery.py
```
