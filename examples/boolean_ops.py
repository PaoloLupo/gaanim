"""Overlapping shapes with the public Scene primitives.

Vector boolean operations, including a live result that follows its sources.
"""

import os

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.text("Overlapping shapes", role="title").fill(WHITE).move_to(0, 230)
    circle_a = scene.geometry.circle(80).fill(BLUE).opacity(0.7).move_to(-60, 60)
    circle_b = scene.geometry.circle(80).fill(RED).opacity(0.7).move_to(60, 60)
    rect_a = scene.geometry.rect(120, 120).fill(GREEN).opacity(0.7).move_to(-80, -150)
    rect_b = scene.geometry.rect(120, 120).fill(GOLD).opacity(0.7).move_to(0, -150)
    union = scene.geometry.union(circle_a, circle_b).fill(WHITE).opacity(0.25)
    difference = scene.geometry.difference(rect_a, rect_b).fill(WHITE).opacity(0.35)

    scene.play([
        title.animate.write().duration(0.8),
        circle_a.animate.grow_from_center().duration(0.8),
        circle_b.animate.grow_from_center().duration(0.8),
        rect_a.animate.create().duration(0.8),
        rect_b.animate.create().duration(0.8),
        union.animate.fade_in().duration(0.8),
        difference.animate.create().duration(0.8),
    ])
    scene.play([
        union.animate.move_to(200, 0),
        difference.animate.move_to(300, -200),
    ])
    scene.wait(1.0)
    if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
        scene.snapshots(snapshots, [0.2])
    else:
        scene.render()


if __name__ == "__main__":
    main()
