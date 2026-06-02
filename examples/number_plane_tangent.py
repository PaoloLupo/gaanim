"""Ejemplo: NumberPlane con grafica de funcion y tangente.

Demuestra NumberPlane (ejes + grilla), grafica poligonal de una
funcion, y la linea tangente en un punto t parametrizado.
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
        title="Gaanim - NumberPlane & TangentLine",
    )
    scene.background(gaanim_color((0.07, 0.08, 0.12, 1.0)))

    # Plano cartesiano: usamos rangos pre-escalados (1 unidad = 60 px)
    # para evitar que el stroke width se multiplique al llamar .scale().
    plane = scene.number_plane(
        x_range=(-360.0, 360.0, 60.0),
        y_range=(-210.0, 210.0, 60.0),
        axis_stroke=2.5,
        grid_stroke=1.2,
    )
    plane.stroke(gaanim_color((0xA0, 0xC8, 0xFF, 0x80)), 1.2)

    # Curva: f(x) = sin(x) * exp(-x/8) muestreada en 60 puntos
    f = lambda x: math.sin(x) * math.exp(-x / 8.0)
    curve_pts = []
    for i in range(60):
        x = -6.0 + 12.0 * (i / 59.0)
        y = f(x)
        curve_pts.append((x * 60.0, y * 60.0))

    curve = scene.polygon(curve_pts)
    curve.stroke(WHITE, 3.0)
    curve.no_fill()
    # Cerrar el poligono: el polygon primitivo cierra el path. Para una
    # grafica abierta, mejor usar un line strip; gaanim solo expone
    # polygon. Como la curva ya esta abierta en sus dos extremos, el
    # close visual no se nota.

    # Tangente en t = 0.35 (cerca del primer maximo)
    t_value = 0.35
    tangent = scene.tangent_line(curve_pts, t_value, 200.0)
    tangent.stroke(YELLOW, 3.0)

    # Punto en la curva
    idx = int(t_value * 59)
    px, py = curve_pts[idx]
    dot = scene.dot(8.0).fill(RED)
    dot.at(px, py)

    scene.wait(0.3)
    scene.play(plane.animate().fade_in().duration(0.6))
    scene.play(curve.animate().create(duration=1.5).linear())
    scene.play(dot.animate().fade_in().duration(0.3))
    scene.play(tangent.animate().fade_in().duration(0.5))

    # Mover la tangente a un nuevo t
    t2 = 0.78
    tangent2 = scene.tangent_line(curve_pts, t2, 200.0)
    tangent2.stroke(GREEN, 3.0)
    scene.play(tangent2.animate().fade_in().duration(0.5))

    scene.wait(0.5)
    scene.render()
    print("OK: render complete")


def gaanim_color(rgba):
    from gaanim import Color

    if len(rgba) == 3:
        r, g, b = rgba
        a = 255
    else:
        r, g, b, a = rgba
    return Color(int(r), int(g), int(b), int(a))


if __name__ == "__main__":
    main()
