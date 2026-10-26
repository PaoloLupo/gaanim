"""glTF morph targets: setting their weights with morph, animating them with animate.morph, and the file's animations that move them.

A clay ball modelled in a 3D editor carries three shapes beside its own,
its morph targets: Squash, Spikes and Lean. A weight of 0 shows the ball as
modelled and 1 the shape in full; weights mix and may go past either end.
Each label is the call that sets the ball above it, or that animates the
ball beside it.
"""

import os

from gaanim import Scene, Zones

FADE = 0.6
HOLD = 0.8
MORPH = 1.2
LOOPS = 4.8

scene = Scene()
accent = scene.colors.accent
muted = scene.colors.muted

# The ball in its file: 2 units across about its origin. Its spikes reach
# out 0.6 more, and its lean takes its top 0.7 aside.
BALL_SIZE = 3.2
# Seen from a little above, so that its spikes show their depth.
TILT = (0.3, 0.4, 0.0)


def ball_in(zone, share=0.9):
    """A ball in the middle of ``zone``, as large as ``share`` of it allows."""
    scale = share * min(zone.width, zone.height) / BALL_SIZE
    return scene.media.gltf("Blob.glb").scale_to(scale).rotate_to_3d(*TILT).move_to(*zone.center)


# morph(name, weight) sets a target's weight; declared, it is the weight the
# ball starts with. morph_targets() lists the names the file gives them.
scene.segment("targets")
VARIANTS = [
    ('gltf("Blob.glb")', []),
    ('morph("Squash", 1.0)', [("Squash", 1.0)]),
    ('morph("Spikes", 1.0)', [("Spikes", 1.0)]),
    ('morph("Lean", 1.0)', [("Lean", 1.0)]),
    ('morph("Lean", -1.0)', [("Lean", -1.0)]),
    ('morph("Spikes", 0.4)', [("Spikes", 0.4)]),
]
grid = scene.layout.zones(Zones.grid(rows=2, columns=3, gap="24px", padding="page_padding"))
balls = []
for zone, (label, weights) in zip(grid, VARIANTS):
    art, caption = zone.split(Zones.rows(["1fr", 0.4]))
    ball = ball_in(art)
    for name, weight in weights:
        ball.morph(name, weight)
    balls.append(ball)
    scene.text(label, role="code").place(caption, fit="scale_down")
scene.play([ball.animate.fade_in() for ball in balls], duration=FADE)
scene.wait(HOLD)

# animate.morph animates a weight, alone or with other targets; the
# targets it leaves out keep theirs.
scene.segment("animate")
stage, *rows = scene.layout.zones(Zones.rows(["1fr", 0.45, 0.45, 0.45], gap="12px", padding="page_padding"))
ball = ball_in(stage)
PLAYS = [
    ('ball.animate.morph("Spikes", 1.0)', ball.animate.morph("Spikes", 1.0)),
    ('ball.animate.morph("Spikes", 0.0).morph("Squash", 0.8)', ball.animate.morph("Spikes", 0.0).morph("Squash", 0.8)),
    ('ball.animate.morph("Lean", 1.0)', ball.animate.morph("Lean", 1.0)),
]
labels = [scene.text(call, role="code").fill(muted).place(row, fit="scale_down") for (call, _), row in zip(PLAYS, rows)]
scene.play(ball.animate.fade_in(), duration=FADE)
for label, (_, play) in zip(labels, PLAYS):
    scene.play([play, label.animate.fill(accent)], duration=MORPH)
    label.fill(muted)
scene.wait(HOLD)

# The file's own animations move the weights too: Bounce squashes the ball
# where it lands, and Wobble grows its spikes as it sways. Smaller, the
# ball has room to hop.
scene.segment("animations")
art, captions = scene.layout.zones(Zones.rows(["1fr", 0.45], gap="12px", padding="page_padding"))
CLIPS = ["Bounce", "Wobble"]
columns = Zones.columns(len(CLIPS), gap="48px")
balls = []
for zone, caption, clip in zip(art.split(columns), captions.split(columns), CLIPS):
    balls.append(ball_in(zone, share=0.6))
    scene.text(f'animation("{clip}", loop=True)', role="code").place(caption, fit="scale_down")
scene.play([ball.animate.fade_in() for ball in balls], duration=FADE)
scene.play([ball.animation(clip, loop=True) for ball, clip in zip(balls, CLIPS)], duration=LOOPS)
scene.wait(HOLD)

# The six balls; the ball grown spikes, then squashed, then squashed and
# leaning; and the two animations as the ball tops its hop, as the other
# grows its spikes in full and as the ball lands.
animate = FADE + HOLD
animations = animate + FADE + 3 * MORPH + HOLD
SEEKS = [
    FADE + 0.4,
    animate + FADE + MORPH,
    animate + FADE + 2 * MORPH,
    animate + FADE + 3 * MORPH,
    animations + FADE + 0.6,
    animations + FADE + 1.0,
    animations + FADE + 1.2,
]
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
