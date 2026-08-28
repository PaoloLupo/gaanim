"""Small scene for `gaanim export ... --output output_test.webp`."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    equation = scene.text.equation("E = m c^2").fill(WHITE).move_to(0, 1.5)
    circle = scene.geometry.circle(1.333333).stroke(GOLD, 0.083333).no_fill().move_to(-2.166667, -1)
    rect = scene.geometry.rect(2.5, 1.5).fill(BLUE).move_to(2.5, -1)

    scene.play([equation.animate.write().duration(1.0), circle.animate.create().duration(0.8), rect.animate.grow_from_center().duration(0.8)])
    scene.play([circle.animate.shift_by(1.5, 0).duration(0.5), rect.animate.rotate_by(1.5708).duration(0.5)])
    scene.wait(0.3)
    scene.render()


if __name__ == "__main__":
    main()
