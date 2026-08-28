"""One Layout v2 tree rendered as either 16:9 or 9:16 without manual coordinates."""

import os

from gaanim import BLUE, GOLD, WHITE, Scene


vertical = os.environ.get("GAANIM_VERTICAL") == "1"
scene = Scene(frame=(9, 16), background="#0b1020", margin=0.6) if vertical else Scene(frame=(16, 9), background="#0b1020", margin=0.6
)


def card(heading: str, copy: str):
    panel = scene.geometry.rounded_rect(4.25, 2.75, 0.225).fill("#17233d")
    content = scene.layout.column(
        [
            scene.text(heading, role="subtitle").fill(GOLD),
            scene.text(copy).fill(WHITE),
        ],
        width="fill",
        height="fill",
        padding=0.35,
        gap=0.25,
        align="stretch",
        justify="center",
    )
    return scene.layout.stack(
        [scene.layout.item(panel, fit="stretch"), content],
        width=4.25,
        height=2.75,
        align="stretch",
    )


cards = scene.layout.row(
    [
        card("Measure", "Text rewraps using the width offered by its card."),
        card("Place", "The same row wraps when the vertical viewport is narrower."),
    ],
    width="fill",
    gap=0.4,
    wrap=True,
    align="center",
    justify="center",
)

page = scene.layout.column(
    [
        scene.text("Responsive layout", role="title").fill(GOLD),
        scene.layout.item(cards, grow=1, align="stretch"),
        scene.text("Set GAANIM_VERTICAL=1 for 9:16").fill(BLUE),
    ],
    within="safe",
    width="fill",
    height="fill",
    padding=scene.canvas.layout_token("page_padding"),
    gap=scene.canvas.layout_token("space_lg"),
    align="stretch",
    justify="between",
)

scene.play([page.animate.fade_in().duration(0.6)])
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.2])
else:
    scene.render()
