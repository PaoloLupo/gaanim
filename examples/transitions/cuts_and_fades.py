"""Cuts and fades: cut, cross_fade, fade_through and zoom_through, with flash and light-leak overlays.

A transition belongs to the segment it enters, ``scene.segment(name,
transition)``, and plays at its start without lengthening the scene. Every
segment here shows one card; the label below the cards is the transition that
brought the current card in.
"""

import os

from gaanim import Overlay, Scene, Transition, Zones

HOLD = 1.6

scene = Scene()
background = scene.canvas.color("background")
accent = scene.canvas.color("accent")
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))
ROLES = ["accent", "success", "chart", "danger"]
SHAPES = [
    lambda: scene.geometry.circle(1.0),
    lambda: scene.geometry.square(1.6),
    lambda: scene.geometry.regular_polygon(3, 1.0),
    lambda: scene.geometry.star(5, 1.0, 0.45),
    lambda: scene.geometry.regular_polygon(6, 1.0),
]
captions = []


def card(index):
    """Fill the stage with card ``index``: a theme color holding a large dark shape."""
    stage = page[0]
    color = scene.canvas.color(ROLES[index % len(ROLES)])
    scene.geometry.rounded_rect(stage.width, stage.height, 0.3).fill(color).place(stage)
    SHAPES[index % len(SHAPES)]().fill(background).place(stage.inset("96px"), fit="contain")


def caption(*lines):
    """Show ``lines`` of code below the stage from the cursor on, in place of the previous caption.

    Persistent drawables stay out of transitions, so the caption reads through
    them; hud() also keeps it still, on its strip of background, while
    zoom_through moves the camera.
    """
    for label in captions[-1:]:
        label.opacity(0)
    strip = page[1].inset(("-12px", "-120px", "-120px"))
    rows = page[1].split(Zones.rows(len(lines)))
    label = scene.geometry.group(
        [scene.geometry.rect(strip.width, strip.height).fill(background).place(strip)]
        + [scene.text(line, role="code").place(row, anchor="left") for line, row in zip(lines, rows)]
    )
    scene.persist(label.hud())
    captions.append(label)


def enter(code, transition):
    """Caption ``code`` (one line or a list of lines), start the next card's segment with ``transition``.

    Returns the segment start.
    """
    caption(*([code] if isinstance(code, str) else code))
    start = scene.cursor
    scene.segment(f"card {len(captions)}", transition)
    card(len(captions))
    scene.wait(HOLD)
    return start


scene.segment("card 0")
card(0)
scene.wait(HOLD)

# A cut switches at once; an overlay is drawn above it, centered on the cut
# (on the midpoint of any other transition). flash rises and fades over its
# duration, white unless given a color.
cut = enter("Transition.cut()", Transition.cut())
flash = enter("Transition.cut(overlay=Overlay.flash())", Transition.cut(overlay=Overlay.flash()))
tinted = enter("Transition.cut(overlay=Overlay.flash(accent, 0.6))", Transition.cut(overlay=Overlay.flash(accent, 0.6)))

# cross_fade blends the two segments; light_leak drifts soft, screen-blended
# light around the midpoint (seed picks the layout, hue is in turns: 0.1 is
# warm orange, 0.6 blue; intensity goes up to 4).
fade = enter("Transition.cross_fade(0.8)", Transition.cross_fade(0.8))
leak = enter("Transition.cross_fade(0.8, overlay=Overlay.light_leak())", Transition.cross_fade(0.8, overlay=Overlay.light_leak()))
blue = Overlay.light_leak(seed=3, hue=0.6, intensity=2)
blue_leak = enter(
    ["blue = Overlay.light_leak(seed=3, hue=0.6, intensity=2)", "Transition.cross_fade(0.8, overlay=blue)"],
    Transition.cross_fade(0.8, overlay=blue),
)

# fade_through fades out to a color in the first half and in from it in the
# second; zoom_through zooms the camera into the outgoing segment and back out
# of the incoming one. sound= plays a file from assets/ as the transition starts.
through = enter("Transition.fade_through(0.8, background)", Transition.fade_through(0.8, background))
zoom = enter('Transition.zoom_through(0.8, sound="whoosh.wav")', Transition.zoom_through(0.8, sound="whoosh.wav"))
zoom_off_center = enter(
    "Transition.zoom_through(0.8, center=(4, 1), max_zoom=8)",
    Transition.zoom_through(0.8, center=(4, 1), max_zoom=8),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Just after each cut, while the flashes fade, and partway through every
    # other transition.
    scene.snapshots(
        snapshots,
        [cut + 0.2, flash + 0.06, tinted + 0.12]
        + [start + 0.4 for start in (fade, leak, blue_leak)]
        + [through + 0.25, zoom + 0.25, zoom_off_center + 0.25],
    )
else:
    scene.render()
