"""Music: audio clips played at the cursor, with their length, volume and fades, and cutting on the beat.

Each lane is a strip of time with the clip drawn where it sounds: its height
is the volume and its slopes are the fades, and a playhead sweeps the lanes
as they play. The first segment plays four short clips one after another; the
second plays background music under a beat grid timed by the tempo that the
music's analysis estimates.
"""

import os

from gaanim import Easing, Scene, Zones

BEAT = "assets/beat.ogg"  # six seconds of drums and bass at 120 BPM

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


class Lanes:
    """Strips of ``span`` seconds across the given zones, each with a playhead."""

    def __init__(self, zones, span):
        self.zones, self.span = zones, span
        self.left, self.right = zones[0].left, zones[0].right
        self.playheads = []
        for zone in zones:
            scene.geometry.line(self.left, zone.bottom, self.right, zone.bottom).stroke(rule, 0.04)
            playhead = scene.geometry.line(self.left, zone.bottom, self.left, zone.top).stroke(foreground, 0.04)
            self.playheads.append(playhead)

    def x(self, t):
        return self.left + (self.right - self.left) * t / self.span

    def clip(self, lane, start, seconds, volume=1.0, fade_in=0.0, fade_out=0.0):
        """Draw a clip's gain over time: it rises over ``fade_in`` and falls over ``fade_out``."""
        zone = self.zones[lane]
        y, top = zone.bottom, zone.bottom + zone.height * 0.85 * volume
        end = start + seconds
        points = [(self.x(start), y), (self.x(start + fade_in), top), (self.x(end - fade_out), top), (self.x(end), y)]
        scene.geometry.polygon(points).fill(accent).no_stroke()

    def sweep(self):
        """Move the playheads across the lanes, from the cursor without moving it."""
        distance = self.right - self.left
        scene.launch([
            playhead.animate.shift_by(distance, 0).duration(self.span).easing(Easing.LINEAR)
            for playhead in self.playheads
        ])


def code(text, zone, anchor="left"):
    return scene.text(text, role="code").place(zone, anchor=anchor)


# A clip starts where scene.play puts the cursor. With duration it plays that
# many seconds and the play lasts as long, so each clip starts where the
# previous one ended. volume is a linear gain; fades ramp it from and to 0.
scene.segment("clips")
CLIP = 1.5
CLIPS = [
    ("scene.media.audio(BEAT, duration=1.5)", {}),
    ("scene.media.audio(BEAT, duration=1.5, volume=0.4)", {"volume": 0.4}),
    ("scene.media.audio(BEAT, duration=1.5, fade_in=0.6)", {"fade_in": 0.6}),
    ("scene.media.audio(BEAT, duration=1.5, fade_out=0.6)", {"fade_out": 0.6}),
]
page = scene.layout.zones(Zones.rows(["1fr", "2fr"] * len(CLIPS), gap="12px", padding="page_padding"))
lanes = Lanes([page[2 * index + 1] for index in range(len(CLIPS))], CLIP * len(CLIPS))
for index, (text, options) in enumerate(CLIPS):
    code(text, page[2 * index])
    lanes.clip(index, CLIP * index, CLIP, **options)
lanes.sweep()
for text, options in CLIPS:
    scene.play(scene.media.audio(BEAT, duration=CLIP, **options))
scene.wait(0.4)

# With end the file is cut at that second, but the play does not wait for it:
# the cursor stays, and what follows is scheduled over the music. Here, a beat
# grid: tempo() estimates the music's beat, scene.tempo lines the scene up
# with it, and wait_until moves the cursor to a bar or to a beat.
scene.segment("background")
page = scene.layout.zones(
    Zones.rows(["1fr", "2fr", "1fr", "1fr", "1fr", "1fr", "1fr", "2fr", "1fr"], gap="12px", padding="page_padding")
)
code("music = scene.media.audio(BEAT, end=6.0, fade_in=1.0, fade_out=2.0)", page[0])
lanes = Lanes([page[1]], 6.0)
lanes.clip(0, 0.0, 6.0, fade_in=1.0, fade_out=2.0)
code("scene.play(music)", page[2])
music = scene.media.audio(BEAT, end=6.0, fade_in=1.0, fade_out=2.0)
beat = music.tempo()
code("beat = music.tempo()", page[3])
reading = f"{beat.bpm:.0f} BPM, first beat at {beat.offset:.2f} s, confidence {beat.confidence:.2f}"
scene.text(reading).fill(muted).place(page[3], anchor="right")
code("scene.tempo(beat.bpm, offset=music.start + beat.offset)", page[4])
code("scene.wait_until(beat=4 * bar + step)", page[5])
code("scene.play(dot.animate.grow_from_center().duration(scene.beats(0.5)))", page[6])
bars = page[7].split(Zones.columns(3, gap="96px"))
dots = []
for bar, zone in enumerate(bars):
    cells = zone.split(Zones.columns(4, gap="12px"))
    dots.append([scene.geometry.circle(min(cell.width, cell.height) * 0.42).move_to(*cell.center) for cell in cells])
    code(f"wait_until(bar={bar})", page[8].split(Zones.columns(3, gap="96px"))[bar], "left")

lanes.sweep()
scene.play(music)
scene.tempo(beat.bpm, offset=music.start + beat.offset)
for bar, row in enumerate(dots):
    scene.wait_until(bar=bar)
    for step, dot in enumerate(row):
        scene.wait_until(beat=4 * bar + step)
        scene.play(dot.animate.grow_from_center().duration(scene.beats(0.5)))
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The playhead in each clip, the clips done, then the beat grid filling.
    scene.snapshots(snapshots, [0.7, 2.2, 3.7, 5.2, 6.2, 7.2, 9.2, 11.2, 12.9])
else:
    scene.render()
