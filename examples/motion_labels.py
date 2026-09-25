"""Timeline labels, GSAP-style insert positions and scene markers."""

import os

from gaanim import BLACK, CORAL, CYAN, GOLD, GRAY, WHITE, Direction, Scene, label, sequence


scene = Scene(frame=(16, 9), background=BLACK)
headline = scene.text("Etiquetas", role="title").fill(WHITE).move_to(0, 2.6)
subtitle = scene.text("posiciones relativas y marcadores").fill(GRAY).scale_by(0.6).move_to(0, 1.6)
logo = scene.geometry.circle(0.9).fill(GOLD).move_to(-2.4, -0.8)
ring = scene.geometry.circle(1.3).no_fill().stroke(CYAN, 0.08).move_to(-2.4, -0.8)
badge = scene.geometry.rounded_rect(3.2, 1.2, 0.3).fill(CORAL).move_to(2.4, -0.8)
footer = scene.text("<  >  +=  -=  golpe+0.15").fill(GRAY).scale_by(0.5).move_to(0, -3.2)

# "golpe" marks where the subtitle starts (0.8 s); the logo lands 0.15 s
# later, the ring starts with it ("<"), the badge starts 0.1 s before the
# ring ends (">-0.1") and the footer overlaps the current end ("-=0.2").
intro = (
    sequence(
        headline.animate.write().duration(0.8),
        label("golpe"),
        subtitle.animate.fade_in().duration(0.4),
    )
    .insert(logo.animate.grow_from_center().duration(0.6), at="golpe+0.15")
    .insert(ring.animate.create().duration(0.6), at="<")
    .insert(badge.animate.fade_in_from(Direction.RIGHT).duration(0.5), at=">-0.1")
    .insert(footer.animate.fade_in().duration(0.4), at="-=0.2")
)
assert abs(intro.schedule().labels["golpe"] - 0.8) < 1e-9

scene.play(intro)
scene.marker("climax")
scene.play([logo.animate.indicate().duration(0.6)])
scene.marker("fin")
scene.wait(0.2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Title writing (0.6), logo and ring growing from "golpe+0.15" (1.2),
    # badge and footer overlapping (1.9), then the "climax" marker (2.15).
    scene.snapshots(snapshots, [0.6, 1.2, 1.9, scene.markers[0].time])
else:
    scene.render()
