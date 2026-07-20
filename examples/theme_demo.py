"""Build a palette with explicit public colors instead of Theme objects."""

from gaanim import BLACK, BLUE, GOLD, GREEN, PINK, PURPLE, WHITE, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.title("Explicit color palette").fill(WHITE).at(0, 220)
    circle = scene.circle(75).fill(GREEN).at(-250, -10)
    square = scene.rect(150, 150).stroke(GOLD, 6).no_fill().at(250, -10)
    equation = scene.equation("H psi = E psi").fill(PURPLE).at(0, -180)

    scene.play([
        circle.grow_from_center().duration(1.2).spring(),
        square.create().duration(1.5).smooth(),
        title.write().duration(1.0),
        equation.spin_in_from_nothing().duration(1.4).smooth(),
    ])
    scene.wait(1.0)
    circle.fill(BLUE)
    equation.fill(PINK)
    scene.play([circle.indicate().duration(1.0), equation.indicate().duration(1.0)])
    scene.wait(0.8)
    scene.render()


if __name__ == "__main__":
    main()
