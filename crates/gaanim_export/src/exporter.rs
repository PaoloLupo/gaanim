use bevy::app::AppExit;
use bevy::camera::Viewport;
use bevy::ecs::observer::On;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use gaanim_renderer::prelude::VelloView;
use gaanim_timeline::timeline::Timeline;

use crate::config::{ExportConfig, ExportTelemetry};
use crate::encoder::{
    EncoderConfig, ExportError, ParallelEncoder, Result, VideoEncoder, resolve_video_encoder,
};
use crate::gpu::GpuContext;

/// An exact timeline seek rendered into an RGBA8 pixel buffer.
#[derive(Debug)]
pub struct CapturedFrame {
    pub time: f64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn create_progress_bar(total_frames: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_frames);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} 🦀 [gaanim] Exporting |{bar:40.cyan/blue}| {pos}/{len} frames ({percent}%) | Speed: {msg} | ETA: {eta}")
            .unwrap()
            .progress_chars("##-")
    );
    pb
}

fn format_label(format: &crate::encoder::ExportFormat) -> &str {
    match format {
        crate::encoder::ExportFormat::Mp4 => "MP4 (H.264)",
        crate::encoder::ExportFormat::Webm => "WebM (VP9)",
        crate::encoder::ExportFormat::Webp => "WebP (animated)",
        crate::encoder::ExportFormat::Gif => "GIF",
        crate::encoder::ExportFormat::PngSequence => "PNG Sequence",
    }
}

fn encoder_label(config: &ExportConfig) -> &'static str {
    match config.format {
        crate::encoder::ExportFormat::Mp4 => config.video_encoder.display_name(),
        crate::encoder::ExportFormat::Webm => "CPU (libvpx-vp9)",
        crate::encoder::ExportFormat::Webp => "CPU (libwebp)",
        crate::encoder::ExportFormat::Gif => "CPU (GIF palette)",
        crate::encoder::ExportFormat::PngSequence => "CPU (PNG sequence)",
    }
}

fn export_log(telemetry: &Option<ExportTelemetry>, line: impl Into<String>) {
    let line = line.into();
    if let Some(telemetry) = telemetry {
        telemetry.push_log(line.clone());
    }
    println!("{line}");
}

fn export_progress(telemetry: &Option<ExportTelemetry>, current: u64, total: u64) {
    if let Some(telemetry) = telemetry {
        telemetry.set_current_frame(current);
    }
    // The isolated 3D worker forwards this marker to the editor over stdout.
    // Normal exports use the shared telemetry directly and stay human-readable.
    if std::env::var_os("GAANIM_EXPORT_WORKER").is_some() {
        println!("GAANIM_EXPORT_PROGRESS {current} {total}");
    }
}

fn publish_benchmark_timings(
    encoder: VideoEncoder,
    render_gpu: Duration,
    encoder_wait: Duration,
    encode_active: Duration,
    finalize: Duration,
    total: Duration,
) {
    if std::env::var("GAANIM_BENCHMARK_SCENARIO").as_deref() != Ok("export") {
        return;
    }
    println!(
        "GAANIM_EXPORT_TIMINGS encoder={} render_gpu_ms={:.3} encoder_wait_ms={:.3} encode_active_ms={:.3} finalize_ms={:.3} total_ms={:.3}",
        encoder.ffmpeg_name(),
        render_gpu.as_secs_f64() * 1000.0,
        encoder_wait.as_secs_f64() * 1000.0,
        encode_active.as_secs_f64() * 1000.0,
        finalize.as_secs_f64() * 1000.0,
        total.as_secs_f64() * 1000.0,
    );
}

#[derive(Resource)]
struct ExportPipeline {
    pub encoder: ParallelEncoder,
    pub progress_bar: ProgressBar,
    pub current_time: f64,
    pub frame_time_step: f64,
    pub total_frames: u64,
    pub rendered_frames: u64,
    pub tx: SyncSender<Vec<u8>>,
    pub rx: Mutex<Receiver<Vec<u8>>>,
    pub waiting_for_gpu: bool,
    pub start_time: Instant,
    pub last_frame_time: Instant,
    pub export_width: u32,
    pub export_height: u32,
    pub resize_filter: image::imageops::FilterType,
    pub telemetry: Option<ExportTelemetry>,
    pub result_tx: SyncSender<Result<()>>,
    pub result_sent: bool,
}

fn check_custom_animation_errors(world: &World) -> Result<()> {
    match world
        .get_resource::<gaanim_animation::CustomAnimationDiagnostics>()
        .and_then(|errors| errors.first_error())
        .or_else(|| {
            world
                .get_resource::<gaanim_animation::PropertyBindingDiagnostics>()
                .and_then(|errors| errors.first_error())
        }) {
        Some(message) => Err(ExportError::Capture(message)),
        None => Ok(()),
    }
}

fn publish_export_result(
    result_tx: &SyncSender<Result<()>>,
    result_sent: &mut bool,
    result: Result<()>,
) {
    if *result_sent {
        return;
    }
    let _ = result_tx.send(result);
    *result_sent = true;
}

#[derive(Resource)]
struct SetupCallback(Option<Box<dyn FnOnce(&mut World) + Send + Sync>>);

#[derive(Resource, Clone, Copy)]
struct WindowRenderSize {
    width: u32,
    height: u32,
}

