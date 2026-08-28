"""Public vector masks for individual drawables and nested groups."""

import os

from gaanim import Anchor, Brush, Scene


scene = Scene(frame=(16, 9), margin=0.65)
scene.canvas.set_theme("tokyo-night")
scene.text("Clipping and masks", role="title").move_to(0, 3.5625, anchor=Anchor.CENTER)
scene.text("Any vector drawable can define the visible region", role="subtitle").move_to(0, 2.875, anchor=Anchor.CENTER)

# One large gradient is constrained to a circular viewport.
orb_mask = scene.geometry.circle(1.475).no_fill().no_stroke().move_to(-4.125, 0.0625)
scene.geometry.rect(5.375, 3.0625).fill(
    Brush.linear(
        ["#7DCFFF", "#BB9AF7", "#F7768E"],
        start=(-2.625, -1.5),
        end=(2.625, 1.5),
    )
).no_stroke().move_to(-4.125, 0.0625).clip(orb_mask)
scene.geometry.circle(1.475).no_fill().stroke("#7DCFFF", 0.05).move_to(-4.125, 0.0625)
scene.text("Drawable mask").move_to(-4.125, -1.9375, anchor=Anchor.CENTER)

# The same API reaches every visual leaf of a nested group.
stripes = []
for index, color in enumerate(("#7AA2F7", "#BB9AF7", "#F7768E", "#E0AF68")):
    stripes.append(
        scene.geometry.rect(1.3125, 3.75).fill(color).no_stroke().move_to(1.625 + index * 1.3125, 0.0625)
    )
stripe_group = scene.geometry.group(stripes)
panel_mask = scene.geometry.rounded_rect(4.875, 2.9375, 0.525).no_fill().no_stroke().move_to(3.5875, 0.0625)
stripe_group.clip(panel_mask)
scene.geometry.rounded_rect(4.875, 2.9375, 0.525).no_fill().stroke("#A9B1D6", 0.05).move_to(3.5875, 0.0625)
scene.text("Group mask").move_to(3.5875, -1.9375, anchor=Anchor.CENTER)

# A vector silhouette can be filled by a normalized, animatable level.
drop = scene.geometry.circle(0.9).no_fill().stroke("#A9B1D6", 0.05).move_to(0, -2.9375).opacity(0)
water = scene.geometry.fill_level(drop, "#38BDF8", 0.0)
scene.play([water.animate.fill_level(0.68).duration(0.8)])

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
