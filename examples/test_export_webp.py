"""Small scene for `gaanim export ... --output output_test.webp`."""

from gaanim import BLACK, BLUE, GOLD, WHITE, Scene


def main():
    scene = Scene(960, 540, background=BLACK)
    equation = scene.text.equation("E = m c^2").fill(WHITE).at(0, 90)
    circle = scene.geometry.circle(80).stroke(GOLD, 5).no_fill().at(-130, -60)
    rect = scene.geometry.rect(150, 90).fill(BLUE).at(150, -60)

    scene.play([equation.write().duration(1.0), circle.create().duration(0.8), rect.grow_from_center().duration(0.8)])
    scene.play([circle.move(90, 0).duration(0.5), rect.rotate(1.5708).duration(0.5)])
    scene.wait(0.3)
    scene.render()


if __name__ == "__main__":
    main()