fn export_pipeline_system(
    mut commands: Commands,
    custom_errors: Option<Res<gaanim_animation::CustomAnimationDiagnostics>>,
    property_errors: Option<Res<gaanim_animation::PropertyBindingDiagnostics>>,
    mut pipeline_res: ResMut<ExportPipeline>,
    mut timeline: ResMut<Timeline>,
    mut exit: MessageWriter<'_, AppExit>,
    gltf_models: Query<(), With<gaanim_scene::GltfModelRoot>>,
    ready_gltf_models: Query<
        (),
        (
            With<gaanim_scene::GltfModelRoot>,
            With<gaanim_scene::GltfModelReady>,
        ),
    >,
) {
    if let Some(message) = custom_errors
        .as_ref()
        .and_then(|errors| errors.first_error())
        .or_else(|| {
            property_errors
                .as_ref()
                .and_then(|errors| errors.first_error())
        })
    {
        let pipeline = &mut *pipeline_res;
        export_log(&pipeline.telemetry, format!("  ERROR: {message}"));
        publish_export_result(
            &pipeline.result_tx,
            &mut pipeline.result_sent,
            Err(ExportError::Capture(message)),
        );
        exit.write(AppExit::Success);
        return;
    }
    if gltf_models.iter().count() != ready_gltf_models.iter().count() {
        return;
    }
    let pipeline = &mut *pipeline_res;
    if pipeline.waiting_for_gpu {
        let rx = pipeline.rx.lock().unwrap();
        match rx.try_recv() {
            Ok(frame_data) => {
                if let Err(e) = pipeline.encoder.push_frame(frame_data) {
                    export_log(&pipeline.telemetry, format!("  ERROR: {e}"));
                    bevy::prelude::error!("Encoder error: {}", e);
                    publish_export_result(&pipeline.result_tx, &mut pipeline.result_sent, Err(e));
                    exit.write(AppExit::Success);
                    return;
                }

                pipeline.rendered_frames += 1;
                export_progress(
                    &pipeline.telemetry,
                    pipeline.rendered_frames,
                    pipeline.total_frames,
                );

                // Throttle progress bar updates: only refresh speed every 10 frames
                if pipeline.rendered_frames.is_multiple_of(10)
                    || pipeline.rendered_frames == pipeline.total_frames
                {
                    let speed = 10.0 / pipeline.last_frame_time.elapsed().as_secs_f64();
                    pipeline
                        .progress_bar
                        .set_message(format!("{:.1} fps", speed));
                }
                pipeline.progress_bar.inc(1);

                pipeline.waiting_for_gpu = false;

                if pipeline.rendered_frames >= pipeline.total_frames {
                    pipeline.progress_bar.finish_with_message("Done!");
                    export_log(&pipeline.telemetry, "  Finalizing video file...");
                    if let Err(e) = pipeline.encoder.finalize() {
                        export_log(&pipeline.telemetry, format!("  ERROR: {e}"));
                        bevy::prelude::error!("Encoder finalization error: {}", e);
                        publish_export_result(
                            &pipeline.result_tx,
                            &mut pipeline.result_sent,
                            Err(e),
                        );
                        exit.write(AppExit::Success);
                        return;
                    }

                    let duration = pipeline.start_time.elapsed();
                    export_log(
                        &pipeline.telemetry,
                        "------------------------------------------------------------",
                    );
                    export_log(
                        &pipeline.telemetry,
                        format!(
                            "✓ Export successfully completed in {:.2}s!",
                            duration.as_secs_f64()
                        ),
                    );
                    export_log(
                        &pipeline.telemetry,
                        "------------------------------------------------------------",
                    );

                    publish_export_result(&pipeline.result_tx, &mut pipeline.result_sent, Ok(()));
                    exit.write(AppExit::Success);
                    return;
                }

                pipeline.current_time += pipeline.frame_time_step;
            }
            Err(TryRecvError::Empty) => {
                std::thread::yield_now();
                return;
            }
            Err(TryRecvError::Disconnected) => {
                let error = ExportError::Capture("GPU frame channel disconnected".to_string());
                export_log(&pipeline.telemetry, format!("  ERROR: {error}"));
                bevy::prelude::error!("{error}");
                publish_export_result(&pipeline.result_tx, &mut pipeline.result_sent, Err(error));
                exit.write(AppExit::Success);
                return;
            }
        }
    }

    pipeline.last_frame_time = Instant::now();

    timeline.seek_request = Some(pipeline.current_time);

    let tx_clone = pipeline.tx.clone();
    let export_width = pipeline.export_width;
    let export_height = pipeline.export_height;
    let resize_filter = pipeline.resize_filter;

    commands.spawn(Screenshot::primary_window()).observe(
        move |mut trigger: On<ScreenshotCaptured>| {
            let format = trigger.event().image.texture_descriptor.format;
            let size = trigger.event().image.texture_descriptor.size;

            let Some(mut data) = core::mem::take(&mut trigger.event_mut().image.data) else {
                return;
            };

            if data.is_empty() {
                return;
            }

            if format == bevy::render::render_resource::TextureFormat::Bgra8Unorm
                || format == bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb
            {
                for chunk in data.chunks_exact_mut(4) {
                    chunk.swap(0, 2);
                }
            }

            if size.width != export_width || size.height != export_height {
                let rgba_image = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                    size.width,
                    size.height,
                    data,
                )
                .expect("Bevy screenshot buffer size mismatch");
                let resized = image::imageops::resize(
                    &rgba_image,
                    export_width,
                    export_height,
                    resize_filter,
                );
                data = resized.into_raw();
            }

            let _ = tx_clone.send(data);
        },
    );

    pipeline.waiting_for_gpu = true;
}

fn setup_scene_system(world: &mut World) {
    if let Some(mut callback_res) = world.get_resource_mut::<SetupCallback>()
        && let Some(callback) = callback_res.0.take()
    {
        callback(world);
    }
}

/// Replay a window-backed scene before Vello creates its render target, then
/// give the Vello camera an explicit physical viewport. This removes the
/// startup race between WindowPlugin and bevy_vello's render-target setup.
fn setup_window_scene_system(world: &mut World) {
    setup_scene_system(world);
    world.flush();

    let Some(size) = world.get_resource::<WindowRenderSize>().copied() else {
        return;
    };
    let mut cameras = world.query_filtered::<&mut Camera, With<VelloView>>();
    for mut camera in cameras.iter_mut(world) {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::ZERO,
            physical_size: UVec2::new(size.width, size.height),
            depth: 0.0..1.0,
        });
    }
}

