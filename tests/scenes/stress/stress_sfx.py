"""Stress test for anchored sound effects (AU-04).

Probes ``scene.media.sfx``, ``Anim.sound`` and ``Transition(..., sound=)`` at
their limits: sounds at 0 s and at the very end, many sounds at one instant,
delays, sequences, staggers, negative offsets, volume 0 and above 1, a
replaced ``.sound()``, repeat/loop animations, transition sounds (slide,
magic_move, cut) across four segments and a ``link`` that replaces a
transition sound.

Audio never appears in a snapshot, so the scene draws the schedule instead: a
persistent strip at the bottom maps the whole timeline, and every sound cue
gets a numbered tick at the second where it must start, computed from the same
constants that build the scene. The legend lists each cue, and the last
segment shows the ``scene.cursor`` checks and the errors that were expected to
raise. The Python API has no read-back of scheduled audio tracks, so the ticks
show the expected schedule and the cursor checks guard the timing behind it.

The WAV files are synthesized at startup; no audio assets are needed.
"""

import math
import os
import random
import tempfile
import wave

from gaanim import (
    BLUE,
    CORAL,
    CYAN,
    GOLD,
    GRAY,
    GREEN,
    WHITE,
    Easing,
    Scene,
    Transition,
    sequence,
    stagger,
)


def synthesize(name: str, seconds: float, sample) -> str:
    """Write an 8 kHz mono WAV whose samples come from ``sample(t, u)``."""
    folder = os.path.join(tempfile.gettempdir(), "gaanim_stress_sfx")
    os.makedirs(folder, exist_ok=True)
    path = os.path.join(folder, name)
    rate = 8000
    count = max(1, int(seconds * rate))
    frames = bytes(
        max(0, min(255, int(128 + 127 * sample(i / rate, i / count)))) for i in range(count)
    )
    with wave.open(path, "wb") as file:
        file.setnchannels(1)
        file.setsampwidth(1)
        file.setframerate(rate)
        file.writeframes(frames)
    return path


noise = random.Random(11)
WHOOSH = synthesize("whoosh.wav", 0.4, lambda t, u: noise.uniform(-0.6, 0.6) * math.sin(math.pi * u) ** 2)
TYPING = synthesize(
    "typing.wav", 0.3, lambda t, u: 0.7 * math.exp(-((t % 0.06) * 300)) * math.sin(2 * math.pi * 1800 * t)
)
POP = synthesize("pop.wav", 0.12, lambda t, u: 0.8 * (1 - u) * math.sin(2 * math.pi * (600 - 400 * u) * t))
MISSING = os.path.join(tempfile.gettempdir(), "gaanim_stress_sfx", "does_not_exist.wav")

# -- Schedule constants: the scene and the strip both read these -------------

OVERLAP_AT = 0.5  # many sfx at one instant
OVERLAP_VOLUMES = [0.0, 0.3, 0.6, 1.0, 1.5, 2.5]  # includes 0 and > 1
A_FADE_DELAY, A_FADE_DUR = 0.4, 0.6  # the sound follows the delay
B_SHIFT_DUR = 0.5  # a second .sound() replaces the first
C_TURN_DUR, C_OFFSET = 0.5, -0.4  # negative offset that stays >= 0
D_DELAY, D_DUR = 0.3, 0.4  # offset == -delay: the sound lands on the cursor
SEQ_DUR, SEQ_GAP, SEQ_COUNT = 0.3, 0.1, 3
STAG_DUR, STAG_EACH, STAG_COUNT = 0.4, 0.15, 4
REP_DUR, REP_COUNT, REP_GAP = 0.3, 3, 0.1
LOOP_DUR, LOOP_UNTIL = 0.25, 1.0
A_TAIL = 0.25
SLIDE_DUR = 0.5
B_WAIT, B_FILL_DUR, B_TAIL = 0.5, 0.5, 0.5
MOVE_DUR, C_WAIT = 0.6, 1.0
D_WAIT = 0.8

