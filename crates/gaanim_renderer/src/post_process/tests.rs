use super::*;
use crate::background::test_gpu::test_gpu;

#[test]
fn passes_know_whether_they_read_the_time() {
    let shader = |body: &str| {
        PostProcessShader::with_uniforms(
            format!("fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, {body}"),
            ["animated"],
        )
        .unwrap()
    };
    let still = shader("time: f32) -> vec4<f32> { return gaanim_scene(uv); }");
    assert!(!still.reads_time(&[1.0]));
    let moving = shader("time: f32) -> vec4<f32> { return gaanim_scene(uv) * sin(time); }");
    assert!(moving.reads_time(&[0.0]));
    let renamed = shader("t: f32) -> vec4<f32> { return gaanim_scene(uv + vec2<f32>(t)); }");
    assert!(renamed.reads_time(&[0.0]));
    assert!(source_reads_time(
        "fn gaanim_post(uv: vec2<f32>, r: vec2<f32>, _t: f32) -> vec4<f32> { return vec4<f32>(gaanim_time()); }"
    ));
    let gated = moving.clone().time_gated_by("animated");
    assert!(!gated.reads_time(&[0.0]) && gated.reads_time(&[1.0]));

    let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
    let request = |shader: &PostProcessShader, time: f32| PostProcessRequest {
        passes: vec![(shader.clone(), vec![0.0])],
        frame,
        time,
        transition: None,
    };
    assert!(request(&still, 1.0).same_output(&request(&still, 2.0)));
    assert!(!request(&moving, 1.0).same_output(&request(&moving, 2.0)));
    assert!(request(&gated, 1.0).same_output(&request(&gated, 2.0)));
    assert!(!request(&still, 1.0).same_output(&request(&moving, 1.0)));
}

const INVERT: &str = "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
         let color = gaanim_scene(uv + vec2<f32>(1.0, 0.0) / resolution);\n\
         return vec4<f32>(vec3<f32>(1.0) - color.rgb, color.a) + time * 0.0;\n}";

fn shader() -> PostProcessShader {
    PostProcessShader::new(INVERT).unwrap()
}

#[test]
fn validation_accepts_the_documented_contract() {
    assert!(shader().source().contains("gaanim_scene"));
}

#[test]
fn validation_rejects_missing_or_mistyped_entry_functions() {
    assert!(matches!(
        PostProcessShader::new("fn other() {}"),
        Err(PostProcessError::InvalidWgsl(_))
    ));
    let wrong = "fn gaanim_post(uv: vec2<f32>) -> vec4<f32> { return gaanim_scene(uv); }";
    assert!(matches!(
        PostProcessShader::new(wrong),
        Err(PostProcessError::InvalidWgsl(_))
    ));
}

#[test]
fn source_can_be_loaded_from_an_asset_file() {
    let path =
        std::env::temp_dir().join(format!("gaanim_post_process_{}.wgsl", std::process::id()));
    std::fs::write(&path, INVERT).unwrap();
    assert_eq!(
        PostProcessShader::from_file(&path).unwrap().source(),
        INVERT
    );
    std::fs::remove_file(&path).unwrap();
    assert!(matches!(
        PostProcessShader::from_file(&path),
        Err(PostProcessError::ReadSource { .. })
    ));
}

#[test]
fn segments_inherit_disable_or_replace_the_scene_post_process() {
    let scene = shader();
    let other = PostProcessShader::new(
        "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             return gaanim_scene(uv);\n}",
    )
    .unwrap();
    let segment = |start, end, post| SegmentPostProcess {
        start_time: start,
        end_time: end,
        hold_at_end: false,
        post,
    };
    let post = CanvasPostProcess {
        passes: vec![scene.clone().into()],
        segments: vec![
            segment(0.0, 1.0, PostProcessOverride::Inherit),
            segment(1.0, 2.0, PostProcessOverride::Disabled),
            segment(
                2.0,
                3.0,
                PostProcessOverride::Passes(vec![other.clone().into()]),
            ),
        ],
        parameters: Vec::new(),
    };
    let source_at = |time| {
        post.passes_at(time)
            .first()
            .map(|pass| pass.shader.source())
    };
    assert_eq!(source_at(0.5), Some(scene.source()));
    assert!(post.passes_at(1.5).is_empty());
    assert_eq!(source_at(2.5), Some(other.source()));
    assert_eq!(source_at(9.0), Some(scene.source()));

    let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
    assert!(post.request(1.5, frame).is_none());
    assert!(post.request(0.5, kurbo::Rect::ZERO).is_none());
    assert!(post.request(f64::NAN, frame).is_none());
    assert_eq!(post.request(0.5, frame).unwrap().time, 0.5);
}

