"""Sine curve unit circle example.

Demonstrates reactive canvas objects: updaters, tracking lines, traced paths,
and position bindings.

Run: gaanim examples/sine_curve.py
"""

from gaanim import BLUE, WHITE, Canvas, Color, Direction, Updater

canvas = Canvas(1280, 720, background=Color(15, 15, 26), margin=50)

# --- Axes ---
canvas.line(-400, 0, 300, 0).stroke(WHITE, 2.0)
canvas.line(-400, -200, -400, 200).stroke(WHITE, 2.0)
title = canvas.title("Ejemplo de updaters").to_edge(Direction.UP)

title.write()

# --- Pi labels ---
for i, label in enumerate(["pi", "2 pi", "3 pi", "5 pi"]):
    canvas.equation(label).at(-200 + 120 * i, -30)

# --- Circle ---
origin_x, origin_y = -400.0, 0.0
circle_radius = 100.0
circle = (
    canvas.circle(circle_radius).at(origin_x, origin_y).stroke(WHITE, 2.0).no_fill()
)

# --- Orbiting dot ---
dot = canvas.dot(8).fill(Color(245, 208, 75)).at(origin_x + circle_radius, origin_y)
dot.add_updater(
    Updater.orbit(
        cx=origin_x,
        cy=origin_y,
        radius=circle_radius,
        speed=1.5,
    )
)

# --- Projection dot (advances X, mirrors orbit dot's Y) ---
proj_dot = canvas.dot(5).fill(Color(255, 200, 100)).at(-300.0, origin_y)
proj_dot.bind_y_from(dot)
proj_dot.add_updater(Updater.advance_x(speed=55.0))

# --- Reactive lines ---
radius_line = canvas.tracking_line((origin_x, origin_y), dot)
radius_line.stroke(Color(50, 100, 220), 2.0).no_fill()

proj_line = canvas.tracking_line(dot, proj_dot)
proj_line.stroke(Color(255, 220, 130), 2.0).no_fill()

# --- Sine curve (traced path of projection dot) ---
sine_curve = canvas.traced_path(proj_dot)
sine_curve.stroke(Color(200, 180, 50), 3.0).no_fill()

# --- Run ---
canvas.wait(8.5)
dot.remove_updater()
canvas.render()
