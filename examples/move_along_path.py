"""Demostracion de MoveAlongPath y GrowArrow.

MoveAlongPath: el circulo azul sigue una trayectoria poligonal.
GrowArrow: la flecha blanca se dibuja con un "punch" final en la cabeza.
"""

import math

from gaanim import (
    BLUE,
    GREEN,
    RED,
    WHITE,
    YELLOW,
    Scene,
)


def main():
    scene = Scene(
        width=1920,
        height=1080,
        title="Gaanim - MoveAlongPath & GrowArrow",
    )
    scene.background(gaanim_color((0.07, 0.08, 0.12, 1.0)))

    # Trayectorias ------------------------------------------------------------
    # Cuadrado
    square = [
        (-300.0, -150.0),
        (300.0, -150.0),
        (300.0, 150.0),
        (-300.0, 150.0),
        (-300.0, -150.0),
    ]
    # Estrella de 5 puntas
    star_pts = []
    for i in range(10):
        angle = math.pi * 2.0 * (i / 10.0) - math.pi / 2.0
        r = 250.0 if i % 2 == 0 else 110.0
        star_pts.append((r * math.cos(angle), r * math.sin(angle)))

    # Curva sinusoidal muestreada
    sine_pts = []
    for i in range(60):
        t = i / 59.0
        x = -450.0 + 900.0 * t
        y = 180.0 * math.sin(t * math.pi * 2.0)
        sine_pts.append((x, y))

    # Guías (contornos amarillos) -------------------------------------------
    guide_sq = scene.polygon(square).stroke(YELLOW, 1.5)
    guide_sq.no_fill()
    guide_sq.opacity(0.4)
    guide_sq.at(0.0, -350.0)

    guide_star = scene.polygon(star_pts).stroke(GREEN, 1.5)
    guide_star.no_fill()
    guide_star.opacity(0.4)
    guide_star.at(0.0, -350.0)

    # Mobject 1: circulo sigue el cuadrado ---------------------------------
    traveler = scene.circle(28.0).fill(BLUE).stroke(WHITE, 2.0).at(-300.0, -500.0)

    scene.wait(0.3)
    scene.play(traveler.move_along_path(square, 3.0).linear())
    scene.wait(0.3)

    # Mobject 2: circulo siguiendo la estrella -----------------------------
    traveler2 = (
        scene.circle(22.0)
        .fill(RED)
        .stroke(WHITE, 1.5)
        .at(star_pts[0][0], star_pts[0][1] - 350.0)
    )

    scene.play(traveler2.move_along_path(star_pts, 4.0).smooth())
    scene.wait(0.3)

    # Mobject 3: dot pequeño sigue la sinusoide ----------------------------
    dot = scene.dot(12.0).fill(WHITE)
    dot.at(sine_pts[0][0], sine_pts[0][1] + 250.0)

    scene.play(dot.move_along_path(sine_pts, 3.0).linear())
    scene.wait(0.3)

    # GrowArrow: la flecha se dibuja con punch final -----------------------
    a1 = scene.arrow(-600.0, 0.0, 0.0, 0.0)
    a1.fill(WHITE)
    a1.stroke(WHITE, 2.5)

    a2 = scene.double_arrow(-600.0, 80.0, 0.0, 80.0, None, None)
    a2.fill(YELLOW)
    a2.stroke(YELLOW, 2.5)

    scene.wait(0.2)
    scene.play(a1.grow_arrow(1.2))
    scene.play(a2.grow_arrow(1.2))

    scene.wait(0.5)
    scene.render()


def gaanim_color(rgba):
    from gaanim import Color

    r, g, b, a = rgba
    return Color(int(r * 255), int(g * 255), int(b * 255), int(a * 255))


if __name__ == "__main__":
    main()
