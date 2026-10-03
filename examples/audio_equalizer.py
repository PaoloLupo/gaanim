"""Audio visualizers: the spectrum and the waveform of a track, drawn your way.

`ritmo.ogg` is a quiet bar (0-2 s of the track) and then a beat at 120 BPM.
`Audio.spectrum()` and `Audio.waveform()` give one signal per band or per
instant, and `scene.viz.equalizer` draws them with a chosen shape, layout
and coloring, or with your own: a star per band laid out on an arc. The
same signals drive a WGSL post-process that draws its own bars along the
bottom edge, and a puck leaves an echo only while the drop plays. The
checks of edge cases run before the scene and print their result at the
bottom. Set GAANIM_SNAPSHOTS to capture exact seeks for visual regression.
"""

import math
import os
from pathlib import Path

from gaanim import BLUE, CORAL, GOLD, WHITE, Falloff, PostProcess, Scene, computed

TRACK = str(Path(__file__).resolve().parent.parent / "docs/fixtures/assets/ritmo.ogg")
DELAY = 0.5

# -- Edge cases, each on a fresh scene ---------------------------------------
checks = []


def expect(name, error, operation):
    """Record whether `operation` raises `error`, or succeeds when `error` is None."""
    try:
        operation()
    except Exception as raised:
        checks.append((name, error is not None and isinstance(raised, error)))
    else:
        checks.append((name, error is None))


def probe():
    scene = Scene(frame=(16, 9))
    return scene, scene.media.audio(TRACK)


def equalize(values=None, **options):
    scene, music = probe()
    scene.viz.equalizer(music.spectrum(8) if values is None else values, **options)


expect("spectrum(0)", ValueError, lambda: probe()[1].spectrum(0))
expect("spectrum(300)", ValueError, lambda: probe()[1].spectrum(300))
expect("banda invertida", ValueError, lambda: probe()[1].spectrum(8, low=500, high=100))
expect("waveform(1)", ValueError, lambda: probe()[1].waveform(1))
expect("waveform span 0", ValueError, lambda: probe()[1].waveform(16, span=0))
expect("waveform solo low", ValueError, lambda: probe()[1].waveform(16, low=100))
expect("32 bandas", None, lambda: len(probe()[1].spectrum(32)) == 32 or 1 / 0)
expect("valores vacíos", ValueError, lambda: equalize([]))
expect("un str no es lista", TypeError, lambda: equalize("abc"))
expect("forma desconocida", ValueError, lambda: equalize(shape="hexagon"))
expect("gap 1", ValueError, lambda: equalize(gap=1.0))
expect("layout que no da (x, y, ángulo)", TypeError, lambda: equalize(layout=lambda i, n: (i, 0)))
expect("forma que no devuelve nada", TypeError, lambda: equalize(shape=lambda s, i, n: None))
expect("opción desconocida", TypeError, lambda: equalize(colour="red"))
def parameters_and_numbers():
    scene = Scene(frame=(16, 9))
    # Values out of 0-1 are clamped; a Parameter animates its bar.
    scene.viz.equalizer([scene.viz.parameter(0.5), 0.2, 1.4, -1.0], layout="mirror")


expect("Parameters y números", None, parameters_and_numbers)
expect("echo con end antes de start", ValueError,
       lambda: Scene(frame=(16, 9)).geometry.circle(1).echo(4, start=2.0, end=1.0))

# -- The scene ---------------------------------------------------------------
scene = Scene(frame=(16, 9), background="#0b1020")
music = scene.media.audio(TRACK)
spectrum = music.spectrum(32, smoothing=0.35)
label = lambda text, x, y: scene.text(text).fill("#94a3b8").scale_to(0.26).move_to(x, y)

# 1. Bars in a row, colored across the group from blue to gold.
bars = scene.viz.equalizer(spectrum, center=(-4.2, 2.6), width=6.4, height=1.7,
                           fill=Falloff.index().gradient(BLUE, GOLD))
label('shape="bar", fill=Falloff.index()', -4.2, 1.45)

