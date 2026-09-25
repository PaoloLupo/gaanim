// Fondo procedural de ejemplo para `Background.shader(Path("assets/background.wgsl"))`.
fn gaanim_background(
    uv: vec2<f32>,
    resolution: vec2<f32>,
    time: f32,
) -> vec4<f32> {
    let pulse = 0.5 + 0.5 * sin(time * 2.0);
    return vec4<f32>(0.03, 0.06 + 0.2 * uv.x, 0.14 + 0.3 * pulse, 1.0);
}
