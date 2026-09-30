"""Follow-through: an inertial settle after a move and a tail of delayed followers."""

import os

from gaanim import BLUE, CORAL, GOLD, WHITE, Easing, Scene


scene = Scene(frame=(16, 9), background="#0f1729")
scene.text("Follow-through", role="title").fill(WHITE).move_to(0, 3.3)

card = scene.geometry.rounded_rect(2.2, 1.3, 0.18).fill(BLUE).move_to(-5.0, 1.2)
leader = scene.geometry.circle(0.35).fill(GOLD).move_to(-6.0, -2.0)
dots = [scene.geometry.circle(0.28 - 0.035 * i).fill(CORAL) for i in range(6)]
for i, dot in enumerate(dots):
    dot.follow(leader, delay=0.06 * (i + 1))

scene.play([dot.animate.fade_in().duration(0.01) for dot in dots])
scene.play(
    [
        card.animate.move_to(0.0, 1.2).duration(0.5).easing(Easing.SMOOTH).settle(overshoot=0.12, frequency=3.0, decay=6.0),
        leader.animate.move_to(-1.0, -0.5).duration(0.8).easing(Easing.SMOOTH),
    ]
)
scene.play(leader.animate.move_to(5.5, -2.2).duration(0.9).easing(Easing.LINEAR).settle(overshoot=0.05))
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Card overshooting, trail stretched mid-move, leader bouncing, all at rest.
    scene.snapshots(snapshots, [0.3, 0.6, 1.2, 2.1, 2.4, 3.6])
else:
    scene.render()
