"""Mathematical text, writing, and transforms using Scene."""

from gaanim import Easing, BLACK, BLUE, CORAL, GOLD, WHITE, Scene


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    title = scene.text("Gaanim Vector Engine", role="title").fill(WHITE).move_to(0, 2.75)
    energy = scene.text.equation("E = m c^2").fill(GOLD).move_to(-2.25, 0)
    sum_formula = scene.text.equation("sum_(i=1)^n i = frac(n(n+1), 2)").fill(CORAL).move_to(2.25, 0)
    halo = scene.geometry.circle(1.25).stroke(BLUE, 0.0625).no_fill().move_to(-2.25, 0)

    scene.play([
        title.animate.write().duration(1.0),
        energy.animate.write().duration(2.0).easing(Easing.SMOOTH),
        sum_formula.animate.write().duration(2.0).easing(Easing.LINEAR),
        halo.animate.create().duration(1.2).easing(Easing.spring(stiffness=90.0, damping=12.0)),
    ])
    scene.wait(0.8)
    target = scene.text.equation("p = m v").fill(GOLD).move_to(-2.25, 0)
    scene.play([energy.animate.transform_to(target).duration(1.5).easing(Easing.spring(stiffness=90.0, damping=12.0))])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
