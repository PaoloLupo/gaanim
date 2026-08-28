"""Example: Multi-scene Scene with transitions and rich animations.

Demonstrates:
  - Named segments with transitions (cross_fade, fade_through, slide)
  - Text, circles, rects, squares, dots, and arrows
  - Animation chaining: fade_in, write, create, grow_from_center, glide_to, indicate
  - Easing: smooth, bounce, ease_out_elastic, ease_out_back
  - fade_out_all for clean scene transitions
  - Delayed and sequenced animations
"""

import math

from gaanim import (
    Easing, EasingCurve,
    BLACK,
    BLUE,
    CORAL,
    GOLD,
    GRAY,
    GREEN,
    ORANGE,
    PURPLE,
    RED,
    TEAL,
    WHITE,
    YELLOW,
    Scene,
    Transition,
)

c = Scene(frame=(16, 9), background=BLACK)

# ── Segment 1: Title ────────────────────────────────────────────
c.segment("intro")

title = c.text("Gaanim", role="title").fill(WHITE).move_to(0, 1.666667)
subtitle = c.text("Animaciones", role="subtitle").fill(GRAY).move_to(0, 0.833333)

c.play([title.animate.write().duration(2.0).easing(Easing.LINEAR)])
c.play([subtitle.animate.fade_in().duration(1.0).delay(1.5).easing(Easing.SMOOTH)])

c.wait(2.0)

# ── Segment 2: Geometric shapes ─────────────────────────────────
c.segment("shapes", Transition.cross_fade(0.8))

heading = c.text("Formas Geometricas").fill(GOLD).move_to(0, 2.083333)
c.play([heading.animate.write().duration(1.0).easing(Easing.LINEAR)])

circle = c.geometry.circle(0.666667).fill(BLUE).stroke(WHITE, 0.025).move_to(-2.5, 0)
rect = c.geometry.rect(1.166667, 0.833333).fill(CORAL).stroke(WHITE, 0.025).move_to(0, 0)
square = c.geometry.square(0.833333).fill(GREEN).stroke(WHITE, 0.025).move_to(2.5, 0)

c.play([circle.animate.grow_from_center().duration(0.8).easing(Easing.ease_out(EasingCurve.ELASTIC))])
c.play([rect.animate.grow_from_center().duration(0.8).delay(0.3).easing(Easing.ease_out(EasingCurve.ELASTIC))])
c.play([square.animate.grow_from_center().duration(0.8).delay(0.6).easing(Easing.ease_out(EasingCurve.ELASTIC))])

c.wait(1.0)

# Animate them moving
c.play([circle.animate.shift_by(0, -0.666667).duration(0.6).easing(Easing.SMOOTH)])
c.play([rect.animate.shift_by(0, -0.666667).duration(0.6).easing(Easing.SMOOTH)])
c.play([square.animate.shift_by(0, -0.666667).duration(0.6).easing(Easing.SMOOTH)])

c.wait(1.0)

# Indicate effect on circle
c.play([circle.animate.indicate().duration(0.6)])

c.wait(1.0)

c.fade_out_all(1.0)
c.wait(0.5)

# ── Segment 3: Typography and arrows ────────────────────────────
c.segment("typography", Transition.fade_through(0.6, BLACK))

label = c.text("Texto y Flechas").fill(YELLOW).move_to(0, 2.083333)
c.play([label.animate.write().duration(1.0).easing(Easing.LINEAR)])

hello = c.text("Hola Mundo!").fill(WHITE).move_to(-1.666667, 0)
c.play([hello.animate.draw_border_then_fill().duration(1.5).stroke_width(2.0)])

arrow = c.geometry.arrow(-0.416667, 0, 1.25, 0).stroke(ORANGE, 0.033333)
c.play([arrow.animate.create().duration(0.8).delay(1.0).easing(Easing.LINEAR)])

world = c.text("Gaanim").fill(PURPLE).move_to(2.5, 0)
c.play([world.animate.fade_in().duration(0.8).delay(1.5).easing(Easing.ease_out(EasingCurve.BOUNCE))])

c.wait(2.0)

# Slide everything to the left
c.play([
    hello.animate.move_to(-3.333333, 0).duration(1.0).easing(Easing.SMOOTH),
    world.animate.move_to(0.833333, 0).duration(1.0).easing(Easing.SMOOTH),
])

c.wait(1.0)

c.fade_out_all(1.0)
c.wait(0.5)

# ── Segment 4: Parallel animations finale ───────────────────────
c.segment("finale", Transition.slide(0.5, "up"))

outro = c.text("Gracias!", role="title").fill(GOLD).move_to(0, 0.833333)
c.play([outro.animate.spin_in_from_nothing().duration(1.2).easing(Easing.ease_out(EasingCurve.BACK))])

# Burst of colored dots
dots = []
colors = [RED, BLUE, GREEN, YELLOW, CORAL, PURPLE, ORANGE, TEAL]
for i, color in enumerate(colors):
    angle = i * (2.0 * math.pi / len(colors))
    dx = 1.666667 * math.cos(angle)
    dy = 1.666667 * math.sin(angle)
    dot = c.geometry.dot(0.125).fill(color).move_to(dx, dy)
    c.play([dot.animate.grow_from_center().duration(0.5).delay(0.8 + i * 0.1).easing(Easing.ease_out(EasingCurve.BACK))])
    dots.append(dot)

c.wait(2.0)

# All dots and title fade out together
c.play([outro.animate.fade_out().duration(1.0)])
for dot in dots:
    c.play([dot.animate.shrink_to_center().duration(0.8)])

c.wait(1.0)

c.render()
