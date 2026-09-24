"""WGSL post-processing: an animated vignette with chromatic aberration."""

import os

from gaanim import BLUE, GOLD, WHITE, PostProcess, Scene


lens = PostProcess.shader("""
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    // Shift red and blue apart by a few output pixels, more at the edges.
    let from_center = uv - vec2<f32>(0.5);
    let shift = from_center * (6.0 + 3.0 * sin(time * 2.0)) / resolution;
    let base = gaanim_scene(uv);
    let red = gaanim_scene(uv + shift).r;
    let blue = gaanim_scene(uv - shift).b;
    let vignette = smoothstep(0.85, 0.3, length(from_center * vec2<f32>(1.0, 0.8)));
    return vec4<f32>(vec3<f32>(red, base.g, blue) * vignette, base.a);
}
""")

scene = Scene(frame=(16, 9), background="#0b1020", post=lens)

scene.segment("lens")
title = scene.text("Post-processing", role="title").fill(WHITE).move_to(0, 2.5)
ring = scene.geometry.circle(1.6).stroke(GOLD, 0.12).no_fill()
dot = scene.geometry.circle(0.45).fill(BLUE)
scene.play([
    title.animate.write().duration(0.8),
    ring.animate.grow_from_center().duration(0.8),
    dot.animate.fade_in().duration(0.8),
])
scene.wait(1.0)

# The same drawing without the lens, for comparison.
scene.segment("plain", post=False)
scene.reuse(title, ring, dot)
scene.play([dot.animate.shift_by(3, 0).duration(1.0)])
scene.wait(0.5)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    scene.snapshots(snapshots, [0.4, 1.6, 2.4, 3.2])
else:
    scene.render()
