"""SineCurveUnitCircle — Manim → Gaanim (equivalencia fiel).

Manim original (heejin_park) — `construct()` orquestaba:

    class SineCurveUnitCircle(Scene):
        def construct(self):
            self.show_axis()
            self.show_circle()
            self.move_dot_and_draw_curve()
            self.wait()

Gaanim no tiene `VGroup`/`always_redraw`/`point_from_proportion`/`get_center`
imperativos. En su lugar usa objetos reactivos declarativos que se resuelven
cada frame en Rust: `Updater`, `tracking_line`, `traced_path` y `bind_*`.

Tabla de equivalencias
----------------------
Manim                              | Gaanim (esta escena)
-----------------------------------|---------------------------------------------
`Line(x_start, x_end)`             | `scene.geometry.line(x1, y1, x2, y2).stroke(...)`
`Circle(radius=1).move_to(origin)` | `scene.geometry.circle(r).move_to(cx, cy)`
`MathTex(r"\\pi").next_to(...)`    | `scene.text.equation("pi").move_to(x, y)`
`Dot().move_to(orbit.point_…)`     | `scene.geometry.dot(r).move_to(x, y)`
`dot.add_updater(go_around_circle)`| `dot.add_updater(Updater.orbit(cx,cy,r,speed))`
`always_redraw(get_line_to_circle)`| `scene.geometry.tracking_line((cx,cy), dot)`
`always_redraw(get_line_to_curve)` | `scene.geometry.tracking_line(dot, proj_dot)`
`always_redraw(get_curve)` +       | `scene.geometry.traced_path(proj_dot)`  (acumula
  `VGroup` incremental de `Line`   |  `BezPath` en Rust cada frame)
`proj = dot.y; proj.x += dt*4`     | `proj_dot.bind_y_from(dot)` +
                                   | `proj_dot.add_updater(Updater.advance_x(speed))`
`self.wait(8.5)`                   | `scene.wait(8.5)`
`dot.remove_updater(...)`          | `dot.remove_updater()` / `proj_dot.remove_updater()`

Notas de escala
---------------
- Manim usa unidades abstractas (radio=1, ejes de -6..6). Gaanim usa píxeles.
  Escala elegida: 1 unidad Manim ≈ 100 px. Por eso `circle_radius = 100`.
- Manim: `t_offset += dt*0.25`; `angle = 2π·t_offset`. En Gaanim
  `Updater.orbit` espera velocidad angular `speed` (rad/s), luego
  `speed = 2π·0.25 = π/2 ≈ 1.57`. Se usa `1.5` (valor de `sine_curve.py`)
  para mantener el trazo dentro de 1280 px en 8.5 s.
- Manim: `x = curve_start.x + t_offset*4` → `dx/dt = 1 unidad/s` → ~100 px/s.
  Se usa `55 px/s` para que `55·8.5 ≈ 467 px` quepa entre `curve_start=-300`
  y el borde derecho `~300` con margen.

Run:
    gaanim examples/sine_curve_unit_circle.py
    # o  just run sine_curve_unit_circle
"""

import os

from gaanim import BLUE, WHITE, YELLOW, Color, Direction, Scene, Updater

# ---------------------------------------------------------------------------
# Escena — equivalente a `Scene.construct` en Manim
# ---------------------------------------------------------------------------
scene = Scene(frame=(16, 9), background=Color(15, 15, 26), margin=0.625)

# Paleta (aprox. Manim YELLOW / BLUE / YELLOW_A / YELLOW_D)
YELLOW_DOT = Color(250, 235, 80)   # Dot principal
YELLOW_A = Color(255, 220, 130)    # línea dot → curva
YELLOW_D = Color(200, 180, 50)     # trazo seno
BLUE_LINE = Color(50, 100, 220)    # radio círculo


# ---------------------------------------------------------------------------
# show_axis  →  Manim: self.show_axis()
# ---------------------------------------------------------------------------
def show_axis():
    """Dibuja ejes X/Y y etiquetas π. Retorna (origin, curve_start)."""
    # Manim:
    #   x_start = np.array([-6,0,0]); x_end = np.array([6,0,0])
    #   y_start = np.array([-4,-2,0]); y_end = np.array([-4,2,0])
    #   x_axis = Line(x_start, x_end)
    #   y_axis = Line(y_start, y_end)
    x_axis = scene.geometry.line(-5, 0, 3.75, 0).stroke(WHITE, 0.025)
    y_axis = scene.geometry.line(-5, -2.5, -5, 2.5).stroke(WHITE, 0.025)

    # Manim: self.add_x_labels()  →  MathTex(r"\pi")… next_to(DOWN)
    # Gaanim: scene.text.equation("pi") — Typst math en unidades lógicas
    for i, label in enumerate([r"pi", r"2 pi", r"3 pi", r"4 pi"]):
        # Manim: next_to(np.array([-1+2*i, 0, 0]), DOWN)
        # Gaanim: separación 1.5 unidades, y=-0.4375 bajo el eje
        scene.text.equation(label).move_to(-2.5 + 1.5 * i, -0.4375).fill(WHITE).scale_to(0.55)

    # Manim:
    #   self.origin_point = np.array([-4,0,0])
    #   self.curve_start  = np.array([-3,0,0])
    origin = (-5.0, 0.0)
    curve_start = (-3.75, 0.0)
    return origin, curve_start, x_axis, y_axis


