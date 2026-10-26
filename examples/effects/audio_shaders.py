"""Audio-reactive shader effects: per-object WGSL passes driven by a music track.

Audio signals are ordinary uniforms of ``shader_effect``: an equalizer glows
with the bass, a word ripples with the loudness and a badge splits its colors
on every hit. With ``audio=`` a pass reads the whole track instead, here a
panel that draws the last seconds of the spectrum inside its own outline.
Each label is the code of its variant.
"""

import os

from gaanim import Brush, Falloff, PostProcess, Scene, Zones

scene = Scene()
accent = scene.colors.accent
foreground = scene.colors.foreground
success = scene.colors.success
danger = scene.colors.danger
music = scene.media.audio("assets/rhythm.ogg")

RIPPLE = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let wave = 0.12 * gaanim_uniforms.amount * sin(uv.x * 14.0 + time * 5.0);
    return gaanim_scene(uv + vec2<f32>(0.0, wave));
}
"""

SPLIT = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let d = vec2<f32>(gaanim_uniforms.amount * 0.05, 0.0);
    let r = gaanim_scene(uv + d);
    let g = gaanim_scene(uv);
    let b = gaanim_scene(uv - d);
    return vec4<f32>(r.r, g.g, b.b, max(max(r.a, g.a), b.a));
}
"""

HALO = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    var halo = 0.0;
    let step = 10.0 / resolution;
    for (var i = -3; i <= 3; i++) {
        for (var j = -3; j <= 3; j++) {
            halo += gaanim_scene(uv + vec2<f32>(f32(i), f32(j)) * step).a;
        }
    }
    halo = min(1.0, halo / 49.0 * gaanim_uniforms.glow * 3.0);
    let alpha = max(color.a, halo);
    return vec4<f32>(mix(vec3<f32>(1.0), color.rgb, color.a / max(alpha, 1e-4)), alpha);
}
"""

# With audio=, gaanim_audio_spectrum(x, ago) reads the spectrum at x (0 is
# 20 Hz, 1 about 11 kHz) as it sounded ago seconds before: here the left edge
# is two seconds ago and the bottom the bass, lit in the drawable's own color.
SPECTROGRAM = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let shape = gaanim_scene(uv);
    let energy = gaanim_audio_spectrum(1.0 - uv.y, 2.0 * (1.0 - uv.x));
    return vec4<f32>(shape.rgb * (0.15 + 0.85 * energy), shape.a);
}
"""


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


grid = scene.layout.zones(
    Zones.grid(rows=4, columns=["15fr", "9fr"], column_gap="48px", row_gap="24px", padding="page_padding")
)
stages = [grid[2 * index + 1] for index in range(4)]

label(
    'bass = music.band(20, 150, smoothing=0.4)\nhalo = PostProcess.shader(HALO,\n    uniforms={"glow": bass})\nbars.shader_effect(halo, margin=0.4)'
).place(grid[0], anchor="right")
bass = music.band(20, 150, smoothing=0.4)
halo = PostProcess.shader(HALO, uniforms={"glow": bass})
stage = stages[0]
bars = scene.viz.equalizer(
    music.spectrum(16, smoothing=0.3),
    shape="capsule",
    center=stage.center,
    width=stage.width * 0.8,
    height=stage.height * 0.6,
    fill=Falloff.index().gradient(accent, danger),
)
bars.shader_effect(halo, margin=0.4)

label(
    'loud = music.level(smoothing=0.5)\nripple = PostProcess.shader(RIPPLE,\n    uniforms={"amount": loud})\nword.shader_effect(ripple, margin=0.5)'
).place(grid[2], anchor="right")
loud = music.level(smoothing=0.5)
ripple = PostProcess.shader(RIPPLE, uniforms={"amount": loud})
scene.text("Loudness", size=stages[1].height * 0.55, weight=800).place(stages[1]).shader_effect(ripple, margin=0.5)

label(
    'hit = music.pulse(0.3)\nsplit = PostProcess.shader(SPLIT,\n    uniforms={"amount": hit})\nbadge.shader_effect(split, margin=0.3)'
).place(grid[4], anchor="right")
hit = music.pulse(0.3)
split = PostProcess.shader(SPLIT, uniforms={"amount": hit})
stage = stages[2]
width, height = stage.width * 0.5, stage.height * 0.8
badge = scene.geometry.group([
    scene.geometry.rounded_rect(width, height, height * 0.2).fill(
        Brush.linear([danger, accent, success], start=(-width / 2, 0), end=(width / 2, 0))
    ),
    scene.geometry.star(5, height * 0.38, height * 0.16).fill(foreground),
]).move_to(*stage.center)
badge.shader_effect(split, margin=0.3)

label(
    "scope = PostProcess.shader(SPECTROGRAM,\n    audio=music)\npanel.shader_effect(scope)"
).place(grid[6], anchor="right")
scope = PostProcess.shader(SPECTROGRAM, audio=music)
stage = stages[3]
panel = scene.geometry.rounded_rect(stage.width * 0.9, stage.height * 0.9, 0.2).move_to(*stage.center)
panel.shader_effect(scope)

scene.play(music)
scene.wait(music.duration)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Silence before the track, then moments across it with different loudness and hits.
    scene.snapshots(snapshots, [0.0, 1.0, 2.3, 3.6, 5.0, 6.4, 7.6])
else:
    scene.render()
