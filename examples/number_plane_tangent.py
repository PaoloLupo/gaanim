"""Coordinate axes, a sampled curve, and a tangent built from primitives."""

import math

from gaanim import BLACK, BLUE, GREEN, RED, WHITE, YELLOW, Scene, stagger


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    axes = [
        scene.geometry.line(-5.833333, 0, 5.833333, 0).stroke(WHITE, 0.016667),
        scene.geometry.line(0, -3, 0, 3).stroke(WHITE, 0.016667),
    ]
    points = [(x / 80, 2.0 * math.sin(x / 130) * math.exp(-x / 1200)) for x in range(-620, 621, 40)]
    curve = [
        scene.geometry.line(x1, y1, x2, y2).stroke(BLUE, 0.033333)
        for (x1, y1), (x2, y2) in zip(points, points[1:])
    ]
    tangent = scene.geometry.line(-1, -0.75, 1.833333, 1.375).stroke(YELLOW, 0.033333)
    dot = scene.geometry.dot(0.1).fill(RED).move_to(0.416667, 0.5)
    label = scene.text("tangent").fill(GREEN).move_to(2.333333, 1.458333)

    scene.play([axis.animate.create().duration(0.6) for axis in axes])
    scene.play(stagger(*[segment.animate.create().duration(0.8) for segment in curve], each=0.03))
    scene.play([dot.animate.grow_from_center().duration(0.4), tangent.animate.create().duration(0.8), label.animate.write().duration(0.6)])
    scene.wait(1.0)
    scene.render()


if __name__ == "__main__":
    main()