fn filter_for_quality(speed: crate::encoder::EncodingSpeed) -> image::imageops::FilterType {
    match speed {
        crate::encoder::EncodingSpeed::Fast => image::imageops::FilterType::Nearest,
        crate::encoder::EncodingSpeed::Balanced => image::imageops::FilterType::CatmullRom,
        crate::encoder::EncodingSpeed::Best => image::imageops::FilterType::Lanczos3,
    }
}

pub fn export_scene<F>(config: ExportConfig, setup_world_fn: F) -> Result<()>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
{
    let start_time = Instant::now();
    let telemetry = config.telemetry.clone();
    let mut config = config.apply_presets();
    config.video_encoder = resolve_video_encoder(config.format, config.video_encoder);
    if let Some(telemetry) = &telemetry {
        telemetry.set_encoder(encoder_label(&config));
    }

    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );
    export_log(&telemetry, "🦀 gaanim — Export");
    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );
    export_log(
        &telemetry,
        format!("  Output file:   {}", config.output_path),
    );
    export_log(
        &telemetry,
        format!("  Resolution:    {}x{}", config.width, config.height),
    );
    export_log(&telemetry, format!("  Framerate:     {} FPS", config.fps));
    export_log(
        &telemetry,
        format!("  Format:        {}", format_label(&config.format)),
    );
    export_log(
        &telemetry,
        format!("  Encoder:       {}", encoder_label(&config)),
    );
    export_log(
        &telemetry,
        format!("  Transparent:   {}", config.transparent),
    );
    if let (Some(s), Some(e)) = (config.start_time, config.end_time) {
        export_log(&telemetry, format!("  Segment:       {s:.2}s to {e:.2}s"));
    }
    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );

    let resize_filter = filter_for_quality(config.encoding_speed);

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    visible: true,
                    position: if config.headless {
                        bevy::window::WindowPosition::At(bevy::prelude::IVec2::new(
                            -32_000, -32_000,
                        ))
                    } else {
                        bevy::window::WindowPosition::Automatic
                    },
                    decorations: !config.headless,
                    title: "Gaanim Render Engine — Export Viewport".to_string(),
                    resolution: (config.width, config.height).into(),
                    resizable: false,
                    resize_constraints: bevy::window::WindowResizeConstraints {
                        min_width: config.width as f32,
                        min_height: config.height as f32,
                        max_width: config.width as f32,
                        max_height: config.height as f32,
                    },
                    ..default()
                }),
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .set(gaanim_scene::gaanim_asset_plugin()),
    )
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_media::GaanimMediaPlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin);

    app.insert_resource(SetupCallback(Some(Box::new(setup_world_fn))));
    app.insert_resource(WindowRenderSize {
        width: config.width,
        height: config.height,
    });
    app.add_systems(PreStartup, setup_window_scene_system);

    app.finish();
    app.cleanup();
    app.update();
    check_custom_animation_errors(app.world())?;
    if let Some(mut background) = app
        .world_mut()
        .get_resource_mut::<gaanim_renderer::pipeline::CanvasBackground>()
    {
        background.pixel_size = (config.width, config.height);
    }

    let timeline_duration = app.world().resource::<Timeline>().cached_duration;
    let render_start = config.start_time.unwrap_or(0.0).max(0.0);
    let render_end = config
        .end_time
        .unwrap_or(timeline_duration)
        .min(timeline_duration);
    let render_length = render_end - render_start;

    let encoder = ParallelEncoder::new(EncoderConfig {
        output_path: config.output_path.clone(),
        width: config.width,
        height: config.height,
        fps: config.fps,
        format: config.format,
        transparent: config.transparent,
        crf: config.crf,
        encoding_speed: config.encoding_speed,
        video_encoder: config.video_encoder,
        audio_tracks: config.audio_tracks.clone(),
        render_start,
        render_duration: render_length,
    })?;

    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    if let Some(telemetry) = &telemetry {
        telemetry.set_total_frames(total_frames);
    }
    let pb = create_progress_bar(total_frames);

    // Bounded channel: at most 4 pending GPU frames to prevent memory blow-up
    let (tx, rx) = sync_channel::<Vec<u8>>(4);
    let (result_tx, result_rx) = sync_channel::<Result<()>>(1);

    app.insert_resource(ExportPipeline {
        encoder,
        progress_bar: pb,
        current_time: render_start,
        frame_time_step: 1.0 / config.fps as f64,
        total_frames,
        rendered_frames: 0,
        tx,
        rx: Mutex::new(rx),
        waiting_for_gpu: false,
        start_time,
        last_frame_time: Instant::now(),
        export_width: config.width,
        export_height: config.height,
        resize_filter,
        telemetry,
        result_tx,
        result_sent: false,
    });

    app.add_systems(Update, export_pipeline_system);

    app.run();
    check_custom_animation_errors(app.world())?;

    match result_rx.try_recv() {
        Ok(result) => result,
        Err(_) => Err(ExportError::General(
            "export pipeline exited without reporting a result".to_string(),
        )),
    }
}