origin_point, curve_start_point, *_ = show_axis()
origin_x, origin_y = origin_point
curve_start_x, curve_start_y = curve_start_point

scene.play([scene.text("Seno · círculo unitario", role="title").to_edge(Direction.UP).fill(WHITE).scale_to(0.9).animate.write()])

# ---------------------------------------------------------------------------
# show_circle  →  Manim: self.show_circle()
# ---------------------------------------------------------------------------
def show_circle(origin):
    """Crea el círculo unitario en `origin`."""
    # Manim:
    #   circle = Circle(radius=1)
    #   circle.move_to(self.origin_point)
    ox, oy = origin
    circle_radius = 1.25
    circle = scene.geometry.circle(circle_radius).move_to(ox, oy).stroke(WHITE, 0.025).no_fill()
    return circle, circle_radius


circle, circle_radius = show_circle(origin_point)


# ---------------------------------------------------------------------------
# move_dot_and_draw_curve  →  Manim: self.move_dot_and_draw_curve()
# ---------------------------------------------------------------------------
def move_dot_and_draw_curve():
    """Orquesta dot orbitante, líneas reactivas y trazo seno."""

    # -- Dot orbitante -------------------------------------------------------
    # Manim:
    #   dot = Dot(radius=0.08, color=YELLOW)
    #   dot.move_to(orbit.point_from_proportion(0))
    #   def go_around_circle(mob, dt):
    #       self.t_offset += dt*0.25
    #       mob.move_to(orbit.point_from_proportion(self.t_offset % 1))
    #   dot.add_updater(go_around_circle)
    #
    # Gaanim: Updater.orbit hace exactamente lo mismo en Rust cada frame:
    #   angle = elapsed * speed; x = cx + r·cos(angle); y = cy + r·sin(angle)
    dot = scene.geometry.dot(0.1).fill(YELLOW_DOT).move_to(origin_x + circle_radius, origin_y)
    dot.add_updater(
        Updater.orbit(cx=origin_x, cy=origin_y, radius=circle_radius, speed=1.5)
    )

    # -- Dot proyección (copia Y del dot, avanza en X) ----------------------
    # Manim:
    #   def get_line_to_curve():
    #       x = self.curve_start[0] + self.t_offset*4
    #       y = dot.get_center()[1]
    #       return Line(dot.get_center(), np.array([x,y,0]))
    #   def get_curve():
    #       new_line = Line(last_line.get_end(), np.array([x,y,0]))
    #       self.curve.add(new_line)
    #
    # Gaanim: se descompone en dos primitivas reactivas
    #   1) bind_y_from → copia Y cada frame
    #   2) Updater.advance_x → x = x0 + speed·elapsed
    proj_dot = scene.geometry.dot(0.0625).fill(YELLOW_A).move_to(curve_start_x, origin_y)
    proj_dot.bind_y_from(dot)
    proj_dot.add_updater(Updater.advance_x(speed=0.6875))

    # -- Líneas reactivas (always_redraw) -----------------------------------
    # Manim:
    #   origin_to_circle_line = always_redraw(get_line_to_circle)
    #   dot_to_curve_line     = always_redraw(get_line_to_curve)
    # Gaanim:
    #   tracking_line regenera un BezPath recta cada frame resolviendo
    #   SpatialTransform de cada endpoint
    radius_line = scene.geometry.tracking_line((origin_x, origin_y), dot)
    radius_line.stroke(BLUE_LINE, 0.025).no_fill()

    proj_line = scene.geometry.tracking_line(dot, proj_dot)
    proj_line.stroke(YELLOW_A, 0.025).no_fill()

    # -- Curva seno (traced_path) -------------------------------------------
    # Manim: VGroup incremental de Lines → crece linealmente en memoria
    # Gaanim: TracedPath acumula Vec<DVec3> en Rust y regenera BezPath
    sine_curve = scene.geometry.traced_path(proj_dot)
    sine_curve.stroke(YELLOW_D, 0.0375).no_fill()

    return dot, proj_dot, radius_line, proj_line, sine_curve


dot, proj_dot, radius_line, proj_line, sine_curve = move_dot_and_draw_curve()
scene.play([
    proj_dot.animate.fade_in().duration(0.3),
    radius_line.animate.fade_in().duration(0.3),
    proj_line.animate.fade_in().duration(0.3),
    sine_curve.animate.fade_in().duration(0.3),
])

# ---------------------------------------------------------------------------
# Playback — Manim: self.wait(8.5) + remove_updater
# ---------------------------------------------------------------------------
scene.wait(8.5)
dot.remove_updater()
proj_dot.remove_updater()

# ---------------------------------------------------------------------------
# Snapshots para `gaanim_diff` / render final
# ---------------------------------------------------------------------------
snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    # Tiempos elegidos para capturar: inicio, 1/4 vuelta, 1/2, etc.
    scene.snapshots(snapshot_dir, [0.0, 0.5, 1.5, 3.0, 5.0, 8.5])
else:
    scene.render()
