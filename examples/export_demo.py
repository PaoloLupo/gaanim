"""Scene used by the executable export command."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    equation = scene.text.equation("integral_a^b f(x) d x = F(b) - F(a)").fill(WHITE).move_to(0, 0.833333)
    circle = scene.geometry.circle(1).stroke(GOLD, 0.05).no_fill().move_to(-2.083333, -1.25)
    rect = scene.geometry.rect(1.666667, 1).fill(BLUE).move_to(2.083333, -1.25)

    scene.play([equation.animate.write().duration(1.5), circle.animate.create().duration(1.2), rect.animate.grow_from_center().duration(1.2)])
    scene.play([circle.animate.shift_by(0.833333, 0).duration(1.0), rect.animate.rotate_by(1.5708).duration(1.0)])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
