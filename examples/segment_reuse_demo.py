"""Reuse and persist the same drawable across named segments."""

import os

from gaanim import BLUE, GOLD, GRAY, WHITE, Scene, Transition


scene = Scene(1280, 720, background="#0f172a", margin=48)
scene.segment("intro")

title = scene.text("Péndulo simple", role="title").fill(GOLD).at(0, 120)
subtitle = scene.text("Un objeto, varios segmentos", role="subtitle").fill(GRAY).at(0, 45)
scene.play([title.write().duration(0.6), subtitle.fade_in().duration(0.4)])
scene.wait(0.4)

scene.segment("pendulum", Transition.cross_fade(0.5))
scene.reuse(title)
scene.play([title.move_to(0, 260).duration(0.4)])

support = scene.line(-120, 170, 120, 170).stroke(WHITE, 5)
rod = scene.line(0, 170, 0, -90).stroke(WHITE, 4)
bob = scene.circle(34).fill(BLUE).stroke(WHITE, 3).at(0, -125)
scene.play([support.create().duration(0.4), rod.create().duration(0.7)])
scene.play([bob.grow_from_center().duration(0.4)])

# From this cursor onward the title is global and future transitions ignore it.
scene.persist(title)
scene.wait(0.5)

scene.segment("detail", Transition.slide(0.5, "left"))
# The title stays fixed through the incoming transition, then belongs to detail.
scene.release(title)
detail = scene.text("La gravedad restaura el movimiento", role="subtitle").fill(WHITE).at(0, 60)
scene.play([detail.write().duration(0.6)])
scene.wait(0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.3, 1.25, 1.75, 2.7, 3.25, 3.7, 4.2])
else:
    scene.render()
