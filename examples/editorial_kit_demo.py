"""Gallery for the themed editorial component kit."""

import os

from gaanim import BLACK, Direction, Scene, parallel


scene = Scene(1600, 900, background=BLACK, margin=52)

variants = ["neutral", "accent", "success", "warning", "danger"]
appearances = ["soft", "solid", "outline", "soft", "solid"]

heading = scene.slides.section_header(
    "Editorial components",
    kicker="GAANIM UI KIT",
    subtitle="Semantic variants · themed typography · ordinary Drawables",
    width=1180,
    align="center",
    variant="accent",
).move_to(0,315)

badges = [
    scene.slides.badge(name.upper(), variant=name, appearance=appearance).move_to(-520 + i * 260, 130)
    for i, (name, appearance) in enumerate(zip(variants, appearances))
]
chips = [
    scene.slides.chip(name.title(), variant=name, appearance=appearances[(i + 1) % 3]).move_to(
        -520 + i * 260, 25
    )
    for i, name in enumerate(variants)
]

compact = scene.geometry.group([heading, *badges, *chips])
scene.play([
    heading.animate.fade_in_from(Direction.DOWN, distance=28),
    *[item.animate.grow_from_center() for item in badges],
])
scene.play([item.animate.fade_in() for item in chips])
scene.wait(0.6)
scene.play([compact.animate.fade_out().duration(0.3)])

card = scene.slides.card(
    "Measured card",
    "Body text wraps inside the authored width without guessed heights.",
    "Theme role: caption",
    variant="accent",
).move_to(-500, 30)
stat = scene.slides.stat_card(
    "98%", "Accuracy", delta="+4.2%", variant="success", appearance="solid"
).move_to(0, 30)
quote = scene.slides.quote_card(
    "Clarity turns motion into explanation.",
    "Gaanim",
    width=500,
    variant="warning",
    appearance="outline",
).move_to(500, 30)
cards = scene.geometry.group([card, stat, quote])
scene.play(parallel(card.animate.fade_in(), stat.animate.grow_from_center(), quote.animate.fade_in()))
scene.wait(0.7)
scene.play([cards.animate.fade_out().duration(0.3)])

banner = scene.slides.banner(
    "Safe-area banner",
    "Auto-height and centered semantic typography",
    variant="accent",
)
lower = scene.slides.lower_third(
    "Ada Lovelace",
    "Mathematician · analytical engine",
    kicker="SPEAKER",
    width=620,
    variant="danger",
    appearance="outline",
)
scene.play([
    banner.animate.fade_in_from(Direction.DOWN, distance=24),
    lower.animate.fade_in_from(Direction.DOWN, distance=24),
])
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [2.2, 4.3, 6.0])
else:
    scene.render()
