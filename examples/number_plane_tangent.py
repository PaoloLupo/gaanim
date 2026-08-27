"""Coordinate axes, a sampled curve, and a tangent built from primitives."""

import math

from gaanim import BLACK, BLUE, GREEN, RED, WHITE, YELLOW, Scene, stagger


def main():
    scene = Scene(1920, 1080, background=BLACK)
    axes = [
        scene.geometry.line(-700, 0, 700, 0).stroke(WHITE, 2),
        scene.geometry.line(0, -360, 0, 360).stroke(WHITE, 2),
    ]
    points = [(x, 160 * math.sin(x / 130) * math.exp(-x / 1200)) for x in range(-620, 621, 40)]
    curve = [
        scene.geometry.line(x1, y1, x2, y2).stroke(BLUE, 4)
        for (x1, y1), (x2, y2) in zip(points, points[1:])
    ]
    tangent = scene.geometry.line(-120, -90, 220, 165).stroke(YELLOW, 4)
    dot = scene.geometry.dot(12).fill(RED).move_to(50, 60)
    label = scene.text("tangent").fill(GREEN).move_to(280, 175)

    scene.play([axis.animate.create().duration(0.6) for axis in axes])
    scene.play(stagger(*[segment.animate.create().duration(0.8) for segment in curve], each=0.03))
    scene.play([dot.animate.grow_from_center().duration(0.4), tangent.animate.create().duration(0.8), label.animate.write().duration(0.6)])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
