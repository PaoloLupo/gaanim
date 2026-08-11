"""One Layout v2 tree rendered as either 16:9 or 9:16 without manual coordinates."""

import os

from gaanim import BLUE, GOLD, WHITE, Scene


vertical = os.environ.get("GAANIM_VERTICAL") == "1"
scene = Scene(720, 1280, background="#0b1020", margin=48) if vertical else Scene(
    1280, 720, background="#0b1020", margin=48
)


def card(heading: str, copy: str):
    panel = scene.rounded_rect(340, 220, 18).fill("#17233d")
    content = scene.column(
        [
            scene.subtitle(heading).fill(GOLD),
            scene.paragraph(copy).fill(WHITE),
        ],
        width="fill",
        height="fill",
        padding=28,
        gap=20,
        align="stretch",
        justify="center",
    )
    return scene.stack(
        [scene.item(panel, fit="stretch"), content],
        width=340,
        height=220,
        align="stretch",
    )


cards = scene.row(
    [
        card("Measure", "Text rewraps using the width offered by its card."),
        card("Place", "The same row wraps when the vertical viewport is narrower."),
    ],
    width="fill",
    gap=32,
    wrap=True,
    align="center",
    justify="center",
)

page = scene.column(
    [
        scene.title("Responsive layout").fill(GOLD),
        scene.item(cards, grow=1, align="stretch"),
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

scene.play([page.fade_in().duration(0.6)])
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.0, 0.6, 1.2])
else:
    scene.render()
