"""Shader effects on single drawables: WGSL passes with uniforms, every preset, chains and margins.

``shader_effect`` draws a drawable into a texture of its own, runs WGSL passes
over it and draws the result back in its place, while everything around it
stays as it is. The first segment writes passes whose uniforms are a number,
an animated Parameter and a value over time; the next two apply every
``PostProcess`` preset to the same badge; the last chains passes in both
orders, widens the texture with ``margin`` and removes an effect. Each label
is the code of its variant.
"""

import math
import os

from gaanim import Brush, PostProcess, Scene, Zones, computed

MOVE = 1.6
HOLD = 0.6

scene = Scene()
accent = scene.canvas.color("accent")
foreground = scene.canvas.color("foreground")
success = scene.canvas.color("success")
danger = scene.canvas.color("danger")

# Inside a pass, gaanim_scene(uv) reads the drawable's own texture, uv spans
# its box plus the margin, and time is the scene time in seconds.
RIPPLE = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let wave = gaanim_uniforms.amount * sin(uv.x * 14.0 + time * 5.0);
    return gaanim_scene(uv + vec2<f32>(0.0, wave));
}
"""

# Red and blue drift apart from green by up to 4 % of the texture.
SPLIT = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let d = vec2<f32>(gaanim_uniforms.amount * 0.04, 0.0);
    let r = gaanim_scene(uv + d);
    let g = gaanim_scene(uv);
    let b = gaanim_scene(uv - d);
    return vec4<f32>(r.r, g.g, b.b, max(max(r.a, g.a), b.a));
}
"""

# A soft white halo around what the texture holds, as strong as glow.
HALO = """
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    var halo = 0.0;
    let step = 12.0 / resolution;
    for (var i = -3; i <= 3; i++) {
        for (var j = -3; j <= 3; j++) {
            halo += gaanim_scene(uv + vec2<f32>(f32(i), f32(j)) * step).a;
        }
    }
    halo = min(1.0, halo / 49.0 * gaanim_uniforms.glow * 2.5);
    let alpha = max(color.a, halo);
    return vec4<f32>(mix(vec3<f32>(1.0), color.rgb, color.a / max(alpha, 1e-4)), alpha);
}
"""


def label(code):
    """A variant label: the code that builds it, without markup."""
    return scene.text(code, role="code")


def gallery(rows, columns, caption=0.45):
    """The zones of a grid of panels, each split into its art and the caption under it."""
    grid = scene.layout.zones(Zones.grid(rows=rows, columns=columns, gap="48px", padding="page_padding"))
    return [zone.split(Zones.rows(["1fr", caption])) for zone in grid]


def badge(zone):
    """A gradient card with a star on it, as one group, filling most of ``zone``."""
    width, height = min(zone.width * 0.6, zone.height * 2.2), zone.height * 0.8
    x, y = zone.center
    card = scene.geometry.rounded_rect(width, height, height * 0.2).fill(
        Brush.linear([danger, accent, success], start=(-width / 2, 0), end=(width / 2, 0))
    )
    star = scene.geometry.star(5, height * 0.38, height * 0.16).fill(foreground)
    return scene.geometry.group([card.move_to(x, y), star.move_to(x, y)])


def word(zone, text):
    """A large word in the middle of ``zone``."""
    return scene.text(text, size=min(zone.height * 0.7, zone.width / (0.65 * len(text))), weight=800).place(zone)


# Uniforms are fields of gaanim_uniforms: a number stays fixed, a Parameter
# can be animated, and a Computed (here of scene.time) is evaluated every frame.
uniforms_start = scene.cursor
scene.segment("uniforms")
shift = scene.viz.parameter(0.0)
pulse = computed(lambda t: 0.5 + 0.5 * math.sin(4 * t), inputs=[scene.time])
grid = scene.layout.zones(
    Zones.grid(rows=3, columns=["15fr", "9fr"], column_gap="48px", row_gap="24px", padding="page_padding")
)
label('ripple = PostProcess.shader(RIPPLE,\n    uniforms={"amount": 0.06})\nword.shader_effect(ripple, margin=0.4)').place(grid[0], anchor="right")
ripple = PostProcess.shader(RIPPLE, uniforms={"amount": 0.06})
word(grid[1], "Ripple").shader_effect(ripple, margin=0.4)
label('split = PostProcess.shader(SPLIT,\n    uniforms={"amount": shift})\nbadge.shader_effect(split, margin=0.3)').place(grid[2], anchor="right")
split = PostProcess.shader(SPLIT, uniforms={"amount": shift})
badge(grid[3]).shader_effect(split, margin=0.3)
label('halo = PostProcess.shader(HALO,\n    uniforms={"glow": pulse})\nstar.shader_effect(halo, margin=0.5)').place(grid[4], anchor="right")
halo = PostProcess.shader(HALO, uniforms={"glow": pulse})
art = grid[5]
scene.geometry.star(5, art.height * 0.4, art.height * 0.17).move_to(*art.center).shader_effect(halo, margin=0.5)
scene.wait(HOLD)
# The split grows with the Parameter and closes again.
scene.play([shift.animate.set(1.0)], duration=MOVE / 2)
scene.play([shift.animate.set(0.0)], duration=MOVE / 2)
scene.wait(HOLD)