#[test]
fn frame_regions_are_clipped_to_the_target() {
    assert_eq!(
        frame_region(kurbo::Rect::new(-4.0, 2.4, 70.0, 20.6), 64, 36),
        Some([0, 2, 64, 19])
    );
    assert_eq!(
        frame_region(kurbo::Rect::new(80.0, 0.0, 90.0, 10.0), 64, 36),
        None
    );
}

#[test]
fn gpu_pass_processes_only_the_camera_frame() {
    let Some(gpu) = test_gpu() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let (width, height) = (64_u32, 36_u32);
    // Vertical stripes: the red channel encodes the column.
    let pixels: Vec<u8> = (0..height)
        .flat_map(|_| (0..width).flat_map(|x| [(x * 4) as u8, 64, 200, 255]))
        .collect();
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        target.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        target.size(),
    );

    let frame = kurbo::Rect::new(8.0, 4.0, 40.0, 28.0);
    let request = CanvasPostProcess {
        passes: vec![shader().into()],
        ..Default::default()
    }
    .request(1.0, frame)
    .unwrap();
    let mut post = GpuPostProcess::default();
    assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);

    let at = |x: u32, y: u32| &out[((y * width + x) * 4) as usize..][..4];
    // Outside the frame nothing changes.
    assert_eq!(at(2, 2), &[8, 64, 200, 255]);
    assert_eq!(at(50, 30), &[200, 64, 200, 255]);
    // The top-left frame pixel is uv ~ (0, 0) and reads its right-hand
    // neighbour (x = 9) through gaanim_scene, inverted.
    assert_eq!(at(8, 4), &[255 - 36, 255 - 64, 255 - 200, 255]);
    // The last column clamps its sample to the frame edge (x = 39).
    assert_eq!(at(39, 27), &[255 - 156, 255 - 64, 255 - 200, 255]);

    assert!(!post.prepare(&gpu.device, &gpu.queue, &target, None, None, None));
}

#[test]
fn an_overlay_draws_above_the_processed_frame() {
    let Some(gpu) = test_gpu() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let (width, height) = (32_u32, 16_u32);
    let texture = |pixels: &[u8]| {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            texture.as_image_copy(),
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        texture
    };
    let target = texture(&[40, 80, 120, 255].repeat((width * height) as usize));
    // A transparent overlay with one opaque green pixel at (5, 3).
    let mut overlay = vec![0_u8; (width * height * 4) as usize];
    overlay[((3 * width + 5) * 4) as usize..][..4].copy_from_slice(&[0, 255, 0, 255]);
    let overlay = texture(&overlay);

    let frame = kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
    let request = CanvasPostProcess {
        passes: vec![shader().into()],
        ..Default::default()
    }
    .request(1.0, frame)
    .unwrap();
    let mut post = GpuPostProcess::default();
    assert!(post.prepare(
        &gpu.device,
        &gpu.queue,
        &target,
        Some(&request),
        None,
        Some(&overlay),
    ));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);
    let at = |x: u32, y: u32| &out[((y * width + x) * 4) as usize..][..4];
    // The pass inverts the frame; the overlay's pixel stays as drawn.
    assert_eq!(at(10, 8), &[215, 175, 135, 255]);
    assert_eq!(at(5, 3), &[0, 255, 0, 255]);
}

const MIX: &str = "fn transition(uv: vec2<f32>) -> vec4<f32> {\n\
         return mix(gaanim_from(uv), gaanim_to(uv), progress * gaanim_uniforms.gain);\n}";