/// Headless GPU-direct export: bypasses Bevy's render graph and winit entirely.
///
/// Uses a minimal Bevy App (ECS + timeline only), own wgpu context with Vello,
/// and the standalone `gaanim_renderer::pipeline::compile_scene_from_world`
/// to produce frames. Frames are piped directly to ffmpeg with no swapchain,
/// no BGRA conversion, and no screenshot overhead.
pub fn export_scene_direct<F>(config: ExportConfig, setup_world_fn: F) -> Result<()>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
{
    let start_time = Instant::now();
    let telemetry = config.telemetry.clone();
    let mut config = config.apply_presets();
    config.video_encoder = resolve_video_encoder(config.format, config.video_encoder);
    if let Some(telemetry) = &telemetry {
        telemetry.set_encoder(encoder_label(&config));
    }

    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );
    export_log(&telemetry, "🦀 gaanim v2 — Headless GPU-Direct Export");
    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );
    export_log(
        &telemetry,
        format!("  Output file:   {}", config.output_path),
    );
    export_log(
        &telemetry,
        format!("  Resolution:    {}x{}", config.width, config.height),
    );
    export_log(&telemetry, format!("  Framerate:     {} FPS", config.fps));
    export_log(
        &telemetry,
        format!("  Format:        {}", format_label(&config.format)),
    );
    export_log(
        &telemetry,
        format!("  Encoder:       {}", encoder_label(&config)),
    );
    export_log(
        &telemetry,
        format!("  Transparent:   {}", config.transparent),
    );
    if let (Some(s), Some(e)) = (config.start_time, config.end_time) {
        export_log(&telemetry, format!("  Segment:       {s:.2}s to {e:.2}s"));
    }
    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );

    let mut gpu = GpuContext::new(config.width, config.height)?;

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_media::GaanimMediaPlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);

    app.insert_resource(SetupCallback(Some(Box::new(setup_world_fn))));
    app.add_systems(Startup, setup_scene_system);

    app.finish();
    app.cleanup();
    app.update();
    check_custom_animation_errors(app.world())?;
    if let Some(mut background) = app
        .world_mut()
        .get_resource_mut::<gaanim_renderer::pipeline::CanvasBackground>()
    {
        background.pixel_size = (config.width, config.height);
    }

    let timeline_duration = app.world().resource::<Timeline>().cached_duration;
    let render_start = config.start_time.unwrap_or(0.0).max(0.0);
    let render_end = config
        .end_time
        .unwrap_or(timeline_duration)
        .min(timeline_duration);
    let render_length = render_end - render_start;

    let mut encoder = ParallelEncoder::new(EncoderConfig {
        output_path: config.output_path.clone(),
        width: config.width,
        height: config.height,
        fps: config.fps,
        format: config.format,
        transparent: config.transparent,
        crf: config.crf,
        encoding_speed: config.encoding_speed,
        video_encoder: config.video_encoder,
        audio_tracks: config.audio_tracks.clone(),
        render_start,
        render_duration: render_length,
    })?;

    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    if let Some(telemetry) = &telemetry {
        telemetry.set_total_frames(total_frames);
    }
    let frame_time_step = 1.0 / config.fps as f64;
    let pb = create_progress_bar(total_frames);

    let mut current_time = render_start;
    let mut last_report = Instant::now();
    let mut render_gpu_time = Duration::ZERO;
    let mut encoder_wait_time = Duration::ZERO;

    for frame_idx in 0..total_frames {
        {
            let world = app.world_mut();
            let mut timeline = world.resource_mut::<Timeline>();
            timeline.seek_request = Some(current_time);
        }

        app.update();
        check_custom_animation_errors(app.world())?;

        let vello_scene = {
            let camera = app.world().get_resource::<gaanim_math::Camera>().cloned();
            let raw_scene = gaanim_renderer::pipeline::compile_scene_from_world(
                app.world_mut(),
                camera.as_ref(),
            );

            let (zoom, pixels_per_unit, viewport_width, viewport_height, cam_x, cam_y) = camera
                .as_ref()
                .map(|c| {
                    let z = match c.projection {
                        gaanim_math::Projection::Orthographic { zoom } => zoom,
                        _ => 1.0,
                    };
                    (
                        z,
                        c.pixels_per_unit(),
                        c.viewport_width.max(1),
                        c.viewport_height.max(1),
                        c.position.x,
                        c.position.y,
                    )
                })
                .unwrap_or((
                    1.0,
                    1.0,
                    config.width.max(1),
                    config.height.max(1),
                    0.0,
                    0.0,
                ));

            let fit_x = config.width as f64 / viewport_width as f64;
            let fit_y = config.height as f64 / viewport_height as f64;
            let fit_scale = match config.fit {
                crate::config::OutputFit::Cover => fit_x.max(fit_y),
                crate::config::OutputFit::Error | crate::config::OutputFit::Contain => {
                    fit_x.min(fit_y)
                }
            };

            let mut scene = bevy_vello::vello::Scene::new();
            let camera_to_vello =
                kurbo::Affine::translate((config.width as f64 / 2.0, config.height as f64 / 2.0))
                    * kurbo::Affine::scale_non_uniform(
                        pixels_per_unit * zoom * fit_scale,
                        -pixels_per_unit * zoom * fit_scale,
                    )
                    * kurbo::Affine::translate((-cam_x, -cam_y));
            scene.append(&raw_scene, Some(camera_to_vello));
            scene
        };

        let bg_color = app
            .world()
            .get_resource::<ClearColor>()
            .map(|cc| {
                let rgba = cc.0.to_srgba();
                bevy_vello::vello::peniko::Color::from_rgba8(
                    (rgba.red * 255.0) as u8,
                    (rgba.green * 255.0) as u8,
                    (rgba.blue * 255.0) as u8,
                    (rgba.alpha * 255.0) as u8,
                )
            })
            .unwrap_or(bevy_vello::vello::peniko::Color::BLACK);

        let render_started_at = Instant::now();
        let frame_data = gpu.render_frame(&vello_scene, bg_color)?;
        render_gpu_time += render_started_at.elapsed();

        let encoder_wait_started_at = Instant::now();
        encoder
            .push_frame(frame_data)
            .map_err(|e| ExportError::Capture(format!("Encoder push error: {}", e)))?;
        encoder_wait_time += encoder_wait_started_at.elapsed();

        if frame_idx.is_multiple_of(10) || frame_idx == total_frames - 1 {
            let speed = 10.0 / last_report.elapsed().as_secs_f64();
            pb.set_message(format!("{:.1} fps", speed));
            last_report = Instant::now();
        }
        pb.inc(1);
        export_progress(&telemetry, frame_idx + 1, total_frames);

        current_time += frame_time_step;
    }

    pb.finish_with_message("Done!");
    export_log(&telemetry, "  Finalizing video file...");

    let finalize_started_at = Instant::now();
    let encode_active_time = encoder.finalize_with_timings().inspect_err(|e| {
        export_log(&telemetry, format!("  ERROR: {e}"));
        bevy::prelude::error!("Encoder finalization error: {}", e);
    })?;
    let finalize_time = finalize_started_at.elapsed();

    let duration = start_time.elapsed();
    publish_benchmark_timings(
        config.video_encoder,
        render_gpu_time,
        encoder_wait_time,
        encode_active_time,
        finalize_time,
        duration,
    );
    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );
    export_log(
        &telemetry,
        format!(
            "✓ Export successfully completed in {:.2}s!",
            duration.as_secs_f64()
        ),
    );
    export_log(
        &telemetry,
        "------------------------------------------------------------",
    );

    Ok(())
}