t_a_fade = A_FADE_DELAY
t_b_shift = t_a_fade + A_FADE_DUR
t_c_turn = t_b_shift + B_SHIFT_DUR
t_d = t_c_turn + C_TURN_DUR
t_seq = t_d + D_DELAY + D_DUR
seq_starts = [t_seq + i * (SEQ_DUR + SEQ_GAP) for i in range(SEQ_COUNT)]
t_stag = seq_starts[-1] + SEQ_DUR
stag_starts = [t_stag + i * STAG_EACH for i in range(STAG_COUNT)]
t_rep = stag_starts[-1] + STAG_DUR
rep_span = REP_COUNT * REP_DUR + (REP_COUNT - 1) * REP_GAP
loop_span = math.floor(LOOP_UNTIL / LOOP_DUR + 1e-9) * LOOP_DUR
SEG_B = t_rep + max(rep_span, loop_span) + A_TAIL
t_b_fill = SEG_B + B_WAIT
SEG_C = t_b_fill + B_FILL_DUR + B_TAIL
SEG_D = SEG_C + C_WAIT
TOTAL = SEG_D + D_WAIT

# (time, color, note) for every cue that must sound.
cues: list[tuple[float, object, str]] = [(0.0, GOLD, "sfx at=0")]
cues += [(OVERLAP_AT, GOLD, f"sfx vol={v:g}") for v in OVERLAP_VOLUMES]
cues += [
    (t_a_fade, CYAN, f"delay {A_FADE_DELAY:g}"),
    (t_b_shift, CYAN, "2nd .sound() wins"),
    (t_c_turn + C_OFFSET, CYAN, f"offset {C_OFFSET:g}"),
    (t_d, CYAN, "offset = -delay"),
]
cues += [(t, CYAN, f"sequence #{i + 1}") for i, t in enumerate(seq_starts)]
cues += [(t, CYAN, f"stagger #{i + 1}") for i, t in enumerate(stag_starts)]
cues += [(t_rep, CYAN, "repeat x3 (once)"), (t_rep, CYAN, "loop (once)")]
cues += [
    (SEG_B, CORAL, "slide sound"),
    (t_b_fill, CYAN, "fill vol=0.6"),
    (SEG_C, CORAL, "magic_move sound"),
    (SEG_D, CORAL, "link: typing replaces pop"),
    (TOTAL, GOLD, "sfx at the end"),
]

# -- Persistent schedule strip -----------------------------------------------

scene = Scene(frame=(16, 9), background="#0b1020")
segment_a = scene.segment("anclas")

STRIP_LEFT, STRIP_RIGHT, STRIP_Y = -7.4, 7.4, -3.3


def strip_x(time: float) -> float:
    return STRIP_LEFT + (STRIP_RIGHT - STRIP_LEFT) * time / TOTAL


persistent = [scene.geometry.line(STRIP_LEFT, STRIP_Y, STRIP_RIGHT, STRIP_Y).stroke(GRAY, 0.04)]
for boundary, name in [(0.0, "A"), (SEG_B, "B"), (SEG_C, "C"), (SEG_D, "D"), (TOTAL, "end")]:
    x = strip_x(boundary)
    persistent.append(scene.geometry.line(x, STRIP_Y - 0.35, x, STRIP_Y + 0.35).stroke(WHITE, 0.03))
    persistent.append(scene.text(f"{name} {boundary:.2f}s", size=0.16).fill(GRAY).move_to(x, STRIP_Y - 0.55))

stacked: dict[float, int] = {}
for number, (time, color, _) in enumerate(cues, start=1):
    key = round(time, 6)
    level = stacked.get(key, 0)
    stacked[key] = level + 1
    x = strip_x(time)
    persistent.append(scene.geometry.line(x, STRIP_Y, x, STRIP_Y + 0.25).stroke(color, 0.03))
    # Cues sharing an instant stack upwards; lone neighbours alternate rows.
    y = STRIP_Y + 0.4 + 0.2 * level + (0.2 * (number % 2) if level == 0 else 0.0)
    persistent.append(scene.text(str(number), size=0.15).fill(color).move_to(x, y))

LEGEND_ROWS = 14
for number, (time, color, note) in enumerate(cues, start=1):
    column, row = divmod(number - 1, LEGEND_ROWS)
    line = f"{number:>2}  {time:5.2f}s  {note}"
    persistent.append(scene.text(line, size=0.17).fill(color).move_to(-5.9 + 3.3 * column, 4.1 - 0.27 * row))

