"""Audio signals: the loudness, bands, hits and onsets of a track, and the properties they drive.

``rhythm.ogg`` is a quiet bar (its first two seconds) and then a beat at 120
BPM: a kick on every beat, a hi-hat between them and a bass line. The first
segment shows each signal on a meter and the onsets on a strip of the track;
the second plays the track again and lets the same signals drive scale,
opacity, rotation, jitter, color, position and a readout. Signals read 0
before the track plays and after it ends.
"""

import os

from gaanim import Falloff, Scene, Updater, Zones, computed

RHYTHM = "assets/rhythm.ogg"
LEAD = 0.3
HOLD = 0.3

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


def rows(count, label_share="5fr", stage_share="3fr"):
    """``count`` rows of a code column and a stage column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=[label_share, stage_share], column_gap="48px", padding="page_padding")
    )


def label(text, zone):
    scene.text(text, role="code").place(zone, anchor="right")


def meter(zone, signal):
    """A horizontal meter across ``zone`` whose fill follows ``signal`` from its left end."""
    mask = scene.geometry.rounded_rect(zone.width, 0.36, 0.18).no_fill().stroke(rule, 0.03).move_to(*zone.center)
    scene.geometry.fill_level(mask, accent, signal, direction="left")


def play(music):
    """Play the whole track from the cursor, with a moment before and after."""
    scene.wait(LEAD)
    scene.play(music)
    scene.wait(music.duration + HOLD)


# Each signal is a Computed of the time from 0 to 1. level follows the
# loudness (smoothing keeps a share of the previous frame; range_db measures
# in decibels), band one range of frequencies, and pulse jumps to 1 at each
# onset and fades out over decay seconds. Here fill_level draws them.
scene.segment("signals")
grid = rows(10)
music = scene.media.audio(RHYTHM)
label("music = scene.media.audio(RHYTHM)", grid[0])
SIGNALS = [
    ("level = music.level()", music.level()),
    ("smooth = music.level(smoothing=0.8)", music.level(smoothing=0.8)),
    ("loud = music.level(range_db=24)", music.level(range_db=24)),
    ("bass = music.band(20, 150)", music.band(20, 150)),
    ("highs = music.band(4000, 10000)", music.band(4000, 10000)),
    ("hits = music.pulse()", music.pulse()),
    ("kicks = music.pulse(0.4, low=40, high=120)", music.pulse(0.4, low=40, high=120)),
]
for index, (text, signal) in enumerate(SIGNALS, start=1):
    label(text, grid[2 * index])
    meter(grid[2 * index + 1], signal)

# beats() lists the onsets in seconds of the track, here on a strip of its
# eight seconds with a playhead; with low and high, only those in that band.
start = scene.cursor + LEAD
for row, (text, onsets) in enumerate(
    [("music.beats()", music.beats()), ("music.beats(low=40, high=120)", music.beats(low=40, high=120))], start=8
):
    label(text, grid[2 * row])
    zone = grid[2 * row + 1]
    (left, y), (right, _) = zone.point("left"), zone.point("right")
    scene.geometry.line(left, y, right, y).stroke(rule, 0.03)
    for onset in onsets:
        x = left + (right - left) * onset / music.duration
        scene.geometry.rect(0.03, 0.4).fill(accent).move_to(x, y)
    head = scene.geometry.dot(0.08).fill(foreground)
    progress = computed(lambda t: min(max(t - start, 0.0), music.duration) / music.duration, inputs=[scene.time])
    head.move_to(computed(lambda p: left + (right - left) * p, inputs=[progress]), y)
play(music)

# The same signals, from a second play of the track, drive anything that
# takes a number that changes with time.
scene.segment("driving")
grid = rows(8, "6fr", "2fr")
music = scene.media.audio(RHYTHM)
level, smooth, loud = music.level(), music.level(smoothing=0.8), music.level(range_db=24)
bass, highs = music.band(20, 150), music.band(4000, 10000)
hits, kicks = music.pulse(), music.pulse(0.4, low=40, high=120)


def stage(index):
    """The stage zone of row ``index`` and its center."""
    zone = grid[2 * index + 1]
    return zone, zone.center


label("disc.scale_to(bass)", grid[0])
zone, center = stage(0)
scene.geometry.circle(0.42).move_to(*center).scale_to(bass)

label("ring.opacity(kicks)", grid[2])
zone, center = stage(1)
scene.geometry.circle(0.36).no_fill().stroke(accent, 0.08).move_to(*center).opacity(kicks)

label("star.add_updater(Updater.rotate(level))", grid[4])
zone, center = stage(2)
star = scene.geometry.star(5, 0.45, 0.2).move_to(*center)
star.add_updater(Updater.rotate(level))

label("square.add_updater(Updater.wiggle(rotation=hits))", grid[6])
zone, center = stage(3)
square = scene.geometry.square(0.6).move_to(*center)
square.add_updater(Updater.wiggle(rotation=hits, frequency=6.0))


def dots(zone):
    cells = zone.split(Zones.columns(6, gap="12px"))
    return scene.geometry.group([scene.geometry.circle(0.2).move_to(*cell.center) for cell in cells])


label('dots.drive("fill",\nFalloff.source(bass).gradient(rule, accent))', grid[8])
dots(stage(4)[0]).drive("fill", Falloff.source(bass).gradient(rule, accent))

label('dots.drive("scale",\nFalloff.source(highs).remap(0.4, 1.4))', grid[10])
dots(stage(5)[0]).drive("scale", Falloff.source(highs).remap(0.4, 1.4))

label("ball.move_to(x, computed(lambda l: y + 0.6 * l,\n                         inputs=[loud]))", grid[12])
zone, (x, y) = stage(6)
y = zone.bottom + 0.1
scene.geometry.line(zone.left, y - 0.1, zone.right, y - 0.1).stroke(rule, 0.03)
scene.geometry.dot(0.1).move_to(x, computed(lambda l: y + 0.6 * l, inputs=[loud]))

label('scene.viz.readout(smooth, format=".2f")', grid[14])
zone, center = stage(7)
scene.viz.readout(smooth, format=".2f").place(zone)

play(music)

# The quiet bar, a kick, just after it, and the hi-hat between two kicks, in
# each segment.
SEEKS = [segment * (LEAD + 8.0 + HOLD) + t for segment in range(2) for t in (1.5, 4.32, 4.45, 4.55)]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
