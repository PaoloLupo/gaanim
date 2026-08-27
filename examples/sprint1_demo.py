"""Math and annotation composition using public Scene primitives."""

from gaanim import BLACK, BLUE, CORAL, GOLD, RED, WHITE, YELLOW, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.text("Math and annotations", role="title").fill(WHITE).move_to(0, 260)
    x_axis = scene.geometry.arrow(-420, 0, 420, 0).stroke(WHITE, 3)
    y_axis = scene.geometry.arrow(0, -220, 0, 220).stroke(WHITE, 3)
    vector = scene.geometry.arrow(0, 0, 180, 140).stroke(CORAL, 4)
    vector_label = scene.text("vector").fill(CORAL).move_to(220, 165)
    formula = scene.text.equation("r = sqrt(x^2 + y^2)").fill(GOLD).move_to(-170, -170)
    callout = scene.geometry.rounded_rect(240, 80, 12).stroke(BLUE, 3).no_fill().move_to(260, -170)
    callout_text = scene.text("distance from origin").fill(YELLOW).move_to(260, -170)
    point = scene.geometry.dot(12).fill(RED).move_to(180, 140)

    scene.play([
        title.animate.write().duration(0.8),
        x_axis.animate.create().duration(0.8),
        y_axis.animate.create().duration(0.8),
    ])
    scene.play([
        vector.animate.create().duration(0.9).spring(),
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