/// Render a sparse set of exact timeline seeks with a single headless GPU context.
///
/// Unlike a PNG-sequence export, this does not advance at a fixed frame rate:
/// every requested timestamp is applied directly through `Timeline::seek_request`.
/// The returned buffers are ordered exactly like `times`.
pub fn capture_scene_direct<F>(
    config: ExportConfig,
    times: &[f64],
    setup_world_fn: F,
) -> Result<Vec<CapturedFrame>>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
{
    let capture_started = Instant::now();
    if times.is_empty() {
        return Err(ExportError::Capture(
            "at least one snapshot timestamp is required".to_string(),
        ));
    }
    if let Some(time) = times.iter().find(|time| !time.is_finite() || **time < 0.0) {
        return Err(ExportError::Capture(format!(
            "snapshot timestamp must be finite and non-negative: {time}"
        )));
    }

    let mut gpu = GpuContext::new(config.width, config.height)?;

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_media::GaanimMediaPlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);

    app.insert_resource(SetupCallback(Some(Box::new(setup_world_fn))));
    app.add_systems(Startup, setup_scene_system);
    app.finish();
    app.cleanup();
    app.update();
    check_custom_animation_errors(app.world())?;
    if let Some(mut background) = app
        .world_mut()
        .get_resource_mut::<gaanim_renderer::pipeline::CanvasBackground>()
    {
        background.pixel_size = (config.width, config.height);
    }
    let setup_ms = capture_started.elapsed().as_secs_f64() * 1000.0;

    let duration = app.world().resource::<Timeline>().cached_duration;
    if let Some(time) = times.iter().find(|time| **time > duration) {
        return Err(ExportError::Capture(format!(
            "snapshot timestamp {time:.6}s exceeds scene duration {duration:.6}s"
        )));
    }

    let mut frames = Vec::with_capacity(times.len());
    let mut timeline_update = Duration::ZERO;
    let mut scene_compile = Duration::ZERO;
    let mut render_readback = Duration::ZERO;
    for &time in times {
        let phase_started = Instant::now();
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        check_custom_animation_errors(app.world())?;
        timeline_update += phase_started.elapsed();

        let phase_started = Instant::now();
        let resolved_camera = app
            .world()
            .get_resource::<gaanim_math::ResolvedCamera>()
            .copied()
            .or_else(|| {
                app.world()
                    .get_resource::<gaanim_math::Camera>()
                    .copied()
                    .map(|camera| {
                        gaanim_math::ResolvedCamera::new(
                            camera,
                            gaanim_math::CameraViewport::default(),
                        )
                    })
            });
        let raw_scene = gaanim_renderer::pipeline::compile_scene_from_world(
            app.world_mut(),
            resolved_camera.as_ref().map(|resolved| &resolved.camera),
        );

        let mut scene = bevy_vello::vello::Scene::new();
        let camera_to_vello = capture_camera_to_vello_transform(
            resolved_camera.as_ref(),
            config.width,
            config.height,
            config.fit,
        );
        scene.append(&raw_scene, Some(camera_to_vello));
        scene_compile += phase_started.elapsed();

        let background = app
            .world()
            .get_resource::<ClearColor>()
            .map(|clear| {
                let rgba = clear.0.to_srgba();
                bevy_vello::vello::peniko::Color::from_rgba8(
                    (rgba.red * 255.0) as u8,
                    (rgba.green * 255.0) as u8,
                    (rgba.blue * 255.0) as u8,
                    (rgba.alpha * 255.0) as u8,
                )
            })
            .unwrap_or(bevy_vello::vello::peniko::Color::BLACK);

        let phase_started = Instant::now();
        let rgba = gpu.render_frame(&scene, background)?;
        render_readback += phase_started.elapsed();
        frames.push(CapturedFrame {
            time,
            width: config.width,
            height: config.height,
            rgba,
        });
    }

    if std::env::var_os("GAANIM_CAPTURE_TELEMETRY").is_some() {
        eprintln!(
            "GAANIM_CAPTURE_TIMINGS setup_ms={setup_ms:.3} timeline_update_ms={:.3} scene_compile_ms={:.3} render_readback_ms={:.3} capture_total_ms={:.3}",
            timeline_update.as_secs_f64() * 1000.0,
            scene_compile.as_secs_f64() * 1000.0,
            render_readback.as_secs_f64() * 1000.0,
            capture_started.elapsed().as_secs_f64() * 1000.0,
        );
    }

    Ok(frames)
}

