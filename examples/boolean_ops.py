"""Overlapping shapes with the public Scene primitives.

Boolean union/intersection are implemented in Rust but are not yet exposed by
the Python API, so this demo intentionally shows composition rather than
claiming those operations are available.
"""

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.text("Overlapping shapes", role="title").fill(WHITE).at(0, 230)
    circle_a = scene.circle(80).fill(BLUE).opacity(0.7).at(-60, 60)
    circle_b = scene.circle(80).fill(RED).opacity(0.7).at(60, 60)
    rect_a = scene.rect(120, 120).fill(GREEN).opacity(0.7).at(-80, -150)
    rect_b = scene.rect(120, 120).fill(GOLD).opacity(0.7).at(0, -150)

    scene.play([
        title.write().duration(0.8),
        circle_a.grow_from_center().duration(0.8),
        circle_b.grow_from_center().duration(0.8),
        rect_a.create().duration(0.8),
        rect_b.create().duration(0.8),
    ])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
