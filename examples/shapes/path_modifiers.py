"""Path modifiers: zig zag, round corners, pucker and bloat, twist, wiggle and offset.

Modifiers reshape a drawable's path without changing the drawable: each one
is added from the cursor on and returns a handle whose numbers animate. The
segments cover edges, warps, offsets and how modifiers stack and combine
with text and trims. Each label is the code that adds and animates the
modifier.
"""

import os

from gaanim import Scene, Zones

PLAY = 1.4
HOLD = 0.6

scene = Scene()
accent = scene.colors.accent


def table(variants):
    """Place each (label, drawable) pair in a row, the label right-aligned before it."""
    grid = scene.layout.zones(
        Zones.grid(rows=len(variants), columns=["3fr", "1fr"], gap="24px", padding="page_padding")
    )
    for index, (label, drawable) in enumerate(variants):
        scene.text(label, role="code").place(grid[2 * index], anchor="right")
        drawable.place(grid[2 * index + 1])


def play(animations):
    """Play the modifier animations of a segment, then hold."""
    scene.play(animations, duration=PLAY)
    scene.wait(HOLD)


# Zig zag puts ridges on every segment, as corners or, smooth, as waves;
# round_corners rounds the corners between straight segments.
scene.segment("edges")
star = scene.geometry.star(5, 0.8, 0.35)
teeth = star.modifiers.zigzag(0, ridges=3)
disc = scene.geometry.circle(0.65)
waves = disc.modifiers.zigzag(0, 10, smooth=True)
badge = scene.geometry.star(5, 0.8, 0.35)
corners = badge.modifiers.round_corners(0)
blob = scene.geometry.circle(0.65)
blob.modifiers.wiggle_path(0.12, seed=3)
table([
    ("teeth = star.modifiers.zigzag(0, ridges=3)\nteeth.animate.size(0.12)", star),
    ("waves = disc.modifiers.zigzag(0, 10, smooth=True)\nwaves.animate.size(0.08)", disc),
    ("corners = badge.modifiers.round_corners(0)\ncorners.animate.radius(0.25)", badge),
    ("blob.modifiers.wiggle_path(0.12, seed=3)", blob),
])
play([teeth.animate.size(0.12), waves.animate.size(0.08), corners.animate.radius(0.25)])

# pucker_bloat pushes the vertices out and pulls the curves between them in
# (amount < 0), or the other way round (amount > 0); twist turns the path
# about its center, less and less toward its farthest vertex.
scene.segment("warps")
pinched = scene.geometry.square(1.2)
pucker = pinched.modifiers.pucker_bloat(0)
swollen = scene.geometry.square(1.2)
bloat = swollen.modifiers.pucker_bloat(0)
pinwheel = scene.geometry.star(6, 0.85, 0.45)
twist = pinwheel.modifiers.twist(0)
spiky = scene.geometry.regular_polygon(8, 0.55)
spikes = spiky.modifiers.pucker_bloat(0)
table([
    ("pucker = pinched.modifiers.pucker_bloat(0)\npucker.animate.amount(-0.5)", pinched),
    ("bloat = swollen.modifiers.pucker_bloat(0)\nbloat.animate.amount(0.4)", swollen),
    ("twist = pinwheel.modifiers.twist(0)\ntwist.animate.angle(1.4)", pinwheel),
    ("spikes = spiky.modifiers.pucker_bloat(0)\nspikes.animate.amount(-0.6)", spiky),
])
play([pucker.animate.amount(-0.5), bloat.animate.amount(0.4), twist.animate.angle(1.4), spikes.animate.amount(-0.6)])

# offset grows a closed shape (amount > 0) or shrinks it (< 0), with miter,
# round or bevel corners; copies draws concentric outlines, so stroke them
# centered. An open path becomes a band to each side.
scene.segment("offset")
ring = scene.geometry.circle(0.25).no_fill().stroke(accent, 0.04, align="center")
rings = ring.modifiers.offset(0.05, copies=3)
grown = scene.geometry.square(0.8)
grow = grown.modifiers.offset(0, join="round")
shrunk = scene.geometry.square(1.4)
shrink = shrunk.modifiers.offset(0)
rail = scene.geometry.line((0, 0), (2.4, 0)).stroke(accent, 0.04)
band = rail.modifiers.offset(0)
table([
    ("rings = ring.modifiers.offset(0.05, copies=3)\nrings.animate.amount(0.17)", ring),
    ('grow = grown.modifiers.offset(0, join="round")\ngrow.animate.amount(0.3)', grown),
    ("shrink = shrunk.modifiers.offset(0)\nshrink.animate.amount(-0.4)", shrunk),
    ("band = rail.modifiers.offset(0)\nband.animate.amount(0.3)", rail),
])
play([rings.animate.amount(0.17), grow.animate.amount(0.3), shrink.animate.amount(-0.4), band.animate.amount(0.3)])

# Modifiers apply in the order they are added, to every glyph of a text, and
# after trims and other animations. set() changes a number at the cursor.
scene.segment("combined")
rounded = scene.geometry.star(5, 0.8, 0.35)
rounded.modifiers.zigzag(0.08, 3)
rounded.modifiers.round_corners(0.04)
word = scene.text("wobble", size=1.1)
word.modifiers.wiggle_path(0.03, seed=7)
drawn = scene.geometry.circle(0.65).no_fill().stroke(accent, 0.05).trim(end=0)
drawn.modifiers.zigzag(0.08, 2)
stepped = scene.geometry.regular_polygon(6, 0.8)
step = stepped.modifiers.zigzag(0.04)
table([
    ("rounded.modifiers.zigzag(0.08, 3)\nrounded.modifiers.round_corners(0.04)", rounded),
    ("word.modifiers.wiggle_path(0.03, seed=7)", word),
    ("drawn.modifiers.zigzag(0.08, 2)\ndrawn.animate.trim(end=1)", drawn),
    ("step = stepped.modifiers.zigzag(0.04)\nstep.set(size=0.15)", stepped),
])
scene.play([drawn.animate.trim(end=1)], duration=PLAY)
step.set(size=0.15)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Half way and settled in every segment.
    scene.snapshots(snapshots, [(PLAY + HOLD) * index + t for index in range(4) for t in (0.7, 1.9)])
else:
    scene.render()
