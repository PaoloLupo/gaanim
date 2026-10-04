"""A complex scene with every costly feature at once, for the runtime harness.

A WGSL background, 1000 x scale duplicated instances turning, 1000 x scale
live particles, 12 x scale cards with per-drawable shader effects, glow and
shadows, liquid glass melting over them, a title and an equation, and a
post-process chain (custom pass, bloom, grain, vignette) over the frame.
Everything moves for the whole run. GAANIM_BENCHMARK_MOTION_BLUR=<samples>
also turns on canvas motion blur, which multiplies the rendered sub-frames
in exports and snapshots. Pass it to
`tests/benchmark_runtime.py --scene examples/performance_heavy.py`.
"""

import math
import os

from gaanim import (
    BLUE, CORAL, CYAN, GOLD, GREEN, PINK, WHITE,
    Background, Distribution, Easing, Emitter, PostProcess, Scene,
)


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
SCALE = max(1, int(os.environ.get("GAANIM_BENCHMARK_SCALE", "1")))
MOTION_BLUR = int(os.environ.get("GAANIM_BENCHMARK_MOTION_BLUR", "0"))

BACKGROUND = """
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let p = (uv - vec2<f32>(0.5)) * vec2<f32>(resolution.x / resolution.y, 1.0);
    let bands = 0.5 + 0.5 * sin(length(p) * 18.0 - time * 1.5 + 2.0 * sin(p.x * 3.0 + time * 0.4));
    let color = mix(vec3<f32>(0.01, 0.015, 0.04), vec3<f32>(0.06, 0.03, 0.12), bands * smoothstep(0.1, 0.9, length(p)));
    return vec4<f32>(color, 1.0);
}
"""

WAVE = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let wave = gaanim_uniforms.amount * 0.03 * sin(uv.y * 20.0 + time * 5.0);
    return gaanim_scene(uv + vec2<f32>(wave, 0.0));
}
"""

LENS = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let from_center = uv - vec2<f32>(0.5);
    let shift = from_center * gaanim_uniforms.amount * 8.0 / resolution;
    let base = gaanim_scene(uv);
    return vec4<f32>(gaanim_scene(uv + shift).r, base.g, gaanim_scene(uv - shift).b, base.a);
}
"""

scene = Scene(frame=(16, 9), background=Background.shader(BACKGROUND, fallback="#05070f"), margin=0.4)
if MOTION_BLUR:
    scene.canvas.motion_blur(180, samples=MOTION_BLUR)
amount = scene.viz.parameter(0.0)
glow = scene.viz.parameter(0.4)
scene.canvas.post = [
    PostProcess.shader(LENS, uniforms={"amount": amount}),
    PostProcess.bloom(threshold=0.6, intensity=glow, radius=0.5),
    PostProcess.grain(0.05),
    PostProcess.vignette(0.45),
]
colors = (BLUE, GOLD, GREEN, CORAL, CYAN, PINK)

# 1. Instances: one shape drawn many times.
columns = 40 * SCALE
rows = 25
cell = scene.geometry.star(4, 0.1 / math.sqrt(SCALE), 0.04 / math.sqrt(SCALE)).fill(CYAN)
field = scene.geometry.duplicate(cell, Distribution.grid(columns, rows, (14.4 / columns, 0.3)))

# 2. Particles: a burst that lives the whole run plus a steady stream.
cloud = scene.fx.particles(
    Emitter.rect(14, 7).at((0, 0)), rate=0, lifetime=(DURATION, DURATION * 1.2),
    speed=(0.1, 0.5), spread=math.tau, size=(0.03, 0.06), color=GOLD, seed=3,
)
cloud.burst(700 * SCALE)
scene.fx.particles(
    Emitter.circle(2.0).at((0, 0)), rate=100 * SCALE, lifetime=(2.5, 3.5),
    speed=(0.3, 1.2), spread=math.tau, drag=0.4, size=(0.03, 0.06), color=[PINK, CYAN], seed=4,
)

# 3. Cards with per-drawable effects.
cards = []
for index in range(12 * SCALE):
    column, row = index % 6, index // 6
    card = scene.geometry.rounded_rect(1.6, 1.0, 0.15).fill(colors[index % len(colors)]).stroke(WHITE, 0.02)
    card.move_to(-5.5 + 2.2 * column, 1.5 - 1.4 * (row % 3))
    if index % 3 == 0:
        card.shader_effect(PostProcess.shader(WAVE, uniforms={"amount": amount}), margin=0.15)
    elif index % 3 == 1:
        card.glow(colors[index % len(colors)], radius=0.2)
    else:
        card.shadow("#000000aa", blur=0.1)
    cards.append(card)

# 4. Liquid glass drifting over the cards.
drops = [scene.geometry.circle(0.8).move_to(dx, -2.6) for dx in (-6.0, -4.8)]
lens = scene.geometry.metaballs(drops, smoothness=0.7).fill("#ffffff12").liquid_glass()

# 5. Text.
title = scene.text("Escena pesada").fill(WHITE).scale_to(1.0).move_to(0, 3.8).glow(CYAN, radius=0.15)
equation = scene.text.equation(r"\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0}").fill(WHITE).scale_to(0.7).move_to(0, -3.9)

motion_duration = max(0.1, DURATION)
scene.play(
    [
        amount.animate.set(1.0).duration(motion_duration / 2).repeat(1, yoyo=True),
        glow.animate.set(1.2).duration(motion_duration / 2).repeat(1, yoyo=True),
        field.animate.rotate_by(0.3).scale_by(0.9).duration(motion_duration).easing(Easing.SMOOTH),
        title.animate.scale_by(1.1).duration(motion_duration),
    ]
    + [drop.animate.shift_by(11.0, 0.0).duration(motion_duration) for drop in drops]
    + [
        card.animate.rotate_by(0.5 * (1 if index % 2 else -1)).duration(motion_duration)
        for index, card in enumerate(cards)
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
