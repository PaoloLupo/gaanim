"""Math and annotation composition using public Scene primitives."""

from gaanim import Easing, BLACK, BLUE, CORAL, GOLD, RED, WHITE, YELLOW, Scene


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    title = scene.text("Math and annotations", role="title").fill(WHITE).move_to(0, 3.25)
    x_axis = scene.geometry.arrow(-5.25, 0, 5.25, 0).stroke(WHITE, 0.0375)
    y_axis = scene.geometry.arrow(0, -2.75, 0, 2.75).stroke(WHITE, 0.0375)
    vector = scene.geometry.arrow(0, 0, 2.25, 1.75).stroke(CORAL, 0.05)
    vector_label = scene.text("vector").fill(CORAL).move_to(2.75, 2.0625)
    formula = scene.text.equation("r = sqrt(x^2 + y^2)").fill(GOLD).move_to(-2.125, -2.125)
    callout = scene.geometry.rounded_rect(3, 1, 0.15).stroke(BLUE, 0.0375).no_fill().move_to(3.25, -2.125)
    callout_text = scene.text("distance from origin").fill(YELLOW).move_to(3.25, -2.125)
    point = scene.geometry.dot(0.15).fill(RED).move_to(2.25, 1.75)

    scene.play([
        title.animate.write().duration(0.8),
        x_axis.animate.create().duration(0.8),
        y_axis.animate.create().duration(0.8),
    ])
    scene.play([
        vector.animate.create().duration(0.9).easing(Easing.spring(stiffness=90.0, damping=12.0)),
        point.animate.grow_from_center().duration(0.5),
        vector_label.animate.write().duration(0.6),
        formula.animate.write().duration(1.0),
        callout.animate.create().duration(0.7),
        callout_text.animate.write().duration(0.7),
    ])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
