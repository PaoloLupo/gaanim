"""Choreography: labels and relative inserts inside a composition, and cuts on a musical beat.

The first segment draws a composition as a timeline: each bar is one of its
animations, drawn while it plays, placed where ``Composition.schedule()``
puts it. The second sets a tempo and starts animations on beats and bars
while a launched sweep keeps running underneath.
"""

import os

from gaanim import Anchor, Easing, Scene, StrokeStyle, Zones, label, sequence

HOLD = 0.6

scene = Scene()
foreground = scene.canvas.color("foreground")
accent = scene.canvas.color("accent")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


def rows(count, label_share):
    """``count`` rows of a label column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[label_share, "1fr"], column_gap="48px", padding="page_padding")
    )


def axis(zone, ticks, scale, origin):
    """Number ``ticks`` along the middle of ``zone``, ``scale`` units apart from ``origin``."""
    y = zone.center[1]
    for index, tick in enumerate(ticks):
        x = origin + index * scale
        scene.geometry.rect(0.02, 0.2).fill(rule).move_to(x, y + 0.2)
        scene.text(tick, role="code").fill(muted).move_to(x, y - 0.22)


# label() names an instant of a sequence; insert() adds an item at a label
# plus an offset, at the start ("<") or end (">") of the previous item, relative
# to the current end ("+=", "-="), or at a number of seconds.
scene.segment("labels")
grid = rows(7, "1fr")
stage = grid[1]
SCALE = stage.width / 2.4
start = stage.left + 0.15


def row_y(row):
    """The height of row ``row``'s stage."""
    return grid[2 * row + 1].center[1]


def bar(row, seconds):
    """A thick line in row ``row`` as long as ``seconds``, and the animation that draws it."""
    line = scene.geometry.line(0, row_y(row), seconds * SCALE, row_y(row))
    line.stroke_style(StrokeStyle(accent, 0.3, cap="butt"))
    return line, line.animate.create().duration(seconds).easing(Easing.LINEAR)


rows_of_bars = [0, 0, 1, 2, 3, 4, 5]
(a_line, a), (b_line, b), (c_line, c), (d_line, d), (e_line, e), (f_line, f), (g_line, g) = (
    bar(row, seconds) for row, seconds in zip(rows_of_bars, [0.8, 0.6, 0.5, 0.7, 0.5, 0.4, 0.4])
)
intro = (
    sequence(a, label("hit"), b)
    .insert(c, at="hit+0.3")
    .insert(d, at="<")
    .insert(e, at=">-0.2")
    .insert(f, at="-=0.4")
    .insert(g, at=0.2)
)
# schedule() resolves every start and label without playing anything; each
# bar moves to where its animation starts.
plan = intro.schedule()
lines = [a_line, b_line, c_line, d_line, e_line, f_line, g_line]
for line, row, entry in zip(lines, rows_of_bars, plan.entries):
    line.move_to(start + entry.start * SCALE, row_y(row), anchor=Anchor.LEFT)
for row, code in enumerate([
    'sequence(a, label("hit"), b)',
    '.insert(c, at="hit+0.3")',
    '.insert(d, at="<")',
    '.insert(e, at=">-0.2")',
    '.insert(f, at="-=0.4")',
    ".insert(g, at=0.2)",
]):
    scene.text(code, role="code").place(grid[2 * row], anchor="right")
hit = start + plan.labels["hit"] * SCALE
scene.geometry.line(hit, row_y(0) + 0.2, hit, grid[11].bottom).stroke(muted, 0.02)
scene.text('"hit"', role="code").move_to(hit, row_y(0) + 0.24, anchor=Anchor.BOTTOM)
axis(grid[13], ["0", "0.5", "1", "1.5", "2"], 0.5 * SCALE, start)
playhead = scene.geometry.line(start, grid[1].top, start, grid[11].bottom).stroke(foreground, 0.03)
# A marker names an instant of the whole timeline: the editor shows it on the
# seek bar and exports accept it in place of seconds.
scene.marker("intro")
scene.play([intro, playhead.animate.shift_by(plan.span * SCALE, 0).duration(plan.span).easing(Easing.LINEAR)])
scene.wait(HOLD)

# tempo() sets the beat grid, four beats to a bar unless beats_per_bar says
# otherwise; wait_until() moves the cursor to a beat or a bar, beats() turns
# beats into seconds, and launch() starts an animation without moving the
# cursor, so the sweep keeps running under everything after it.
scene.segment("beats")
grid = rows(6, "1.3fr")
BEATS = 8
stage = grid[1]
step = (stage.width - 0.3) / BEATS
origin = stage.left + 0.15
scene.tempo(120, offset=scene.cursor)
for row, code in enumerate([
    "scene.wait_until(beat=n)",
    "scene.wait_until(beat=n + 0.5)",
    "scene.wait_until(bar=n)",
    ".duration(scene.beats(1.5))",
    "scene.launch(sweep)",
    "scene.tempo(120, offset=scene.cursor)",
]):
    scene.text(code, role="code").place(grid[2 * row], anchor="right")
axis(grid[11], [str(beat) for beat in range(BEATS + 1)], step, origin)


def at(row, beat):
    """The point of ``row``'s stage at ``beat``."""
    return origin + beat * step, grid[2 * row + 1].center[1]


def pop(row, beat, shape):
    """``shape`` at ``beat`` of ``row``, and a quarter-beat entrance."""
    return shape.move_to(*at(row, beat)).animate.grow_from_center().duration(scene.beats(0.25))


# Cues are (beat, on_bar, animation); each starts on its beat, or on its bar.
cues = []
for beat in range(BEATS):
    cues.append((beat, False, pop(0, beat, scene.geometry.dot(0.1))))
    cues.append((beat + 0.5, False, pop(1, beat + 0.5, scene.geometry.dot(0.1))))
for beat in range(0, BEATS, 4):
    cues.append((beat, True, pop(2, beat, scene.geometry.square(0.36))))
for beat in range(0, BEATS, 2):
    (x, y), (x_end, _) = at(3, beat), at(3, beat + 1.5)
    stroke = scene.geometry.line(x, y, x_end, y).stroke(accent, 0.12)
    cues.append((beat, False, stroke.animate.create().duration(scene.beats(1.5)).easing(Easing.LINEAR)))
track_y = at(4, 0)[1]
scene.geometry.line(origin, track_y, origin + BEATS * step, track_y).stroke(rule, 0.02)
sweeper = scene.geometry.dot(0.14).move_to(origin, track_y)
sweep = sweeper.animate.shift_by(BEATS * step, 0).duration(scene.beats(BEATS)).easing(Easing.LINEAR)

beats_start = scene.cursor
scene.launch(sweep)
for beat, on_bar, animation in sorted(cues, key=lambda cue: cue[0]):
    if on_bar:
        scene.wait_until(bar=beat // 4)
    else:
        scene.wait_until(beat=beat)
    scene.launch(animation)
scene.wait_until(bar=BEATS // 4)
scene.wait(HOLD)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The timeline as items start, overlap and end; the beats one bar in, a
    # bar and a half in, and done.
    scene.snapshots(
        snapshots,
        [scene.markers[0].time + t for t in (0.5, 1.0, 1.35, 1.75, 2.4)]
        + [beats_start + t for t in (1.2, 2.25, 3.4, 4.3)],
    )
else:
    scene.render()
