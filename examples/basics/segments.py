"""Segments: the chapters of a scene, what carries over between them, and the stops and markers inside them.

``scene.segment`` starts a new page: what earlier segments drew leaves the
frame, unless ``reuse`` adopts it into the new segment or ``persist`` keeps it
in every segment that follows, like the clock at the bottom. ``stop`` pauses a
presentation (a ``loop`` keeps it moving meanwhile) and ``marker`` names an
instant. The text at the top of each page is the code that builds it, and the
last page draws the scene's own map from ``scene.stops`` and ``scene.markers``.
"""

import math
import os

from gaanim import Easing, Scene, Transition, Zones

DRAW = 1.2
HOLD = 0.6
FADE = 0.8

scene = Scene()
accent = scene.canvas.color("accent")
chart = scene.canvas.color("chart")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")
success = scene.canvas.color("success")

# Where each segment starts on the timeline, read from scene.cursor.
starts = {}


def page(*calls):
    """Write ``calls`` at the top of a new page; returns its stage and footer zones."""
    zones = scene.layout.zones(Zones.rows([1.0, "1fr", 0.6], gap="32px", padding="page_padding"))
    scene.text("\n".join(calls), role="code").place(zones[0], anchor="top_left")
    return zones[1], zones[2]


# A segment is a page of the scene; notes are what Presenter View shows for it.
scene.segment("shapes", notes="Three shapes, and a clock that stays.")
starts["shapes"] = scene.cursor
stage, footer = page('scene.segment("shapes", notes="Three shapes, and a clock that stays.")')
circle = scene.geometry.circle(1.3)
square = scene.geometry.square(2.3)
triangle = scene.geometry.regular_polygon(3, 1.5)
stage.arrange(circle, square, triangle, gap="240px")

# persist() keeps the clock and its caption in every later segment, outside
# the transitions between them.
caption = scene.text("scene.persist(clock, caption)", role="code").fill(muted)
clock = scene.viz.readout(scene.time, label="t", format=".1f", unit="s", font_size=0.4)
footer.arrange(caption, clock, gap="64px", justify="start")
scene.play([shape.animate.create() for shape in (circle, square, triangle)]
           + [caption.animate.fade_in(), clock.animate.fade_in()], duration=DRAW)
scene.persist(clock, caption)
scene.marker("drawn")
scene.wait(HOLD)
scene.stop("shapes")

# The square and the triangle leave with the cross fade; reuse() adopts the
# circle into the new segment, which then moves it.
scene.segment("reuse", Transition.cross_fade(FADE))
starts["reuse"] = scene.cursor
stage, _ = page(
    'scene.segment("reuse", Transition.cross_fade(0.8))',
    "scene.reuse(circle)",
    'scene.stop("orbit", loop=orbit.animate.rotate_by(math.tau / 6))',
)
scene.reuse(circle)
x, y = stage.center
orbit = scene.geometry.group([
    scene.geometry.dot(0.2).fill(chart).move_to(x + 2.4 * math.cos(k * math.tau / 6), y + 2.4 * math.sin(k * math.tau / 6))
    for k in range(6)
])
scene.wait(FADE)
scene.play([circle.animate.move_to(x, y), orbit.animate.create()], duration=DRAW)
scene.marker("centered")
# A stop's loop repeats while a presentation rests there; exports play it once.
# A sixth of a turn ends where it starts, so the repeat is seamless.
scene.stop("orbit", loop=orbit.animate.rotate_by(math.tau / 6).duration(1.5).easing(Easing.LINEAR))
scene.wait(HOLD)

scene.segment("map", Transition.cross_fade(FADE))
starts["map"] = scene.cursor
stage, _ = page(
    'scene.segment("map", Transition.cross_fade(0.8))',
    "scene.release(clock, caption)",
    "scene.fade_out_all(0.8)",
)
# The page waits out the cross fade, draws the map, holds it and fades out.
MAP = FADE + DRAW + HOLD + FADE
end = scene.cursor + MAP


def x_at(t):
    return stage.left + 0.2 + (stage.width - 0.4) * t / end


# The map: one bar per segment, the stops above it and the markers below.
mid = stage.center[1]
pieces = []
names = list(starts)
background = scene.canvas.color("background")
for index, name in enumerate(names):
    start = starts[name]
    stop = starts[names[index + 1]] if index + 1 < len(names) else end
    center = (x_at(start) + x_at(stop)) / 2
    bar = scene.geometry.rect(x_at(stop) - x_at(start) - 0.06, 0.9).fill(accent if index % 2 == 0 else chart)
    label = scene.text(f'"{name}"', role="code").fill(background).move_to(center, mid)
    pieces += [bar.move_to(center, mid), label]
for item in scene.stops:
    x = x_at(item.time)
    if item.loop_duration:
        loop = scene.geometry.line(x, mid + 0.8, x_at(item.time + item.loop_duration), mid + 0.8)
        pieces.append(loop.stroke(success, 0.14))
    tick = scene.geometry.line(x, mid + 0.45, x, mid + 1.45).stroke(foreground, 0.04)
    name = scene.text(f'stop("{item.name}")', role="code").move_to(x, mid + 1.8)
    pieces += [tick, name]
for item in scene.markers:
    x = x_at(item.time)
    flag = scene.geometry.polygon([(x, mid - 0.5), (x - 0.2, mid - 0.85), (x + 0.2, mid - 0.85)]).fill(success)
    name = scene.text(f'marker("{item.name}")', role="code").move_to(x, mid - 1.2)
    pieces += [flag, name]
playhead = scene.geometry.line(x_at(scene.cursor), mid - 0.6, x_at(scene.cursor), mid + 0.6).stroke(foreground, 0.06)

scene.launch(playhead.animate.move_to(x_at(end), mid).duration(MAP).easing(Easing.LINEAR))
scene.wait(FADE)
scene.play([piece.animate.fade_in() for piece in pieces], duration=DRAW)
scene.wait(HOLD)
# release() hands the clock back to this segment, so fade_out_all() takes it too.
scene.release(clock, caption)
scene.fade_out_all(FADE)

# Drawing, cross fading and settled on each page, and the final fade.
SEEKS = [0.6, DRAW + 0.3]
SEEKS += [starts["reuse"] + t for t in (0.4, FADE + 0.6, FADE + DRAW + 0.7)]
SEEKS += [starts["map"] + t for t in (0.4, FADE + DRAW + 0.3, MAP - 0.3)]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
