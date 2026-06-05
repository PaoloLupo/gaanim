#import "../../components/section.typ": docs-chapter

#show: docs-chapter.with(
  title: "Animations",
  description: "Animation types and rate functions",
  route: "/api/animations/",
  code-langs: (),
  updated: datetime.today().display(),
)

= Animations

Animations are created by calling `.animate()` on a mobject, then chaining animation methods. Each `scene.play()` call advances the timeline.

== Basic Animations

```python
# Write: Manim-style pen-stroke progressive draw
scene.play(circle.animate().write(duration=2.0))

# Create: progressive draw (parallel, no stagger)
scene.play(circle.animate().create(duration=1.5))

# Uncreate / Unwrite: reverse
scene.play(circle.animate().uncreate(duration=1.0))
scene.play(circle.animate().unwrite(duration=1.0))
```

== Transform Animations

```python
# Shift: translate by delta
scene.play(circle.animate().shift_anim(200, 0).duration(1.0))

# Translate to: move to absolute position
scene.play(circle.animate().translate_to_anim(100, 200).duration(1.0))

# Scale
scene.play(circle.animate().scale_anim(2.0).duration(1.0))

# Rotate
scene.play(circle.animate().rotate_anim(3.14).duration(1.5))

# Color transitions
scene.play(circle.animate().fill_color_anim(RED).duration(1.0))
scene.play(circle.animate().stroke_color_anim(GOLD).duration(1.0))
```

== Fade Animations

```python
scene.play(circle.animate().fade_in_anim().duration(1.0))
scene.play(circle.animate().fade_out_anim().duration(0.5))
scene.play(circle.animate().fade_to_anim(0.5).duration(1.0))  # Fade to specific opacity
```

== Entrance Animations

```python
scene.play(circle.animate().grow_from_center().duration(1.0).spring())
scene.play(circle.animate().shrink_to_center().duration(0.8))
scene.play(circle.animate().spin_in_from_nothing().duration(1.0))
scene.play(circle.animate().grow_from_point(0, 0).duration(1.0))
scene.play(circle.animate().grow_from_edge("left").duration(1.0))
scene.play(circle.animate().draw_border_then_fill().duration(2.0))
```

== Emphasis Animations

```python
# Indicate: highlight pulse
scene.play(circle.animate().indicate(color=GOLD, scale_factor=1.3).duration(1.0))

# Circumscribe: surrounding highlight
scene.play(circle.animate().circumscribe(color=RED).duration(1.0))

# Flash: radiant flash
scene.play(circle.animate().flash(color=YELLOW, n_lines=16, radius=100).duration(0.5))

# Wiggle: horizontal vibration
scene.play(circle.animate().wiggle().duration(1.0))
```

== Path & Arrow Animations

```python
# Move along path
waypoints = [(0,0), (100,100), (200,0), (300,100)]
scene.play(dot.animate().move_along_path(waypoints, duration=3.0))

# Grow arrow
scene.play(arrow.animate().grow_arrow(duration=1.0))

# Passing flash (neon effect)
scene.play(circle.animate().show_passing_flash(duration=1.0, time_width=0.5))
```

== Transform Between Mobjects

```python
# Fade transform: cross-fade between mobjects
scene.play(circle.animate().fade_transform(rect).duration(1.0))
```

== Rate Functions

Chain a rate function after the animation method:

```python
scene.play(circle.animate().shift_anim(200, 0).duration(1.0).linear())
scene.play(circle.animate().shift_anim(200, 0).duration(1.0).smooth())
scene.play(circle.animate().shift_anim(200, 0).duration(1.0).spring())
scene.play(circle.animate().shift_anim(200, 0).duration(1.0).steps(5))
scene.play(circle.animate().shift_anim(200, 0).duration(1.0).cubic_bezier(0.4, 0, 0.2, 1))
```

=== Available Rate Functions

#table(
  columns: (1fr, 2fr),
  [*Name*], [*Description*],
  [`linear`], [Constant speed],
  [`smooth`], [Ease-in-out (default)],
  [`spring`], [Spring physics with overshoot],
  [`steps(n)`], [Discrete steps],
  [`cubic_bezier(x1,y1,x2,y2)`], [Custom cubic bezier curve],
  [`ease_in`], [Slow start, fast end],
  [`ease_out`], [Fast start, slow end],
  [`ease_in_out`], [Slow start and end],
  [`bounce_out`], [Bouncing at the end],
  [`elastic_in_out`], [Elastic spring effect],
  [`there_and_back`], [Forward then reverse],
  [`there_and_back_with_pause(ratio)`], [Forward, pause, reverse],
  [`mirror(name)`], [Mirror another rate function],
)

== Timing Rules

- *Parallel*: multiple animations in a single `scene.play()` call play simultaneously
- *Sequential*: each separate `scene.play()` or `scene.wait()` call advances the timeline after the previous one finishes

```python
# These two play in parallel
scene.play(
    circle.animate().shift(200, 0).duration(1.0),
    rect.animate().rotate(3.14).duration(1.0),
)

# This plays after the above finishes
scene.play(circle.animate().fade_out_anim().duration(0.5))
```
