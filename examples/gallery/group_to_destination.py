"""A cluster of points that travels so its gold leader lands on a target, the others trailing it on delayed followers.

Only the leader is animated: every other point follows it with its own
offset and a delay that grows with its distance, so the cluster stretches
in flight and gathers again on arrival. Lines join neighbours that are close,
and each target flashes when the leader reaches it.
"""

import math
import os

from gaanim import Easing, Scene

scene = Scene(theme="gaanim")
violet = scene.colors.accent
gold = scene.colors.title
lilac = scene.colors.chart
mist = scene.colors.rule
geometry = scene.geometry

start = (-4.6, -1.4)
leader = geometry.dot(0.2).fill(gold).no_stroke().move_to(*start)
members = [leader]
# Two rings of points around the leader, followers of it.
for ring, (count, radius) in enumerate(((6, 0.9), (10, 1.7))):
    for index in range(count):
        a = math.tau * index / count + ring * 0.3
        offset = (radius * math.cos(a), radius * math.sin(a))
        member = geometry.dot(0.12 if ring == 0 else 0.09).fill(violet if ring == 0 else lilac).no_stroke()
        member.move_to(start[0] + offset[0], start[1] + offset[1]).follow(leader, offset=offset, delay=0.08 * radius)
        members.append(member)
web = geometry.connect(members, max_distance=1.2).stroke(mist, 0.025).z_index(-1)

targets = [(4.4, 1.8), (0.0, -2.2), (-4.0, 2.2)]
rings = [geometry.circle(0.32).no_fill().stroke(gold, 0.04).move_to(*target) for target in targets]

scene.play([member.animate.grow_from_center() for member in members], duration=0.6, easing=Easing.back())
scene.play([web.animate.fade_in(), *(ring.animate.create() for ring in rings)], duration=0.6)
for target, ring in zip(targets, rings):
    scene.play(leader.animate.move_to(*target).path_arc(1.2), duration=1.6, easing=Easing.SMOOTH)
    scene.play(ring.animate.flash_around(), duration=0.7)
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The cluster, in flight to each target, and gathered on the last.
    scene.snapshots(snapshots, [1.2, 2.0, 3.0, 4.6, 6.0, 7.9])
else:
    scene.render()