playhead = scene.geometry.polygon([(-0.12, 0.0), (0.12, 0.0), (0.0, -0.2)]).fill(WHITE)
playhead.move_to(strip_x(0.0), STRIP_Y + 0.12)
persistent.append(playhead)
scene.persist(*persistent)
scene.play(
    playhead.animate.move_to(strip_x(TOTAL), STRIP_Y + 0.12).duration(TOTAL).easing(Easing.LINEAR),
    advance=False,
)

# -- Expected errors and cursor checks ----------------------------------------

raised: list[str] = []
not_raised: list[str] = []


def expect_error(label: str, action) -> None:
    try:
        action()
    except (ValueError, TypeError):
        raised.append(label)
    else:
        not_raised.append(label)


cursor_problems: list[str] = []


def check_cursor(label: str, expected: float) -> None:
    if abs(scene.cursor - expected) > 1e-6:
        cursor_problems.append(f"{label}: {scene.cursor:.3f} != {expected:.3f}")


# -- Segment A: free and animation-anchored sounds ----------------------------

x0 = 1.0
box_a = scene.geometry.rect(1.0, 0.7).fill(BLUE).move_to(x0, 1.9)
box_b = scene.geometry.rect(1.0, 0.7).fill(BLUE).move_to(x0 + 1.5, 1.9)
box_c = scene.geometry.rect(1.0, 0.7).fill(BLUE).move_to(x0 + 3.0, 1.9)
box_d = scene.geometry.rect(1.0, 0.7).fill(BLUE).move_to(x0 + 4.5, 1.9)
seq_dots = [scene.geometry.circle(0.25).fill(GREEN).move_to(x0 + 0.8 * i, 0.6) for i in range(SEQ_COUNT)]
stag_dots = [scene.geometry.circle(0.25).fill(CYAN).move_to(x0 + 2.8 + 0.8 * i, 0.6) for i in range(STAG_COUNT)]
badge = scene.geometry.circle(0.45).fill(GOLD).move_to(x0 + 0.5, -0.9)
ring = scene.geometry.circle(0.35).stroke(CORAL, 0.08).move_to(x0 + 2.5, -0.9)

# Each of these raises at declaration or play time and schedules nothing.
expect_error("sfx missing file", lambda: scene.media.sfx(MISSING))
expect_error("sfx at=-1", lambda: scene.media.sfx(POP, at=-1.0))
expect_error("sfx volume=-1", lambda: scene.media.sfx(POP, volume=-1.0))
expect_error("sound('')", lambda: box_a.animate.fade_in().sound(""))
expect_error("sound volume=-1", lambda: box_a.animate.fade_in().sound(POP, volume=-1.0))
expect_error("sound offset=nan", lambda: box_a.animate.fade_in().sound(POP, offset=float("nan")))
expect_error("Transition sound=''", lambda: Transition.cut(sound=""))
expect_error("play missing sound", lambda: scene.play(box_a.animate.fade_in().sound(MISSING)))
expect_error("play sound before 0 s", lambda: scene.play(box_a.animate.fade_in().sound(POP, offset=-0.5)))
check_cursor("after rejected plays", 0.0)

scene.media.sfx(POP, at=0.0)
for volume in OVERLAP_VOLUMES:
    scene.media.sfx(POP if volume < 1.0 else WHOOSH, at=OVERLAP_AT, volume=volume)
check_cursor("sfx never moves the cursor", 0.0)

