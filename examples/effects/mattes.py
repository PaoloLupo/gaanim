"""Track mattes and metaballs: showing a drawable through another one, and drops that melt together.

``matte`` shows a drawable only where a source is opaque, or bright, or the
opposite, and follows the source as it moves, scales, fades or is written.
The first segment shows the four modes, the second a matte that follows its
source's animations, and the third ``scene.geometry.metaballs`` with its
smoothness and threshold. Each label is the code of its variant.
"""

import math
import os

from gaanim import Brush, Scene, Zones

MOVE = 1.6
HOLD = 0.6

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
background = scene.colors.background
rule = scene.colors.rule
COLORS = [accent, scene.colors.success, scene.colors.danger, scene.colors.chart]


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


def gallery(rows=2, columns=2, caption=0.45):
    """The zones of a grid of panels, each split into its art and the caption under it."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows(["1fr", caption])) for zone in grid]


def stripes(zone):
    """Vertical stripes of theme colors filling ``zone``, as one group."""
    count = 16
    width = zone.width / count
    x, y = zone.center
    return scene.geometry.group([
        scene.geometry.rect(width, zone.height).fill(COLORS[index % len(COLORS)]).move_to(x + width * (index - (count - 1) / 2), y)
        for index in range(count)
    ])


def frame(zone):
    """A thin outline of ``zone``, to show where the matted stripes could be."""
    return scene.geometry.rect(zone.width, zone.height).no_fill().stroke(rule, 0.02).move_to(*zone.center)


def word(zone, text="MATTE"):
    """A large bold word in the middle of ``zone``, used as a matte source."""
    return scene.text(text, size=min(zone.height * 0.8, zone.width / (1.25 * len(text))), weight=800).place(zone)


def ramp(zone):
    """A rectangle that fades from light to dark, its dark end covering ``zone``."""
    shape = scene.geometry.rect(zone.width * 2.4, zone.height)
    shape.fill(Brush.linear([foreground, foreground, background, background], start=(-zone.width * 1.2, 0), end=(zone.width * 1.2, 0)))
    x, y = zone.center
    return shape.move_to(x - zone.width * 0.7, y)


# "alpha" shows the drawable where the source is opaque and "luma" where it is
# bright; the inverted modes show the opposite. The source is no longer drawn
# on its own, and the matte follows it: here the words slide and the ramps
# sweep across, revealing the stripes softly.
modes_start = scene.cursor
scene.segment("modes")
moves = []
grid = scene.layout.zones(
    Zones.grid(rows=4, columns=["15fr", "9fr"], column_gap="48px", row_gap="24px", padding="page_padding")
)
for index, (code, mode) in enumerate([
    ("stripes.matte(word)", "alpha"),
    ('stripes.matte(word, "alpha_inverted")', "alpha_inverted"),
    ('stripes.matte(ramp, "luma")', "luma"),
    ('stripes.matte(ramp, "luma_inverted")', "luma_inverted"),
]):
    label(code).place(grid[2 * index], anchor="right")
    art = grid[2 * index + 1]
    frame(art)
    if mode.startswith("alpha"):
        source = word(art).shift_by(-art.width * 0.1, 0)
        moves.append(source.animate.shift_by(art.width * 0.2, 0))
    else:
        source = ramp(art)
        moves.append(source.animate.shift_by(art.width * 1.4, 0))
    stripes(art).matte(source, mode)
scene.wait(HOLD)
scene.play(moves, duration=MOVE)
scene.wait(HOLD)


# A matte follows everything that happens to its source: writing, turning,
# scaling and fading. A blurred source softens the matte's edge, and
# matte(None) removes it.
def written(art):
    """Stripes matted by a word that is written."""
    source = word(art)
    stripes(art).matte(source)
    return source.animate.write()


def turned(art):
    """Stripes matted by a star that turns."""
    source = scene.geometry.star(5, art.height * 0.48, art.height * 0.2).move_to(*art.center)
    stripes(art).matte(source)
    return source.animate.rotate_by(math.pi / 5)


def iris(art):
    """Stripes matted by a dot that grows twentyfold, an iris reveal."""
    source = scene.geometry.circle(art.width * 0.025).move_to(*art.center)
    stripes(art).matte(source)
    return source.animate.scale_to(20)


def soft(art):
    """Stripes matted by a blurred word."""
    stripes(art).matte(word(art).blur(0.12))


def faded(art):
    """Stripes matted by a word that fades to a fifth of its opacity."""
    source = word(art)
    stripes(art).matte(source)
    return source.animate.opacity(0.2)


def removed(art):
    """Stripes whose matte was removed again."""
    stripes(art).matte(word(art)).matte(None)


follows_start = scene.cursor
scene.segment("follows")
plays = []
for (art, caption), (code, build) in zip(gallery(rows=3, caption=0.9), [
    ("stripes.matte(word)\nword.animate.write()", written),
    ("stripes.matte(star)\nstar.animate.rotate_by(math.pi / 5)", turned),
    ("stripes.matte(dot)\ndot.animate.scale_to(20)", iris),
    ("stripes.matte(word.blur(0.12))", soft),
    ("stripes.matte(word)\nword.animate.opacity(0.2)", faded),
    ("stripes.matte(word).matte(None)", removed),
]):
    label(code).place(caption)
    frame(art)
    play = build(art)
    if play is not None:
        plays.append(play)
scene.wait(HOLD)
scene.play(plays, duration=MOVE)
scene.wait(HOLD)


# metaballs melts drawables into one outline, counting each as a circle that
# follows its animations; the drawables themselves stop being drawn. Drops
# closer than smoothness join with a neck, and threshold divides every radius.
# The result is a filled path to style like any other.
def drops(art):
    """Three drops in ``art``: two that approach each other and one that waits above them."""
    (x, y), radius = art.center, art.height * 0.28
    left = scene.geometry.circle(radius).move_to(x - art.width * 0.32, y - radius * 0.4)
    right = scene.geometry.circle(radius * 0.8).move_to(x + art.width * 0.32, y - radius * 0.4)
    top = scene.geometry.circle(radius * 0.6).move_to(x, y + radius * 1.2)
    moves.extend([left.animate.shift_by(art.width * 0.26, 0), right.animate.shift_by(-art.width * 0.26, 0)])
    return [left, right, top]


metaballs_start = scene.cursor
scene.segment("metaballs")
moves = []
for (art, caption), (code, build) in zip(gallery(rows=3, caption=0.9), [
    ("metaballs(drops)", lambda balls: scene.geometry.metaballs(balls)),
    ("metaballs(drops, smoothness=0.05)", lambda balls: scene.geometry.metaballs(balls, smoothness=0.05)),
    ("metaballs(drops, smoothness=1.0)", lambda balls: scene.geometry.metaballs(balls, smoothness=1.0)),
    ("metaballs(drops, threshold=1.4)", lambda balls: scene.geometry.metaballs(balls, threshold=1.4)),
    ("metaballs(drops, threshold=0.7)", lambda balls: scene.geometry.metaballs(balls, threshold=0.7)),
    (
        "metaballs(drops, smoothness=0.8)\n.no_fill().stroke(accent, 0.06)",
        lambda balls: scene.geometry.metaballs(balls, smoothness=0.8).no_fill().stroke(accent, 0.06),
    ),
]):
    label(code).place(caption)
    build(drops(art))
scene.wait(HOLD)
scene.play(moves, duration=MOVE)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each segment before, during and after its sources or drops move.
    scene.snapshots(
        snapshots,
        [start + t for start in (modes_start, follows_start, metaballs_start) for t in (0.3, HOLD + MOVE / 2, HOLD + MOVE + 0.3)],
    )
else:
    scene.render()
