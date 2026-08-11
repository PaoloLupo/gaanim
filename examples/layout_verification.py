"""Layout verification using Scene, Anchor, and Direction."""

from gaanim import Anchor, BLACK, BLUE, GOLD, GREEN, RED, WHITE, Direction, Scene


def main():
    scene = Scene(1280, 720, background=BLACK, margin=40)
    title = scene.text("Scene layout", role="title").fill(WHITE).to_edge(Direction.UP, 32)
    square = scene.square(100).fill(RED).at(-180, 0)
    label = scene.text("anchor").fill(WHITE).next_to(square, Direction.DOWN, 24)
    circle = scene.circle(48).fill(BLUE).next_to(square, Direction.RIGHT, 48)
    dot = scene.dot(14).fill(GOLD).align_to(circle, Anchor.CENTER)
    group = scene.group([square, circle, dot, label])

    scene.play([title.write().duration(0.8), group.grow_from_center().duration(1.0)])
    scene.play([group.move(120, -40).duration(1.0).smooth()])
    scene.play([circle.indicate().duration(0.6), dot.indicate().duration(0.6)])
    scene.render()


if __name__ == "__main__":
    main()
