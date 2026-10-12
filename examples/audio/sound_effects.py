"""Sound effects: sounds anchored to an animation, placed at a second, or played by a transition.

Each row is a strip of time. The accent bar is an animation as it plays, the
tick is the cursor where the row's code runs, and a dot with ripples marks the
instant its sound starts (smaller ripples for a quieter sound). Rows play one
after another so each sound can be heard on its own. The last segment puts the
three kinds of anchoring to use.
"""

import os

from gaanim import Easing, Scene, Transition, Zones, sequence, stagger

POP = "assets/pop.wav"
TYPING = "assets/typing.wav"
WHOOSH = "assets/whoosh.wav"

WINDOW = 1.4  # seconds each strip spans
LEAD = 0.3  # where on its strip each row's cursor sits
GROW = 0.6  # length of the animation that carries the sound

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
muted = scene.canvas.color("muted")
rule = scene.canvas.color("rule")


class Strip:
    """A strip of WINDOW seconds across ``zone``, with the cursor's tick at LEAD."""

    def __init__(self, zone):
        zone = zone.inset((0, "24px"))
        (self.left, self.y), (self.right, _) = zone.point("left"), zone.point("right")
        scene.geometry.line(self.left, self.y, self.right, self.y).stroke(rule, 0.04)
        scene.geometry.rect(0.04, 0.6).fill(muted).move_to(self.x(LEAD), self.y)
        self.elapsed = scene.geometry.line(self.left, self.y, self.right, self.y).stroke(muted, 0.04)

    def x(self, t):
        """The x of ``t`` seconds into the strip."""
        return self.left + (self.right - self.left) * t / WINDOW

    def bar(self, start, seconds=GROW):
        """A bar that an animation of ``seconds`` draws from ``start``."""
        return scene.geometry.line(self.x(start), self.y, self.x(start + seconds), self.y).stroke(accent, 0.24)

    def sound(self, t, volume=1.0):
        """Mark a sound at ``t``: a dot that appears there and sends two ripples sized by ``volume``."""
        x = self.x(t)
        dot = scene.geometry.dot(0.14).fill(foreground).move_to(x, self.y)
        marks = [dot.animate.fade_in().duration(0.05).delay(t)]
        for lag in (0.0, 0.2):
            ring = scene.geometry.circle(0.14).no_fill().stroke(foreground, 0.05).move_to(x, self.y).opacity(0)
            ripple = ring.animate.keyframes([0, 0.05, 1], scale=[1, 1, 1 + 4 * volume], opacity=[0, 1, 0])
            marks.append(ripple.duration(0.8).delay(t + lag))
        return marks

    def start(self, *marks):
        """Sweep the strip and launch its sound marks, then move the cursor to the tick."""
        self.began = scene.cursor
        scene.launch([self.elapsed.animate.create().duration(WINDOW).easing(Easing.LINEAR), *marks])
        scene.wait(LEAD)

    def finish(self):
        """Wait until the strip has run out."""
        scene.wait(self.began + WINDOW - scene.cursor)


def rows(count):
    """``count`` rows of a code column and a strip column, padded from the frame."""
    return scene.layout.zones(
        Zones.grid(rows=count, columns=["3fr", "2fr"], column_gap="48px", padding="page_padding")
    )


def label(text, zone, anchor="right"):
    scene.text(text, role="code").place(zone, anchor=anchor)


def grow(bar, seconds=GROW):
    """The animation each bar carries: drawing it at a steady pace."""
    return bar.animate.create().duration(seconds).easing(Easing.LINEAR)