def presets(variants, caption=0.45):
    """Draw a badge per (label, effect) pair through that effect, in a 2 x 3 grid."""
    for (art, text_zone), (code, effect) in zip(gallery(3, 2, caption), variants):
        label(code).place(text_zone)
        badge(art).shader_effect(effect, margin=0.3)
    scene.wait(MOVE)


# Every PostProcess preset works on a single drawable too. Presets that change
# over time (grain, glitch) renew with the scene time.
styles_start = scene.cursor
scene.segment("presets")
presets([
    ("PostProcess.pixelate(60)", PostProcess.pixelate(60)),
    ("PostProcess.halftone(40)", PostProcess.halftone(40)),
    ("PostProcess.dither(2)", PostProcess.dither(2)),
    ("PostProcess.crt()", PostProcess.crt()),
    ("PostProcess.grain(0.4, size=3)", PostProcess.grain(0.4, size=3)),
    ("PostProcess.glitch(0.8, seed=2)", PostProcess.glitch(0.8, seed=2)),
])

# Color presets: grades of exposure, contrast, saturation and temperature,
# bloom, a vignette and the color fringes of a lens.
grades_start = scene.cursor
scene.segment("color_presets")
presets(
    [
        ("PostProcess.color_grade(\n    saturation=0, contrast=1.4)", PostProcess.color_grade(saturation=0, contrast=1.4)),
        ("PostProcess.color_grade(\n    exposure=-1, temperature=-1)", PostProcess.color_grade(exposure=-1, temperature=-1)),
        ("PostProcess.color_grade(\n    saturation=2, temperature=1)", PostProcess.color_grade(saturation=2, temperature=1)),
        ("PostProcess.bloom(\n    threshold=0.5, intensity=1.5)", PostProcess.bloom(threshold=0.5, intensity=1.5)),
        ("PostProcess.vignette(\n    strength=0.9, softness=0.3)", PostProcess.vignette(strength=0.9, softness=0.3)),
        ("PostProcess\n    .chromatic_aberration(0.05)", PostProcess.chromatic_aberration(0.05)),
    ],
    caption=0.9,
)


# A list of passes runs in order, each on what the previous one left. margin
# widens the texture for passes that reach past the outline: with 0, the halo
# is cut at the star's box. None removes the effect.
def star(art):
    """A star as tall as most of ``art``, in its middle."""
    return scene.geometry.star(5, art.height * 0.46, art.height * 0.2).move_to(*art.center)


aura = PostProcess.shader(HALO, uniforms={"glow": 1.0})
chains_start = scene.cursor
scene.segment("chains")
VARIANTS = [
    (
        "[PostProcess.pixelate(60),\n PostProcess.halftone(40)]",
        lambda art: badge(art).shader_effect([PostProcess.pixelate(60), PostProcess.halftone(40)], margin=0.3),
    ),
    (
        "[PostProcess.halftone(40),\n PostProcess.pixelate(60)]",
        lambda art: badge(art).shader_effect([PostProcess.halftone(40), PostProcess.pixelate(60)], margin=0.3),
    ),
    ("star.shader_effect(aura, margin=0)", lambda art: star(art).shader_effect(aura, margin=0)),
    ("star.shader_effect(aura,\n    margin=0.6)", lambda art: star(art).shader_effect(aura, margin=0.6)),
    ("word.shader_effect([ripple, split])", lambda art: word(art, "Chain").shader_effect([ripple, split], margin=0.4)),
    (
        "badge.shader_effect(crt)\n     .shader_effect(None)",
        lambda art: badge(art).shader_effect(PostProcess.crt()).shader_effect(None),
    ),
]
for (art, text_zone), (code, build) in zip(gallery(3, 2, 0.9), VARIANTS):
    label(code).place(text_zone)
    build(art)
scene.wait(HOLD)
scene.play([shift.animate.set(1.0)], duration=MOVE / 2)
scene.play([shift.animate.set(0.0)], duration=MOVE / 2)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # The uniforms before, during and after the split, then each gallery twice as time renews it.
    scene.snapshots(
        snapshots,
        [uniforms_start + t for t in (0.3, HOLD + MOVE / 2, HOLD + MOVE + 0.3)]
        + [start + t for start in (styles_start, grades_start) for t in (0.3, 1.2)]
        + [chains_start + t for t in (0.3, HOLD + MOVE / 2)],
    )
else:
    scene.render()