/// Map a canvas-sized scene into a capture target while preserving its aspect
/// ratio. Thumbnail captures are deliberately much smaller than their source
/// canvases, so their camera viewport must be scaled down as well.
fn capture_camera_to_vello_transform(
    resolved: Option<&gaanim_math::ResolvedCamera>,
    output_width: u32,
    output_height: u32,
    fit: crate::config::OutputFit,
) -> kurbo::Affine {
    let (zoom, pixels_per_unit, viewport_width, viewport_height, cam_x, cam_y, angle, viewport) =
        resolved
            .map(|resolved| {
                let camera = &resolved.camera;
                let zoom = match camera.projection {
                    gaanim_math::Projection::Orthographic { zoom } => zoom,
                    _ => 1.0,
                };
                (
                    zoom,
                    camera.pixels_per_unit(),
                    camera.viewport_width.max(1),
                    camera.viewport_height.max(1),
                    camera.position.x,
                    camera.position.y,
                    camera.z_angle(),
                    resolved.viewport,
                )
            })
            .unwrap_or((
                1.0,
                1.0,
                output_width.max(1),
                output_height.max(1),
                0.0,
                0.0,
                0.0,
                gaanim_math::CameraViewport::default(),
            ));
    let fit_x = output_width as f64 / viewport_width as f64;
    let fit_y = output_height as f64 / viewport_height as f64;
    let fit_scale = match fit {
        crate::config::OutputFit::Cover => fit_x.max(fit_y),
        crate::config::OutputFit::Error | crate::config::OutputFit::Contain => fit_x.min(fit_y),
    };
    let scale = pixels_per_unit * zoom * fit_scale * viewport.scale;
    let offset_y = viewport.offset_y * fit_scale;

    kurbo::Affine::translate((
        output_width as f64 / 2.0,
        output_height as f64 / 2.0 + offset_y,
    )) * kurbo::Affine::scale_non_uniform(scale, -scale)
        * kurbo::Affine::rotate(-angle)
        * kurbo::Affine::translate((-cam_x, -cam_y))
}

#[derive(Resource)]
struct HybridCapturePipeline {
    times: Vec<f64>,
    index: usize,
    warmup_pending: bool,
    phase: u8,
    frames: Vec<CapturedFrame>,
    tx: SyncSender<Vec<u8>>,
    rx: Mutex<Receiver<Vec<u8>>>,
    width: u32,
    height: u32,
    result_tx: SyncSender<Vec<CapturedFrame>>,
    result_sent: bool,
}

/// Number of complete render frames allowed after an exact seek before the
/// screenshot request is submitted. Native mesh extraction and GPU pipeline
/// preparation can take more than two frames on a cold Vulkan renderer.
const HYBRID_CAPTURE_SETTLE_FRAMES: u8 = 6;
/// The first hybrid frame also pays for render-pipeline and shader creation.
/// A longer one-time warm-up prevents an otherwise valid first seek from being
/// captured before native 3D content reaches the swapchain.
const HYBRID_CAPTURE_COLD_SETTLE_FRAMES: u8 = 30;
const HYBRID_CAPTURE_VISIBLE_GLTF_SETTLE_FRAMES: u8 = 6;

fn publish_hybrid_capture_result(pipeline: &mut HybridCapturePipeline) {
    if pipeline.result_sent {
        return;
    }
    let frames = core::mem::take(&mut pipeline.frames);
    let _ = pipeline.result_tx.send(frames);
    pipeline.result_sent = true;
}

fn accept_hybrid_capture(pipeline: &mut HybridCapturePipeline, rgba: Vec<u8>) {
    if pipeline.warmup_pending {
        // The first screenshot request primes Bevy's swapchain/render graph.
        // Discard it and repeat the same exact seek for the first fixture.
        pipeline.warmup_pending = false;
        pipeline.phase = 0;
        return;
    }

    let time = pipeline.times[pipeline.index];
    pipeline.frames.push(CapturedFrame {
        time,
        width: pipeline.width,
        height: pipeline.height,
        rgba,
    });
    pipeline.index += 1;
    pipeline.phase = 0;
}

fn hybrid_capture_system(
    mut commands: Commands,
    custom_errors: Option<Res<gaanim_animation::CustomAnimationDiagnostics>>,
    property_errors: Option<Res<gaanim_animation::PropertyBindingDiagnostics>>,
    mut pipeline: ResMut<HybridCapturePipeline>,
    mut timeline: ResMut<Timeline>,
    mut exit: MessageWriter<'_, AppExit>,
    gltf_models: Query<Option<&gaanim_scene::Visible>, With<gaanim_scene::GltfModelRoot>>,
    ready_gltf_models: Query<
        (),
        (
            With<gaanim_scene::GltfModelRoot>,
            With<gaanim_scene::GltfModelReady>,
        ),
    >,
    visible_gltf_meshes: Query<
        &bevy::camera::visibility::ViewVisibility,
        With<gaanim_scene::GltfMaterialBaseline>,
    >,
) {
    if custom_errors
        .as_ref()
        .is_some_and(|errors| errors.first_error().is_some())
        || property_errors
            .as_ref()
            .is_some_and(|errors| errors.first_error().is_some())
    {
        publish_hybrid_capture_result(&mut pipeline);
        exit.write(AppExit::Success);
        return;
    }
    if gltf_models.iter().count() != ready_gltf_models.iter().count() {
        return;
    }
    if pipeline.index >= pipeline.times.len() {
        publish_hybrid_capture_result(&mut pipeline);
        exit.write(AppExit::Success);
        return;
    }
    let settle_frames = if pipeline.warmup_pending {
        HYBRID_CAPTURE_COLD_SETTLE_FRAMES
    } else {
        HYBRID_CAPTURE_SETTLE_FRAMES
    };
    match pipeline.phase {
        0 => {
            timeline.seek_request = Some(pipeline.times[pipeline.index]);
            pipeline.phase = 1;
        }
        phase if phase <= settle_frames => {
            // Propagate the exact seek through camera, hierarchy, material,
            // and mesh systems, then allow changed assets to reach Bevy's
            // render world. Several frames are required on a cold renderer
            // while native mesh pipelines are being prepared asynchronously.
            pipeline.phase += 1;
        }
        phase if phase == settle_frames + 1 => {
            let expects_visible_gltf = gltf_models.iter().any(|visible| visible.is_some());
            let has_visible_gltf_mesh = visible_gltf_meshes.iter().any(|visible| visible.get());
            if expects_visible_gltf && !has_visible_gltf_mesh {
                return;
            }
            if expects_visible_gltf {
                // ViewVisibility confirms main-world culling, but render-asset
                // preparation may still be catching up after a model changes
                // from hidden to visible. Count a bounded warm-up below.
                pipeline.phase += 1;
            } else {
                pipeline.phase = settle_frames + HYBRID_CAPTURE_VISIBLE_GLTF_SETTLE_FRAMES + 2;
            }
        }
        phase if phase <= settle_frames + HYBRID_CAPTURE_VISIBLE_GLTF_SETTLE_FRAMES + 1 => {
            pipeline.phase += 1;
        }
        phase if phase == settle_frames + HYBRID_CAPTURE_VISIBLE_GLTF_SETTLE_FRAMES + 2 => {
            let tx = pipeline.tx.clone();
            let width = pipeline.width;
            let height = pipeline.height;
            commands.spawn(Screenshot::primary_window()).observe(
                move |mut trigger: On<ScreenshotCaptured>| {
                    let format = trigger.event().image.texture_descriptor.format;
                    let size = trigger.event().image.texture_descriptor.size;
                    let Some(mut data) = core::mem::take(&mut trigger.event_mut().image.data)
                    else {
                        return;
                    };
                    if matches!(
                        format,
                        bevy::render::render_resource::TextureFormat::Bgra8Unorm
                            | bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb
                    ) {
                        for pixel in data.chunks_exact_mut(4) {
                            pixel.swap(0, 2);
                        }
                    }
                    if size.width != width || size.height != height {
                        let image = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                            size.width,
                            size.height,
                            data,
                        )
                        .expect("Bevy screenshot buffer size mismatch");
                        data = image::imageops::resize(
                            &image,
                            width,
                            height,
                            image::imageops::FilterType::CatmullRom,
                        )
                        .into_raw();
                    }
                    let _ = tx.send(data);
                },
            );
            pipeline.phase += 1;
        }
        _ => {
            let received = pipeline.rx.lock().unwrap().try_recv();
            match received {
                Ok(rgba) => {
                    accept_hybrid_capture(&mut pipeline, rgba);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    exit.write(AppExit::Success);
                }
            }
        }
    }
}

