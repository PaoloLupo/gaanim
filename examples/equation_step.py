"""A derivation step that preserves the glyphs shared by both equations."""

import os

from gaanim import Anchor, BLACK, GOLD, RED, WHITE, Scene, part


scene = Scene(1920, 1080, background=BLACK)
title = scene.text("Resolver paso a paso", role="title").fill(WHITE).move_to(0, 220, anchor=Anchor.CENTER)
before = scene.text.equation(
    part("variable", "x"), "dot 5 =", part("factor", "25")
).move_to(0, 0, anchor=Anchor.CENTER).scale_to(2)
after = scene.text.equation(
    part("variable", "x"), "=", part("result", "5")
).move_to(0, 0, anchor=Anchor.CENTER).scale_to(2)
before["factor"].fill(RED)
after["result"].fill(GOLD)

scene.play([title.animate.write(), before.animate.write()])
scene.wait(0.4)
scene.play([before.animate.transform_to(after).duration(0.8)])
scene.play([after["result"].animate.indicate().duration(0.45)])
scene.wait(0.4)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scene.snapshots(snapshot_dir, [0.0, 1.4, 1.8, 2.2, 2.65])

scene.render()