# Anim.sound anchors a sound to the animation: it starts when the animation
# starts, wherever a delay or a composition puts it, and offset shifts it.
scene.segment("anim_sound")
grid = rows(7)
label("grow = bar.animate.create()", grid[0])
SOUNDS = [
    # label, delay of the bar, the animation, second of its sound, volume
    ("grow.sound(POP)", 0.0, lambda bar: grow(bar).sound(POP), 0.0, 1.0),
    ("grow.sound(POP, volume=0.4)", 0.0, lambda bar: grow(bar).sound(POP, volume=0.4), 0.0, 0.4),
    ("grow.delay(0.4).sound(POP)", 0.4, lambda bar: grow(bar).delay(0.4).sound(POP), 0.4, 1.0),
    ("grow.sound(POP, offset=0.4)", 0.0, lambda bar: grow(bar).sound(POP, offset=0.4), 0.4, 1.0),
    ("grow.sound(POP, offset=-0.3)", 0.0, lambda bar: grow(bar).sound(POP, offset=-0.3), -0.3, 1.0),
]
for index, (text, delay, build, at, volume) in enumerate(SOUNDS, start=1):
    label(text, grid[2 * index])
    strip = Strip(grid[2 * index + 1])
    bar = strip.bar(LEAD + delay)
    strip.start(*strip.sound(LEAD + at, volume))
    scene.play(build(bar))
    strip.finish()

# In a sequence, each animation brings its own sound.
label("sequence(grow.sound(POP), grow.sound(POP))", grid[12])
strip = Strip(grid[13])
half = GROW / 2
first, second = strip.bar(LEAD, half), strip.bar(LEAD + half, half)
strip.start(*strip.sound(LEAD), *strip.sound(LEAD + half))
scene.play(sequence(grow(first, half).sound(POP), grow(second, half).sound(POP)))
strip.finish()

# scene.media.sfx places a sound at an absolute second, the cursor by
# default. It never moves the cursor, and it can sit before it.
scene.segment("sfx")
grid = rows(5)
label("t = scene.cursor", grid[0])
PLACED = [
    ("scene.media.sfx(POP)", lambda t: scene.media.sfx(POP), 0.0, 1.0),
    ("scene.media.sfx(POP, volume=0.4)", lambda t: scene.media.sfx(POP, volume=0.4), 0.0, 0.4),
    ("scene.media.sfx(POP, at=t + 0.6)", lambda t: scene.media.sfx(POP, at=t + 0.6), 0.6, 1.0),
    ("scene.media.sfx(POP, at=t - 0.3)", lambda t: scene.media.sfx(POP, at=t - 0.3), -0.3, 1.0),
]
for index, (text, place, at, volume) in enumerate(PLACED, start=1):
    label(text, grid[2 * index])
    strip = Strip(grid[2 * index + 1])
    strip.start(*strip.sound(LEAD + at, volume))
    place(scene.cursor)
    strip.finish()

# A transition's sound starts with the transition, at the start of the
# segment it enters. Sounds on real animations: typing under a write, and a
# pop on each dot of a stagger.
scene.segment("in_use", Transition.slide(0.6, "left", sound=WHOOSH))
page = scene.layout.zones(Zones.rows(["1fr", "1fr", "2fr", "1fr", "2fr"], padding="page_padding"))
label('Transition.slide(0.6, "left", sound=WHOOSH)', page[0], "left")
label("words.animate.write().sound(TYPING)", page[1], "left")
words = scene.text("Every sound follows its animation").scale_to(1.6).place(page[2], anchor="left")
label("stagger(*[dot.animate.fade_in().sound(POP) for dot in dots], each=0.25)", page[3], "left")
stage = page[4]
dots = [
    scene.geometry.circle(0.45).move_to(stage.left + 0.45 + index * (stage.width - 0.9) / 5, stage.center[1])
    for index in range(6)
]
scene.wait(0.8)
scene.play(words.animate.write().duration(1.2).sound(TYPING))
scene.play(stagger(*[dot.animate.fade_in().duration(0.2).sound(POP) for dot in dots], each=0.25))
scene.wait(0.6)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # A sound starting in some rows, then each segment settled, and the last
    # segment sliding in, writing and popping.
    scene.snapshots(snapshots, [0.6, 3.8, 5.2, 5.9, 7.75, 8.35, 12.4, 12.9, 13.95, 14.3, 15.4, 16.6, 17.6])
else:
    scene.render()
