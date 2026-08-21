"""Overlapping shapes with the public Scene primitives.

Vector boolean operations, including a live result that follows its sources.
"""

import os

from gaanim import BLACK, BLUE, GOLD, GREEN, RED, WHITE, Scene


def main():
    scene = Scene(1280, 720, background=BLACK)
    title = scene.text("Overlapping shapes", role="title").fill(WHITE).at(0, 230)
    circle_a = scene.circle(80).fill(BLUE).opacity(0.7).at(-60, 60)
    circle_b = scene.circle(80).fill(RED).opacity(0.7).at(60, 60)
    rect_a = scene.rect(120, 120).fill(GREEN).opacity(0.7).at(-80, -150)
    rect_b = scene.rect(120, 120).fill(GOLD).opacity(0.7).at(0, -150)
    union = scene.union(circle_a, circle_b).fill(WHITE).opacity(0.25)
    difference = scene.difference(rect_a, rect_b).fill(WHITE).opacity(0.35)

    scene.play([
        title.write().duration(0.8),
        circle_a.grow_from_center().duration(0.8),
        circle_b.grow_from_center().duration(0.8),
        rect_a.create().duration(0.8),
        rect_b.create().duration(0.8),
        union.fade_in().duration(0.8),
        difference.create().duration(0.8),
    ])
    scene.play([
        union.move_to(200, 0),
        difference.move_to(300, -200),
    ])
    scene.wait(1.0)
    if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
        scene.snapshots(snapshots, [0.2])
    else:
        scene.render()


if __name__ == "__main__":
    main()
