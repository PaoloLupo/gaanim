"""A broadcast lower third, a live badge and a news ticker over drifting footage.

The footage is free drawables that no layout owns: soft lights drifting
behind everything. One zone template divides the frame for the whole
programme, title-safe: the lower third sits in the bottom band, the live badge
in the top-right corner and the ticker along the bottom edge. The card slides
in with its accent bar, its caption is retyped when the story changes, and it
slides away while the ticker keeps crawling.
"""

import os

from gaanim import Anchor, Direction, Easing, Scene, Zones, component

scene = Scene(background="#0b1120")
L = scene.layout

ORANGE = "#f97316"
WHITE = "#ffffff"
SILVER = "#cbd5e1"
GLASS = "#0f172ae6"
LENGTH = 7.5

# The footage: blurred lights at seeded places, drifting slowly to the right.
rng = scene.random(seed=7)
lights = []
for index in range(14):
    color = ("#1e3a8a", "#1d4ed8", "#0e7490", "#7c3aed")[index % 4]
    light = scene.geometry.circle(rng.uniform(0.5, 1.8)).fill(color).opacity(rng.uniform(0.2, 0.45))
    lights.append(light.blur(0.25).move_to(rng.uniform(-9, 7), rng.uniform(-3, 4.5)))
scene.launch([light.animate.shift_by(1.2 + 0.1 * index, 0).duration(LENGTH).easing(Easing.LINEAR)
              for index, light in enumerate(lights)])

# One template for the whole programme: a picture area inset to the title-safe
# margins over a ticker band along the bottom edge.
frame = L.zones(Zones.rows(["1fr", "72px"], names=["picture", "ticker"], padding=0), within="frame")
safe = frame["picture"].split(Zones.grid(rows=["1fr", "240px"], columns=["1fr", "1fr"],
                                          names=["top", "corner", "band", "spare"], padding=("64px", "96px")))


@component
def lower_third(scene, *, name: str, role: str, accent: str = ORANGE):
    bar = L.box(width="12px", height="fill", background=accent, radius="6px")
    # The caption is a Text of its own, so it can be retyped in place; the
    # card reserves room for the longest caption it will show.
    caption = scene.text(role, size=0.34, color=SILVER)
    copy = L.column(L.box(name, font_size="64px", weight=700, color=WHITE), caption, gap="8px", min_width="780px")
    card = L.row(bar, copy, gap="28px", padding=("24px", "48px", "24px", "28px"), background=GLASS,
                 radius="16px", align="stretch")
    return card, bar, caption


card, bar, caption = lower_third(scene, name="Ana Martínez", role="Director of research")
card.place(safe["band"], anchor="bottom_left")

beacon = L.box(width="24px", height="24px", radius="full", background="#ef4444")
live = L.row(beacon, L.box("LIVE", font_size="40px", weight=700, color=WHITE, letter_spacing="6px"),
             gap="16px", align="center", padding=("12px", "28px"), radius="full", background="#00000099")
live.place(safe["corner"], anchor="top_right")

# The ticker: an orange label and a dark crawl band along the bottom edge. The
# band is also the mask that clips the headlines sliding through it.
label_zone, crawl_zone = frame["ticker"].split(Zones.columns(["240px", "1fr"]))
HEADLINES = "   ·   ".join([
    "Science prize goes to coastal ecology research",
    "Rail line reopens after a year of repairs",
    "City library extends weekend hours",
    "Markets close higher on energy shares",
])
crawl = scene.geometry.rect(crawl_zone.width, crawl_zone.height).fill("#020617").move_to(*crawl_zone.center)
headlines = scene.text(HEADLINES, size=0.32, color=WHITE, wrap=False)
headlines.move_to(crawl_zone.left + 0.4, crawl_zone.center[1], anchor=Anchor.LEFT).clip(crawl)
ticker = scene.geometry.group([
    scene.geometry.rect(label_zone.width, label_zone.height).fill(ORANGE).move_to(*label_zone.center),
    scene.text("NEWS", size=0.32, weight=700, color=WHITE).move_to(*label_zone.center),
    crawl,
    headlines,
])

scene.play([live.animate.fade_in().duration(0.6), ticker.animate.fade_in_from(Direction.DOWN, 0.3).duration(0.6)])
scene.launch(beacon.animate.opacity(0.25).duration(0.5).repeat(14, yoyo=True))
scene.launch(headlines.animate.shift_by(-14, 0).duration(LENGTH - 0.6).easing(Easing.LINEAR))
scene.play([card.animate.fade_in_from(Direction.LEFT, distance=0.8).duration(0.7).easing(Easing.SMOOTH),
            bar.animate.grow_from_edge(Direction.DOWN).duration(0.5)])
scene.wait(1.2)
scene.stop("lower third")

# The story changes: the caption is deleted back and typed again.
scene.play(caption.animate.retype("National Science Prize 2025", cps=26))
caption.fill("#fde68a")
scene.wait(1.2)
scene.stop("new caption")

scene.play(card.animate.fade_out().duration(0.5))
scene.wait(LENGTH - scene.cursor)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 0.9, 1.6, 2.7, 3.3, 4.6, 5.6, 7.0])
else:
    scene.render()
