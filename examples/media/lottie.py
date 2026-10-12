"""Lottie: vector animations from Lottie JSON and dotLottie packages, played on the timeline, drawn in and driven by state machines.

A Lottie clip shows its first frame until a play call starts it, then
follows the scene clock, seeks included. A dotLottie package can also hold
several animations, themes and a state machine whose inputs and events are
set at the cursor. Each label is the code that builds its variant.
"""

import os

from gaanim import Scene, Zones

BALLS = "assets/balls.json"  # 1920 x 1080, 4.15 seconds at 60 fps
TOGGLE = "assets/toggle.lottie"  # animations idle and active, theme gold, state machine main

scene = Scene()
accent = scene.canvas.color("accent")

HOLD = 0.6


def cells(labels, rows=2, columns=2):
    """A padded grid with a code label above each cell; return the stages."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    stages = []
    for cell, label in zip(grid, labels):
        top, stage = cell.split(Zones.rows(["1fr", "5fr"]))
        scene.text(label, role="code", markup=False).place(top, anchor="bottom")
        stages.append(stage)
    return stages


def balls(stage, **options):
    """The balls animation fitted to ``stage``."""
    return scene.media.lottie(BALLS, width=stage.width, height=stage.height, **options).move_to(*stage.center)


# offset and duration choose the source seconds, loop repeats them and
# speed plays faster or slower; a finite clip lasts its output duration.
scene.segment("playback")
stages = cells([
    "lottie(BALLS)",
    "lottie(BALLS, offset=1, duration=1.5)",
    "lottie(BALLS, duration=1, loop=True)",
    "lottie(BALLS, speed=0.5)",
])
clips = [
    balls(stages[0]),
    balls(stages[1], offset=1, duration=1.5),
    balls(stages[2], duration=1, loop=True),
    balls(stages[3], speed=0.5),
]
scene.wait(0.3)
scene.play(clips)
scene.wait(HOLD)

# Before it plays, a clip can be drawn in like a shape: write and create
# trace its contours and then fill them, fade_in brings it in whole.
scene.segment("drawing")
stages = cells(
    ["clip.animate.write()", "clip.animate.create()", "clip.animate.fade_in()"], rows=1, columns=3
)
drawn = [balls(stage) for stage in stages]
scene.play([drawn[0].animate.write(), drawn[1].animate.create(), drawn[2].animate.fade_in()], duration=1.2)
scene.play(drawn)
scene.wait(HOLD)

# A dotLottie state machine runs while the scene waits: set_input and
# fire_event change its state at the cursor, and set_theme swaps the
# package's colors. The call that is in effect is lit.
scene.segment("state_machine")
(stage,) = scene.layout.zones(Zones.rows(1, padding="page_padding"))
left, right = stage.split(Zones.columns(["3fr", "2fr"], gap="96px"))
steps = left.split(Zones.rows(5, gap="24px"))
CALLS = [
    'clip = scene.media.lottie(\n    TOGGLE, state_machine_id="main")',
    'clip.set_input("active", True)',
    'clip.set_theme("gold")',
    'clip.fire_event("reset")',
    "clip.set_theme(None)",
]
labels = [scene.text(call, role="code", markup=False).place(row, anchor="right") for row, call in zip(steps, CALLS)]
toggle = scene.media.lottie(TOGGLE, state_machine_id="main", width=right.width * 0.8).move_to(*right.center)
labels[0].fill(accent)
scene.play([toggle.animate.write().duration(0.75)])
scene.play([toggle])
scene.wait(0.75)
for index, change in enumerate([
    lambda: toggle.set_input("active", True),
    lambda: toggle.set_theme("gold"),
    lambda: toggle.fire_event("reset"),
    lambda: toggle.set_theme(None),
], start=1):
    change()
    labels[index - 1].fill(scene.canvas.color("foreground"))
    labels[index].fill(accent)
    scene.wait(1.0)

PLAYBACK_END = 0.3 + 4.15 * 2 + HOLD
DRAWING_END = PLAYBACK_END + 1.2 + 4.15 + HOLD
STATES = DRAWING_END + 0.75 + 0.75
if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Playback at three moments; the drawings midway and the clips playing;
    # the state machine after every change.
    scene.snapshots(
        snapshots,
        [0.2, 1.5, 3.5]
        + [PLAYBACK_END + 0.5, PLAYBACK_END + 1.2 + 1.5]
        + [STATES - 0.1] + [STATES + index + 0.6 for index in range(4)],
    )
else:
    scene.render()
