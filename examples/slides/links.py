"""Links: drawables that jump to a backup slide or open a web address when clicked while presenting.

drawable.link_to(segment) follows a link to a segment, and
scene.segment(..., skip=True) makes a backup slide: a presentation's steps
pass over it, and moving on from its end returns to the step the link was
followed from. drawable.link_to("https://...") opens the address in the
browser. The pointer turns into a hand over a link; PDF decks and .gaanim
bundles keep the links, and videos ignore them. Present it with
`gaanim --present links.py` and click the cards.
"""

import os

from gaanim import Scene, Zones, stagger

scene = Scene()
S = scene.slides
L = scene.layout
seeks = []


def code(label):
    return scene.text(label, role="code")


def columns(variants):
    """Each (label, drawable) in its own column of the page, the label under it."""
    shown = []
    for zone, (label, item) in zip(L.zones(Zones.columns(len(variants), gap="48px", padding="page_padding")),
                                   variants):
        rest, bottom = zone.split(Zones.rows(["1fr", "96px"]))
        shown += [item.place(rest), code(label).place(bottom)]
    start = scene.cursor
    scene.play(stagger(*[item.animate.fade_in().duration(0.4) for item in shown], each=0.1))
    return start


# A backup slide is declared like any other, here after the slide that links
# to it; the presentation skips it until a link visits it.
scene.segment("results")
trial = S.card("Trial details", "Click for the full protocol.", width=4.6, variant="accent")
paper = S.card("Published paper", "Opens doi.org in the browser.", width=4.6)
start = columns([
    ('card.link_to(details)', trial),
    ('card.link_to("https://doi.org/...")', paper),
])
scene.stop("cards")
seeks.extend([start + 0.3, scene.cursor - 0.05])

details = scene.segment("details", skip=True)
trial.link_to(details)
paper.link_to("https://doi.org/10.1000/182")
start = columns([
    ('scene.segment("details", skip=True)',
     S.bullets(["Twelve sites, 480 patients", "Double blind, 24 weeks", "Primary end point at week 12"])),
])
scene.stop("protocol")
seeks.append(scene.cursor - 0.05)

# A link may also name a segment as a string, found when the scene builds.
scene.segment("summary")
back = S.chip("Back to the protocol", variant="accent")
back.link_to("details")
start = columns([('chip.link_to("details")', back)])
scene.stop("summary")
seeks.append(scene.cursor - 0.05)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, seeks)
else:
    scene.render()
