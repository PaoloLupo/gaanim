"""Solid colors and gradients share the same Drawable fill/stroke API."""

import os

from gaanim import Anchor, Brush, Scene


scene = Scene(frame=(16, 9), margin=0.6)
scene.canvas.set_theme("tokyo-night")

headline = Brush.linear(
    ["#7DCFFF", "#BB9AF7", "#F7768E"],
    start=(-4.125, 0),
    end=(4.125, 0),
)
scene.text("Brush: one paint API", role="title").fill(headline).move_to(0, 3.4375, anchor=Anchor.CENTER)
scene.text("Linear, radial and sweep gradients rendered by Vello", role="subtitle").move_to(0, 2.75, anchor=Anchor.CENTER)

linear = Brush.linear(
    ["#7AA2F7", "#BB9AF7", "#F7768E"],
    start=(-2.875, 0),
    end=(2.875, 0),
)
scene.geometry.rounded_rect(5.75, 2.3125, 0.35).fill(linear).no_stroke().move_to(-3.9375, 0.6875)
scene.text("Linear").fill("white").move_to(-3.9375, 0.6875, anchor=Anchor.CENTER)

radial = Brush.radial(
    ["#E0AF68", "#FF9E64", "#F7768E"],
    center=(-0.3125, 0.375),
    radius=1.5625,
)
scene.geometry.circle(1.25).fill(radial).no_stroke().move_to(3.75, 0.6875)
scene.text("Radial").fill("#1A1B26").move_to(3.75, 0.6875, anchor=Anchor.CENTER)

sweep = Brush.sweep(
    ["#7DCFFF", "#9ECE6A", "#E0AF68", "#F7768E", "#7DCFFF"],
    center=(0, 0),
)
scene.geometry.circle(1.3125).fill("#1A1B26").stroke(sweep, 0.275).move_to(-2.875, -2.375)
scene.text("Sweep stroke").move_to(-2.875, -2.375, anchor=Anchor.CENTER)

repeat = Brush.linear(
    ["#7AA2F7", "#1A1B26"],
    start=(-0.5, 0),
    end=(0.5, 0),
    extend="reflect",
)
scene.geometry.rounded_rect(5.75, 1.8125, 0.3).fill(repeat).stroke(
    scene.canvas.color("rule"), 0.025
).move_to(3.9375, -2.375)
scene.text("Reflect").move_to(3.9375, -2.375, anchor=Anchor.CENTER)

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
