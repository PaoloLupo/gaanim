"""Minimal Write smoke test.

A single circle outline is written from arc-length 0 to 1 over 2 seconds.
If the bug were still present, the circle would appear complete from frame 0.
"""

from gaanim import BLUE, GOLD, RED, Scene


def main():
    scene = Scene(width=800, height=600, title="Write Smoke")

    # Single leaf: should write the outline once.
    scene.circle(80).stroke(GOLD, 4).no_fill().at(150, 150).write(2.0).linear()

    # Filled text: should write the outline of each glyph in sequence.
    scene.title("Hola!").at(150, 320).write(2.5).smooth()

    # Filled equation: should write outline of each glyph.
    scene.equation("x^2 + y^2 = z^2").at(150, 420).write(3.0).linear()
    scene.render()


if __name__ == "__main__":
    main()
