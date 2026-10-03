"""Shader effects on single drawables: Drawable.shader_effect.

Each drawable here is drawn into a texture of its own, run through WGSL
passes and drawn back in its place, while everything around it stays as it
is: a title rippling in time, a group whose colors split as a Parameter
grows, an audio equalizer glowing with the bass, a logo through built-in
presets (pixelate, then a chain with chromatic aberration), and a card
drawn above an effect to show it keeps the draw order. The checks of edge
cases run before the scene and print their result at the bottom. Set
GAANIM_SNAPSHOTS to capture exact seeks for visual regression.
"""

import os
from pathlib import Path

from gaanim import BLUE, CORAL, GOLD, WHITE, Falloff, PostProcess, Scene, Text

TRACK = str(Path(__file__).resolve().parent.parent / "docs/fixtures/assets/ritmo.ogg")

RIPPLE = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let wave = gaanim_uniforms.amount * sin(uv.x * 14.0 + time * 5.0);
    return gaanim_scene(uv + vec2<f32>(0.0, wave));
}
"""

SPLIT = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let d = vec2<f32>(gaanim_uniforms.amount * 0.04, 0.0);
    let r = gaanim_scene(uv + d);
    let g = gaanim_scene(uv);
    let b = gaanim_scene(uv - d);
    return vec4<f32>(r.r, g.g, b.b, max(max(r.a, g.a), b.a));
}
"""

# A soft halo around what the texture holds, as bright as `glow`.
GLOW = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    var halo = 0.0;
    let step = 6.0 / resolution;
    for (var i = -3; i <= 3; i++) {
        for (var j = -3; j <= 3; j++) {
            halo += gaanim_scene(uv + vec2<f32>(f32(i), f32(j)) * step).a;
        }
    }
    halo = min(1.0, halo / 49.0 * gaanim_uniforms.glow * 2.5);
    let tint = vec3<f32>(0.4, 0.8, 1.0);
    let alpha = max(color.a, halo);
    return vec4<f32>(mix(tint, color.rgb, color.a / max(alpha, 1e-4)), alpha);
}
"""

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


def shape():
    return Scene(frame=(16, 9)).geometry.circle(1)


def foreign_parameter():
    other = Scene(frame=(16, 9)).viz.parameter(1.0)
    shape().shader_effect(PostProcess.shader(SPLIT, uniforms={"amount": other}))


expect("margen negativo", ValueError, lambda: shape().shader_effect(PostProcess.grain(), margin=-1))
expect("un str no es un efecto", TypeError, lambda: shape().shader_effect("ripple"))
expect("None quita el efecto", None, lambda: shape().shader_effect(PostProcess.grain()).shader_effect(None))
expect("cadena de pases", None, lambda: shape().shader_effect([PostProcess.pixelate(6), PostProcess.grain()]))
expect("Parameter de otra escena", ValueError, foreign_parameter)
expect("un Text sigue siendo Text", None,
       lambda: isinstance(Scene(frame=(16, 9)).text("a").shader_effect(PostProcess.grain()), Text) or 1 / 0)
expect("WGSL inválido", ValueError, lambda: PostProcess.shader("fn nope() {}"))

# -- The scene ---------------------------------------------------------------
scene = Scene(frame=(16, 9), background="#0b1020")
label = lambda text, x, y: scene.text(text).fill("#94a3b8").scale_to(0.26).move_to(x, y)

# 1. A title rippling with time; the subtitle under it is untouched.
title = scene.text("Ondas por objeto").fill(WHITE).scale_to(1.3).move_to(-3.6, 3.0)
title.shader_effect(PostProcess.shader(RIPPLE, uniforms={"amount": 0.12}), margin=0.4)
label("solo el título ondula", -3.6, 2.1)

# 2. A group whose colors split as a Parameter grows, then close again.
split = scene.viz.parameter(0.0)
coins = scene.geometry.group([
    scene.geometry.circle(0.6).fill(color).no_stroke().move_to(2.6 + 1.0 * i, 2.8)
    for i, color in enumerate([BLUE, GOLD, CORAL])
])
coins.shader_effect(PostProcess.shader(SPLIT, uniforms={"amount": split}), margin=0.3)
label("uniform = Parameter", 3.6, 1.9)

# 3. The audio equalizer glowing with the bass.
music = scene.media.audio(TRACK)
bars = scene.viz.equalizer(music.spectrum(24, smoothing=0.3), center=(-3.6, -0.6), width=6.0, height=1.6,
                           fill=Falloff.index().gradient(BLUE, GOLD))
bars.shader_effect(PostProcess.shader(GLOW, uniforms={"glow": music.band(20, 150, smoothing=0.4)}), margin=0.4)
label("uniform = música (bajo)", -3.6, -1.75)

# 4. A logo through presets: pixelated, then a chain with aberration and grain.
logo = scene.geometry.star(6, 1.0, 0.45).fill(GOLD).no_stroke().move_to(2.6, -0.6)
logo.shader_effect(PostProcess.pixelate(10))
ring = scene.geometry.circle(0.9).no_fill().stroke(CORAL, 0.18).move_to(5.0, -0.6)
ring.shader_effect([PostProcess.chromatic_aberration(0.02), PostProcess.grain(0.2)], margin=0.3)
label("presets: pixelate; cadena de pases", 3.8, -1.75)

# 5. A card drawn after the ripple keeps covering it: the effect keeps its place.
card = scene.geometry.rounded_rect(2.2, 0.7, 0.15).fill("#1e293b").stroke("#475569", 0.03).move_to(-1.8, 2.75)
label("la tarjeta tapa al efecto", -1.8, 2.1)

ok = sum(passed for _, passed in checks)
failed = [name for name, passed in checks if not passed]
summary = f"validaciones {ok}/{len(checks)}" + ("" if not failed else ": falla " + ", ".join(failed))
status = scene.text(summary).fill("#86efac" if not failed else "#fca5a5").scale_to(0.3).move_to(0, -3.8)

# -- Timeline ----------------------------------------------------------------
scene.play(music)
scene.play([split.animate.set(1.0).duration(1.5)])
scene.play([split.animate.set(0.0).duration(1.5)])
scene.wait(music.duration - 3.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.2, 1.5, 2.6, 4.3])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
