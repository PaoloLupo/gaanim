"""Timeline-driven WGSL scene background with preview/export parity."""

import os

from gaanim import Background, GRAY, Scene, WHITE


SHADER = r"""
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = uv - vec2<f32>(0.5);
    let aspect = resolution.x / resolution.y;
    let q = vec2<f32>(p.x * aspect, p.y);

    // Slow domain warping produces soft, folded motion without sharp visual noise.
    let warped = q + vec2<f32>(
        0.085 * sin(q.y * 4.2 + time * 0.34) + 0.035 * cos(q.x * 3.0 - time * 0.22),
        0.075 * sin(q.x * 3.6 - time * 0.29) + 0.030 * cos(q.y * 5.1 + time * 0.18),
    );
    let broad_fold = 0.5 + 0.5 * sin(
        warped.y * 6.2 + 1.15 * sin(warped.x * 3.3 - time * 0.20),
    );
    let fine_fold = 0.5 + 0.5 * sin(
        warped.x * 4.0 - warped.y * 3.1 + time * 0.16,
    );
    let ribbon = smoothstep(0.48, 0.93, broad_fold) * (0.55 + 0.45 * fine_fold);

    // Keep the middle quiet for text and scene objects; folds remain near the edges.
    let edge_weight = smoothstep(0.18, 0.82, length(q));
    let slate = vec3<f32>(0.030, 0.046, 0.076);
    let muted_lavender = vec3<f32>(0.105, 0.078, 0.155);
    let color = vec3<f32>(0.006, 0.008, 0.014)
        + mix(slate, muted_lavender, fine_fold) * ribbon * edge_weight * 0.72;
    return vec4<f32>(color, 1.0);
}
"""

scene = Scene(frame=(16, 9),
    background=Background.shader(SHADER, fallback="#080A10"),
    margin=0.8,
)

title = scene.text("WGSL BACKGROUND", role="title").fill(WHITE).move_to(0, 0.7)
rule = scene.geometry.line(-3, -0.333333, 3, -0.333333).stroke(GRAY, 0.066667)
scene.play([title.animate.write().duration(0.8), rule.animate.create().duration(0.8)])
scene.wait(4.0)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    count = scene.snapshots(snapshot_dir, [0.0, 1.0, 2.5, 4.0])
    print(f"[gaanim-diff] captured {count} exact seeks in {snapshot_dir}")

scene.render()
