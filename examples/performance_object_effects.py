"""Per-drawable effects for the runtime performance harness.

24 x scale cards, each with one effect in turn: a custom WGSL
`shader_effect` whose uniform follows a Parameter, a chain of preset passes
(pixelate, chromatic aberration), glow, soft shadow, blur and an echo. Half
the cards move every frame and half stay still, so the run measures both
the per-effect offscreen passes and what the renderer manages to retain.
Over them slide 2 + scale glass panels (frosted glass, backdrop blur, and
liquid glass on metaballs), and a striped band shows through a title by an
alpha matte. Set GAANIM_BENCHMARK_SCALE to grow the counts; pass it to
`tests/benchmark_runtime.py --scene examples/performance_object_effects.py`.
"""

import math
import os

from gaanim import BLUE, CORAL, CYAN, GOLD, GREEN, PINK, WHITE, Easing, PostProcess, Scene


FPS = 30
FRAME_COUNT = max(30, int(os.environ.get("GAANIM_BENCHMARK_FRAMES", "300")))
DURATION = FRAME_COUNT / FPS
SCALE = max(1, int(os.environ.get("GAANIM_BENCHMARK_SCALE", "1")))
CARDS = 24 * SCALE
PANELS = 2 + SCALE

RIPPLE = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let wave = gaanim_uniforms.amount * 0.03 * sin(uv.x * 18.0 + time * 6.0);
    return gaanim_scene(uv + vec2<f32>(0.0, wave));
}
"""

scene = Scene(frame=(16, 9), background="#0b1020", margin=0.4)
colors = (BLUE, GOLD, GREEN, CORAL, CYAN, PINK)
amount = scene.viz.parameter(0.0)

columns = math.ceil(math.sqrt(CARDS * 16 / 9))
rows = math.ceil(CARDS / columns)
cell = min(15.0 / columns, 7.0 / rows)

moving = []
for index in range(CARDS):
    row, column = divmod(index, columns)
    x = (column - (columns - 1) / 2) * cell
    y = ((rows - 1) / 2 - row) * cell - 0.6
    color = colors[index % len(colors)]
    card = scene.geometry.group([
        scene.geometry.rounded_rect(cell * 0.8, cell * 0.6, cell * 0.08).fill(color).stroke(WHITE, 0.02),
        scene.geometry.star(5, cell * 0.2, cell * 0.08).fill(WHITE).no_stroke(),
    ]).move_to(x, y)
    effect = index % 6
    if effect == 0:
        card.shader_effect(PostProcess.shader(RIPPLE, uniforms={"amount": amount}), margin=0.15)
    elif effect == 1:
        card.shader_effect([PostProcess.pixelate(4), PostProcess.chromatic_aberration(0.02)], margin=0.1)
    elif effect == 2:
        card.glow(color, radius=0.2, intensity=1.2)
    elif effect == 3:
        card.shadow("#000000aa", x=0.08, y=-0.08, blur=0.1)
    elif effect == 4:
        card.blur(0.03)
    else:
        card.echo(4, delay=0.05)
    if index % 2 == 0:
        moving.append(card)

title = scene.text("RENDIMIENTO").fill(WHITE).scale_to(1.2).move_to(0, 3.7)
stripes = scene.geometry.group([
    scene.geometry.rect(0.3, 1.4).fill(colors[i % len(colors)]).no_stroke().move_to(-4.5 + 0.3 * i, 3.7)
    for i in range(31)
])
stripes.matte(title)

panels = []
for index in range(PANELS):
    y = -3.2 + 6.0 * index / max(1, PANELS - 1) - 0.6
    kind = index % 3
    if kind == 0:
        panel = scene.geometry.rounded_rect(3.2, 1.4, 0.3).fill("#ffffff18").stroke("#ffffff55", 0.02)
        panel.glass(blur=0.25, refraction=0.1)
    elif kind == 1:
        panel = scene.geometry.rounded_rect(2.6, 1.2, 0.3).fill("#0f172a66").no_stroke()
        panel.backdrop_blur(0.3)
    else:
        drops = [scene.geometry.circle(0.7).move_to(dx, 0) for dx in (-0.6, 0.6)]
        panel = scene.geometry.metaballs(drops, smoothness=0.6).fill("#ffffff12").liquid_glass()
    panels.append(panel.move_to(-6.5, y))

motion_duration = max(0.1, DURATION)
scene.play(
    [amount.animate.set(1.0).duration(motion_duration / 2).repeat(1, yoyo=True)]
    + [
        card.animate.rotate_by(math.tau * (1 if index % 2 else -1)).duration(motion_duration).easing(Easing.LINEAR)
        for index, card in enumerate(moving)
    ]
    + [
        panel.animate.shift_by(13.0, 0.0).duration(motion_duration).easing(Easing.SMOOTH)
        for panel in panels
    ]
    + [stripes.animate.shift_by(1.5, 0.0).duration(motion_duration)]
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
