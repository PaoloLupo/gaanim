"""Advanced animation timing with the public Scene API."""

from gaanim import BLACK, BLUE, CYAN, GOLD, PURPLE, WHITE, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    square = scene.rect(150, 150).stroke(PURPLE, 6).no_fill().at(300, -20)
    circle = scene.circle(75).fill(CYAN).at(-300, -20)
    title = scene.text("Advanced Animations", role="title").fill(WHITE).at(0, 180)
    equation = scene.equation("f(x) = x^2 - 2x + 1").fill(GOLD).at(0, -20)

    scene.play([
        square.create().duration(2.0).smooth(),
        circle.grow_from_center().duration(1.5).spring(),
        title.spin_in_from_nothing().duration(1.8).smooth(),
        equation.write().duration(2.4).linear(),
    ])
    scene.wait(1.0)
    scene.play([circle.indicate().duration(1.0), equation.indicate().duration(1.0)])
    scene.wait(0.8)
    scene.play([
        square.uncreate().duration(1.4).smooth(),
        circle.shrink_to_center().duration(1.2).smooth(),
        title.unwrite().duration(1.4).smooth(),
        equation.unwrite().duration(1.6).linear(),
    ])
    scene.render()


if __name__ == "__main__":
    main()
