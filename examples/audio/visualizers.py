"""Audio visualizers: equalizer shapes, layouts and fills, and the spectrum, waveform and samples they draw.

``scene.viz.equalizer`` draws one element per signal, each as long as its
value. Every segment plays a few seconds of drums and bass under a set of
tiles, and each tile is labelled with what it changes in the call shown at
the top of the segment. The last segment compares the signals a track
offers: its spectrum, its recent loudness, its sound wave and, through a
shader, its spectrogram.
"""

import math
import os

from gaanim import Falloff, PostProcess, Scene, Zones, computed

BEAT = "assets/beat.ogg"  # six seconds of drums and bass at 120 BPM
SECONDS = 3.0
LEAD = 0.3
HOLD = 0.3

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
chart = scene.colors.chart
rule = scene.colors.rule
success = scene.colors.success
warning = scene.colors.warning
danger = scene.colors.danger


def tiles(header, labels, columns=2):
    """A header row with the shared call, then one tile per label: a stage above its code."""
    page = scene.layout.zones(Zones.rows(["1fr", "11fr"], gap="24px", padding="page_padding"))
    scene.text(header, role="code").place(page[0], anchor="left")
    rows = len(labels) // columns
    grid = page[1].split(Zones.grid(rows=rows, columns=columns, row_gap="36px", column_gap="48px"))
    stages = []
    for cell, text in zip(grid, labels):
        stage, caption = cell.split(Zones.rows(["3fr", "1fr"], gap="36px"))
        scene.text(text, role="code").place(caption, anchor="top")
        stages.append(stage)
    return stages


def box(zone):
    """The keyword arguments that fit an equalizer row in ``zone``."""
    return {"center": zone.center, "width": zone.width, "height": zone.height}


def play(music):
    """Play the clip after a moment, then hold its last frame."""
    scene.wait(LEAD)
    scene.play(music)
    scene.wait(HOLD)


# shape picks what each element is: a bar, a capsule, a dot at its tip, one
# line through the tips or that line filled down. A function makes your own
# element, stretched along the slot or, with stretch=False, scaled whole.
def star(scene, index, count):
    return scene.geometry.star(5, 0.3, 0.13)


scene.segment("shapes")
music = scene.media.audio(BEAT, duration=SECONDS)
spectrum = music.spectrum(16, smoothing=0.3)
SHAPES = [
    ('shape="bar"', {"shape": "bar"}),
    ('shape="capsule"', {"shape": "capsule"}),
    ('shape="dot"', {"shape": "dot"}),
    ('shape="line"', {"shape": "line"}),
    ('shape="area"', {"shape": "area"}),
    # Scaled whole, the stars stay in their slots when no taller than a slot is wide.
    ("shape=star, stretch=False", {"shape": star, "stretch": False, "height": 0.6}),
]
stages = tiles("spectrum = music.spectrum(16, smoothing=0.3)", [text for text, _ in SHAPES])
for stage, (_, options) in zip(stages, SHAPES):
    scene.viz.equalizer(spectrum, fill=accent, **{**box(stage), **options})
play(music)


# layout places the elements: in a row growing up, mirrored about a middle
# line, or around a circle growing out. A function returns where each one
# starts and the angle it grows at. gap is the share of each slot left empty.
scene.segment("layouts")
music = scene.media.audio(BEAT, duration=SECONDS)
spectrum = music.spectrum(24, smoothing=0.3)
LAYOUTS = ['layout="row"', 'layout="mirror"', 'layout="radial"',
           'layout="radial",\nshape="dot"', "layout=arc", 'layout="row",\ngap=0.7']
stages = tiles("scene.viz.equalizer(spectrum, fill=accent, ...)", LAYOUTS, columns=3)
size = min(stages[2].width, stages[2].height)


def arc(index, count):
    """Start each element on an arch over the fifth stage, growing outward."""
    angle = math.pi * (0.9 - 0.8 * index / (count - 1))
    (x, _), bottom = stages[4].center, stages[4].bottom
    return x + size * 0.45 * math.cos(angle), bottom + size * 0.45 * math.sin(angle), angle


def ring(stage, **options):
    return {"layout": "radial", "center": stage.center, "radius": size * 0.25, "height": size * 0.22, **options}