scene.play(box_a.animate.fade_in().duration(A_FADE_DUR).delay(A_FADE_DELAY).sound(POP))
check_cursor("delayed fade", t_b_shift)
scene.play(box_b.animate.shift_by(0.0, -0.4).duration(B_SHIFT_DUR).sound(TYPING).sound(POP, volume=0.8))
check_cursor("replaced sound", t_c_turn)
scene.play(box_c.animate.rotate_by(math.pi / 2).duration(C_TURN_DUR).sound(WHOOSH, offset=C_OFFSET))
check_cursor("negative offset", t_d)
scene.play(box_d.animate.fill(CORAL).duration(D_DUR).delay(D_DELAY).sound(POP, offset=-D_DELAY))
check_cursor("offset = -delay", t_seq)
scene.play(
    sequence(
        *[dot.animate.shift_by(0.0, 0.5).duration(SEQ_DUR).sound(POP, volume=0.7) for dot in seq_dots],
        gap=SEQ_GAP,
    )
)
check_cursor("sequence", t_stag)
scene.play(
    stagger(
        *[dot.animate.shift_by(0.0, -0.5).duration(STAG_DUR).sound(TYPING, volume=0.5) for dot in stag_dots],
        each=STAG_EACH,
    )
)
check_cursor("stagger", t_rep)
scene.play(
    [
        badge.animate.scale_to(1.3).duration(REP_DUR).repeat(REP_COUNT, yoyo=True, delay=REP_GAP).sound(POP),
        ring.animate.shift_by(0.5, 0.0).duration(LOOP_DUR).loop("pingpong", until=LOOP_UNTIL).sound(TYPING),
    ]
)
scene.wait(A_TAIL)
check_cursor("end of segment A", SEG_B)

# -- Segment B: slide sound, then an animation sound --------------------------

scene.segment("slide", Transition.slide(SLIDE_DUR, "left", sound=WHOOSH))
hero_b = scene.geometry.rect(2.0, 1.2).fill(BLUE).move_to(2.0, 1.2).named("hero")
scene.text("B: slide + whoosh", size=0.3).fill(WHITE).move_to(4.0, -0.6)
scene.wait(B_WAIT)
scene.play(hero_b.animate.fill(GOLD).duration(B_FILL_DUR).sound(TYPING, volume=0.6))
scene.wait(B_TAIL)
check_cursor("end of segment B", SEG_C)

# -- Segment C: magic_move sound ----------------------------------------------

segment_c = scene.segment("magic", Transition.magic_move(MOVE_DUR, sound=WHOOSH))
scene.geometry.rect(3.2, 2.0).fill(CORAL).move_to(4.5, 0.6).named("hero")
scene.text("C: magic_move + whoosh", size=0.3).fill(WHITE).move_to(4.0, -1.2)
scene.wait(C_WAIT)
check_cursor("end of segment C", SEG_D)

# -- Segment D: a relinked cut replaces its sound; sfx at the very end --------

segment_d = scene.segment("fin", Transition.cut(sound=POP))
scene.link(segment_c, segment_d, Transition.cut(sound=TYPING))
scene.text("D: cut, relinked (typing replaces pop)", size=0.26).fill(WHITE).move_to(3.9, 2.2)
scene.wait(D_WAIT)
check_cursor("end of segment D", TOTAL)
expect_error("segment w/ missing sound", lambda: scene.segment("broken", Transition.cut(sound=MISSING)))
scene.media.sfx(TYPING, at=scene.cursor, volume=0.8)
check_cursor("final sfx", TOTAL)


def text_lines(items: list[str], per_line: int = 3) -> list[str]:
    return [", ".join(items[i : i + per_line]) for i in range(0, len(items), per_line)] or ["(none)"]


report = [f"raised as expected ({len(raised)}):"] + text_lines(raised)
if not_raised:
    report += [f"DID NOT RAISE ({len(not_raised)}):"] + text_lines(not_raised)
report.append("cursor checks OK" if not cursor_problems else "cursor checks FAILED:")
report += cursor_problems
for row, line in enumerate(report):
    failed = line.startswith(("DID NOT", "cursor checks FAILED")) or (cursor_problems and ":" in line and "!=" in line)
    scene.text(line, size=0.19).fill(CORAL if failed else GREEN).move_to(3.9, 1.5 - 0.32 * row)
# The report is built once every probe ran, at the very end; hold it on screen
# so the last seek captures it.
REPORT_HOLD = 0.6
scene.wait(REPORT_HOLD)

if "GAANIM_SNAPSHOTS" in os.environ:
    seeks = [
        0.05,
        OVERLAP_AT + 0.05,
        1.2,
        3.2,
        4.4,
        5.4,
        SEG_B + 0.25,
        t_b_fill + 0.3,
        SEG_C + 0.3,
        TOTAL - 0.1,
        TOTAL + REPORT_HOLD / 2,
    ]
    scene.snapshots(os.environ["GAANIM_SNAPSHOTS"], seeks)
else:
    scene.render()
