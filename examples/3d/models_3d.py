"""glTF models: importing one with scene.media.gltf and playing its animations, with fades, loops, speeds and reversal.

A wolf stands in its rest pose, then eats, walks and runs, each animation
fading from the one before, and three copies show the ways a play runs.
Each label is the call that plays the wolf above or beside it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 0.6
WALKS = 3.0

scene = Scene()
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")

# The wolf's size in its file, in its units: about 1.8 tall and, seen in
# three-quarter view, 2.3 long. Its origin lies between its feet.
WOLF_HEIGHT = 1.8
WOLF_LENGTH = 2.3
# A three-quarter view: turned toward the right, seen from a little above.
TURN = (0.25, 0.9, 0.0)


def wolf_on(zone, share=0.85):
    """A wolf standing on the bottom of ``zone``, as large as ``share`` of it allows."""
    scale = share * min(zone.height / WOLF_HEIGHT, zone.width / WOLF_LENGTH)
    return scene.media.gltf("Wolf.glb").scale_to(scale).rotate_to_3d(*TURN).move_to(*zone.point("bottom"))


# A model arrives in its rest pose, in the file's units: scale_to brings it
# to the scene, and move_to places its origin.
scene.segment("import")
stage, caption = scene.layout.zones(Zones.rows(["1fr", 0.6], gap="24px", padding="page_padding"))
wolf = wolf_on(stage)
scene.text('scene.media.gltf("Wolf.glb")', role="code").place(caption)
scene.play(wolf.animate.fade_in(), duration=FADE)
scene.wait(HOLD)

# animations() lists the file's animations and animation(name) plays one;
# transition fades from the animation before it, which keeps running.
scene.segment("animations")
stage, *rows = scene.layout.zones(Zones.rows(["1fr", 0.5, 0.5, 0.5], gap="12px", padding="page_padding"))
wolf = wolf_on(stage)
PLAYS = [
    ('wolf.animation("Idle_Eating")', wolf.animation("Idle_Eating"), None),
    ('wolf.animation("Walk", loop=True, transition=0.4)', wolf.animation("Walk", loop=True, transition=0.4), 2.0),
    ('wolf.animation("Run", loop=True, transition=0.3)', wolf.animation("Run", loop=True, transition=0.3), 2.0),
]
labels = [scene.text(call, role="code").fill(muted).place(row) for (call, _, _), row in zip(PLAYS, rows)]
scene.play(wolf.animate.fade_in(), duration=FADE)
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
wolves = []
for index, (call, _) in enumerate(VARIANTS):
    scene.text(call, role="code").place(grid[2 * index], anchor="right")
    wolves.append(wolf_on(grid[2 * index + 1]))
scene.play([wolf.animate.fade_in() for wolf in wolves], duration=FADE)
scene.play(
    [wolf.animation("Walk", **options) for wolf, (_, options) in zip(wolves, VARIANTS)],
    duration=WALKS,
)
scene.wait(HOLD)

# The rest pose; the wolf eating, fading into its walk, walking and running;
# and the three plays, twice apart.
EATING = 10 / 3
animations = FADE + HOLD
plays = animations + FADE + EATING + 4.0 + HOLD
SEEKS = [
    FADE + 0.2,
    animations + FADE + 1.5,
    animations + FADE + EATING + 0.2,
    animations + FADE + EATING + 1.5,
    animations + FADE + EATING + 3.0,
    plays + FADE + 0.9,
    plays + FADE + 2.2,
]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
