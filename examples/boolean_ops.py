"""Overlapping shapes with the public Scene primitives.

Vector boolean operations, including a live result that follows its sources.
"""

import os

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene


def main():
    scene = Scene(frame=(16, 9), background=BLACK)
    title = scene.text("Overlapping shapes", role="title").fill(WHITE).move_to(0, 2.875)
    circle_a = scene.geometry.circle(1).fill(BLUE).opacity(0.7).move_to(-0.75, 0.75)
    circle_b = scene.geometry.circle(1).fill(RED).opacity(0.7).move_to(0.75, 0.75)
    rect_a = scene.geometry.rect(1.5, 1.5).fill(GREEN).opacity(0.7).move_to(-1, -1.875)
    rect_b = scene.geometry.rect(1.5, 1.5).fill(GOLD).opacity(0.7).move_to(0, -1.875)
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
        union.animate.move_to(2.5, 0),
        difference.animate.move_to(3.75, -2.5),
    ])
    scene.wait(1.0)
    if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
        scene.snapshots(snapshots, [0.2])
    else:
        scene.render()


if __name__ == "__main__":
    main()