# 2. Mirrored capsules, each colored by its own loudness.
caps = scene.viz.equalizer(music.spectrum(20, smoothing=0.35), shape="capsule", layout="mirror",
                           center=(4.2, 2.6), width=6.4, height=1.9,
                           value_colors=["#22d3ee", "#a78bfa", "#f43f5e"])
label('"capsule", layout="mirror", value_colors', 4.2, 1.45)

# 3. Dots around a disc that breathes with the bass.
disc = scene.geometry.circle(0.75).fill("#1e293b").no_stroke().move_to(-4.2, -0.9)
disc.scale_to(computed(lambda b: 1.0 + 0.25 * b, inputs=[music.band(20, 150, smoothing=0.4)]))
ring = scene.viz.equalizer(music.spectrum(40, smoothing=0.3), shape="dot", layout="radial",
                           center=(-4.2, -0.9), radius=1.05, height=0.6, fill=[CORAL, GOLD])
label('"dot", layout="radial"', -4.2, -2.75)

# 4. The last two seconds of loudness, as a trace and as a filled area.
trace = scene.viz.equalizer(music.waveform(96, span=2.0, smoothing=0.2), shape="line", layout="mirror",
                            center=(4.2, -0.4), width=6.4, height=1.3, fill="#38bdf8")
floor = scene.viz.equalizer(music.waveform(96, span=2.0, low=20, high=150), shape="area",
                            center=(4.2, -1.75), width=6.4, height=0.7, fill="#f59e0b80")
label('waveform: "line" en espejo y "area" del bajo', 4.2, -2.35)


# 5. Your own shape and layout: a star per band along an arc.
def arc(index, count):
    angle = math.pi * (0.85 - 0.7 * index / (count - 1))
    return 1.6 * math.cos(angle), -4.6 + 1.6 * math.sin(angle), angle


stars = scene.viz.equalizer(music.spectrum(9, smoothing=0.3), layout=arc, height=0.5, stretch=False,
                            shape=lambda s, i, n: s.geometry.star(5, 0.25, 0.1).fill(GOLD).no_stroke())
label("shape=función, layout=función", 0, -3.95)

# 6. A puck whose trail exists only while the drop plays (2.5-4.5 s of the scene).
puck = scene.geometry.circle(0.18).fill(WHITE).no_stroke().move_to(-1.2, 3.7)
puck.echo(8, delay=0.05, decay=0.75, start=DELAY + 2.0, end=DELAY + 4.0)
label("echo(start=, end=): estela solo en el drop", 0, 3.25)

# 7. The same spectrum in a shader: 16 bars along the bottom edge.
uniforms = {f"b{i}": value for i, value in enumerate(music.spectrum(16, smoothing=0.35))}
pick = "\n".join(f"    if (i == {i}u) {{ return gaanim_uniforms.b{i}; }}" for i in range(16))
scene.canvas.post = PostProcess.shader(f"""
fn band(i: u32) -> f32 {{
{pick}
    return 0.0;
}}

fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{
    let color = gaanim_scene(uv);
    let column = min(u32(uv.x * 16.0), 15u);
    let inside = fract(uv.x * 16.0);
    let height = 0.06 * band(column);
    if (uv.y > 1.0 - height && inside > 0.15 && inside < 0.85) {{
        let tint = mix(vec3<f32>(0.13, 0.83, 0.93), vec3<f32>(0.96, 0.25, 0.37), band(column));
        return vec4<f32>(mix(color.rgb, tint, 0.85), 1.0);
    }}
    return color;
}}
""", uniforms=uniforms)

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.3).move_to(5.6, -3.95)

# -- Timeline ----------------------------------------------------------------
scene.wait(DELAY)
scene.play(music)
scene.play([puck.animate.move_to(1.2, 3.7).duration(1.0)])
scene.play([puck.animate.move_to(-1.2, 3.7).duration(1.0)])
scene.play([puck.animate.move_to(1.2, 3.7).duration(1.0)])
scene.play([puck.animate.move_to(-1.2, 3.7).duration(1.0)])
scene.play([puck.animate.move_to(1.2, 3.7).duration(1.0)])
scene.wait(music.duration - 5.0)
scene.wait(0.5)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.3, 1.6, 3.0, 4.3, 5.6, 8.0])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
