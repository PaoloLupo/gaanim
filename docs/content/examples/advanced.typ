#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Advanced examples",
  description: "Transforms, groups, segments, and reactive paths",
  route: "/examples/advanced/",
  updated: datetime.today().display(),
)

= Advanced examples

== Transforms across segments

```python
from gaanim import BLACK, BLUE, GOLD, GREEN, WHITE, Scene, Transition

scene = Scene(1280, 720, background=BLACK)
scene.segment("shapes")
circle = scene.circle(80).fill(BLUE).stroke(WHITE, 4).at(-180, 0)
scene.play([circle.create().duration(0.8)])

scene.segment("text", Transition.cross_fade(0.4))
headline = scene.title("A stable transform").fill(GOLD)
scene.play([circle.replacement_transform(headline).duration(1.4).spring()])

formula = scene.equation("E = m c^2").fill(GREEN).at(0, -150)
scene.play([headline.transform(formula).duration(1.4).smooth()])
# Run this file with: gaanim transforms.py
```

== Groups

```python
from gaanim import BLACK, BLUE, GREEN, RED, Scene

scene = Scene(1280, 720, background=BLACK)
left = scene.circle(40).fill(BLUE).at(-80, 0)
middle = scene.circle(40).fill(RED).at(0, 0)
right = scene.circle(40).fill(GREEN).at(80, 0)
group = scene.group([left, middle, right])

scene.play([group.grow_from_center().duration(1.0).spring()])
scene.play([group.move(0, 120).duration(1.0), group.rotate(3.14159).duration(1.0)])
# Run this file with: gaanim groups.py
```

== Reactive path

```python
from gaanim import BLACK, Color, Scene, Updater

scene = Scene(1280, 720, background=BLACK)
dot = scene.dot(10).fill(Color(255, 180, 70)).at(200, 0)
dot.add_updater(Updater.orbit(0, 0, 200, 1.5))
trail = scene.traced_path(dot).stroke(Color(80, 220, 220), 3).no_fill()

scene.play([dot.fade_in().duration(0.3), trail.create().duration(0.3)])
scene.wait(4.0)
dot.remove_updater()
# Run this file with: gaanim reactive_path.py
```
