"""Post-process chains: an animated uniform feeding a custom pass, then finishing presets."""

import os

from gaanim import BLUE, CORAL, GOLD, WHITE, PostProcess, Scene


scene = Scene(frame=(16, 9), background="#0b1020")

# A custom pass whose split distance follows a Parameter.
split = scene.viz.parameter(0.0)
rgb_split = PostProcess.shader(
    """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let shift = vec2<f32>(gaanim_uniforms.amount * 0.02, 0.0);
    let base = gaanim_scene(uv);
    return vec4<f32>(gaanim_scene(uv + shift).r, base.g, gaanim_scene(uv - shift).b, base.a);
}
""",
    uniforms={"amount": split},
)
# Passes run in order: the split, then grain over it, then the vignette.
scene.canvas.post = [rgb_split, PostProcess.grain(0.08, animated=False), PostProcess.vignette(0.6)]

scene.segment("chain")
title = scene.text("Cadena de post-procesos", role="title").fill(WHITE).move_to(0, 2.6)
ring = scene.geometry.circle(1.6).stroke(GOLD, 0.14).no_fill()
dot = scene.geometry.circle(0.5).fill(BLUE).move_to(-3.5, -1.5)
bar = scene.geometry.rect(3.0, 0.5).fill(CORAL).move_to(3.5, -1.5)
scene.wait(0.4)
scene.play([split.animate.set(1.0).duration(0.6).repeat(1, yoyo=True)])
scene.wait(0.4)

# A segment replaces the chain, then another draws without it.
scene.segment("grade", post=[PostProcess.color_grade(exposure=0.3, saturation=0.4), PostProcess.vignette(0.8)])
scene.reuse(title, ring, dot, bar)
scene.wait(1.0)
scene.segment("plain", post=False)
scene.reuse(title, ring, dot, bar)
scene.wait(1.0)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Before the split, at its peak, back, the replaced chain, and no post-process.
    scene.snapshots(snapshots, [0.2, 0.7, 1.2, 1.9, 2.9])
else:
    scene.render()