#[test]
fn transitions_require_their_function_and_validate() {
    let shader = PostProcessShader::transition(MIX, ["gain"], None).unwrap();
    assert!(shader.is_transition());
    assert!(matches!(
        PostProcessShader::transition(INVERT, std::iter::empty::<&str>(), None),
        Err(PostProcessError::InvalidWgsl(_))
    ));
    assert!(matches!(
        PostProcessShader::transition(
            "fn transition(uv: vec2<f32>) -> f32 { return 1.0; }",
            std::iter::empty::<&str>(),
            None
        ),
        Err(PostProcessError::InvalidWgsl(_))
    ));
    let rebuilt =
        PostProcessShader::from_parts(shader.source(), shader.uniforms(), None, false, true)
            .unwrap();
    assert_eq!(rebuilt, shader);
}

#[test]
fn gpu_transition_blends_both_segments_under_the_layer_above() {
    let Some(gpu) = test_gpu() else {
        return;
    };
    let (width, height) = (16_u32, 8_u32);
    let texture = |rgba: [u8; 4], left_only: bool| {
        let pixels: Vec<u8> = (0..height)
            .flat_map(|_| {
                (0..width).flat_map(move |x| {
                    if left_only && x >= 4 {
                        [0, 0, 0, 0]
                    } else {
                        rgba
                    }
                })
            })
            .collect();
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            texture.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            texture.size(),
        );
        texture
    };
    let target = texture([200, 0, 0, 255], false);
    let incoming = texture([0, 0, 200, 255], false);
    let above = texture([0, 255, 0, 255], true);
    let shader = Arc::new(gaanim_scene::TransitionShader {
        source: MIX.to_string(),
        uniforms: vec!["gain".to_string()],
        values: vec![1.0],
        data: None,
    });
    let frame = kurbo::Rect::new(0.0, 0.0, 12.0, f64::from(height));
    let request = PostProcessRequest::with_transition(
        None,
        Some(&gaanim_scene::TransitionShaderFrame {
            values: shader.values.clone(),
            shader,
            progress: 0.25,
        }),
        frame,
        0.0,
    )
    .unwrap();
    let mut post = GpuPostProcess::default();
    assert!(post.prepare(
        &gpu.device,
        &gpu.queue,
        &target,
        Some(&request),
        Some(TransitionInputs {
            incoming: &incoming,
            above: &above,
        }),
        None,
    ));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);
    let at = |x: u32, y: u32| &out[((y * width + x) * 4) as usize..][..4];
    // The layer above covers the left columns.
    assert_eq!(at(1, 3), &[0, 255, 0, 255]);
    // A quarter of the way from the outgoing red to the incoming blue.
    assert_eq!(at(8, 3), &[150, 0, 50, 255]);
    // Outside the camera frame the outgoing segment stays.
    assert_eq!(at(14, 3), &[200, 0, 0, 255]);

    // Without its inputs the transition does not run.
    assert!(!post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
}

const TINT: &str = "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
         let color = gaanim_scene(uv);\n\
         return vec4<f32>(color.r + gaanim_uniforms.red, color.g * gaanim_uniforms.green, color.b, color.a);\n}";

#[test]
fn uniforms_are_declared_fields_with_valid_names() {
    let tint = PostProcessShader::with_uniforms(TINT, ["red", "green"]).unwrap();
    assert_eq!(tint.uniforms().len(), 2);
    // A field the shader reads must be declared.
    assert!(matches!(
        PostProcessShader::with_uniforms(TINT, ["red"]),
        Err(PostProcessError::InvalidWgsl(_))
    ));
    for bad in ["2x", "a-b", "", "_", "__x"] {
        assert!(
            matches!(
                PostProcessShader::with_uniforms(TINT, [bad, "red", "green"]),
                Err(PostProcessError::InvalidUniforms(_))
            ),
            "{bad:?}"
        );
    }
    assert!(matches!(
        PostProcessShader::with_uniforms(TINT, ["red", "green", "red"]),
        Err(PostProcessError::InvalidUniforms(_))
    ));
    let many: Vec<String> = (0..=MAX_POST_UNIFORMS).map(|i| format!("u{i}")).collect();
    assert!(matches!(
        PostProcessShader::with_uniforms(TINT, &many),
        Err(PostProcessError::InvalidUniforms(_))
    ));
    assert!(matches!(
        PostProcessPass::constant(tint.clone(), &[1.0]),
        Err(PostProcessError::InvalidUniforms(_))
    ));
    assert!(matches!(
        PostProcessShader::with_data(INVERT, std::iter::empty::<&str>(), Vec::new()),
        Err(PostProcessError::InvalidWgsl(_))
    ));
}

