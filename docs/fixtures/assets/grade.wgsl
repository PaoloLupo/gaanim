// Corrección de color de ejemplo para `PostProcess.shader(Path("assets/grade.wgsl"))`.
fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let color = gaanim_scene(uv);
    let graded = pow(color.rgb, vec3<f32>(0.9)) * vec3<f32>(1.04, 1.0, 0.94);
    return vec4<f32>(graded, color.a);
}
