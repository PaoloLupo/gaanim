#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Mobjects",
  description: "All available shapes, text, and mathematical objects",
  route: "/api/mobjects/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Mobjects

Mobjects (mathematical objects) are the visual elements in a scene. Every mobject-spawning method on `Scene` returns a `Mobject` handle with a fluent configuration API.

== Fluent Configuration

All mobjects support chained configuration:

```python
circle = scene.circle(80)
    .fill(BLUE)                     # Fill color
    .stroke(GOLD, 4.0)             # Stroke color + width
    .opacity(0.8)                   # Opacity (0.0–1.0)
    .z_index(10)                   # Z-order
    .at(100, 200)                  # Absolute position
    .shift(50, -30)                # Relative offset
    .scale(1.5)                    # Uniform scale
    .rotate(3.14)                  # Rotate (radians)
    .to_edge("top", buff=20)       # Position at screen edge
    .to_corner("top_left")         # Position at screen corner
    .next_to(other, "right", 20)   # Place adjacent to another mobject
```

== Primitives

=== Shapes

```python
scene.circle(radius)               # Circle
scene.rectangle(width, height)     # Rectangle
scene.square(side)                 # Square
scene.dot(radius)                  # Small dot
scene.ellipse(rx, ry)              # Ellipse
scene.rounded_rect(w, h, radius)   # Rounded rectangle
scene.sector(cx, cy, r, start, sweep)  # Pie sector
scene.annulus(outer_r, inner_r)    # Ring/annulus
```

=== Lines & Arrows

```python
scene.line(x1, y1, x2, y2)        # Line segment
scene.arrow(x1, y1, x2, y2)       # Arrow
scene.double_arrow(...)            # Double-headed arrow
scene.dashed_line(...)             # Dashed line
scene.arc(cx, cy, rx, ry, start, sweep)  # Arc
scene.arc_between_points(x1,y1,x2,y2,angle)
```

=== Polygons

```python
scene.polygon(waypoints)           # Polygon from points
scene.star(n_points, outer, inner) # Star shape
scene.regular_polygon(n_sides, radius)  # Regular polygon
```

=== Special Shapes

```python
scene.checkmark(size)              # Checkmark symbol
scene.cross(size)                  # X cross
scene.right_angle(arm_length)      # Right-angle marker
scene.surrounding_rectangle(w, h, corner_r)
scene.background_rectangle(w, h)
```

== Text & Math

```python
scene.text(content, role)          # Text (body/title/subtitle/caption)
scene.title(content)               # Title text
scene.subtitle(content)            # Subtitle text
scene.body(content)                # Body text
scene.caption(content)             # Caption text
scene.equation(formula)            # Typst math equation
```

Equations use Typst math syntax:

```python
eq = scene.equation("integral_0^1 x^2 d x = 1/3")
eq = scene.equation("sum_(i=1)^n i = frac(n(n+1), 2)")
```

== Number Planes & Graphs

```python
plane = scene.number_plane(
    x_range: (-5, 5, 1),
    y_range: (-3, 3, 1),
)

tangent = scene.tangent_line(curve, t, length)
```

== Groups

```python
group = scene.group([circle, rect, text])
# Group inherits all fluent methods
group.scale(2.0).at(0, 0)
```

== Boolean Operations

```python
union = scene.union(a, b)
intersection = scene.intersection(a, b)
difference = scene.difference(a, b)
exclusion = scene.exclusion(a, b)
```

== Reactive Features

```python
# ValueTracker: animatable float
tracker = scene.value_tracker(0.0)
counter = scene.decimal_number(tracker, num_decimals=2, prefix="Count: ")
scene.play(tracker.animate_to(100.0, duration=4.0).spring())

# Updaters: continuous per-frame behaviors
dot = scene.circle(15).fill(ORANGE)
dot.add_orbit_updater(scene, cx=0.0, cy=0.0, radius=150.0, speed=2.0)
dot.add_bob_updater(scene, amplitude=50.0, frequency=0.5)
dot.add_pulse_updater(scene, min_scale=0.7, max_scale=1.3, frequency=1.0)
dot.add_rotate_updater(scene, speed=1.5)
dot.add_follow_updater(scene, target, ox=50.0, oy=0.0, smoothing=0.1)
dot.remove_updater(scene)

# TracedPath: trail following a mobject
trail = scene.traced_path(dot, color=CYAN, width=4.0, min_distance=2.0, max_points=200)
```

== Glyph Selection

Select individual characters in text or equations:

```python
eq = scene.equation("E = m c^2")
mc2 = scene.select(eq, "m c^2")
scene.fill_selection(mc2, CORAL)
scene.set_stroke_selection(mc2, RED, 2.0)

# Animate selected glyphs
sel_anim = scene.selection_anim(mc2, dx=0.0, dy=30.0)
sel_anim.duration(1.5).spring()
scene.play(sel_anim.build(scene))
```