/// Capture exact seeks through Bevy's shared PBR + Vello camera stack.
/// Used for scenes containing native 3D assets; the window stays hidden.
pub fn capture_scene_hybrid<F>(
    config: ExportConfig,
    times: &[f64],
    setup_world_fn: F,
) -> Result<Vec<CapturedFrame>>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
{
    if times.is_empty() {
        return Err(ExportError::Capture(
            "at least one snapshot timestamp is required".to_string(),
        ));
    }
    let (tx, rx) = sync_channel(1);
    let (result_tx, result_rx) = sync_channel(1);
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    // Winit does not render fully hidden windows on every platform.
                    // Keep the swapchain alive outside the visible desktop instead.
                    visible: true,
                    position: bevy::window::WindowPosition::At(bevy::prelude::IVec2::new(
                        -32_000, -32_000,
                    )),
                    decorations: false,
                    resolution: (config.width, config.height).into(),
                    resizable: false,
                    ..default()
                }),
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .set(gaanim_scene::gaanim_asset_plugin()),
    )
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_media::GaanimMediaPlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin)
    .insert_resource(SetupCallback(Some(Box::new(setup_world_fn))))
    .insert_resource(WindowRenderSize {
        width: config.width,
        height: config.height,
    })
    .insert_resource(HybridCapturePipeline {
        times: times.to_vec(),
        index: 0,
        warmup_pending: true,
        phase: 0,
        frames: Vec::with_capacity(times.len()),
        tx,
        rx: Mutex::new(rx),
        width: config.width,
        height: config.height,
        result_tx,
        result_sent: false,
    })
    .add_systems(PreStartup, setup_window_scene_system)
    .add_systems(
        Update,
        hybrid_capture_system.after(gaanim_scene::SceneSet::Extraction),
    );

    app.run();
    check_custom_animation_errors(app.world())?;
    let frames = result_rx.recv().map_err(|_| {
        ExportError::Capture("hybrid capture exited before returning its frames".to_string())
    })?;
    if frames.len() != times.len() {
        return Err(ExportError::Capture(format!(
            "captured {} of {} requested hybrid frames",
            frames.len(),
            times.len()
        )));
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_rejects_custom_callback_diagnostics() {
        let mut world = World::new();
        assert!(check_custom_animation_errors(&world).is_ok());
        world.insert_resource(gaanim_animation::CustomAnimationDiagnostics(vec![
            gaanim_animation::CustomAnimationDiagnostic {
                target: gaanim_core::ObjectId::from_raw(7),
                alpha: 0.375,
                message: "invalid callback output".into(),
            },
        ]));
        let error = check_custom_animation_errors(&world)
            .unwrap_err()
            .to_string();
        assert!(error.contains("0.375"));
        assert!(error.contains("invalid callback output"));
        world.remove_resource::<gaanim_animation::CustomAnimationDiagnostics>();
        world.insert_resource(gaanim_animation::PropertyBindingDiagnostics(vec![
            gaanim_animation::PropertyBindingDiagnostic {
                target: gaanim_core::ObjectId::from_raw(8),
                time: 1.25,
                message: "invalid computed value".into(),
            },
        ]));
        let error = check_custom_animation_errors(&world)
            .unwrap_err()
            .to_string();
        assert!(error.contains("1.25"));
        assert!(error.contains("invalid computed value"));
    }

    #[test]
    fn export_result_channel_preserves_the_first_failure() {
        let (result_tx, result_rx) = sync_channel(1);
        let mut result_sent = false;

        publish_export_result(
            &result_tx,
            &mut result_sent,
            Err(ExportError::Capture("encoder failed".to_string())),
        );
        publish_export_result(&result_tx, &mut result_sent, Ok(()));

        let error = result_rx.recv().unwrap().unwrap_err();
        assert!(error.to_string().contains("encoder failed"));
    }

    #[test]
    fn direct_capture_scales_a_canvas_down_to_a_thumbnail() {
        let camera = gaanim_math::Camera::ortho_2d(1920, 1080);
        let resolved =
            gaanim_math::ResolvedCamera::new(camera, gaanim_math::CameraViewport::default());
        let transform = capture_camera_to_vello_transform(
            Some(&resolved),
            320,
            180,
            crate::config::OutputFit::Error,
        );

        let top_left = transform * kurbo::Point::new(-960.0, 540.0);
        let bottom_right = transform * kurbo::Point::new(960.0, -540.0);
        assert!((top_left.x - 0.0).abs() < 1e-9);
        assert!((top_left.y - 0.0).abs() < 1e-9);
        assert!((bottom_right.x - 320.0).abs() < 1e-9);
        assert!((bottom_right.y - 180.0).abs() < 1e-9);
    }

    #[test]
    fn direct_capture_contains_or_covers_a_logical_frame() {
        let camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1280, 720);
        let resolved =
            gaanim_math::ResolvedCamera::new(camera, gaanim_math::CameraViewport::default());

        let contain = capture_camera_to_vello_transform(
            Some(&resolved),
            1000,
            1000,
            crate::config::OutputFit::Contain,
        );
        let contain_top_left = contain * kurbo::Point::new(-8.0, 4.5);
        let contain_bottom_right = contain * kurbo::Point::new(8.0, -4.5);
        assert!((contain_top_left.x - 0.0).abs() < 1e-9);
        assert!((contain_top_left.y - 218.75).abs() < 1e-9);
        assert!((contain_bottom_right.x - 1000.0).abs() < 1e-9);
        assert!((contain_bottom_right.y - 781.25).abs() < 1e-9);

        let cover = capture_camera_to_vello_transform(
            Some(&resolved),
            1000,
            1000,
            crate::config::OutputFit::Cover,
        );
        let cover_top_left = cover * kurbo::Point::new(-8.0, 4.5);
        let cover_bottom_right = cover * kurbo::Point::new(8.0, -4.5);
        assert!((cover_top_left.x + 388.8888888888889).abs() < 1e-9);
        assert!((cover_top_left.y - 0.0).abs() < 1e-9);
        assert!((cover_bottom_right.x - 1388.888888888889).abs() < 1e-9);
        assert!((cover_bottom_right.y - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn direct_capture_uses_resolved_rig_pose_rotation_and_viewport() {
        let mut camera = gaanim_math::Camera::ortho_2d(960, 540);
        camera.position = gaanim_core::glam::DVec3::new(120.0, -35.0, 0.0);
        camera.rotation = gaanim_core::glam::DQuat::from_rotation_z(0.25);
        camera.projection = gaanim_math::Projection::Orthographic { zoom: 1.4 };
        let viewport = gaanim_math::CameraViewport {
            scale: 0.8,
            offset_y: 24.0,
        };
        let expected = camera.to_vello_transform_with_viewport(viewport);
        let resolved = gaanim_math::ResolvedCamera::new(camera, viewport);
        let actual = capture_camera_to_vello_transform(
            Some(&resolved),
            960,
            540,
            crate::config::OutputFit::Error,
        );

        for (actual, expected) in actual.as_coeffs().iter().zip(expected.as_coeffs()) {
            assert!((actual - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn window_scene_setup_assigns_vello_viewport_before_startup() {
        let mut app = App::new();
        app.insert_resource(SetupCallback(Some(Box::new(|world| {
            world.spawn((Camera2d, Camera::default(), VelloView));
        }))));
        app.insert_resource(WindowRenderSize {
            width: 640,
            height: 360,
        });

        setup_window_scene_system(app.world_mut());

        let mut query = app.world_mut().query_filtered::<&Camera, With<VelloView>>();
        let viewport = query
            .single(app.world())
            .expect("Vello camera")
            .viewport
            .as_ref()
            .expect("explicit viewport");
        assert_eq!(viewport.physical_position, UVec2::ZERO);
        assert_eq!(viewport.physical_size, UVec2::new(640, 360));
    }

    #[test]
    fn hybrid_result_survives_world_cleanup() {
        let (frame_tx, frame_rx) = sync_channel(1);
        let (result_tx, result_rx) = sync_channel(1);
        let mut pipeline = HybridCapturePipeline {
            times: vec![0.25],
            index: 1,
            warmup_pending: false,
            phase: 0,
            frames: vec![CapturedFrame {
                time: 0.25,
                width: 1,
                height: 1,
                rgba: vec![1, 2, 3, 4],
            }],
            tx: frame_tx,
            rx: Mutex::new(frame_rx),
            width: 1,
            height: 1,
            result_tx,
            result_sent: false,
        };

        publish_hybrid_capture_result(&mut pipeline);
        drop(pipeline);

        let frames = result_rx.recv().expect("external capture result");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].rgba, vec![1, 2, 3, 4]);
    }

    #[test]
    fn hybrid_capture_discards_the_cold_swapchain_frame() {
        let (frame_tx, frame_rx) = sync_channel(1);
        let (result_tx, _result_rx) = sync_channel(1);
        let mut pipeline = HybridCapturePipeline {
            times: vec![0.25],
            index: 0,
            warmup_pending: true,
            phase: 99,
            frames: Vec::new(),
            tx: frame_tx,
            rx: Mutex::new(frame_rx),
            width: 1,
            height: 1,
            result_tx,
            result_sent: false,
        };

        accept_hybrid_capture(&mut pipeline, vec![0, 0, 0, 255]);
        assert!(!pipeline.warmup_pending);
        assert_eq!(pipeline.phase, 0);
        assert_eq!(pipeline.index, 0);
        assert!(pipeline.frames.is_empty());

        accept_hybrid_capture(&mut pipeline, vec![1, 2, 3, 4]);
        assert_eq!(pipeline.index, 1);
        assert_eq!(pipeline.frames.len(), 1);
        assert_eq!(pipeline.frames[0].rgba, vec![1, 2, 3, 4]);
    }
}
