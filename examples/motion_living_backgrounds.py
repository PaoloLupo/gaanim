"""Living backgrounds: mesh and noise gradients, aurora and a drifting dot grid."""

import os

from gaanim import WHITE, Background, Scene


NAVY = "#0b1040"
PURPLE = "#6a2cc8"
TEAL = "#12b8a6"
PINK = "#f25f8b"

BACKGROUNDS = [
    ("mesh_gradient", Background.mesh_gradient([NAVY, PURPLE, TEAL, PINK], speed=0.3, seed=4)),
    ("noise_gradient", Background.noise_gradient([NAVY, PURPLE, PINK, "#ffd166"], scale=1.6, speed=0.1, seed=1)),
    ("aurora", Background.aurora(["#3dffa8", "#4fb3ff", "#b36bff"], speed=0.4, seed=2)),
    ("dot_grid", Background.dot_grid(0.5, radius=0.05, color="#8fa3ff80", drift=(0.25, 0.0))),
]

scene = Scene(frame=(16, 9))
for index, (name, background) in enumerate(BACKGROUNDS):
    scene.segment(name, background=background)
    scene.text(name, role="title").fill(WHITE).move_to(0, 0)
    scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Each background early and late in its segment, to show it moving.
    scene.snapshots(snapshots, [time for index in range(4) for time in (index + 0.1, index + 0.9)])
else:
    scene.render()
