"""Narration: voiceover blocks timed by their takes, markers in the text, a narration script and a live take.

Each segment shows its code on the left, the drawing it animates on the
right, the text being read below (marker words light up as the voice
reaches them) and the block's take as a strip of time. Without a recorded
take, as here, a block lasts as long as its text takes to read at about 150
words per minute and markers sit where their words fall in it; record takes
from the editor's narration panel and the same code follows your voice.
"""

import os

from gaanim import Easing, Scene, Zones

scene = Scene()
SEEKS = []  # moments to capture, collected as the scene is built
accent = scene.colors.accent
foreground = scene.colors.foreground
muted = scene.colors.muted
rule = scene.colors.rule


def page():
    """The zones of a segment: code, drawing, the text being read and the strip of the take."""
    rows = scene.layout.zones(Zones.rows(["5fr", "2fr", "1fr"], gap="24px", padding="page_padding"))
    code, drawing = rows[0].split(Zones.columns(["3fr", "2fr"], gap="48px"))
    return code, drawing, rows[1], rows[2]


def code(text, zone):
    scene.text(text, role="code").place(zone, anchor="left")


def read(text, zone):
    """The text being read, at the left of ``zone``."""
    return scene.text(text, size=0.5).place(zone, anchor="left")


class Take:
    """A strip of the take of ``vo`` across ``zone``: a bar grows over it and marks show the markers."""

    def __init__(self, vo, zone, spoken):
        # spoken is the Text being read; spoken["word"] selects a word in it.
        (self.left, self.y), (self.right, _) = zone.point("left"), zone.point("right")
        self.vo, self.spoken = vo, spoken
        scene.geometry.line(self.left, self.y, self.right, self.y).stroke(rule, 0.04)
        bar = scene.geometry.line(self.left, self.y, self.right, self.y).stroke(accent, 0.14)
        scene.launch(bar.animate.create().duration(vo.duration).easing(Easing.LINEAR))

    def reach(self, marker):
        """Light up ``marker`` in the text and mark the cursor's place in the take."""
        x = self.left + (self.right - self.left) * (scene.cursor - self.vo.start) / self.vo.duration
        tick = scene.geometry.rect(0.05, 0.5).fill(foreground).move_to(x, self.y)
        scene.launch([tick.animate.fade_in().duration(0.1), self.spoken[marker].animate.fill(accent).duration(0.2)])
        SEEKS.append(scene.cursor + 0.25)


def drawing(zone):
    """A parabola across ``zone`` with a point and a tangent that follow ``x``, from 0 to 1 along it."""
    (cx, _), width, height = zone.center, zone.width * 0.9, zone.height * 0.85
    points = [(cx + width / 2 * u, zone.bottom + height * u * u) for u in (i / 40 - 1 for i in range(81))]
    curve = scene.geometry.polyline(points).no_fill().stroke(foreground, 0.04)
    x = scene.viz.parameter(0.3)
    dot = scene.geometry.point_on_curve(curve, x).scale_to(1.6)
    tangent = scene.geometry.tangent_on_curve(curve, x, length=2.4).stroke(accent, 0.05)
    return curve, x, dot, tangent


# A voiceover block starts at the cursor. Its text is the segment's notes
# unless text= gives it, until() is the time to a marker (a duration that
# ends on it) and wait_until() moves the cursor to it. Leaving the block
# waits for the rest of the take.
NOTES = "A derivative is the slope of a curve at a point."
scene.segment("slope", notes=NOTES)
code_zone, stage, reading, strip = page()
code(
    'scene.segment("slope", notes=NOTES)\n'
    'with scene.voiceover("slope") as vo:\n'
    "    scene.play(curve.animate.create(),\n"
    '        duration=vo.until("slope"))\n'
    '    vo.wait_until("curve")\n'
    "    scene.play(dot.animate.fade_in())\n"
    "    scene.play(tangent.animate.create(),\n"
    '        duration=vo.until("point"))',
    code_zone,
)
curve, x, dot, tangent = drawing(stage)
spoken = read(NOTES, reading)
take_zone, facts_zone = strip.split(Zones.columns(["2fr", "1fr"], gap="48px"))
with scene.voiceover("slope") as vo:
    take = Take(vo, take_zone, spoken)
    # Its length and whether a recorded take backs it are known when it starts.
    code(f"vo.duration == {vo.duration:.1f}\nvo.recorded == {vo.recorded}", facts_zone)
    scene.play(curve.animate.create(), duration=vo.until("slope"))
    take.reach("slope")
    vo.wait_until("curve")
    take.reach("curve")
    scene.play(dot.animate.fade_in().duration(0.3))
    scene.play(tangent.animate.create(), duration=vo.until("point"))
    take.reach("point")
scene.wait(0.5)
SEEKS.append(scene.cursor - 0.1)

# A narration script keeps the texts out of the code: each "## key" section
# of a Markdown file is the text of voiceover("key"). Without the with
# statement, finish() waits for the rest of the take.
scene.segment("script")
code_zone, stage, reading, strip = page()
code(
    'scene.narration_script("assets/script.md")\n'
    'vo = scene.voiceover("tangent")\n'
    'vo.wait_until("moves")\n'
    "scene.play(x.animate.set(0.9),\n"
    '    duration=vo.until("tangent"))\n'
    "vo.finish()",
    code_zone,
)
curve, x, dot, tangent = drawing(stage)
scene.narration_script("assets/script.md")
vo = scene.voiceover("tangent")
section, sentence = reading.split(Zones.rows(2))
code("## tangent", section)
spoken = read(vo.text, sentence)
take = Take(vo, strip, spoken)
scene.play([curve.animate.fade_in(), dot.animate.fade_in(), tangent.animate.fade_in()], duration=0.3)
vo.wait_until("moves")
take.reach("moves")
scene.play(x.animate.set(0.9), duration=vo.until("tangent"))
take.reach("tangent")
vo.finish()
scene.wait(0.5)
SEEKS.append(scene.cursor - 0.1)

# A live take records you presenting: every stop after it becomes a pause
# as long as yours when you recorded, and stays an interactive stop until
# then. The teleprompter shows the script's section named like the segment.
scene.segment("close")
code_zone, stage, reading, strip = page()
code(
    'scene.live_take("talk")\n'
    "for step in steps:\n"
    "    scene.play(step.animate.fade_in())\n"
    "    scene.stop()",
    code_zone,
)
cells = stage.split(Zones.columns(3, gap="24px"))
steps = [scene.geometry.circle(min(cell.width, cell.height) * 0.4).move_to(*cell.center) for cell in cells]
section, sentence = reading.split(Zones.rows(2))
code("## close", section)
read("That is all for today.", sentence)
scene.live_take("talk")
for step in steps:
    scene.play(step.animate.fade_in().duration(0.4))
    scene.stop()
    SEEKS.append(scene.cursor - 0.02)
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
