"""Advanced animation timing with the public Scene API."""

from gaanim import Easing, BLACK, BLUE, CYAN, GOLD, PURPLE, WHITE, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    square = scene.geometry.rect(150, 150).stroke(PURPLE, 6).no_fill().move_to(300, -20)
    circle = scene.geometry.circle(75).fill(CYAN).move_to(-300, -20)
    title = scene.text("Advanced Animations", role="title").fill(WHITE).move_to(0, 180)
    equation = scene.text.equation("f(x) = x^2 - 2x + 1").fill(GOLD).move_to(0, -20)

    scene.play([
        square.animate.create().duration(2.0).easing(Easing.SMOOTH),
        circle.animate.grow_from_center().duration(1.5).easing(Easing.spring(stiffness=90.0, damping=12.0)),
        title.animate.spin_in_from_nothing().duration(1.8).easing(Easing.SMOOTH),
        equation.animate.write().duration(2.4).easing(Easing.LINEAR),
    ])
    scene.wait(1.0)
    scene.play([circle.animate.indicate().duration(1.0), equation.animate.indicate().duration(1.0)])
    scene.wait(0.8)
    scene.play([
        square.animate.uncreate().duration(1.4).easing(Easing.SMOOTH),
        circle.animate.shrink_to_center().duration(1.2).easing(Easing.SMOOTH),
        title.animate.unwrite().duration(1.4).easing(Easing.SMOOTH),
        equation.animate.unwrite().duration(1.6).easing(Easing.LINEAR),
    ])
    scene.render()


if __name__ == "__main__":
    main()