#[test]
fn requests_evaluate_uniforms_from_parameter_signals() {
    let tint = PostProcessShader::with_uniforms(TINT, ["red", "green"]).unwrap();
    let parameter = ObjectId::from_raw(3);
    let entity = Entity::from_raw_u32(7).unwrap();
    let post = CanvasPostProcess {
        passes: vec![
            PostProcessPass::new(
                tint,
                vec![
                    ScalarSource::signal(parameter),
                    ScalarSource::constant(f64::NAN),
                ],
            )
            .unwrap(),
            shader().into(),
        ],
        parameters: vec![(parameter, entity)],
        ..Default::default()
    };
    let frame = kurbo::Rect::new(0.0, 0.0, 16.0, 9.0);
    let request = post
        .request_with(0.5, frame, |held| (held == entity).then_some(0.25))
        .unwrap();
    assert_eq!(request.passes.len(), 2);
    // Non-finite values are sent as zero.
    assert_eq!(request.passes[0].1, [0.25, 0.0]);
    assert!(request.passes[1].1.is_empty());
    // Without the signal the uniform falls back to zero.
    assert_eq!(post.request(0.5, frame).unwrap().passes[0].1, [0.0, 0.0]);
}

#[test]
fn gpu_chains_passes_and_reads_uniforms_and_data() {
    let Some(gpu) = test_gpu() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let (width, height) = (16_u32, 8_u32);
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|_| [40_u8, 200, 10, 255])
        .collect();
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        target.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        target.size(),
    );
    let tint = PostProcessShader::with_uniforms(TINT, ["red", "green"]).unwrap();
    let blue = PostProcessShader::with_data(
        "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {\n\
             let color = gaanim_scene(uv);\n\
             return vec4<f32>(color.rg, gaanim_data[1].z, color.a);\n}",
        std::iter::empty::<&str>(),
        vec![[0.0; 4], [0.0, 0.0, 1.0, 0.0]],
    )
    .unwrap();
    let request = CanvasPostProcess {
        passes: vec![
            PostProcessPass::constant(tint, &[0.5, 0.5]).unwrap(),
            blue.into(),
            // The inversion reads what the tint and data passes wrote.
            shader().into(),
        ],
        ..Default::default()
    }
    .request(0.0, kurbo::Rect::new(0.0, 0.0, 16.0, 8.0))
    .unwrap();
    let mut post = GpuPostProcess::default();
    assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);
    // red 40/255 + 0.5 = 168, green 200 * 0.5 = 100, blue from data = 255,
    // then inverted.
    let pixel = &out[..4];
    assert!((i32::from(pixel[0]) - (255 - 168)).abs() <= 1, "{pixel:?}");
    assert!((i32::from(pixel[1]) - (255 - 100)).abs() <= 1, "{pixel:?}");
    assert_eq!(pixel[2], 0);
    assert_eq!(pixel[3], 255);
}

/// `pixels` (one row of RGBA8) after `pass` on the GPU, or `None`
/// without a GPU adapter.
fn row_after_pass(pixels: &[[u8; 4]], pass: PostProcessPass) -> Option<Vec<[u8; 4]>> {
    let gpu = test_gpu()?;
    let width = pixels.len() as u32;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        target.as_image_copy(),
        pixels.as_flattened(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        target.size(),
    );
    let request = CanvasPostProcess {
        passes: vec![pass],
        ..Default::default()
    }
    .request(0.0, kurbo::Rect::new(0.0, 0.0, f64::from(width), 1.0))
    .unwrap();
    let mut post = GpuPostProcess::default();
    assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);
    Some(
        out.chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect(),
    )
}

fn assert_near(pixel: [u8; 4], expected: [u8; 4]) {
    assert!(
        pixel
            .iter()
            .zip(expected)
            .all(|(value, expected)| value.abs_diff(expected) <= 2),
        "{pixel:?} is not {expected:?}"
    );
}

