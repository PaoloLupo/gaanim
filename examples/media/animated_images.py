"""Animated images: GIF, WebP and PNG frames and numbered image sequences played on the timeline like a video.

An animated image is a silent video of its own frames: it plays from the
play call that includes it, loops by default, and takes the video's
offset, speed and segments, without FFmpeg. An image sequence shows
numbered files one per frame. Each label is the code that builds its
variant.
"""

import os

from gaanim import Scene, Zones

PULSE = "assets/pulse.gif"  # 16 frames of 70 ms
ORBIT = "assets/orbit.webp"  # 20 frames of 60 ms
STAMP = "assets/stamp.png"  # an APNG that grows, then holds
SPIN = "assets/spin/spin_%02d.png"  # 12 numbered PNG files
HOLD = 1.6
SIDE = 2.4

scene = Scene()


def cells(labels):
    """A padded 2 x 2 grid with a code label above each cell; return the stages."""
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))
    stages = []
    for cell, label in zip(grid, labels):
        top, stage = cell.split(Zones.rows(["2fr", "7fr"]))
        scene.text(label, role="code").place(top, anchor="bottom")
        stages.append(stage)
    return stages


# Each file keeps its own frame times; a still image would be one frame.
scene.segment("formats")
stages = cells([
    "animated_image(PULSE)",
    "animated_image(ORBIT)",
    "animated_image(STAMP)",
    "image_sequence(SPIN, fps=12,\nloop=True)",
])
clips = [
    scene.media.animated_image(PULSE),
    scene.media.animated_image(ORBIT),
    scene.media.animated_image(STAMP),
    scene.media.image_sequence(SPIN, fps=12, loop=True),
]
for clip, stage in zip(clips, stages):
    clip.frame(SIDE, SIDE).move_to(*stage.center)
scene.play(clips)
scene.wait(HOLD)

# The video's timing: speed, an offset into the frames, a single pass that
# holds its last frame, and a segment of source seconds.
scene.segment("timing")
stages = cells([
    "animated_image(PULSE, speed=0.5)",
    "animated_image(ORBIT, offset=0.6)",
    "animated_image(STAMP, loop=False)",
    "image_sequence(SPIN, fps=12)\n.segment(start=0.5, end=1)",
])
timed = [
    scene.media.animated_image(PULSE, speed=0.5),
    scene.media.animated_image(ORBIT, offset=0.6),
    scene.media.animated_image(STAMP, loop=False),
]
spin = scene.media.image_sequence(SPIN, fps=12)
for clip, stage in zip(timed + [spin], stages):
    clip.frame(SIDE, SIDE).move_to(*stage.center)
scene.play(timed + [spin.segment(start=0.5, end=1.0)])
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.25, 0.6, 1.3, HOLD + 0.3, HOLD + 0.8, HOLD + 1.3])
else:
    scene.render()
