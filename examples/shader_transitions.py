"""Shader transitions: built-in presets and a custom WGSL blend.

Every segment lasts 1.5 s and each transition takes 0.8 s, so transition k
starts at 1.5 * k and its midpoint is 1.5 * k + 0.4. The persistent badge
stays sharp above every blend.
"""

import os
from pathlib import Path

from gaanim import (
    CORAL,
    CYAN,
    GOLD,
    GREEN,
    PURPLE,
    WHITE,
    Scene,
    Transition,
)


SEGMENT = 1.5
LUMA_MAP = Path(__file__).parent.parent / "docs" / "fixtures" / "assets" / "tinta.png"
scene = Scene(frame=(16, 9), background="#0f172a")

badge = scene.text("gaanim", role="caption").fill(WHITE).move_to(6.4, -3.9)
scene.persist(badge)


def card(title: str, subtitle: str, color) -> None:
    """Static content: seeks mid-transition show both segments at rest."""
    scene.geometry.rect(11, 5.2).fill(color).move_to(0, -0.3)
    scene.geometry.circle(1.1).fill(WHITE).move_to(-3.6, 0.2)
    scene.text(title, role="title").fill(WHITE).move_to(0, 3.3)
    scene.text(subtitle).fill(WHITE).move_to(1.2, -0.6)
    scene.wait(SEGMENT)


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

scene.segment("intro", background="#0f172a")
card("Shaders", "segmento inicial", "#1e3a8a")

scene.segment("zoom", Transition.preset("cross_zoom", 0.8, strength=0.6), background="#3b0764")
card("cross_zoom", "strength=0.6", PURPLE)

scene.segment(
    "warp", Transition.preset("directional_warp", 0.8, dx=0, dy=1), background="#052e16"
)
card("directional_warp", "dx=0, dy=1", GREEN)

scene.segment("ripple", Transition.preset("ripple", 0.8), background="#431407")
card("ripple", "amplitude=100, speed=50", CORAL)

scene.segment(
    "glitch", Transition.preset("glitch_displace", 0.8, strength=0.8), background="#083344"
)
card("glitch_displace", "strength=0.8", CYAN)

scene.segment(
    "luma", Transition.preset("luma", 0.8, image=LUMA_MAP, softness=0.15), background="#422006"
)
card("luma", "image=tinta.png", GOLD)

scene.segment(
    "swirl", Transition.shader(SWIRL, 0.8, uniforms={"turns": 0.6}), background="#0f172a"
)
card("Transition.shader", "un remolino en WGSL", "#1e3a8a")

if snapshots := os.environ.get("GAANIM_SNAPSHOTS"):
    # Midpoint of each transition, plus one at rest.
    scene.snapshots(snapshots, [1.0, 1.9, 3.4, 4.9, 6.2, 7.9, 9.4])
else:
    scene.render()
