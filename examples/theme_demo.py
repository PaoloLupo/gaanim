"""Use the public technical theme with deliberate semantic accents."""

import os

from gaanim import Easing, Anchor, BLUE, GOLD, GREEN, Direction, Scene


def main():
    scene = Scene(frame=(16, 9), margin=0.9)
    scene.canvas.set_theme("technical")
    title = scene.text("Technical visual language", role="title").move_to(0, 2.75, anchor=Anchor.CENTER)
    subtitle = scene.text("A reusable scientific theme", role="subtitle").move_to(0, 1.9375, anchor=Anchor.CENTER)
    circle = scene.geometry.circle(0.9375).fill(GREEN).move_to(-3.125, -0.125)
    square = scene.geometry.rect(1.875, 1.875).stroke(GOLD, 0.075).no_fill().move_to(3.125, -0.125)
    equation = scene.text.equation("H psi = E psi").move_to(0, -2.25, anchor=Anchor.CENTER)

    scene.play([
        circle.animate.grow_from_center().duration(1.2).easing(Easing.spring(stiffness=90.0, damping=12.0)),
        square.animate.create().duration(1.5).easing(Easing.SMOOTH),
        title.animate.write().duration(1.0),
        subtitle.animate.fade_in_from(Direction.DOWN, distance=0.25).duration(0.4),
        equation.animate.spin_in_from_nothing().duration(1.4).easing(Easing.SMOOTH),
    ])
    scene.wait(1.0)
    circle.fill(BLUE)
    scene.play([circle.animate.indicate().duration(0.8)])
    if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
        scene.snapshots(snapshots, [0.0, 1.0, 2.2])
    else:
        scene.render()


if __name__ == "__main__":
    main()
