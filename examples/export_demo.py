"""Scene used by the executable export command."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    scene = Scene(1920, 1080, background=BLACK)
    equation = scene.equation("integral_a^b f(x) d x = F(b) - F(a)").fill(WHITE).at(0, 100)
    circle = scene.circle(120).stroke(GOLD, 6).no_fill().at(-250, -150)
    rect = scene.rect(200, 120).fill(BLUE).at(250, -150)

    scene.play([equation.write().duration(1.5), circle.create().duration(1.2), rect.grow_from_center().duration(1.2)])
    scene.play([circle.move(100, 0).duration(1.0), rect.rotate(1.5708).duration(1.0)])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
