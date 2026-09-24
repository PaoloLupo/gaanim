"""Animated shadow, glow and blur."""

import os

from gaanim import BLACK, CYAN, GRAY, NAVY, WHITE, Scene


scene = Scene(frame=(16, 9), background="#1e293b")
title = scene.text("glow · blur · shadow animables", role="title").fill(WHITE).move_to(0, 3.6)

card = scene.geometry.rect(3.0, 2.0).fill("#f8fafc").move_to(-4.5, 0.3)
card.shadow(BLACK, 0, -0.05, 0.05)
orb = scene.geometry.circle(0.8).fill(CYAN).move_to(0.0, 0.3)
hero = scene.geometry.regular_polygon(6, 1.0).fill(WHITE).move_to(4.5, 0.3)
hero.blur(0.12)
for x, label in [(-4.5, "shadow + scale_to"), (0.0, "glow().repeat(yoyo)"), (4.5, "blur(0) = blur-in")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -1.8)

scene.play([
    card.animate.shadow(BLACK, 0, -0.3, 0.35).scale_to(1.06).duration(0.8),
    orb.animate.glow(CYAN, radius=0.6, intensity=2.0).duration(0.4).repeat(2, yoyo=True),
    hero.animate.blur(0.0).duration(0.8),
])
scene.wait(0.4)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.4, 1.0])
else:
    scene.render()
