"""Sine curve unit circle example.

Demonstrates reactive scene objects: updaters, tracking lines, traced paths,
and position bindings.

Run: gaanim examples/sine_curve.py
"""

from gaanim import BLUE, WHITE, Color, Direction, Scene, Updater

scene = Scene(1280, 720, background=Color(15, 15, 26), margin=50)

# --- Axes ---
scene.geometry.line(-400, 0, 300, 0).stroke(WHITE, 2.0)
scene.geometry.line(-400, -200, -400, 200).stroke(WHITE, 2.0)
title = scene.text("Ejemplo de updaters", role="title").to_edge(Direction.UP)

scene.play([title.animate.write()])

# --- Pi labels ---
for i, label in enumerate(["pi", "2 pi", "3 pi", "5 pi"]):
    scene.text(f"${label}$").move_to(-200 + 120 * i, -30)

# --- Circle ---
origin_x, origin_y = -400.0, 0.0
circle_radius = 100.0
circle = (
    scene.geometry.circle(circle_radius).move_to(origin_x, origin_y).stroke(WHITE, 2.0).no_fill()
)

# --- Orbiting dot ---
dot = scene.geometry.dot(8).fill(Color(250, 250, 210)).move_to(origin_x + circle_radius, origin_y)
dot.add_updater(
    Updater.orbit(
        cx=origin_x,
        cy=origin_y,
        radius=circle_radius,
        speed=1.5,
    )
)

# --- Projection dot (advances X, mirrors orbit dot's Y) ---
proj_dot = scene.geometry.dot(5).fill(Color(255, 200, 100)).move_to(-300.0, origin_y)
proj_dot.bind_y_from(dot)
proj_dot.add_updater(Updater.advance_x(speed=55.0))

# --- Reactive lines ---
radius_line = scene.geometry.tracking_line((origin_x, origin_y), dot)
radius_line.stroke(Color(50, 100, 220), 2.0).no_fill()

proj_line = scene.geometry.tracking_line(dot, proj_dot)
proj_line.stroke(Color(255, 220, 130), 2.0).no_fill()

# --- Sine curve (traced path of projection dot) ---
sine_curve = scene.geometry.traced_path(proj_dot)
sine_curve.stroke(Color(200, 180, 50), 3.0).no_fill()

# --- Run ---
scene.play([
    proj_dot.animate.fade_in().duration(0.3),
    radius_line.animate.fade_in().duration(0.3),
    proj_line.animate.fade_in().duration(0.3),
    sine_curve.animate.fade_in().duration(0.3),
])
scene.wait(8.5)
dot.remove_updater()
scene.render()
