"""Video: MP4 clips synchronized with the timeline, framed and cropped, trimmed, looped, sped up and cut into segments.

A video plays from the play call that includes it, so seeks, previews and
exports all show the same frame. The clip here shows its own timestamp,
which makes every offset, speed and loop visible. Each label is the code
that builds its variant.
"""

import os

from gaanim import Scene, Zones

CLIP = "assets/clip.mp4"  # 480 x 270 px, 4 seconds at 30 fps, with a tone

scene = Scene()
rule = scene.colors.rule
HOLD = 0.6


def cells(labels, bands=("2fr", "7fr")):
    """A padded 2 x 2 grid with a code label above each cell; return the stages."""
    grid = scene.layout.zones(Zones.grid(rows=2, columns=2, gap="48px", padding="page_padding"))
    stages = []
    for cell, label in zip(grid, labels):
        top, stage = cell.split(Zones.rows(list(bands)))
        scene.text(label, role="code").place(top, anchor="bottom")
        stages.append(stage)
    return stages


# frame() fits the clip to a box like an image: contain keeps it whole,
# cover fills the box and cuts the rest, stretch distorts it. crop()
# keeps a window of the source. Each square box is outlined.
scene.segment("framing")
stages = cells([
    "video(CLIP)\n    .frame(s, s)",
    'video(CLIP)\n    .frame(s, s, fit="cover")',
    'video(CLIP)\n    .frame(s, s, fit="stretch")',
    "video(CLIP).frame(s, s)\n    .crop(0.5, 0, 0.5, 0.5,\n          normalized=True)",
], bands=("3fr", "7fr"))
fits = ["contain", "cover", "stretch", "contain"]
videos = []
for stage, fit in zip(stages, fits):
    side = stage.height * 0.95
    scene.geometry.rect(side, side).no_fill().stroke(rule, 0.03).move_to(*stage.center)
    video = scene.media.video(CLIP, audio=False).frame(side, side, fit=fit)
    videos.append(video.move_to(*stage.center))
videos[3].crop(0.5, 0, 0.5, 0.5, normalized=True)
scene.play(videos)
scene.wait(HOLD)

# offset and duration choose the source seconds, loop repeats them and
# speed plays faster or slower (the audio keeps its pitch). A finite clip
# lasts its output duration; a looping one runs until the scene moves on.
scene.segment("timing")
stages = cells([
    "video(CLIP, offset=1, duration=2)",
    "video(CLIP, duration=1, loop=True)",
    "video(CLIP, speed=0.5)",
    "video(CLIP, speed=2, volume=0.3)",
])
timed = [
    scene.media.video(CLIP, offset=1, duration=2, audio=False),
    scene.media.video(CLIP, duration=1, loop=True, audio=False),
    scene.media.video(CLIP, speed=0.5, audio=False),
    scene.media.video(CLIP, speed=2, volume=0.3),
]
for video, stage in zip(timed, stages):
    video.frame(stage.width * 0.9, stage.height * 0.9).move_to(*stage.center)
scene.play(timed)
scene.wait(HOLD)

# segment() plays a stretch of the source as its own play, at its own
# speed, so one clip can be cut and reordered across the timeline.
scene.segment("segments")
(stage,) = scene.layout.zones(Zones.rows(1, padding="page_padding"))
top, rest = stage.split(Zones.rows(["2fr", "5fr"], gap="24px"))
scene.text(
    "scene.play([clip.segment(start=0, end=1, speed=2)])\n"
    "scene.play([clip.segment(start=3, end=4)])\n"
    "scene.play([clip.segment(start=1, end=2)])",
    role="code",
).place(top, anchor="left")
clip = scene.media.video(CLIP, audio=False).frame(rest.width, rest.height).move_to(*rest.center)
scene.play([clip.segment(start=0, end=1, speed=2)])
scene.wait(0.3)
scene.play([clip.segment(start=3, end=4)])
scene.wait(0.3)
scene.play([clip.segment(start=1, end=2)])
scene.wait(HOLD)

FRAMING_END = 4 + HOLD
TIMING_END = FRAMING_END + 8 + HOLD
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The framings at two moments; the timings at three; each segment midway.
    scene.snapshots(
        snapshots,
        [1.0, 3.5]
        + [FRAMING_END + t for t in (0.5, 1.5, 3.5)]
        + [TIMING_END + 0.25, TIMING_END + 0.5 + 0.3 + 0.5, TIMING_END + 0.5 + 0.3 + 1 + 0.3 + 0.5],
    )
else:
    scene.render()
