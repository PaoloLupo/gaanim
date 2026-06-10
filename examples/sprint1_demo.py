"""Example: Sprint 1 Mathematical and Annotation Features.

Demonstrates:
  - Coordinate axes with ticks and labels
  - NumberLine with ticks and labels
  - Function graphing y = f(x)
  - Parametric curve r(t) = (x(t), y(t))
  - CurvedArrow with custom angle curvature
  - Vector starting from origin
  - Decorative curly Brace
  - LabeledArrow and LabeledBrace annotations
"""

import math
from gaanim import (
    BLUE,
    CORAL,
    GOLD,
    RED,
    WHITE,
    YELLOW,
    Scene,
)


def main():
    print("[Gaanim Python] Initializing GPU Scene for Sprint 1...")
    scene = Scene(width=1280, height=720, title="Gaanim — Sprint 1 Math & Annotations")
    scene.background(Color(20, 20, 30))

    # 1. Spawn coordinate axes with ticks and labels
    print("[Gaanim Python] Spawning Axes...")
    axes = scene.axes(
        x_range=(-300.0, 300.0, 100.0),
        y_range=(-200.0, 200.0, 100.0),
        include_labels=True,
    )

    # 2. Spawn a function graph (y = sin(x / 50) * 120)
    print("[Gaanim Python] Spawning Function Graph...")
    graph = scene.function_graph(
        x_range=(-250.0, 250.0),
        steps=100,
        f=lambda x: math.sin(x / 50.0) * 120.0,
    ).stroke(GOLD, 3.5).no_fill()

    # 3. Spawn a parametric curve (Archimedean spiral)
    print("[Gaanim Python] Spawning Parametric Curve...")
    spiral = scene.parametric_curve(
        t_range=(0.0, 4.0 * math.pi),
        steps=150,
        f=lambda t: (10.0 * t * math.cos(t), 10.0 * t * math.sin(t)),
    ).stroke(YELLOW, 2.5).no_fill()

    # 4. Spawn vector from origin
    print("[Gaanim Python] Spawning Vector...")
    vec = scene.vector(150.0, 150.0).stroke(CORAL, 3.0)

    # 5. Spawn curved arrow
    print("[Gaanim Python] Spawning Curved Arrow...")
    curved_arr = scene.curved_arrow(
        x1=-200.0,
        y1=180.0,
        x2=200.0,
        y2=180.0,
        angle=math.pi / 3.0,
    ).stroke(BLUE, 3.0)

    # 6. Spawn decorative brace
    print("[Gaanim Python] Spawning Brace...")
    brace = scene.brace(
        x1=-200.0,
        y1=-220.0,
        x2=200.0,
        y2=-220.0,
        height=20.0,
    ).stroke(WHITE, 2.5)

    # 7. Spawn LabeledArrow
    print("[Gaanim Python] Spawning Labeled Arrow...")
    lab_arrow = scene.labeled_arrow(
        x1=-400.0,
        y1=-150.0,
        x2=-400.0,
        y2=100.0,
        label="Height",
        spacing=20.0,
    )

    # 8. Spawn LabeledBrace
    print("[Gaanim Python] Spawning Labeled Brace...")
    lab_brace = scene.labeled_brace(
        x1=400.0,
        y1=-150.0,
        x2=400.0,
        y2=100.0,
        label="Width",
        height=20.0,
        spacing=10.0,
    )

    # Coordinated create/fade animations
    print("[Gaanim Python] Playing animations...")
    scene.play(axes.animate().fade_in().duration(1.0).linear())
    scene.play(graph.animate().create(duration=1.5).linear())
    scene.play(spiral.animate().create(duration=1.5).linear())
    scene.play(vec.animate().create(duration=1.0).linear())
    scene.play(curved_arr.animate().create(duration=1.0).linear())
    scene.play(brace.animate().create(duration=1.0).linear())
    scene.play(lab_arrow.animate().fade_in().duration(1.0).linear())
    scene.play(lab_brace.animate().fade_in().duration(1.0).linear())

    scene.wait(2.0)

    # Render
    print("[Gaanim Python] Rendering scene...")
    scene.render()


# Helper to construct color matching demo style
def Color(r, g, b):
    from gaanim import Color as GColor
    return GColor(r, g, b, 255)


if __name__ == "__main__":
    main()
