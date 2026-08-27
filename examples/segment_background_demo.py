"""Optional color and gradient backgrounds scoped to presentation segments."""

import os

from gaanim import Brush, Scene, Transition, WHITE


scene = Scene(960, 540, background="#111827")

scene.segment("Solid", background="#312e81")
scene.play([scene.text("Fondo sólido", role="title").fill(WHITE).animate.write(0.6)])
scene.wait(0.4)

gradient = Brush.linear(
    ["#0f766e", "#164e63"],
    start=(-480, 0),
    end=(480, 0),
)
scene.segment("Gradient", Transition.cross_fade(0.35), background=gradient)
scene.play([scene.text("Fondo degradado", role="title").fill(WHITE).animate.write(0.6)])
scene.wait(0.4)

# Omitting background returns to the Scene-level background.
scene.segment("Scene default", Transition.cross_fade(0.35))
scene.play([scene.text("Fondo de Scene", role="title").fill(WHITE).animate.write(0.6)])
scene.wait(0.4)

if output := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(output, [0.9, 1.9, 2.9])

scene.render()
