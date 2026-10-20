"""glTF models: importing one with scene.media.gltf and playing its animations, with fades, loops, speeds and reversal.

The fox of the glTF sample models stands in its rest pose, then plays its
three animations one after another, each fading from the one before, and
three copies show the ways a play runs. Each label is the call that plays
the fox above or beside it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 0.6
WALKS = 3.0

scene = Scene()
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")

# The fox's size in its file, in its units: about 80 tall and, seen in
# three-quarter view, 150 long. Its origin lies between its feet.
FOX_HEIGHT = 80.0
FOX_LENGTH = 150.0
# A three-quarter view: turned toward the right, seen from a little above.
TURN = (0.25, 0.9, 0.0)


def fox_on(zone, share=0.85):
    """A fox standing on the bottom of ``zone``, as large as ``share`` of it allows."""
    scale = share * min(zone.height / FOX_HEIGHT, zone.width / FOX_LENGTH)
    return scene.media.gltf("Fox.glb").scale_to(scale).rotate_to_3d(*TURN).move_to(*zone.point("bottom"))


# A model arrives in its rest pose, in the file's units: scale_to brings it
# to the scene, and move_to places its origin.
scene.segment("import")
stage, caption = scene.layout.zones(Zones.rows(["1fr", 0.6], gap="24px", padding="page_padding"))
fox = fox_on(stage)
scene.text('scene.media.gltf("Fox.glb")', role="code").place(caption)
scene.play(fox.animate.fade_in(), duration=FADE)
scene.wait(HOLD)

# animations() lists the file's animations and animation(name) plays one;
# transition fades from the animation before it, which keeps running.
scene.segment("animations")
stage, *rows = scene.layout.zones(Zones.rows(["1fr", 0.5, 0.5, 0.5], gap="12px", padding="page_padding"))
fox = fox_on(stage)
PLAYS = [
    ('fox.animation("Survey")', fox.animation("Survey"), None),
    ('fox.animation("Walk", loop=True, transition=0.4)', fox.animation("Walk", loop=True, transition=0.4), 2.0),
    ('fox.animation("Run", loop=True, transition=0.3)', fox.animation("Run", loop=True, transition=0.3), 2.0),
]
labels = [scene.text(call, role="code").fill(muted).place(row) for (call, _, _), row in zip(PLAYS, rows)]
scene.play(fox.animate.fade_in(), duration=FADE)
for label, (_, play, duration) in zip(labels, PLAYS):
    scene.play([play, label.animate.fill(accent)], duration=duration)
    label.fill(muted)
scene.wait(HOLD)

# A play runs once by default; loop wraps it, speed scales its time and
# reverse runs it backwards.
scene.segment("plays")
VARIANTS = [
    ('animation("Walk", loop=True)', {"loop": True}),
    ('animation("Walk", loop=True, speed=0.5)', {"loop": True, "speed": 0.5}),
    ('animation("Walk", loop=True, reverse=True)', {"loop": True, "reverse": True}),
]
grid = scene.layout.zones(
    Zones.grid(rows=len(VARIANTS), columns=["1.6fr", "1fr"], gap="12px", column_gap="48px", padding="page_padding")
)
foxes = []
for index, (call, _) in enumerate(VARIANTS):
    scene.text(call, role="code").place(grid[2 * index], anchor="right")
    foxes.append(fox_on(grid[2 * index + 1]))
scene.play([fox.animate.fade_in() for fox in foxes], duration=FADE)
scene.play(
    [fox.animation("Walk", **options) for fox, (_, options) in zip(foxes, VARIANTS)],
    duration=WALKS,
)
scene.wait(HOLD)

# The rest pose; the fox looking around, fading into its walk, walking and
# running; and the three plays, twice apart.
SURVEY = 3.4
animations = FADE + HOLD
plays = animations + FADE + SURVEY + 4.0 + HOLD
SEEKS = [
    FADE + 0.2,
    animations + FADE + 1.5,
    animations + FADE + SURVEY + 0.2,
    animations + FADE + SURVEY + 1.5,
    animations + FADE + SURVEY + 3.0,
    plays + FADE + 0.9,
    plays + FADE + 2.2,
]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
