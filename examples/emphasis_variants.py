"""Every emphasis variant side by side: blink, broadcast, flashes, spotlight and boundaries."""

import os

from gaanim import BLUE, CYAN, GOLD, GREEN, PINK, PURPLE, WHITE, Easing, Scene

scene = Scene(frame=(16, 9), background="#0f172a")


def label(text, x, y):
    return scene.text(text, role="body").fill("#94a3b8").scale_by(0.55).move_to(x, y)


def sentence(text, x, y):
    return scene.text(text, role="body").fill(WHITE).move_to(x, y)


def card(x, y, width=2.2, height=1.1):
    return scene.geometry.rounded_rect(width, height, 0.2).fill("#1e293b").stroke("#334155", 0.03).move_to(x, y)


# -- Column 1: blink and flash_under -------------------------------------
label("blink(1) and blink(4)", -5.4, 3.6)
short = scene.geometry.rect(0.14, 0.8).fill(WHITE).move_to(-6.4, 2.5)
long = scene.geometry.rect(0.14, 0.8).fill(GOLD).move_to(-4.4, 2.5)

label("flash_under", -5.4, 0.9)
bare = scene.geometry.rounded_rect(1.2, 0.6, 0.15).fill("#1e293b").move_to(-7.0, -0.1)
text_under = sentence("subrayar palabra", -4.5, -0.1)

# -- Column 2: broadcast and animated_boundary ----------------------------
label("broadcast: default and count=2", 0.0, 3.6)
ripple = scene.geometry.dot(0.16).fill(PINK).move_to(-1.3, 2.5)
burst = scene.geometry.regular_polygon(4, 0.2).fill(CYAN).move_to(1.3, 2.5)

label("animated_boundary", 0.0, 0.9)
fast_card = card(-1.4, -0.6)
slow_card = card(1.4, -0.6)
fast_card.animated_boundary([BLUE, PINK], cycle_rate=1.0, width=0.04)
slow_card.animated_boundary([BLUE, PURPLE, CYAN], cycle_rate=0.25, width=0.06, padding=(0.1, 0.3), corner_radius=0.3)

# -- Column 3: flash_around and spotlight ---------------------------------
label("flash_around", 5.4, 3.6)
frame_target = scene.geometry.rounded_rect(1.2, 0.6, 0.15).fill("#1e293b").move_to(4.0, 2.5)
text_around = sentence("un marco", 6.3, 2.5)

label("spotlight on one and on two", 5.4, 0.9)
left = card(4.1, -0.6, 1.8)
right = card(6.3, -0.6, 1.8)

# -- Stage 1: all the flashes at once -------------------------------------
marks = [0.0]
scene.play([
    short.animate.blink(1).duration(1.0),
    long.animate.blink(4).duration(2.0),
    bare.animate.flash_under().duration(1.5),
    text_under["palabra"].animate.flash_under(color=GOLD, width=0.05, gap=0.12, overhang=0.15, time_width=0.7).duration(1.5),
    ripple.animate.broadcast().duration(2.0),
    burst.animate.broadcast(count=2, max_scale=5.0, lag=0.0).duration(2.0),
    frame_target.animate.flash_around().duration(1.5),
    text_around["marco"].animate.flash_around(color=GREEN, width=0.05, padding=(0.05, 0.25), corner_radius=0.2).duration(1.5),
])
marks += [0.5, 1.0, 1.5]
scene.wait(0.3)
marks.append(scene.cursor)

# -- Stage 2: spotlights ---------------------------------------------------
scene.play([scene.fx.spotlight(left, dim=0.5, padding=0.15).duration(1.5)])
marks.append(scene.cursor - 0.75)
scene.play([scene.fx.spotlight([left, right], dim=0.85, padding=0.3, corner_radius=0.3).duration(2.0).easing(Easing.SMOOTH)])
marks.append(scene.cursor - 0.4)
scene.wait(0.4)
marks.append(scene.cursor)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, marks)
else:
    scene.render()
