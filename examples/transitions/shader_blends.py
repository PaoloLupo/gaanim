"""Shader transitions: the built-in presets with their settings, a grayscale reveal map, and WGSL of our own.

Shader transitions render each segment on its own and blend the two on the
GPU: a zoom blur, a directional warp, ripples, a glitch, a reveal that
follows an image, or any function written in WGSL. Every segment shows one
card, and the label below the cards is the transition that brought the
current card in.
"""

import os

from gaanim import Scene, Transition, Zones

HOLD = 1.4
INK = "assets/ink.png"

# fn transition(uv) returns the blended color at uv ((0, 0) is the top left);
# gaanim_from and gaanim_to sample the outgoing and incoming segments,
# progress runs from 0 to 1 and the uniforms are f32 fields of gaanim_uniforms.
SWIRL = """
fn transition(uv: vec2<f32>) -> vec4<f32> {
    let center = vec2<f32>(0.5);
    let offset = uv - center;
    let turn = gaanim_uniforms.turns * 6.2831853 * sin(progress * 3.14159265) * (1.0 - length(offset));
    let q = center + vec2<f32>(
        offset.x * cos(turn) - offset.y * sin(turn),
        offset.x * sin(turn) + offset.y * cos(turn),
    );
    return mix(gaanim_from(q), gaanim_to(q), smoothstep(0.35, 0.65, progress));
}
"""

scene = Scene()
background = scene.canvas.color("background")
page = scene.layout.zones(Zones.rows(["6fr", "1fr"], gap="24px", padding="page_padding"))
ROLES = ["accent", "success", "chart", "danger"]
SHAPES = [
    lambda: scene.geometry.circle(1.0),
    lambda: scene.geometry.square(1.6),
    lambda: scene.geometry.regular_polygon(3, 1.0),
    lambda: scene.geometry.star(5, 1.0, 0.45),
    lambda: scene.geometry.regular_polygon(6, 1.0),
]
captions = []


def card(index):
    """Fill the stage with card ``index``: a theme color holding a large dark shape."""
    stage = page[0]
    color = scene.canvas.color(ROLES[index % len(ROLES)])
    scene.geometry.rounded_rect(stage.width, stage.height, 0.3).fill(color).place(stage)
    SHAPES[index % len(SHAPES)]().fill(background).place(stage.inset("96px"), fit="contain")


def caption(*lines):
    """Show ``lines`` of code below the stage from the cursor on, in place of the previous caption.

    A persistent drawable is composited above a shader blend, so the caption,
    on its strip of background, stays sharp.
    """
    for label in captions[-1:]:
        label.opacity(0)
    strip = page[1].inset(("-12px", "-120px", "-120px"))
    rows = page[1].split(Zones.rows(len(lines)))
    label = scene.geometry.group(
        [scene.geometry.rect(strip.width, strip.height).fill(background).place(strip)]
        + [scene.text(line, role="code").place(row, anchor="left") for line, row in zip(lines, rows)]
    )
    scene.persist(label)
    captions.append(label)


def enter(code, transition):
    """Caption ``code`` (one line or a list of lines), start the next card's segment with ``transition``.

    Returns the segment start.
    """
    caption(*([code] if isinstance(code, str) else code))
    start = scene.cursor
    scene.segment(f"card {len(captions)}", transition)
    card(len(captions))
    return start


scene.segment("card 0")
card(0)
scene.wait(HOLD)
starts = []

# Presets and their settings: cross_zoom blurs radially into the incoming
# segment (strength 0..1); directional_warp sweeps it in along (dx, dy), y
# up, with a given smoothness; ripple sends waves across the frame;
# glitch_displace slips bands sideways (strength, bands and seed).
PRESETS = [
    ('Transition.preset("cross_zoom", 0.8)', Transition.preset("cross_zoom", 0.8)),
    ('Transition.preset("cross_zoom", 0.8, strength=0.9)', Transition.preset("cross_zoom", 0.8, strength=0.9)),
    ('Transition.preset("directional_warp", 0.8)', Transition.preset("directional_warp", 0.8)),
    ('Transition.preset("directional_warp", 0.8, dx=0, dy=1)', Transition.preset("directional_warp", 0.8, dx=0, dy=1)),
    ('Transition.preset("ripple", 0.8)', Transition.preset("ripple", 0.8)),
    ('Transition.preset("ripple", 0.8, amplitude=30, speed=20)', Transition.preset("ripple", 0.8, amplitude=30, speed=20)),
    ('Transition.preset("glitch_displace", 0.8)', Transition.preset("glitch_displace", 0.8)),
    ('Transition.preset("glitch_displace", 0.8, strength=1.0, bands=8)',
     Transition.preset("glitch_displace", 0.8, strength=1.0, bands=8)),
    # luma reveals the incoming segment where a grayscale image is darkest
    # first, or brightest with invert=True; softness blurs the edge.
    ('Transition.preset("luma", 0.8, image=INK)', Transition.preset("luma", 0.8, image=INK)),
    ('Transition.preset("luma", 0.8, image=INK, invert=True)', Transition.preset("luma", 0.8, image=INK, invert=True)),
    ('Transition.preset("luma", 0.8, image=INK, softness=0.4)', Transition.preset("luma", 0.8, image=INK, softness=0.4)),
    ('Transition.shader(SWIRL, 0.8, uniforms={"turns": 0.6})', Transition.shader(SWIRL, 0.8, uniforms={"turns": 0.6})),
]
for code, transition in PRESETS:
    starts.append(enter(code, transition))
    scene.wait(HOLD)

# A uniform can also be a Parameter (or Variable, Computed, scene.time),
# read on every frame of the transition.
twist = scene.viz.parameter(0.0)
starts.append(enter(
    ['Transition.shader(SWIRL, 0.8, uniforms={"turns": twist})', "twist.animate.set(1.5).duration(0.8)"],
    Transition.shader(SWIRL, 0.8, uniforms={"turns": twist}),
))
scene.play([twist.animate.set(1.5).duration(0.8)])
scene.wait(HOLD - 0.8)

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Partway through every transition.
    scene.snapshots(snapshots, [start + 0.35 for start in starts])
else:
    scene.render()
