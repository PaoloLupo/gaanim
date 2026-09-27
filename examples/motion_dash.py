"""Animated dash offset: marching borders, flowing pipes and a group that stops."""

import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Scene, StrokeStyle, Updater


scene = Scene(frame=(16, 9), background=BLACK)
title = scene.text("Dash offset", role="title").fill(WHITE).move_to(0, 3.6)

border = scene.geometry.rounded_rect(3.6, 2.4, 0.3).no_fill().move_to(-4.6, 0.4)
border.stroke_style(StrokeStyle(GOLD, 0.08, dashes=[0.3, 0.2]))
pipe = scene.geometry.polyline([(-1.8, -0.6), (-0.6, 1.4), (0.6, -0.6), (1.8, 1.4)]).no_fill()
pipe.stroke_style(StrokeStyle(CYAN, 0.12, cap="round", dashes=[0.18, 0.2]))
loops = scene.geometry.group([
    scene.geometry.circle(0.8).no_fill().move_to(x, 0.4).stroke_style(StrokeStyle(CORAL, 0.08, dashes=[0.25, 0.15]))
    for x in (4.0, 5.2)
])
for x, label in [(-4.6, "animate.dash_offset"), (0.0, "Updater.dash_flow"), (4.6, "grupo que se detiene")]:
    scene.text(label).fill(GRAY).scale_by(0.5).move_to(x, -1.6)

pipe.add_updater(Updater.dash_flow(speed=0.8))
loops.add_updater(Updater.dash_flow(speed=-0.6))
scene.play([border.animate.dash_offset(2.0).duration(2.0)])
loops.remove_updater()
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Start, mid-march, loops just stopped, and loops frozen while the pipe flows.
    scene.snapshots(snapshots, [0.0, 1.0, 2.0, 2.8])
else:
    scene.render()
