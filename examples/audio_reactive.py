"""Audio-reactive animation: the track's data animates the scene.

`ritmo.ogg` is a quiet bar (0-2 s of the track) and then a beat at 120 BPM:
a kick on every beat, a hi-hat between them and a bass line. Its signals
drive a scale, a rotation, a jitter, colors, a meter, a readout and the
camera, and its onsets become ticks on a strip. The checks of edge cases run
before the scene and print their result at the bottom. Set GAANIM_SNAPSHOTS
to capture exact seeks for visual regression.
"""

import os
from pathlib import Path

from gaanim import BLUE, GOLD, WHITE, Falloff, Scene, Updater, computed

TRACK = str(Path(__file__).resolve().parent.parent / "docs/fixtures/assets/ritmo.ogg")
DELAY = 0.5

scene = Scene(frame=(16, 9), background="#0b1020")
music = scene.media.audio(TRACK)
level = music.level(smoothing=0.5)
bass = music.band(20, 150, smoothing=0.4)
treble = music.band(4000, 10000)
pulse = music.pulse(decay=0.12)
beats = music.beats()

# -- Edge cases --------------------------------------------------------------
checks = []


def expect(name, error, operation):
    """Record whether `operation` raises `error`, or succeeds when `error` is None."""
    try:
        operation()
    except Exception as raised:
        checks.append((name, error is not None and isinstance(raised, error)))
    else:
        checks.append((name, error is None))


parameter = scene.viz.parameter(1.0)
expect("banda invertida", ValueError, lambda: music.band(200, 100))
expect("banda sobre 11 kHz", ValueError, lambda: music.band(12000, 15000))
expect("smoothing 1", ValueError, lambda: music.level(smoothing=1.0))
expect("decay 0", ValueError, lambda: music.pulse(decay=0))
expect("Falloff con Parameter", ValueError,
       lambda: Falloff.source(computed(lambda p: p, inputs=[parameter])))
expect("rotate con Parameter calculado", ValueError,
       lambda: Updater.rotate(computed(lambda p: p, inputs=[parameter])))
expect("archivo inexistente", ValueError, lambda: scene.media.audio("no/existe.ogg"))
expect("start antes de sonar", None, lambda: music.start is None or 1 / 0)
expect("golpes ordenados", None, lambda: beats == sorted(beats) or 1 / 0)
expect("12 bombos + 12 hi-hats", None, lambda: len(beats) >= 24 or 1 / 0)
expect("duración 8 s", None, lambda: abs(music.duration - 8.0) < 0.01 or 1 / 0)

# -- 1. A disc that breathes with the bass and flashes with the hits --------
disc = scene.geometry.circle(1.0).fill(BLUE).no_stroke().move_to(-5, 1)
disc.scale_to(computed(lambda b: 0.8 + 0.6 * b, inputs=[bass]))
halo = scene.geometry.circle(1.5).no_fill().stroke(GOLD, 0.06).move_to(-5, 1)
halo.opacity(pulse)

# -- 2. A star turned by the loudness and shaken by the hits ----------------
star = scene.geometry.star(5, 0.9, 0.4).fill(GOLD).no_stroke().move_to(-1.5, 1)
star.add_updater(Updater.rotate(computed(lambda l: 3.0 * l, inputs=[level])))
star.add_updater(Updater.wiggle(position=computed(lambda p: 0.15 * p, inputs=[pulse]), frequency=6.0, seed=3))

# -- 3. Dots whose color follows the bass and size the hi-hat ---------------
dots = scene.geometry.group([scene.geometry.circle(0.22).move_to(1.6 + 0.65 * i, 1) for i in range(7)])
dots.drive("fill", Falloff.source(bass).gradient(BLUE, GOLD))
dots.drive("scale", Falloff.source(treble).remap(0.7, 1.6))

# -- 4. A loudness meter and its readout -------------------------------------
meter_frame = scene.geometry.rect(0.5, 2.6).no_fill().stroke("#334155", 0.03).move_to(6.6, 1)
meter_shape = scene.geometry.rect(0.4, 2.5).no_fill().no_stroke().move_to(6.6, 1)
meter = scene.geometry.fill_level(meter_shape, "#86efac", level)
readout = scene.viz.readout(level, label="nivel", format=".2f", font_size=0.32, color=WHITE).move_to(6.0, -0.75)

# -- 5. The onsets as ticks on a strip, with a playhead ----------------------
strip_y = -2.4
width = 13.0
x0 = -6.5
strip = scene.geometry.line(x0, strip_y, x0 + width, strip_y).stroke("#475569", 0.03)
ticks = [
    scene.geometry.line(x0 + width * (DELAY + b) / 9.5, strip_y - 0.25,
                        x0 + width * (DELAY + b) / 9.5, strip_y + 0.25).stroke(GOLD, 0.04)
    for b in beats
]
head = scene.geometry.circle(0.12).fill(WHITE).no_stroke()
head.move_to(computed(lambda t: x0 + width * min(t, 9.5) / 9.5, inputs=[scene.time]), strip_y)
scene.text("golpes de music.beats() sobre la línea de tiempo").fill("#94a3b8").scale_to(0.3).move_to(0, strip_y - 0.55)

# -- 6. The camera breathes with the bass ------------------------------------
scene.camera.bind_2d(zoom=computed(lambda b: 1.0 + 0.04 * b, inputs=[bass]))

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.34).move_to(0, -3.9)
labels = [
    ("bajo → escala, golpes → halo", -5),
    ("nivel → giro, golpes → temblor", -1.5),
    ("bajo → color, agudos → tamaño", 3.55),
]
for text, x in labels:
    scene.text(text).fill("#94a3b8").scale_to(0.28).move_to(x, -0.75)

# -- Timeline ----------------------------------------------------------------
scene.wait(DELAY)  # Signals read 0 until the track plays.
scene.play(music)
scene.wait(music.duration)
scene.wait(1.0)  # After the track ends, everything rests.

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.3, 1.5, 2.5, 2.75, 3.0, 4.26, 6.1, 9.2])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
