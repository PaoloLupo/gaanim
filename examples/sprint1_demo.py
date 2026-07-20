"""Math and annotation composition using public Scene primitives."""

from gaanim import BLACK, BLUE, CORAL, GOLD, RED, WHITE, YELLOW, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.title("Math and annotations").fill(WHITE).at(0, 260)
    x_axis = scene.arrow(-420, 0, 420, 0).stroke(WHITE, 3)
    y_axis = scene.arrow(0, -220, 0, 220).stroke(WHITE, 3)
    vector = scene.arrow(0, 0, 180, 140).stroke(CORAL, 4)
    vector_label = scene.text("vector").fill(CORAL).at(220, 165)
    formula = scene.equation("r = sqrt(x^2 + y^2)").fill(GOLD).at(-170, -170)
    callout = scene.rounded_rect(240, 80, 12).stroke(BLUE, 3).no_fill().at(260, -170)
    callout_text = scene.text("distance from origin").fill(YELLOW).at(260, -170)
    point = scene.dot(12).fill(RED).at(180, 140)

    scene.play([
        title.write().duration(0.8),
        x_axis.create().duration(0.8),
        y_axis.create().duration(0.8),
    ])
    scene.play([
        vector.create().duration(0.9).spring(),
        point.grow_from_center().duration(0.5),
        vector_label.write().duration(0.6),
        formula.write().duration(1.0),
        callout.create().duration(0.7),
        callout_text.write().duration(0.7),
    ])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
