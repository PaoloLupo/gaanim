"""Small scene for `gaanim export ... --output output_test.webp`."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    scene = Scene(960, 540, background=BLACK)
    equation = scene.text.equation("E = m c^2").fill(WHITE).move_to(0, 90)
    circle = scene.geometry.circle(80).stroke(GOLD, 5).no_fill().move_to(-130, -60)
    rect = scene.geometry.rect(150, 90).fill(BLUE).move_to(150, -60)

    scene.play([equation.animate.write().duration(1.0), circle.animate.create().duration(0.8), rect.animate.grow_from_center().duration(0.8)])
    scene.play([circle.animate.shift_by(90, 0).duration(0.5), rect.animate.rotate_by(1.5708).duration(0.5)])
    scene.wait(0.3)
    scene.render()


if __name__ == "__main__":
    main()