#[test]
fn gpu_tint_and_duotone_map_brightness_onto_their_colors() {
    use crate::post_presets::PostPreset;
    // Black, the ink's own brightness, white, and a translucent gray.
    let ink = [0.8_f64, 0.4, 0.2];
    let ink_luma = 0.2126 * ink[0] + 0.7152 * ink[1] + 0.0722 * ink[2];
    let level = (ink_luma * 255.0).round() as u8;
    let pixels = [
        [0, 0, 0, 255],
        [level, level, level, 255],
        [255, 255, 255, 255],
        [128, 128, 128, 128],
    ];
    let byte = |value: f64| (value * 255.0).round() as u8;
    let tint = |amount| {
        PostProcessPass::constant(PostPreset::Tint.shader(), &[ink[0], ink[1], ink[2], amount])
            .unwrap()
    };
    let Some(full) = row_after_pass(&pixels, tint(1.0)) else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    assert_near(full[0], [0, 0, 0, 255]);
    assert_near(full[1], [byte(ink[0]), byte(ink[1]), byte(ink[2]), 255]);
    assert_near(full[2], [255, 255, 255, 255]);
    assert_eq!(full[3][3], 128, "alpha is kept");
    // Half the amount is halfway back to the original.
    let half = row_after_pass(&pixels, tint(0.5)).unwrap();
    assert_near(
        half[1],
        [
            byte((ink[0] + ink_luma) / 2.0),
            byte((ink[1] + ink_luma) / 2.0),
            byte((ink[2] + ink_luma) / 2.0),
            255,
        ],
    );

    let shadows = [0.1_f64, 0.2, 0.4];
    let highlights = [1.0_f64, 0.9, 0.6];
    let duotone = PostProcessPass::constant(
        PostPreset::Duotone.shader(),
        &[
            shadows[0],
            shadows[1],
            shadows[2],
            highlights[0],
            highlights[1],
            highlights[2],
            1.0,
        ],
    )
    .unwrap();
    let toned = row_after_pass(&pixels, duotone).unwrap();
    assert_near(
        toned[0],
        [byte(shadows[0]), byte(shadows[1]), byte(shadows[2]), 255],
    );
    assert_near(
        toned[2],
        [
            byte(highlights[0]),
            byte(highlights[1]),
            byte(highlights[2]),
            255,
        ],
    );
    let t = f64::from(level) / 255.0;
    let between = |axis: usize| byte(shadows[axis] + (highlights[axis] - shadows[axis]) * t);
    assert_near(toned[1], [between(0), between(1), between(2), 255]);
}

#[test]
fn gpu_outline_rings_the_covered_texels_with_round_corners() {
    let Some(gpu) = test_gpu() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let (width, height) = (24_u32, 16_u32);
    // A red 4x4 square at x 10..14, y 6..10 on transparent texels.
    let inside = |x: u32, y: u32| (10..14).contains(&x) && (6..10).contains(&y);
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|index| {
            if inside(index % width, index / width) {
                [200, 40, 40, 255]
            } else {
                [0, 0, 0, 0]
            }
        })
        .collect();
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        target.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        target.size(),
    );
    // Three pixels wide, white.
    let passes =
        crate::object_effects::outline_passes(3.0, gaanim_core::peniko::Color::WHITE).unwrap();
    let request = CanvasPostProcess {
        passes,
        ..Default::default()
    }
    .request(0.0, kurbo::Rect::new(0.0, 0.0, 24.0, 16.0))
    .unwrap();
    let mut post = GpuPostProcess::default();
    assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);
    let at = |x: u32, y: u32| {
        let index = ((y * width + x) * 4) as usize;
        [out[index], out[index + 1], out[index + 2], out[index + 3]]
    };
    assert_eq!(at(11, 7), [200, 40, 40, 255], "the square keeps its color");
    for x in 7..10 {
        assert_eq!(at(x, 7), [255, 255, 255, 255], "the ring at x {x}");
    }
    assert_eq!(at(6, 7)[3], 0, "the ring is three pixels wide");
    assert_eq!(at(14, 7), [255, 255, 255, 255]);
    assert_eq!(at(17, 7)[3], 0);
    // Its corners are round: 2.5 pixels away diagonally is inside,
    // 3.9 is outside.
    assert_eq!(at(8, 4)[3], 255);
    assert_eq!(at(7, 3)[3], 0);
    // Nothing appears along the texture's edges.
    for x in 0..width {
        assert_eq!(at(x, 0)[3], 0, "top row at x {x}");
        assert_eq!(at(x, height - 1)[3], 0, "bottom row at x {x}");
    }
}

