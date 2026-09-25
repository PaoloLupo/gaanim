"""Vector segment transitions with easing, plus overlays on the cut.

Every segment lasts 1.5 s and each transition takes 0.8 s, so transition k
starts at 1.5 * k and its midpoint is 1.5 * k + 0.4.
"""

import os

from gaanim import (
    CORAL,
    CYAN,
    GOLD,
    GREEN,
    PURPLE,
    WHITE,
    Easing,
    Overlay,
    Scene,
    Transition,
)


SEGMENT = 1.5
scene = Scene(frame=(16, 9), background="#0f172a")


def card(title: str, subtitle: str, color) -> None:
    """Static content: seeks mid-transition show both segments at rest."""
    scene.geometry.rect(11, 5.2).fill(color).move_to(0, -0.3)
    scene.geometry.circle(1.1).fill(WHITE).move_to(-3.6, 0.2)
    scene.text(title, role="title").fill(WHITE).move_to(0, 3.3)
    scene.text(subtitle).fill(WHITE).move_to(1.2, -0.6)
    scene.wait(SEGMENT)


scene.segment("intro", background="#0f172a")
card("Transiciones", "segmento inicial", "#1e3a8a")

scene.segment("wipe", Transition.wipe(0.8, direction="left", feather=0.15), background="#3b0764")
card("wipe", 'direction="left", feather=0.15', PURPLE)

scene.segment("clock", Transition.clock_wipe(0.8, start_angle=90), background="#052e16")
card("clock_wipe", "start_angle=90", GREEN)

scene.segment("iris", Transition.iris(0.8, center=(2, 1), shape="star"), background="#431407")
card("iris", 'center=(2, 1), shape="star"', CORAL)

scene.segment("blinds", Transition.blinds(0.8, count=8, angle=0), background="#083344")
card("blinds", "count=8, angle=0", CYAN)

scene.segment("push", Transition.push(0.8, direction="up"), background="#422006")
card("push", 'direction="up"', GOLD)

scene.segment(
    "slide",
    Transition.slide(0.8, "left", easing=Easing.spring(bounce=0.2)),
    background="#0f172a",
)
card("slide", "easing=Easing.spring(bounce=0.2)", "#1e3a8a")

scene.segment("flash", Transition.cut(overlay=Overlay.flash(WHITE, 0.3)), background="#3b0764")
card("cut + flash", "Overlay.flash(WHITE, 0.3)", PURPLE)

scene.segment(
    "leak",
    Transition.cross_fade(0.8, overlay=Overlay.light_leak(seed=2, hue=0.1)),
    background="#052e16",
)
card("cross_fade + light_leak", "Overlay.light_leak(seed=2, hue=0.1)", GREEN)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Midpoint of each transition; the flash is sampled just after its cut.
    scene.snapshots(snapshots, [1.9, 3.4, 4.9, 6.4, 7.9, 9.4, 10.55, 12.4])
else:
    scene.render()
