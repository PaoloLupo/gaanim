// Graph paper that scrolls slowly upward: a fine line every half unit and a
// stronger one every two units, measured in scene units.
fn gaanim_background(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {
    let size = gaanim_frame_size(resolution);
    // uv grows downward; adding time moves the paper up.
    let p = uv * size + vec2<f32>(0.0, time * 0.25);
    let pixel = size.x / resolution.x;
    let fine_cell = abs(fract(p * 2.0 + 0.5) - 0.5) * 0.5;
    let bold_cell = abs(fract(p * 0.5 + 0.5) - 0.5) * 2.0;
    let fine = 1.0 - smoothstep(0.0, pixel * 1.5, min(fine_cell.x, fine_cell.y));
    let bold = 1.0 - smoothstep(0.0, pixel * 2.5, min(bold_cell.x, bold_cell.y));
    let paper = vec3<f32>(0.055, 0.075, 0.105);
    let ink = vec3<f32>(0.25, 0.42, 0.6);
    let color = paper + ink * max(fine * 0.35, bold * 0.8);
    return vec4<f32>(color, 1.0);
}
