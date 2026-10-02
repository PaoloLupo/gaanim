"""Light analytic physics: a thrown ball that bounces to rest, and flicks that glide to a snap."""

import os

from gaanim import BLUE, CORAL, GOLD, TEAL, WHITE, Scene


scene = Scene(frame=(16, 9))
scene.text("throw · inertia", role="title").fill(WHITE).move_to(0, 3.3)

floor = scene.geometry.line(-7.5, -3.0, 7.5, -3.0).stroke(WHITE, 0.03).opacity(0.4)

# Bounces on the floor line: the ball's lowest point lands at y = -3.
ball = scene.geometry.circle(0.35).fill(GOLD).move_to(-6.5, -1.0)
# Without a floor it lands back at its starting height.
pebble = scene.geometry.circle(0.2).fill(CORAL).move_to(-6.5, 1.8)

# Glides to the nearest notch on the rail.
rail = scene.geometry.line(-1.0, 1.0, 7.0, 1.0).stroke(WHITE, 0.03).opacity(0.4)
for x in (0.0, 2.0, 4.0, 6.0):
    scene.geometry.circle(0.06).fill(WHITE).move_to(x, 1.0)
coin = scene.geometry.circle(0.3).fill(TEAL).move_to(0.0, 1.0)
# A 2D flick snapping to a point.
puck = scene.geometry.rounded_rect(0.6, 0.6, 0.12).fill(BLUE).move_to(1.0, -1.0)

scene.play(
    [
        ball.animate.throw(velocity=(3.0, 4.0), gravity=9.8, floor=-3.0, restitution=0.6),
        pebble.animate.throw(velocity=(2.5, 5.0), gravity=9.8),
        coin.animate.inertia(velocity=7.0, friction=2.5, snap=[0.0, 2.0, 4.0, 6.0]),
        puck.animate.inertia(velocity=(6.0, -1.0), friction=2.0, snap=[(4.5, -2.0), (6.0, 0.0)]),
    ]
)
# A throw continues from where the last one rested.
scene.play(ball.animate.throw(velocity=(-2.0, 3.0), floor=-3.0, restitution=0.3))
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Rising, first apex, after the first bounce, gliding, all resting, second throw.
    scene.snapshots(snapshots, [0.2, 0.45, 1.2, 1.6, 2.6, 3.3, 3.8])
else:
    scene.render()
