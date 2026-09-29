"""A broadcast lower third and a corner badge, placed by zones over free footage."""

import os

from gaanim import Direction, Scene, Zones, component

scene = Scene(frame=(16, 9), theme="paper", background="#0b1120", margin=0.5)
L = scene.layout

# "Footage": free objects the layout does not own.
for index, color in enumerate(("#1e3a8a", "#1d4ed8", "#0e7490")):
    scene.geometry.circle(3.2 - index * 0.8).fill(color).opacity(0.35).move_to(3.5 - index * 1.3, 0.8 + index * 0.3)


@component
def lower_third(scene, *, name: str, role: str, accent: str = "#f97316"):
    bar = L.box(width="8px", height="fill", background=accent, radius="4px")
    copy = L.column(
        L.box(name, font_size="40px", weight=700, color="white"),
        L.box(role, font_size="22px", color="#cbd5e1"),
        gap="4px",
    )
    return L.row(bar, copy, gap="18px", padding=("16px", "28px", "16px", "18px"),
                 background="#0f172ae6", radius="10px", align="stretch")


# One reusable template for the whole video: a bottom band and a top-right corner.
frame = L.zones(Zones.grid(rows=["1fr", "220px"], columns=["1fr", "1fr"],
                           names=["top", "corner", "band", "bottom_right"]))
card = lower_third(scene, name="Ana Martínez", role="Directora de investigación")
card.place(frame["band"], anchor="bottom_left")
live = L.row(L.box(width="12px", height="12px", radius="full", background="#ef4444"),
             L.box("EN VIVO", font_size="18px", weight=700, color="white", letter_spacing="2px"),
             gap="8px", align="center", padding=("6px", "14px"), radius="full", background="#00000080")
live.place(frame["corner"], anchor="top_right")

scene.play([card.animate.fade_in_from(Direction.LEFT, distance=0.6).duration(0.6),
            live.animate.fade_in().duration(0.6)])
scene.stop("rotulo")

# The role changes: the card resizes around the new text.
card[1].replace(card[1][1], L.box("Premio Nacional de Ciencia 2025", font_size="22px", color="#fde68a"),
                duration=0.5)
scene.stop("nuevo cargo")

scene.play([card.animate.fade_out().duration(0.5)])

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 0.6, 0.85, 1.1, 1.35])
else:
    scene.render()
