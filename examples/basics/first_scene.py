"""A first scene: objects on a stage, and play, wait and cuts moving the timeline cursor.

The top half is the stage. The bottom half charts the script that animates it,
one row per line, labelled with that line: its bar spans the seconds the line
takes on the timeline, and the numbers under the chart are the cursor after
each line. ``play`` starts its animations together at the cursor and moves the
cursor to their end, ``wait`` only moves it, and a setter called once the
timeline has started, like ``box.fill(success)``, is an instant cut.
"""

import os

from gaanim import Easing, Scene, StrokeStyle, Zones

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
muted = scene.colors.muted
rule = scene.colors.rule
success = scene.colors.success

page = scene.layout.zones(Zones.rows(["1fr", "1.35fr"], gap="40px", padding="page_padding"))
stage, chart = page[0], page[1]

# Factories on the scene make objects, each at the origin until it is placed.
# Building them is free: they show up when their entrance animation starts.
ball = scene.geometry.circle(1.2)
box = scene.geometry.square(2.2)
word = scene.text("Hello", size=1.3)
stage.arrange(ball, box, word, gap="200px")

# The script below, as (line, seconds it takes on the timeline).
SCRIPT = [
    ("scene.play(ball.animate.create())", 1.0),
    ("scene.play([box.animate.fade_in(),\n            word.animate.write()])", 1.0),
    ("scene.wait(0.5)", 0.5),
    ("box.fill(success)", 0.0),
    ("scene.play(box.animate.rotate_by(0.8))", 1.0),
    ("scene.play(ball.animate.scale_to(0.6))", 1.0),
    ("scene.wait(1)", 1.0),
]
TOTAL = sum(seconds for _, seconds in SCRIPT)


def draw_chart():
    """Chart SCRIPT under the stage and sweep a playhead across it in real time."""
    tracks = ["2fr" if "\n" in line else "1fr" for line, _ in SCRIPT] + ["1fr"]
    grid = chart.split(Zones.grid(rows=tracks, columns=[8.3, "1fr"], column_gap="32px"))
    lanes = [grid[2 * index + 1] for index in range(len(tracks))]
    left, right = lanes[0].left + 0.1, lanes[0].right - 0.3
    top, bottom = lanes[0].top, lanes[-1].top

    def x_at(t):
        return left + (right - left) * t / TOTAL

    start = 0.0
    times = [start]
    reveals = []
    for index, (line, seconds) in enumerate(SCRIPT):
        scene.text(line, role="code").place(grid[2 * index], anchor="left")
        y = lanes[index].center[1]
        if seconds == 0:
            # A cut takes no time: a tick at the cursor where it happens.
            mark = scene.geometry.rect(0.1, 0.42).fill(success).move_to(x_at(start), y)
            reveals.append(mark.animate.fade_in().duration(0.1).delay(start))
        else:
            # A play fills its bar; a wait leaves the cursor's path dim.
            color = rule if line.startswith("scene.wait") else accent
            bar = scene.geometry.line(x_at(start), y, x_at(start + seconds), y)
            bar.stroke_style(StrokeStyle(color, 0.3, cap="butt")).trim(end=0)
            reveals.append(bar.animate.trim(end=1).duration(seconds).delay(start).easing(Easing.LINEAR))
        start += seconds
        times.append(start)
    for t in sorted(set(times)):
        scene.geometry.line(x_at(t), top, x_at(t), bottom).stroke(rule, 0.01)
        scene.text(f"{t:g}", role="code").fill(muted).move_to(x_at(t), lanes[-1].center[1])
    playhead = scene.geometry.line(x_at(0), top, x_at(0), bottom).stroke(foreground, 0.03)
    reveals.append(playhead.animate.shift_by(x_at(TOTAL) - x_at(0), 0).duration(TOTAL).easing(Easing.LINEAR))
    # launch() starts the chart at the cursor without moving it.
    scene.launch(reveals)


draw_chart()

scene.play(ball.animate.create())
scene.play([box.animate.fade_in(), word.animate.write()])
scene.wait(0.5)
box.fill(success)
scene.play(box.animate.rotate_by(0.8))
scene.play(ball.animate.scale_to(0.6))
scene.wait(1)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # During each line of the script, and settled at the end.
    scene.snapshots(snapshots, [0.5, 1.5, 2.25, 3.0, 4.0, 5.4])
else:
    scene.render()
