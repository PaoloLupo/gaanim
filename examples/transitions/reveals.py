"""Vector reveals: wipe, clock_wipe, iris, blinds, push and slide, with their directions and shapes.

These transitions clip both segments with animated vector paths, so they
stay sharp at any resolution; without ``easing=`` they run on
``Easing.SMOOTH``. Every segment shows one card, and the label below the cards
is the transition that brought the current card in.
"""

import os

from gaanim import Easing, Scene, Transition, Zones

HOLD = 1.4

scene = Scene()
background = scene.canvas.color("background")
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


def caption(code):
    """Show ``code`` below the stage from the cursor on, in place of the previous caption.

    Persistent drawables stay out of transitions, so the caption, on its strip
    of background, is neither clipped nor pushed.
    """
    for label in captions[-1:]:
        label.opacity(0)
    strip = page[1].inset(("-12px", "-120px", "-120px"))
    label = scene.geometry.group([
        scene.geometry.rect(strip.width, strip.height).fill(background).place(strip),
        scene.text(code, role="code").place(page[1], anchor="left"),
    ])
    scene.persist(label)
    captions.append(label)


def enter(code, transition):
    """Caption ``code`` and start the next card's segment with ``transition``; return its start."""
    caption(code)
    start = scene.cursor
    scene.segment(f"card {len(captions)}", transition)
    card(len(captions))
    scene.wait(HOLD)
    return start


scene.segment("card 0")
card(0)
scene.wait(HOLD)
starts = []

# wipe sweeps a straight edge in `direction` ("left" starts revealing at the
# right; diagonals such as "up_right" work too), softened by `feather`, a
# fraction of the travel. clock_wipe sweeps a hand clockwise from start_angle,
# in degrees counter-clockwise from +x.
starts.append(enter("Transition.wipe(0.8)", Transition.wipe(0.8)))
starts.append(enter('Transition.wipe(0.8, "down", feather=0)', Transition.wipe(0.8, "down", feather=0)))
starts.append(enter('Transition.wipe(0.8, "up_right", feather=0.5)', Transition.wipe(0.8, "up_right", feather=0.5)))
starts.append(enter("Transition.clock_wipe(0.8)", Transition.clock_wipe(0.8)))
starts.append(enter("Transition.clock_wipe(0.8, start_angle=0)", Transition.clock_wipe(0.8, start_angle=0)))

# iris opens a shape from `center` (scene units) until it fills the frame.
# Any drawable's outline works as the shape; the drawable itself still draws,
# so a stencil is made invisible.
starts.append(enter("Transition.iris(0.8)", Transition.iris(0.8)))
starts.append(enter('Transition.iris(0.8, center=(-4, 1), shape="diamond")', Transition.iris(0.8, center=(-4, 1), shape="diamond")))
starts.append(enter('Transition.iris(0.8, shape="square")', Transition.iris(0.8, shape="square")))
starts.append(enter('Transition.iris(0.8, shape="star")', Transition.iris(0.8, shape="star")))
arrow = scene.geometry.polygon([(-1, 0.3), (0.2, 0.3), (0.2, 0.7), (1, 0), (0.2, -0.7), (0.2, -0.3), (-1, -0.3)])
arrow.opacity(0)
arrow_start = enter("Transition.iris(0.8, shape=arrow)", Transition.iris(0.8, shape=arrow))

# blinds opens `count` slats together, tilted by `angle` degrees.
starts.append(enter("Transition.blinds(0.8)", Transition.blinds(0.8)))
starts.append(enter("Transition.blinds(0.8, count=4, angle=45)", Transition.blinds(0.8, count=4, angle=45)))

# push moves both segments one frame in `direction`; slide moves only the
# incoming one, over the still outgoing segment. Any Easing reshapes the
# progress, springs included.
starts.append(enter("Transition.push(0.8)", Transition.push(0.8)))
starts.append(enter('Transition.push(0.8, "left")', Transition.push(0.8, "left")))
starts.append(enter('Transition.slide(0.8, "left")', Transition.slide(0.8, "left")))
spring_start = enter(
    'Transition.slide(0.8, "down", easing=Easing.spring(bounce=0.4))',
    Transition.slide(0.8, "down", easing=Easing.spring(bounce=0.4)),
)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Halfway through every transition; earlier for the arrow, whose outline
    # covers the frame sooner, and the spring, which lands early.
    scene.snapshots(snapshots, sorted([start + 0.4 for start in starts] + [arrow_start + 0.25, spring_start + 0.2]))
else:
    scene.render()
