"""Full-frame shader work for the runtime performance harness.

A WGSL background with a few octaves of noise under 300 moving neon
shapes, then a post-process chain on the whole frame: scale custom passes
(an RGB split with a 9-tap sample whose distance follows a Parameter),
multipass bloom, chromatic aberration, a color grade, animated grain and a
vignette. Every pass reads and writes the full frame, so the cost tracks
resolution and pass count. Set GAANIM_BENCHMARK_SCALE to add custom passes;
pass it to `tests/benchmark_runtime.py --scene examples/performance_post.py`.
"""

import math
import os

from gaanim import CORAL, CYAN, GOLD, PINK, Background, Easing, PostProcess, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
SCALE = max(1, int(os.environ.get("GAANIM_BENCHMARK_SCALE", "1")))
SHAPES = 300

BACKGROUND = """
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y,
    );
}

fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    var p = uv * vec2<f32>(resolution.x / resolution.y, 1.0) * 3.0;
    var value = 0.0;
    var amplitude = 0.5;
    for (var octave = 0; octave < 5; octave++) {
        value += amplitude * noise(p + vec2<f32>(time * 0.15, -time * 0.1));
        p = p * 2.03;
        amplitude = amplitude * 0.5;
    }
    let color = mix(vec3<f32>(0.01, 0.02, 0.05), vec3<f32>(0.10, 0.05, 0.18), value);
    return vec4<f32>(color, 1.0);
}
"""

SPLIT = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let d = vec2<f32>(gaanim_uniforms.amount * 6.0, 0.0) / resolution;
    var blurred = vec4<f32>(0.0);
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            blurred += gaanim_scene(uv + vec2<f32>(f32(i), f32(j)) / resolution);
        }
    }
    let base = blurred / 9.0;
    return vec4<f32>(gaanim_scene(uv + d).r, base.g, gaanim_scene(uv - d).b, base.a);
}
"""

scene =Scene(frame=(16, 9), background=Background.shader(BACKGROUND, fallback="#05070f"), margin=0.4)
split = scene.viz.parameter(0.0)
glow = scene.viz.parameter(0.4)
custom = [PostProcess.shader(SPLIT, uniforms={"amount": split}) for _ in range(SCALE)]
scene.canvas.post = custom + [
    PostProcess.bloom(threshold=0.55, intensity=glow, radius=0.6),
    PostProcess.chromatic_aberration(0.004),
    PostProcess.color_grade(exposure=0.1, saturation=1.2),
    PostProcess.grain(0.06),
    PostProcess.vignette(0.5),
]

colors = (CYAN, PINK, GOLD, CORAL)
shapes = []
for index in range(SHAPES):
    angle = index * 2.399963
    radius = 0.3 + 3.8 * math.sqrt(index / SHAPES)
    shape = (
        scene.geometry.circle(0.12 + 0.06 * (index % 3)).no_fill()
        if index % 2
        else scene.geometry.regular_polygon(3 + index % 4, 0.16)
        .no_fill()
    )
    shape.stroke(colors[index % len(colors)], 0.04).move_to(
        radius * math.cos(angle) * 1.6, radius * math.sin(angle)
    )
    shapes.append(shape)

motion_duration = max(0.1, DURATION)
scene.play(
    [
        split.animate.set(1.0).duration(motion_duration / 2).repeat(1, yoyo=True),
        glow.animate.set(1.2).duration(motion_duration / 2).repeat(1, yoyo=True),
    ]
    + [
        shape.animate.rotate_by(math.pi).scale_by(1.4 if index % 2 else 0.7)
        .duration(motion_duration).easing(Easing.SMOOTH)
        for index, shape in enumerate(shapes)
    ]
)

snapshot_dir = os.environ.get("GAANIM_SNAPSHOTS")
if snapshot_dir:
    scenario = os.environ.get("GAANIM_BENCHMARK_SCENARIO", "seek")
    if scenario == "preview":
        times = [min(index / FPS, DURATION) for index in range(FRAME_COUNT)]
    else:
        # Same coprime stride as performance_benchmark.py: random access
        # without a PRNG.
        times = [((index * 37) % FRAME_COUNT) / FPS for index in range(FRAME_COUNT)]
    scene.snapshots(snapshot_dir, times)
else:
    scene.render()
