"""Layout verification using Scene, Anchor, and Direction."""

from gaanim import Easing, Anchor, BLACK, BLUE, GOLD, GREEN, RED, WHITE, Direction, Scene


def main():
    scene = Scene(frame=(16, 9), background=BLACK, margin=0.5)
    title = scene.text("Scene layout", role="title").fill(WHITE).to_edge(Direction.UP, 32)
    square = scene.geometry.square(1.25).fill(RED).move_to(-2.25, 0)
    label = scene.text("anchor").fill(WHITE).next_to(square, Direction.DOWN, 24)
    circle = scene.geometry.circle(0.6).fill(BLUE).next_to(square, Direction.RIGHT, 48)
    dot = scene.geometry.dot(0.175).fill(GOLD).align_to(circle, Anchor.CENTER)
    group = scene.geometry.group([square, circle, dot, label])

    scene.play([title.animate.write().duration(0.8), group.animate.grow_from_center().duration(1.0)])
    scene.play([group.animate.shift_by(1.5, -0.5).duration(1.0).easing(Easing.SMOOTH)])
    scene.play([circle.animate.indicate().duration(0.6), dot.animate.indicate().duration(0.6)])
    scene.render()


if __name__ == "__main__":
    main()
