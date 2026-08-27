"""Use the public technical theme with deliberate semantic accents."""

import os

from gaanim import Anchor, BLUE, GOLD, GREEN, Direction, Scene


def main():
    scene = Scene(1280, 720, margin=72)
    scene.canvas.set_theme("technical")
    title = scene.text("Technical visual language", role="title").at(0, 220, anchor=Anchor.CENTER)
    subtitle = scene.text("A reusable scientific theme", role="subtitle").at(0, 155, anchor=Anchor.CENTER)
    circle = scene.geometry.circle(75).fill(GREEN).at(-250, -10)
    square = scene.geometry.rect(150, 150).stroke(GOLD, 6).no_fill().at(250, -10)
    equation = scene.text.equation("H psi = E psi").at(0, -180, anchor=Anchor.CENTER)

    scene.play([
        circle.grow_from_center().duration(1.2).spring(),
        square.create().duration(1.5).smooth(),
        title.write().duration(1.0),
        subtitle.fade_in_from(Direction.DOWN, distance=20).duration(0.4),
        equation.spin_in_from_nothing().duration(1.4).smooth(),
    ])
    scene.wait(1.0)
    circle.fill(BLUE)
    scene.play([circle.indicate().duration(0.8)])
    if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
        scene.snapshots(snapshots, [0.0, 1.0, 2.2])
    else:
        scene.render()


if __name__ == "__main__":
    main()
