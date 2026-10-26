"""Points that travel along a circle, a square and a star with a trail of echoes, and an arm that turns about its end.

move_along carries each point along the outline of its shape; the arrowhead
on the star turns to face where it goes. Every traveller draws delayed copies
of itself behind it with echo, and the last one is a bar pinned at one end
whose free end turns about the pin.
"""

import math
import os

from gaanim import Easing, Scene, Zones, stagger

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
paper = scene.colors.foreground
mist = scene.colors.rule
geometry = scene.geometry

zones = scene.layout.zones(Zones.columns(4, gap="48px", padding="page_padding"))
size = min(zones[0].width, zones[0].height) * 0.36
shapes = [
    geometry.circle(size),
    geometry.square(1.7 * size),
    geometry.star(5, 1.1 * size, 0.5 * size),
]
for shape, zone in zip(shapes, zones):
    shape.no_fill().stroke(violet, 0.05).move_to(*zone.center)

travellers = [
    geometry.dot(0.14).fill(gold).glow(gold, radius=0.2),
    geometry.dot(0.14).fill(gold).glow(gold, radius=0.2),
    geometry.regular_polygon(3, 0.2).fill(gold).glow(gold, radius=0.2),
]
for traveller in travellers:
    traveller.no_stroke().echo(10, delay=0.035, decay=0.75).hidden()

# The arm: a bar from a pin, and a dot on its free end, turning together
# about the pin.
px, py = zones[3].center
pin = geometry.dot(0.1).fill(paper).move_to(px, py)
arm = geometry.group([
    geometry.line(px, py, px + size, py).stroke(lilac, 0.06),
    geometry.dot(0.14).fill(gold).no_stroke().move_to(px + size, py).echo(10, delay=0.035, decay=0.75),
])
guide = geometry.circle(size).no_fill().stroke(mist, 0.02).move_to(px, py)

scene.play(stagger(*(shape.animate.create().duration(0.9) for shape in shapes), each=0.2))
scene.play([pin.animate.grow_from_center(), arm.animate.create(), guide.animate.create()], duration=0.6)
scene.play(stagger(
    travellers[0].animate.move_along(shapes[0]).duration(2.0),
    travellers[1].animate.move_along(shapes[1]).duration(2.0),
    travellers[2].animate.move_along(shapes[2], orient=True, rotate_offset=-math.pi / 2).duration(2.0),
    arm.animate.rotate_by(math.tau).about_point(px, py).duration(2.0),
    each=0.35,
    easing=Easing.SMOOTH,
))
# Once more, all at once and at a constant speed.
scene.play(
    [
        travellers[0].animate.move_along(shapes[0]),
        travellers[1].animate.move_along(shapes[1]),
        travellers[2].animate.move_along(shapes[2], orient=True, rotate_offset=-math.pi / 2),
        arm.animate.rotate_by(-math.tau).about_point(px, py),
    ],
    duration=2.4,
    easing=Easing.LINEAR,
)
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Drawn, staggered laps, and the laps together.
    scene.snapshots(snapshots, [1.0, 2.6, 3.4, 4.2, 5.8, 6.8])
else:
    scene.render()
