"""grow_arrow on reactive connectors: the tip travels the live polyline."""

import os

from gaanim import Anchor, BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene


scene = Scene(frame=(16, 9), background=BLACK)
scene.text("Connector + grow_arrow", role="title").fill(WHITE).move_to(0, 3.6)

source = scene.geometry.rect(2.6, 1.3).no_fill().stroke(CYAN, 0.04).move_to(-5, 1.2)
target = scene.geometry.rect(2.6, 1.3).no_fill().stroke(GOLD, 0.04).move_to(4.5, 1.2)
sink = scene.geometry.rect(2.6, 1.3).no_fill().stroke(CORAL, 0.04).move_to(4.5, -2.2)

direct = scene.geometry.connector(
    source.anchor_point(Anchor.RIGHT),
    target.anchor_point(Anchor.LEFT),
    head_length=0.32,
    head_width=0.28,
    body_width=0.06,
).fill(GOLD)
routed = scene.geometry.connector(
    source.anchor_point(Anchor.BOTTOM),
    sink.anchor_point(Anchor.LEFT),
    via=[source.anchor_point(Anchor.BOTTOM, offset=(0, -3.4))],
    head_length=0.32,
    head_width=0.28,
    body_width=0.06,
).fill(CORAL)
scene.text("tip follows the waypoint; endpoints stay live").fill(GRAY).scale_by(0.5).move_to(0, -3.6)

# The routed arrow grows while its target card is still moving.
scene.play([
    direct.animate.grow_arrow().duration(1.2),
    routed.animate.grow_arrow().duration(1.6),
    sink.animate.shift_by(-1.5, 0.6).duration(1.6),
])
scene.wait(0.3)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 0.8, 1.2, 1.9])
else:
    scene.render()
