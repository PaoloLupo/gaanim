"""Scene used by the executable export command."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    scene = Scene(1920, 1080, background=BLACK)
    equation = scene.text.equation("integral_a^b f(x) d x = F(b) - F(a)").fill(WHITE).move_to(0, 100)
    circle = scene.geometry.circle(120).stroke(GOLD, 6).no_fill().move_to(-250, -150)
    rect = scene.geometry.rect(200, 120).fill(BLUE).move_to(250, -150)

    scene.play([equation.animate.write().duration(1.5), circle.animate.create().duration(1.2), rect.animate.grow_from_center().duration(1.2)])
    scene.play([circle.animate.shift_by(100, 0).duration(1.0), rect.animate.rotate_by(1.5708).duration(1.0)])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
