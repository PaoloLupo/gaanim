"""Solid colors and gradients share the same Drawable fill/stroke API."""

import os

from gaanim import Brush, Scene


scene = Scene(1280, 720, margin=48)
scene.canvas.set_theme("tokyo-night")

headline = Brush.linear(
    ["#7DCFFF", "#BB9AF7", "#F7768E"],
    start=(-330, 0),
    end=(330, 0),
)
scene.title("Brush: one paint API").fill(headline).at(0, 275)
scene.subtitle("Linear, radial and sweep gradients rendered by Vello").at(0, 220)

linear = Brush.linear(
    ["#7AA2F7", "#BB9AF7", "#F7768E"],
    start=(-230, 0),
    end=(230, 0),
)
scene.rounded_rect(460, 185, 28).fill(linear).no_stroke().at(-315, 55)
scene.text("Linear").fill("white").at(-315, 55)

radial = Brush.radial(
    ["#E0AF68", "#FF9E64", "#F7768E"],
    center=(-25, 30),
    radius=125,
)
scene.circle(100).fill(radial).no_stroke().at(300, 55)
scene.text("Radial").fill("#1A1B26").at(300, 55)

sweep = Brush.sweep(
    ["#7DCFFF", "#9ECE6A", "#E0AF68", "#F7768E", "#7DCFFF"],
    center=(0, 0),
)
scene.circle(105).fill("#1A1B26").stroke(sweep, 22).at(-230, -190)
scene.text("Sweep stroke").at(-230, -190)

repeat = Brush.linear(
    ["#7AA2F7", "#1A1B26"],
    start=(-40, 0),
    end=(40, 0),
    extend="reflect",
)
scene.rounded_rect(460, 145, 24).fill(repeat).stroke(
    scene.canvas.color("rule"), 2
).at(315, -190)
scene.text("Reflect").at(315, -190)

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
