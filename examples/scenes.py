"""Example: Multi-scene Canvas with transitions and rich animations.

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
    Canvas,
    Transition,
)

c = Canvas(1920, 1080, background=BLACK)

# ── Segment 1: Title ────────────────────────────────────────────
c.segment("intro")

title = c.title("Gaanim").fill(WHITE).at(0.0, 200.0)
subtitle = c.subtitle("Animaciones").fill(GRAY).at(0.0, 100.0)

title.write().duration(2.0).smooth()
subtitle.fade_in().duration(1.0).delay(1.5).smooth()

c.wait(2.0)

# ── Segment 2: Geometric shapes ─────────────────────────────────
c.segment("shapes", Transition.cross_fade(0.8))

heading = c.text("Formas Geometricas").fill(GOLD).at(0.0, 250.0)
heading.write().duration(1.0)

circle = c.circle(80.0).fill(BLUE).stroke(WHITE, 3.0).at(-300.0, 0.0)
rect = c.rect(140.0, 100.0).fill(CORAL).stroke(WHITE, 3.0).at(0.0, 0.0)
square = c.square(100.0).fill(GREEN).stroke(WHITE, 3.0).at(300.0, 0.0)

circle.grow_from_center().duration(0.8).ease("ease_out_elastic")
rect.grow_from_center().duration(0.8).delay(0.3).ease("ease_out_elastic")
square.grow_from_center().duration(0.8).delay(0.6).ease("ease_out_elastic")

c.wait(1.0)

# Animate them moving
circle.move(0.0, -80.0).duration(0.6).smooth()
rect.move(0.0, -80.0).duration(0.6).smooth()
square.move(0.0, -80.0).duration(0.6).smooth()

c.wait(1.0)

# Indicate effect on circle
circle.indicate().duration(0.6)

c.wait(1.0)

c.fade_out_all(1.0)
c.wait(0.5)

# ── Segment 3: Typography and arrows ────────────────────────────
c.segment("typography", Transition.fade_through(0.6, BLACK))

label = c.text("Texto y Flechas").fill(YELLOW).at(0.0, 250.0)
label.write().duration(1.0)

hello = c.text("Hola Mundo!").fill(WHITE).at(-200.0, 0.0)
hello.write().duration(1.5).smooth()

arrow = c.arrow(-50.0, 0.0, 150.0, 0.0).stroke(ORANGE, 4.0)
arrow.create().duration(0.8).delay(1.0)

world = c.text("Gaanim").fill(PURPLE).at(300.0, 0.0)
world.fade_in().duration(0.8).delay(1.5).ease("bounce")

c.wait(2.0)

# Slide everything to the left
hello.glide_to(-400.0, 0.0).duration(1.0).smooth()
world.glide_to(100.0, 0.0).duration(1.0).smooth()

c.wait(1.0)

c.fade_out_all(1.0)
c.wait(0.5)

# ── Segment 4: Parallel animations finale ───────────────────────
c.segment("finale", Transition.slide(0.5, "up"))

outro = c.title("Gracias!").fill(GOLD).at(0.0, 100.0)
outro.spin_in_from_nothing().duration(1.2).ease("ease_out_back")

# Burst of colored dots
dots = []
colors = [RED, BLUE, GREEN, YELLOW, CORAL, PURPLE, ORANGE, TEAL]
for i, color in enumerate(colors):
    angle = i * (2.0 * math.pi / len(colors))
    dx = 200.0 * math.cos(angle)
    dy = 200.0 * math.sin(angle)
    dot = c.dot(15.0).fill(color).at(dx, dy)
    dot.grow_from_center().duration(0.5).delay(0.8 + i * 0.1).ease("ease_out_back")
    dots.append(dot)

c.wait(2.0)

# All dots and title fade out together
outro.fade_out().duration(1.0)
for dot in dots:
    dot.shrink_to_center().duration(0.8)

c.wait(1.0)

c.render()
