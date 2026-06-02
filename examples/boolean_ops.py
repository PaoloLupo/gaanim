"""Example: Boolean operations between shapes.

Demonstrates union, intersection, difference, and exclusion of overlapping
primitives. Each result is a single Mobject that you can position, animate,
or compose with other shapes.
"""

from gaanim import BLUE, GOLD, GREEN, RED, Scene


def main():
    scene = Scene(
        width=1280,
        height=720,
        title="Gaanim - Boolean Operations",
    )

    circle_a = scene.circle(radius=80.0).fill(BLUE).at(-40.0, 0.0)
    circle_b = scene.circle(radius=80.0).fill(RED).at(40.0, 0.0)

    rect_a = scene.rectangle(width=120.0, height=120.0).fill(GREEN).at(-200.0, 0.0)
    rect_b = scene.rectangle(width=120.0, height=120.0).fill(GOLD).at(-120.0, 0.0)

    scene.union(circle_a, circle_b).at(0.0, 150.0)
    scene.intersection(rect_a, rect_b).at(-160.0, 150.0)
    scene.difference(circle_a, circle_b).at(0.0, -120.0)
    scene.exclusion(rect_a, rect_b).at(-160.0, -120.0)

    scene.render()


if __name__ == "__main__":
    main()