#[test]
fn gpu_overlay_covers_the_frame_and_blends_with_it() {
    use crate::post_presets::{ImageOverlay, OverlayBlend};
    // A white texel and a black one cover a 4x1 frame of mid gray.
    let overlay = ImageOverlay::from_rgba8(2, 1, &[255, 255, 255, 255, 0, 0, 0, 255]).unwrap();
    let run = |blend: OverlayBlend, opacity: f64| -> Option<Vec<u8>> {
        let gpu = test_gpu()?;
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            target.as_image_copy(),
            &[128_u8, 128, 128, 255].repeat(4),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: None,
            },
            target.size(),
        );
        let pass = PostProcessPass::constant(
            overlay.shader().unwrap(),
            &[2.0, 1.0, blend.mode(), opacity],
        )
        .unwrap();
        let request = CanvasPostProcess {
            passes: vec![pass],
            ..Default::default()
        }
        .request(0.0, kurbo::Rect::new(0.0, 0.0, 4.0, 1.0))
        .unwrap();
        let mut post = GpuPostProcess::default();
        assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        Some(
            gpu.read(&target)
                .chunks_exact(4)
                .map(|pixel| pixel[0])
                .collect(),
        )
    };
    let Some(multiplied) = run(OverlayBlend::Multiply, 1.0) else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    // White keeps the gray, black blacks it out, bilinear in between.
    let near = |values: &[u8], expected: [u8; 4]| {
        assert!(
            values
                .iter()
                .zip(expected)
                .all(|(value, expected)| value.abs_diff(expected) <= 2),
            "{values:?} is not {expected:?}"
        );
    };
    near(&multiplied, [128, 96, 32, 0]);
    near(
        &run(OverlayBlend::Multiply, 0.5).unwrap(),
        [128, 112, 80, 64],
    );
    near(
        &run(OverlayBlend::Screen, 1.0).unwrap(),
        [255, 223, 159, 128],
    );
    near(&run(OverlayBlend::Normal, 1.0).unwrap(), [255, 191, 64, 0]);
}

#[test]
fn gpu_bloom_spreads_light_around_bright_pixels_only() {
    let Some(gpu) = test_gpu() else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    let (width, height) = (256_u32, 64_u32);
    // Black with a white 8x8 square on the left and a dim gray one far to
    // the right, below the threshold.
    let pixels: Vec<u8> = (0..width * height)
        .flat_map(|index| {
            let (x, y) = (index % width, index / width);
            let inside = |x0: u32| (x0..x0 + 8).contains(&x) && (28..36).contains(&y);
            if inside(28) {
                [255, 255, 255, 255]
            } else if inside(220) {
                [90, 90, 90, 255]
            } else {
                [0, 0, 0, 255]
            }
        })
        .collect();
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        target.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: None,
        },
        target.size(),
    );
    let bloom = crate::post_presets::PostPreset::Bloom.shader();
    let request = CanvasPostProcess {
        passes: vec![PostProcessPass::constant(bloom, &[0.8, 1.0, 0.6]).unwrap()],
        ..Default::default()
    }
    .request(0.0, kurbo::Rect::new(0.0, 0.0, 256.0, 64.0))
    .unwrap();
    let mut post = GpuPostProcess::default();
    // Twice, to reuse the chain's textures on the second frame.
    for _ in 0..2 {
        assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        post.encode(&mut encoder);
        gpu.queue.submit(Some(encoder.finish()));
        gpu.queue.write_texture(
            target.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            target.size(),
        );
    }
    assert!(post.prepare(&gpu.device, &gpu.queue, &target, Some(&request), None, None));
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    post.encode(&mut encoder);
    gpu.queue.submit(Some(encoder.finish()));
    let out = gpu.read(&target);
    let red = |x: u32, y: u32| out[((y * width + x) * 4) as usize];
    assert_eq!(red(31, 31), 255, "the bright square stays white");
    assert!(red(40, 32) > 20, "light spreads next to the bright square");
    assert!(red(40, 32) > red(50, 32), "and fades with distance");
    assert!(red(224, 32).abs_diff(90) <= 2, "dim pixels do not bloom");
    assert!(red(212, 32) <= 3, "nothing glows around dim pixels");
    assert!(red(252, 2) <= 3, "far corners stay dark");
}
