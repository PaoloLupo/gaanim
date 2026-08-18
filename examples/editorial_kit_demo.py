"""Gallery for the themed editorial component kit."""

import os

from gaanim import BLACK, AnimationGroup, Direction, Scene


scene = Scene(1600, 900, background=BLACK, margin=52)

variants = ["neutral", "accent", "success", "warning", "danger"]
appearances = ["soft", "solid", "outline", "soft", "solid"]

heading = scene.section_header(
    "Editorial components",
    kicker="GAANIM UI KIT",
    subtitle="Semantic variants · themed typography · ordinary Drawables",
    width=1180,
    align="center",
    variant="accent",
).at(0,315)

badges = [
    scene.badge(name.upper(), variant=name, appearance=appearance).at(-520 + i * 260, 130)
    for i, (name, appearance) in enumerate(zip(variants, appearances))
]
chips = [
    scene.chip(name.title(), variant=name, appearance=appearances[(i + 1) % 3]).at(
        -520 + i * 260, 25
    )
    for i, name in enumerate(variants)
]

compact = scene.group([heading, *badges, *chips])
scene.play([
    heading.fade_in_from(Direction.DOWN, distance=28),
    *[item.grow_from_center() for item in badges],
])
scene.play([item.fade_in() for item in chips])
scene.wait(0.6)
scene.play([compact.fade_out().duration(0.3)])

card = scene.card(
    "Measured card",
    "Body text wraps inside the authored width without guessed heights.",
    "Theme role: caption",
    variant="accent",
).at(-500, 30)
stat = scene.stat_card(
    "98%", "Accuracy", delta="+4.2%", variant="success", appearance="solid"
).at(0, 30)
quote = scene.quote_card(
    "Clarity turns motion into explanation.",
    "Gaanim",
    width=500,
    variant="warning",
    appearance="outline",
).at(500, 30)
cards = scene.group([card, stat, quote])
scene.play(AnimationGroup(card.fade_in(), stat.grow_from_center(), quote.fade_in()))
scene.wait(0.7)
scene.play([cards.fade_out().duration(0.3)])

banner = scene.banner(
    "Safe-area banner",
    "Auto-height and centered semantic typography",
    variant="accent",
)
lower = scene.lower_third(
    "Ada Lovelace",
    "Mathematician · analytical engine",
    kicker="SPEAKER",
    width=620,
    variant="danger",
    appearance="outline",
)
scene.play([
    banner.fade_in_from(Direction.DOWN, distance=24),
    lower.fade_in_from(Direction.DOWN, distance=24),
])
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [2.2, 4.3, 6.0])
else:
    scene.render()
