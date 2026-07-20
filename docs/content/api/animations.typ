#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Animations",
  description: "Build animations from Drawable handles",
  route: "/api/animations/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Animations

Animation methods live directly on a `Drawable`; there is no `.animate()`
wrapper. Pass the resulting animations to `Scene.play` as a list.

```python
from gaanim import BLUE, GOLD, Scene

scene = Scene(1280, 720)
circle = scene.circle(80).fill(BLUE)
label = scene.text("Hello").fill(GOLD).at(0, 160)

scene.play([
    circle.create().duration(1.0).smooth(),
    label.write().duration(0.8),
])
scene.play([circle.move(200, 0).duration(1.0).spring()])
scene.play([label.fade_out().duration(0.5)])
```

== Available animations

`move`, `move_to`, `glide_to`, `scale`, `rotate`, `fade_in`, `fade_out`,
`fade_to`, `write`, `create`, `unwrite`, `uncreate`, `grow_from_center`,
`shrink_to_center`, `spin_in_from_nothing`, `draw_border_then_fill`, `indicate`,
`wiggle`, `fade_transform`, `transform`, and `replacement_transform` are
available on every drawable.

== Timing and easing

```python
scene.play([
    circle.move(240, 0).duration(1.0).linear(),
    label.fade_to(0.5).duration(1.0).smooth(),
])
scene.play([circle.rotate(3.14159).duration(0.8).spring()])
```

Use `duration`, `delay`, `linear`, `smooth`, `spring`, `ease`, `rate`, `steps`,
and `lag_ratio` to configure the returned animation. Supply `lag=` to
`scene.play` when a list should be staggered.

== Transforms

```python
target = scene.rect(180, 100).fill(GOLD).at(180, 0)
scene.play([circle.transform(target).duration(1.5).spring()])

formula = scene.equation("E = m c^2")
scene.play([target.replacement_transform(formula).duration(1.2)])
```
