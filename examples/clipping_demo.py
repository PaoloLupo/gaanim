"""Public vector masks for individual drawables and nested groups."""

import os

from gaanim import Brush, Scene


scene = Scene(1280, 720, margin=52)
scene.canvas.set_theme("tokyo-night")
scene.title("Clipping and masks").at(0, 285)
scene.subtitle("Any vector drawable can define the visible region").at(0, 230)

# One large gradient is constrained to a circular viewport.
orb_mask = scene.circle(118).no_fill().no_stroke().at(-330, 5)
scene.rect(430, 245).fill(
    Brush.linear(
        ["#7DCFFF", "#BB9AF7", "#F7768E"],
        start=(-210, -120),
        end=(210, 120),
    )
).no_stroke().at(-330, 5).clip(orb_mask)
scene.circle(118).no_fill().stroke("#7DCFFF", 4).at(-330, 5)
scene.text("Drawable mask").at(-330, -155)

# The same API reaches every visual leaf of a nested group.
stripes = []
for index, color in enumerate(("#7AA2F7", "#BB9AF7", "#F7768E", "#E0AF68")):
    stripes.append(
        scene.rect(105, 300).fill(color).no_stroke().at(130 + index * 105, 5)
    )
stripe_group = scene.group(stripes)
panel_mask = scene.rounded_rect(390, 235, 42).no_fill().no_stroke().at(287, 5)
stripe_group.clip(panel_mask)
scene.rounded_rect(390, 235, 42).no_fill().stroke("#A9B1D6", 4).at(287, 5)
scene.text("Group mask").at(287, -155)

scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.2])
else:
    scene.render()