for stage, options in zip(stages, [
    box(stages[0]),
    {"layout": "mirror", **box(stages[1])},
    ring(stages[2]),
    ring(stages[3], shape="dot"),
    {"layout": arc, "height": size * 0.4, "thickness": 0.07},
    {"gap": 0.7, **box(stages[5])},
]):
    scene.viz.equalizer(spectrum, fill=accent, **options)
play(music)

# fill takes one paint, a list cycled over the elements, a ramp across the
# group or a function of the index; value_colors colors each element by its
# own value instead.
scene.segment("fills")
music = scene.media.audio(BEAT, duration=SECONDS)
spectrum = music.spectrum(16, smoothing=0.3)
FILLS = [
    ("fill=foreground", {"fill": foreground}),
    ("fill=[accent, chart]", {"fill": [accent, chart]}),
    ("fill=Falloff.index()\n.gradient(rule, accent)", {"fill": Falloff.index().gradient(rule, accent)}),
    ("fill=lambda i, n:\naccent if i < n / 4 else rule", {"fill": lambda i, n: accent if i < n / 4 else rule}),
    ("value_colors=[rule, accent]", {"value_colors": [rule, accent]}),
    ("value_colors=\n[success, warning, danger]", {"value_colors": [success, warning, danger]}),
]
stages = tiles('scene.viz.equalizer(spectrum, shape="capsule", ...)', [text for text, _ in FILLS])
for stage, (_, options) in zip(stages, FILLS):
    scene.viz.equalizer(spectrum, shape="capsule", **box(stage), **options)
play(music)

# The signals: a spectrum against the loudest band instead of each band's
# own loud end, or over a range of frequencies; the loudness of the last
# seconds, of the whole track or of a band; and the sound wave itself, which
# a polyline draws. A shader reads the whole spectrogram with audio=.
SPECTROGRAM = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    // Left two seconds ago, right now; low frequencies at the bottom.
    let energy = gaanim_audio_spectrum(1.0 - uv.y, 2.0 * (1.0 - uv.x));
    return gaanim_scene(uv) * energy;
}
"""

scene.segment("signals")
music = scene.media.audio(BEAT, duration=SECONDS)
stages = tiles("scene.viz.equalizer(signals, fill=accent, ...)", [
    'music.spectrum(24,\nnormalize="global")',
    "music.spectrum(24, low=40,\nhigh=400)",
    'music.waveform(64, span=2.0)\nshape="line", layout="mirror"',
    'music.waveform(64, low=20,\nhigh=150), shape="area"',
    "music.samples(160, span=0.03)\nscene.geometry.polyline(...)",
    "panel.shader_effect(spectrogram)\nPostProcess.shader(..., audio=music)",
])
scene.viz.equalizer(music.spectrum(24, normalize="global"), fill=accent, **box(stages[0]))
scene.viz.equalizer(music.spectrum(24, low=40, high=400), fill=accent, **box(stages[1]))
scene.viz.equalizer(music.waveform(64, span=2.0), shape="line", layout="mirror", fill=accent, **box(stages[2]))
scene.viz.equalizer(music.waveform(64, low=20, high=150), shape="area", fill=accent, **box(stages[3]))

# The wave as an oscilloscope: one point per sample of the last 30 ms.
stage = stages[4]
wave = music.samples(160, span=0.03)
left, (_, middle) = stage.left, stage.center
scene.geometry.polyline([
    (left + stage.width * index / (len(wave) - 1), computed(lambda v: middle + 0.5 * stage.height * v, inputs=[sample]))
    for index, sample in enumerate(wave)
]).stroke(accent, 0.03)

stage = stages[5]
panel = scene.geometry.rect(stage.width, stage.height).fill(accent).no_stroke().move_to(*stage.center)
spectrogram = PostProcess.shader(SPECTROGRAM, audio=music)
panel.shader_effect(spectrogram, margin=0.0)
play(music)

# Each segment on a kick and on the hi-hat after it.
SEEKS = [segment * (LEAD + SECONDS + HOLD) + t for segment in range(4) for t in (2.32, 2.56)]

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, SEEKS)
else:
    scene.render()
