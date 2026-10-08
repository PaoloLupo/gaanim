use bevy::app::AppExit;
use bevy::camera::Viewport;
use bevy::ecs::observer::On;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use gaanim_core::console;
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

/// The terminal progress bar every export draws, from `gaanim export` or the
/// editor.
pub fn create_progress_bar(total_frames: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_frames).with_prefix("render");
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  {spinner:.99} {prefix:<9.bold.99} {bar:32.99/238} {pos:>4}/{len} frames · {msg} · {eta} left")
            .unwrap()
            .progress_chars("━━─")
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

/// Prints a status line on stdout and records it, without colour, in the
/// telemetry the editor's export dialog shows.
fn export_log(
    telemetry: &Option<ExportTelemetry>,
    level: console::Level,
    label: &str,
    message: impl Into<String>,
) {
    let message = message.into();
    if let Some(telemetry) = telemetry {
        telemetry.push_log(console::format_line(level, label, &message, false));
    }
    let color = console::color_enabled(console::Stream::Stdout);
    println!("{}", console::format_line(level, label, &message, color));
}

/// An export setting under the heading. `Encoder:` lines are also parsed by
/// the editor and the export smoke test, so keys keep their colon.
fn export_detail(telemetry: &Option<ExportTelemetry>, key: &str, value: impl std::fmt::Display) {
    let value = value.to_string();
    if let Some(telemetry) = telemetry {
        telemetry.push_log(console::format_detail(key, &value, false));
    }
    let color = console::color_enabled(console::Stream::Stdout);
    println!("{}", console::format_detail(key, &value, color));
}

/// The settings every export prints before rendering its first frame.
fn export_summary(telemetry: &Option<ExportTelemetry>, config: &ExportConfig) {
    export_log(
        telemetry,
        console::Level::Info,
        "export",
        config.output_path.clone(),
    );
    export_detail(
        telemetry,
        "Resolution",
        format!("{}×{} at {} fps", config.width, config.height, config.fps),
    );
    export_detail(telemetry, "Format", format_label(&config.format));
    export_detail(telemetry, "Encoder", encoder_label(config));
    if config.transparent {
        export_detail(telemetry, "Background", "transparent");
    }
    if let (Some(start), Some(end)) = (config.start_time, config.end_time) {
        export_detail(telemetry, "Segment", format!("{start:.2}s to {end:.2}s"));
    }
}

pub(crate) fn export_progress(telemetry: &Option<ExportTelemetry>, current: u64, total: u64) {
    if let Some(telemetry) = telemetry {
        telemetry.set_current_frame(current);
    }
    // The isolated 3D worker forwards this marker to the editor over stdout.
    // Normal exports use the shared telemetry directly and stay human-readable.
    if std::env::var_os("GAANIM_EXPORT_WORKER").is_some() {
        println!("GAANIM_EXPORT_PROGRESS {current} {total}");
    }
}

/// Where the time of one headless export went, for the benchmark harness.
#[derive(Default)]
struct ExportTimings {
    /// GPU context, ECS setup and the first update, before the first frame.
    setup: Duration,
    /// Seeking the timeline and running the ECS schedule for each frame.
    update: Duration,
    /// Composing each frame's Vello scene, effects and post-processing.
    scene_build: Duration,
    render_gpu: Duration,
    /// The part of `render_gpu` spent waiting for the GPU to finish a frame.
    readback_wait: Duration,
    encoder_wait: Duration,
    encode_active: Duration,
    finalize: Duration,
    total: Duration,
    /// Frames sent again because they equal the frame before.
    reused_frames: u64,
}

fn publish_benchmark_timings(encoder: VideoEncoder, timings: &ExportTimings) {
    if std::env::var("GAANIM_BENCHMARK_SCENARIO").as_deref() != Ok("export") {
        return;
    }
    let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
    println!(
        "GAANIM_EXPORT_TIMINGS encoder={} setup_ms={:.3} update_ms={:.3} scene_build_ms={:.3} render_gpu_ms={:.3} readback_wait_ms={:.3} encoder_wait_ms={:.3} encode_active_ms={:.3} finalize_ms={:.3} total_ms={:.3} reused_frames={}",
        encoder.ffmpeg_name(),
        ms(timings.setup),
        ms(timings.update),
        ms(timings.scene_build),
        ms(timings.render_gpu),
        ms(timings.readback_wait),
        ms(timings.encoder_wait),
        ms(timings.encode_active),
        ms(timings.finalize),
        ms(timings.total),
        timings.reused_frames,
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
    /// When the progress bar last showed the speed, and at which frame.
    pub last_report: (Instant, u64),
    pub export_width: u32,
    pub export_height: u32,
    pub resize_filter: image::imageops::FilterType,
    pub telemetry: Option<ExportTelemetry>,
    pub output_path: String,
    pub result_tx: SyncSender<Result<()>>,
    pub result_sent: bool,
}

/// Name the frame whose scene did not fit the GPU renderer.
/// Read back the frame `in_flight` names, if any, timing it as rendering.
fn finish_in_flight(
    gpu: &mut GpuContext,
    in_flight: &mut Option<f64>,
    timings: &mut ExportTimings,
) -> Result<Option<crate::gpu::FramePixels>> {
    let Some(time) = in_flight.take() else {
        return Ok(None);
    };
    let started = Instant::now();
    let pixels = gpu
        .finish_frame()
        .map_err(|error| frame_render_error(error, time))?;
    timings.render_gpu += started.elapsed();
    Ok(Some(pixels))
}

/// The scene's clear color, which an exported frame is drawn over.
fn clear_color(world: &World) -> vello::peniko::Color {
    world
        .get_resource::<ClearColor>()
        .map(|cc| {
            let rgba = cc.0.to_srgba();
            vello::peniko::Color::from_rgba8(
                (rgba.red * 255.0) as u8,
                (rgba.green * 255.0) as u8,
                (rgba.blue * 255.0) as u8,
                (rgba.alpha * 255.0) as u8,
            )
        })
        .unwrap_or(vello::peniko::Color::BLACK)
}

fn frame_render_error(error: crate::gpu::GpuContextError, time: f64) -> ExportError {
    match error {
        crate::gpu::GpuContextError::SceneTooComplex { .. } => {
            ExportError::Capture(format!("frame at {time:.2} s: {error}"))
        }
        other => other.into(),
    }
}

pub(crate) fn check_custom_animation_errors(world: &World) -> Result<()> {
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

/// One-shot world setup run before the first exported frame.
type WorldSetup = Box<dyn FnOnce(&mut World) + Send + Sync>;

#[derive(Resource)]
pub(crate) struct SetupCallback(Option<WorldSetup>);

impl SetupCallback {
    pub(crate) fn new(setup: impl FnOnce(&mut World) + Send + Sync + 'static) -> Self {
        Self(Some(Box::new(setup)))
    }
}

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
        export_log(
            &pipeline.telemetry,
            console::Level::Error,
            "error",
            message.to_string(),
        );
        publish_export_result(
            &pipeline.result_tx,
            &mut pipeline.result_sent,
            Err(ExportError::Capture(message)),
        );
        exit.write(AppExit::Success);
        return;
    }
    let pipeline = &mut *pipeline_res;
    if pipeline.waiting_for_gpu {
        let rx = pipeline.rx.lock().unwrap();
        match rx.try_recv() {
            Ok(frame_data) => {
                if let Err(e) = pipeline.encoder.push_frame(frame_data) {
                    export_log(
                        &pipeline.telemetry,
                        console::Level::Error,
                        "error",
                        e.to_string(),
                    );
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
                    let (reported_at, reported_frames) = pipeline.last_report;
                    let frames = pipeline.rendered_frames - reported_frames;
                    let speed = frames as f64 / reported_at.elapsed().as_secs_f64();
                    pipeline
                        .progress_bar
                        .set_message(format!("{:.1} fps", speed));
                    pipeline.last_report = (Instant::now(), pipeline.rendered_frames);
                }
                pipeline.progress_bar.inc(1);

                pipeline.waiting_for_gpu = false;

                if pipeline.rendered_frames >= pipeline.total_frames {
                    pipeline.progress_bar.finish_and_clear();
                    export_log(
                        &pipeline.telemetry,
                        console::Level::Info,
                        "encode",
                        "Finalizing the file",
                    );
                    if let Err(e) = pipeline.encoder.finalize() {
                        export_log(
                            &pipeline.telemetry,
                            console::Level::Error,
                            "error",
                            e.to_string(),
                        );
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
                        console::Level::Success,
                        "done",
                        format!(
                            "Exported in {:.2}s: {}",
                            duration.as_secs_f64(),
                            pipeline.output_path
                        ),
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
                export_log(
                    &pipeline.telemetry,
                    console::Level::Error,
                    "error",
                    error.to_string(),
                );
                bevy::prelude::error!("{error}");
                publish_export_result(&pipeline.result_tx, &mut pipeline.result_sent, Err(error));
                exit.write(AppExit::Success);
                return;
            }
        }
    }

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
                for chunk in data.as_chunks_mut::<4>().0 {
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

pub(crate) fn setup_scene_system(world: &mut World) {
    if let Some(mut callback_res) = world.get_resource_mut::<SetupCallback>()
        && let Some(callback) = callback_res.0.take()
    {
        callback(world);
    }
}

/// Replay a window-backed scene before Vello creates its render target, then
/// give the Vello camera an explicit physical viewport. This removes the
/// startup race between WindowPlugin and the Vello canvas texture setup.
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

    export_summary(&telemetry, &config);

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
            .set(gaanim_scene::gaanim_asset_plugin())
            .set(gaanim_scene::logging::log_plugin()),
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
    validate_render_range(render_start, render_end, timeline_duration)?;

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
        nv12_input: false,
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
        last_report: (Instant::now(), 0),
        export_width: config.width,
        export_height: config.height,
        resize_filter,
        telemetry,
        output_path: config.output_path.clone(),
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

    export_summary(&telemetry, &config);

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
    validate_render_range(render_start, render_end, timeline_duration)?;

    // Video encoders convert to YUV 4:2:0: the GPU does it and the readback
    // carries less than half the bytes. Motion blur averages RGBA frames.
    let nv12 = match config.format {
        crate::encoder::ExportFormat::Mp4 => true,
        crate::encoder::ExportFormat::Webm => !config.transparent,
        _ => false,
    } && !scene_has_motion_blur(app.world())
        && gpu.read_nv12();
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
        nv12_input: nv12,
    })?;

    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    if let Some(telemetry) = &telemetry {
        telemetry.set_total_frames(total_frames);
    }
    let frame_time_step = 1.0 / config.fps as f64;
    let pb = create_progress_bar(total_frames);

    let mut current_time = render_start;
    let mut last_report = (Instant::now(), 0_u64);
    let mut timings = ExportTimings {
        setup: start_time.elapsed(),
        ..Default::default()
    };
    let mut delivered = 0_u64;
    let mut deliver =
        |pixels: crate::gpu::FramePixels, timings: &mut ExportTimings| -> Result<()> {
            let encoder_wait_started_at = Instant::now();
            encoder.push_frame(pixels).map_err(|e| match e {
                // FFmpeg failures already explain the cause and the fix.
                ExportError::FFmpeg(_) => e,
                other => ExportError::Capture(format!("Encoder push error: {}", other)),
            })?;
            timings.encoder_wait += encoder_wait_started_at.elapsed();
            delivered += 1;
            if delivered.is_multiple_of(10) || delivered == total_frames {
                let (reported_at, reported) = last_report;
                let speed = (delivered - reported) as f64 / reported_at.elapsed().as_secs_f64();
                pb.set_message(format!("{:.1} fps", speed));
                last_report = (Instant::now(), delivered);
            }
            pb.inc(1);
            export_progress(&telemetry, delivered, total_frames);
            Ok(())
        };
    // The time of the frame the GPU is drawing: the next frame is updated
    // and composed meanwhile, and the frame is read back after that.
    let mut in_flight: Option<f64> = None;
    // While frames repeat the last one (a wait() or a stop), its pixels are
    // sent again instead of rendering it.
    let mut held: Option<(crate::gpu::FrameJob, std::sync::Arc<[u8]>)> = None;

    for _ in 0..total_frames {
        let update_started_at = Instant::now();
        {
            let world = app.world_mut();
            let mut timeline = world.resource_mut::<Timeline>();
            timeline.seek_request = Some(current_time);
        }

        app.update();
        check_custom_animation_errors(app.world())?;
        timings.update += update_started_at.elapsed();

        if let Some(blur) = frame_motion_blur(app.world()) {
            if let Some(pixels) = finish_in_flight(&mut gpu, &mut in_flight, &mut timings)? {
                deliver(pixels, &mut timings)?;
            }
            let render_started_at = Instant::now();
            let frame_data = render_motion_blurred(
                &mut app,
                &mut gpu,
                &config,
                current_time,
                f64::from(config.fps),
                blur,
            )?;
            timings.render_gpu += render_started_at.elapsed();
            deliver(frame_data.into(), &mut timings)?;
            current_time += frame_time_step;
            continue;
        }

        let scene_build_started_at = Instant::now();
        let frame = {
            let resolved_camera = frame_camera(app.world());
            let composed = gaanim_renderer::pipeline::compile_frame_from_world(
                app.world_mut(),
                resolved_camera.as_ref().map(|resolved| &resolved.camera),
            );
            let camera_to_vello = capture_camera_to_vello_transform(
                resolved_camera.as_ref(),
                config.width,
                config.height,
                config.fit,
            );
            let composed = composed.transformed(camera_to_vello);
            let frame = capture_camera_frame(
                resolved_camera.as_ref(),
                config.width,
                config.height,
                config.fit,
            );
            let post = export_post_process(app.world(), frame, composed.transition.as_ref());
            crate::gpu::FrameJob {
                scene: composed.scene,
                layers: composed.transition,
                effects: composed.effects,
                backgrounds: composed.backgrounds,
                base_color: clear_color(app.world()),
                post,
            }
        };
        timings.scene_build += scene_build_started_at.elapsed();

        if let Some((job, pixels)) = &held
            && frame.same_output(job)
        {
            timings.reused_frames += 1;
            deliver(
                crate::gpu::FramePixels::Shared(pixels.clone()),
                &mut timings,
            )?;
            current_time += frame_time_step;
            continue;
        }
        held = None;
        if gpu
            .pending_frame()
            .is_some_and(|pending| frame.same_output(pending))
        {
            // The frame in flight is held from here on: keep its pixels.
            let pixels: std::sync::Arc<[u8]> =
                match finish_in_flight(&mut gpu, &mut in_flight, &mut timings)? {
                    Some(pixels) => pixels.into_pixels().into(),
                    None => unreachable!("a pending frame is in flight"),
                };
            deliver(
                crate::gpu::FramePixels::Shared(pixels.clone()),
                &mut timings,
            )?;
            timings.reused_frames += 1;
            deliver(
                crate::gpu::FramePixels::Shared(pixels.clone()),
                &mut timings,
            )?;
            held = Some((frame, pixels));
            current_time += frame_time_step;
            continue;
        }
        if let Some(pixels) = finish_in_flight(&mut gpu, &mut in_flight, &mut timings)? {
            deliver(pixels, &mut timings)?;
        }
        let render_started_at = Instant::now();
        gpu.submit_frame(frame)
            .map_err(|error| frame_render_error(error, current_time))?;
        timings.render_gpu += render_started_at.elapsed();
        in_flight = Some(current_time);

        current_time += frame_time_step;
    }
    if let Some(pixels) = finish_in_flight(&mut gpu, &mut in_flight, &mut timings)? {
        deliver(pixels, &mut timings)?;
    }

    pb.finish_and_clear();
    export_log(
        &telemetry,
        console::Level::Info,
        "encode",
        "Finalizing the file",
    );

    let finalize_started_at = Instant::now();
    timings.encode_active = encoder.finalize_with_timings().inspect_err(|e| {
        export_log(&telemetry, console::Level::Error, "error", e.to_string());
        bevy::prelude::error!("Encoder finalization error: {}", e);
    })?;
    timings.finalize = finalize_started_at.elapsed();
    timings.readback_wait = gpu.readback_wait();

    let duration = start_time.elapsed();
    timings.total = duration;
    publish_benchmark_timings(config.video_encoder, &timings);
    export_log(
        &telemetry,
        console::Level::Success,
        "done",
        format!(
            "Exported in {:.2}s: {}",
            duration.as_secs_f64(),
            config.output_path
        ),
    );

    Ok(())
}

/// Export a video from a recorded playback bundle, without Python.
///
/// The video runs at the bundle's recorded frame rate, so every output frame
/// is a recorded frame: the very frame an export of the scene at that rate
/// renders, and the video matches it pixel for pixel.
pub fn export_bundle(bundle_path: &std::path::Path, config: ExportConfig) -> Result<()> {
    let start_time = Instant::now();
    let telemetry = config.telemetry.clone();
    let mut bundle = gaanim_bundle::Bundle::open(bundle_path)
        .map_err(|error| ExportError::General(error.to_string()))?;
    let mut config = config.apply_presets();
    // Every output frame is a recorded frame: the video runs at the rate
    // the bundle was recorded at, whatever the quality preset.
    config.fps = bundle.scene.fps;
    let (frame_width, frame_height) = bundle.scene.output_size;
    if config.fit == crate::config::OutputFit::Error && frame_width > 0 && frame_height > 0 {
        let aspect = f64::from(frame_width) / f64::from(frame_height);
        let expected_height = f64::from(config.width) / aspect;
        let expected_width = f64::from(config.height) * aspect;
        if (expected_height - f64::from(config.height)).abs() > 1.0
            && (expected_width - f64::from(config.width)).abs() > 1.0
        {
            return Err(ExportError::General(format!(
                "output {}x{} does not match the bundle's {frame_width}x{frame_height} frame; choose a matching resolution or set fit to contain/cover",
                config.width, config.height
            )));
        }
    }
    config.video_encoder = resolve_video_encoder(config.format, config.video_encoder);
    if let Some(telemetry) = &telemetry {
        telemetry.set_encoder(encoder_label(&config));
    }
    if !bundle.scene.audio.is_empty() {
        let dir = std::env::temp_dir().join("gaanim-bundle-media");
        let files = bundle
            .extract_media(&dir)
            .map_err(|error| ExportError::General(error.to_string()))?;
        for audio in &bundle.scene.audio {
            if let Some(path) = files.get(&audio.media) {
                config.audio_tracks.push(crate::config::AudioTrack {
                    path: path.clone(),
                    start_time: audio.start_time,
                    duration: audio.duration,
                    volume: audio.volume,
                    fade_in: audio.fade_in,
                    fade_out: audio.fade_out,
                    source_offset: audio.source_offset,
                    source_duration: audio.source_duration,
                    speed: audio.speed,
                    looping: audio.looping,
                });
            }
        }
    }
    export_summary(&telemetry, &config);

    let duration = bundle.scene.duration;
    let render_start = config.start_time.unwrap_or(0.0).max(0.0);
    let render_end = config.end_time.unwrap_or(duration).min(duration);
    let render_length = render_end - render_start;
    validate_render_range(render_start, render_end, duration)?;
    let mut renderer = BundleRenderer::new(bundle, config.width, config.height, config.fit)?;

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
        nv12_input: false,
    })?;
    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    if let Some(telemetry) = &telemetry {
        telemetry.set_total_frames(total_frames);
    }
    let frame_time_step = 1.0 / config.fps as f64;
    let pb = create_progress_bar(total_frames);
    let mut current_time = render_start;
    for frame_idx in 0..total_frames {
        let frame_data = renderer.render(current_time)?;
        encoder.push_frame(frame_data).map_err(|e| match e {
            ExportError::FFmpeg(_) => e,
            other => ExportError::Capture(format!("Encoder push error: {}", other)),
        })?;
        pb.inc(1);
        export_progress(&telemetry, frame_idx + 1, total_frames);
        current_time += frame_time_step;
    }
    pb.finish_and_clear();
    encoder.finalize_with_timings()?;
    export_log(
        &telemetry,
        console::Level::Success,
        "done",
        format!(
            "Exported in {:.2}s: {}",
            start_time.elapsed().as_secs_f64(),
            config.output_path
        ),
    );
    Ok(())
}

/// Renders the frames of a playback bundle, as an export of the scene renders
/// them at the same size.
pub struct BundleRenderer {
    bundle: gaanim_bundle::Bundle,
    frames: FrameRasterizer,
    /// Live zones replay their preview players over the recorded frames.
    zones: gaanim_animation::live::PreviewReplay,
}

impl BundleRenderer {
    pub fn new(
        bundle: gaanim_bundle::Bundle,
        width: u32,
        height: u32,
        fit: crate::config::OutputFit,
    ) -> Result<Self> {
        let frames = FrameRasterizer::new(&bundle.scene, width, height, fit)?;
        Ok(Self {
            bundle,
            frames,
            zones: Default::default(),
        })
    }

    /// RGBA pixels of the recorded frame shown at `time`; a motion-blurred
    /// frame averages its sub-frames as an export does.
    pub fn render(&mut self, time: f64) -> Result<Vec<u8>> {
        let index = self.bundle.frame_index_at(time);
        let frame = self
            .bundle
            .frame(index)
            .map_err(|error| ExportError::General(error.to_string()))?;
        let overlay = self.zones.overlay(
            &self.bundle.scene.live_zones,
            self.bundle.scene.rehearsal.as_ref(),
            time,
        );
        self.frames.render_with(&frame, time, Some(&overlay))
    }
}

/// Rasterizes captured frames of a scene, as an export of the scene renders
/// them at the same size.
pub struct FrameRasterizer {
    gpu: GpuContext,
    background: Option<gaanim_renderer::pipeline::CanvasBackground>,
    bg_color: vello::peniko::Color,
    post_shaders: Vec<gaanim_renderer::post_process::PostProcessShader>,
    store: gaanim_renderer::fragment::FragmentStore,
    width: u32,
    height: u32,
    fit: crate::config::OutputFit,
}

impl FrameRasterizer {
    pub fn new(
        scene: &gaanim_bundle::SceneData,
        width: u32,
        height: u32,
        fit: crate::config::OutputFit,
    ) -> Result<Self> {
        let gpu = GpuContext::new(width, height)?;
        let background = scene.background.clone().map(|mut background| {
            background.pixel_size = (width, height);
            background
        });
        let bg_color = scene
            .clear_color
            .map(|[r, g, b, a]| vello::peniko::Color::from_rgba8(r, g, b, a))
            .unwrap_or(vello::peniko::Color::BLACK);
        Ok(Self {
            gpu,
            background,
            bg_color,
            post_shaders: scene.post_shaders.clone(),
            store: gaanim_renderer::fragment::FragmentStore::default(),
            width,
            height,
            fit,
        })
    }

    /// RGBA pixels of `frame`; a motion-blurred frame averages its
    /// sub-frames as an export does.
    pub fn render(&mut self, frame: &gaanim_bundle::Frame, time: f64) -> Result<Vec<u8>> {
        self.render_with(frame, time, None)
    }

    /// [`FrameRasterizer::render`] with `overlay` (live zones) drawn above
    /// the frame's drawables.
    pub fn render_with(
        &mut self,
        frame: &gaanim_bundle::Frame,
        time: f64,
        overlay: Option<&gaanim_animation::live::LiveOverlay>,
    ) -> Result<Vec<u8>> {
        if frame.motion_blur.is_empty() {
            let job = self.frame_job(frame, overlay);
            return self
                .gpu
                .render_frame_layers(
                    &job.scene,
                    job.layers.as_ref(),
                    &job.effects,
                    &job.backgrounds,
                    job.base_color,
                    job.post.as_ref(),
                )
                .map_err(|error| frame_render_error(error, time));
        }
        let mut average = LinearAverage::new(self.width as usize, self.height as usize);
        // The GPU draws each sub-frame while the next one is composed.
        let mut in_flight: Option<f64> = None;
        for sample in &frame.motion_blur {
            let job = self.frame_job(sample, overlay);
            let drawn = in_flight
                .take()
                .map(|previous| {
                    self.gpu
                        .finish_frame()
                        .map_err(|error| frame_render_error(error, previous))
                })
                .transpose()?;
            self.gpu
                .submit_frame(job)
                .map_err(|error| frame_render_error(error, sample.time))?;
            in_flight = Some(sample.time);
            if let Some(pixels) = drawn {
                average.add_frame(&pixels);
            }
        }
        if let Some(previous) = in_flight {
            let pixels = self
                .gpu
                .finish_frame()
                .map_err(|error| frame_render_error(error, previous))?;
            average.add_frame(&pixels);
        }
        Ok(average.finish())
    }

    fn frame_job(
        &mut self,
        frame: &gaanim_bundle::Frame,
        overlay: Option<&gaanim_animation::live::LiveOverlay>,
    ) -> crate::gpu::FrameJob {
        let resolved =
            gaanim_math::ResolvedCamera::new(frame.camera, gaanim_math::CameraViewport::default());
        let composed = compose_bundle_layers(
            frame,
            self.background.as_ref(),
            &mut self.store,
            self.width,
            self.height,
            self.fit,
            overlay,
        );
        let camera_frame = capture_camera_frame(Some(&resolved), self.width, self.height, self.fit);
        let post = (!frame.post.is_empty())
            .then(|| {
                let passes = frame
                    .post
                    .iter()
                    .filter_map(|pass| {
                        let shader = self.post_shaders.get(pass.shader as usize)?.clone();
                        let values: Vec<f64> =
                            pass.values.iter().map(|value| f64::from(*value)).collect();
                        gaanim_renderer::post_process::PostProcessPass::constant(shader, &values)
                            .ok()
                    })
                    .collect();
                gaanim_renderer::post_process::CanvasPostProcess {
                    passes,
                    ..Default::default()
                }
                .request(frame.time, camera_frame)
            })
            .flatten();
        let post = gaanim_renderer::post_process::PostProcessRequest::with_transition(
            post,
            composed.transition.as_ref().map(|layers| &layers.shader),
            camera_frame,
            frame.time as f32,
        );
        crate::gpu::FrameJob {
            scene: composed.scene,
            layers: composed.transition,
            effects: composed.effects,
            backgrounds: composed.backgrounds,
            base_color: self.bg_color,
            post,
        }
    }
}

/// The scene a bundle `frame` draws at `width` by `height` pixels, before
/// post-processing, as [`FrameRasterizer`] renders it. `background` is sized
/// to the output; `store` keeps fragments across the frames of one bundle.
pub fn compose_bundle_frame(
    frame: &gaanim_bundle::Frame,
    background: Option<&gaanim_renderer::pipeline::CanvasBackground>,
    store: &mut gaanim_renderer::fragment::FragmentStore,
    width: u32,
    height: u32,
    fit: crate::config::OutputFit,
    overlay: Option<&gaanim_animation::live::LiveOverlay>,
) -> vello::Scene {
    compose_bundle_parts(frame, background, store, width, height, fit, overlay, false).flattened()
}

/// [`compose_bundle_frame`] keeping a shader transition's segments apart,
/// for [`GpuContext::render_frame_layers`].
pub fn compose_bundle_layers(
    frame: &gaanim_bundle::Frame,
    background: Option<&gaanim_renderer::pipeline::CanvasBackground>,
    store: &mut gaanim_renderer::fragment::FragmentStore,
    width: u32,
    height: u32,
    fit: crate::config::OutputFit,
    overlay: Option<&gaanim_animation::live::LiveOverlay>,
) -> gaanim_renderer::pipeline::ComposedFrame {
    compose_bundle_parts(frame, background, store, width, height, fit, overlay, true)
}

/// [`compose_bundle_layers`]; with `gpu`, shader backgrounds are left to
/// the renderer's device as [`ComposedFrame::backgrounds`], otherwise they
/// are drawn through the CPU.
///
/// [`ComposedFrame::backgrounds`]: gaanim_renderer::pipeline::ComposedFrame::backgrounds
#[allow(clippy::too_many_arguments)]
fn compose_bundle_parts(
    frame: &gaanim_bundle::Frame,
    background: Option<&gaanim_renderer::pipeline::CanvasBackground>,
    store: &mut gaanim_renderer::fragment::FragmentStore,
    width: u32,
    height: u32,
    fit: crate::config::OutputFit,
    overlay: Option<&gaanim_animation::live::LiveOverlay>,
    gpu: bool,
) -> gaanim_renderer::pipeline::ComposedFrame {
    let resolved =
        gaanim_math::ResolvedCamera::new(frame.camera, gaanim_math::CameraViewport::default());
    // Pad opacity layers for this output, as a direct export does.
    let pixels_per_unit = background.and_then(|background| {
        gaanim_renderer::pipeline::output_pixels_per_unit(&frame.camera, background.pixel_size.0)
    });
    let mut background_requests = Vec::new();
    let mut composed = gaanim_renderer::pipeline::compose_captured_frame(
        &frame.capture,
        store,
        background.map(|background| (background, background.pixel_size)),
        pixels_per_unit,
        gpu.then_some(&mut background_requests),
        0.0,
        pixels_per_unit.unwrap_or(gaanim_renderer::pipeline::DEFAULT_EFFECT_DENSITY),
    );
    composed.backgrounds = background_requests;
    store.end_frame();
    if let Some(overlay) = overlay {
        gaanim_renderer::pipeline::append_live_overlay(composed.top_mut(), overlay);
    }
    composed.transformed(capture_camera_to_vello_transform(
        Some(&resolved),
        width,
        height,
        fit,
    ))
}

/// Render the frames of the bundle at `bundle_path` shown at `times`, handing
/// each to `on_frame` as soon as it is read back; see
/// [`capture_scene_direct_streaming`].
pub fn capture_bundle_streaming<C>(
    bundle_path: &std::path::Path,
    width: u32,
    height: u32,
    times: &[f64],
    mut on_frame: C,
) -> Result<()>
where
    C: FnMut(CapturedFrame) -> std::ops::ControlFlow<()>,
{
    let bundle = gaanim_bundle::Bundle::open(bundle_path)
        .map_err(|error| ExportError::General(error.to_string()))?;
    let mut renderer =
        BundleRenderer::new(bundle, width, height, crate::config::OutputFit::Contain)?;
    for &time in times {
        let rgba = renderer.render(time)?;
        if on_frame(CapturedFrame {
            time,
            width,
            height,
            rgba,
        })
        .is_break()
        {
            break;
        }
    }
    Ok(())
}

/// Reject a time range that selects no frames, naming the scene duration.
fn validate_render_range(start: f64, end: f64, duration: f64) -> Result<()> {
    if end - start <= 0.0 {
        return Err(ExportError::General(format!(
            "export range {start:.3}s to {end:.3}s selects no frames; the scene lasts {duration:.3}s"
        )));
    }
    Ok(())
}

/// Timestamps this close to the scene duration capture its final frame.
///
/// Segment metadata and timeline clips accumulate the same authored durations
/// in different orders, so a stop authored at the very end of a scene can land
/// a few ULPs after the clip-derived duration.
const CAPTURE_DURATION_TOLERANCE: f64 = 1e-6;

/// Clamp capture timestamps that differ from the scene end only by floating
/// point accumulation; reject timestamps that are genuinely out of range.
fn resolve_capture_times(times: &[f64], duration: f64) -> Result<Vec<f64>> {
    times
        .iter()
        .map(|&time| {
            if time > duration + CAPTURE_DURATION_TOLERANCE {
                Err(ExportError::Capture(format!(
                    "snapshot timestamp {time:.6}s exceeds scene duration {duration:.6}s"
                )))
            } else {
                Ok(time.min(duration))
            }
        })
        .collect()
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
    let mut frames = Vec::with_capacity(times.len());
    capture_scene_direct_streaming(config, times, setup_world_fn, |frame| {
        frames.push(frame);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(frames)
}

/// Streaming form of [`capture_scene_direct`].
///
/// `on_frame` receives every frame as soon as it has been read back, in the
/// order of `times`. Returning [`ControlFlow::Break`](std::ops::ControlFlow)
/// stops the capture early without an error, which lets interactive callers
/// cancel obsolete work between frames.
pub fn capture_scene_direct_streaming<F, C>(
    config: ExportConfig,
    times: &[f64],
    setup_world_fn: F,
    mut on_frame: C,
) -> Result<()>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
    C: FnMut(CapturedFrame) -> std::ops::ControlFlow<()>,
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
    let seek_times = resolve_capture_times(times, duration)?;

    let mut timeline_update = Duration::ZERO;
    let mut scene_compile = Duration::ZERO;
    let mut render_readback = Duration::ZERO;
    let frame = |time: f64, rgba: Vec<u8>| CapturedFrame {
        time,
        width: config.width,
        height: config.height,
        rgba,
    };
    // The time of the frame the GPU is drawing: the next frame is seeked
    // and composed meanwhile, and the frame is read back after that.
    let mut in_flight: Option<f64> = None;
    let mut cancelled = false;
    for (&time, &seek_time) in times.iter().zip(&seek_times) {
        let phase_started = Instant::now();
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(seek_time);
        app.update();
        check_custom_animation_errors(app.world())?;
        timeline_update += phase_started.elapsed();

        let phase_started = Instant::now();
        let pending = in_flight.take();
        if let Some(blur) = frame_motion_blur(app.world()) {
            if let Some(pending) = pending {
                let rgba = gpu
                    .finish_frame()
                    .map_err(|error| frame_render_error(error, pending))?
                    .into_pixels();
                if on_frame(frame(pending, rgba)).is_break() {
                    cancelled = true;
                    break;
                }
            }
            let rgba = render_motion_blurred(
                &mut app,
                &mut gpu,
                &config,
                seek_time,
                f64::from(config.fps),
                blur,
            )?;
            render_readback += phase_started.elapsed();
            if on_frame(frame(time, rgba)).is_break() {
                cancelled = true;
                break;
            }
            continue;
        }

        let phase_started = Instant::now();
        let resolved_camera = frame_camera(app.world());
        let composed = gaanim_renderer::pipeline::compile_frame_from_world(
            app.world_mut(),
            resolved_camera.as_ref().map(|resolved| &resolved.camera),
        )
        .transformed(capture_camera_to_vello_transform(
            resolved_camera.as_ref(),
            config.width,
            config.height,
            config.fit,
        ));
        let post = export_post_process(
            app.world(),
            capture_camera_frame(
                resolved_camera.as_ref(),
                config.width,
                config.height,
                config.fit,
            ),
            composed.transition.as_ref(),
        );
        let job = crate::gpu::FrameJob {
            scene: composed.scene,
            layers: composed.transition,
            effects: composed.effects,
            backgrounds: composed.backgrounds,
            base_color: clear_color(app.world()),
            post,
        };
        scene_compile += phase_started.elapsed();

        let phase_started = Instant::now();
        if let Some(pending) = pending {
            let rgba = gpu
                .finish_frame()
                .map_err(|error| frame_render_error(error, pending))?
                .into_pixels();
            if on_frame(frame(pending, rgba)).is_break() {
                cancelled = true;
                break;
            }
        }
        gpu.submit_frame(job)
            .map_err(|error| frame_render_error(error, time))?;
        in_flight = Some(time);
        render_readback += phase_started.elapsed();
    }
    if let Some(pending) = in_flight.filter(|_| !cancelled) {
        let phase_started = Instant::now();
        let rgba = gpu
            .finish_frame()
            .map_err(|error| frame_render_error(error, pending))?
            .into_pixels();
        render_readback += phase_started.elapsed();
        let _ = on_frame(frame(pending, rgba));
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

    Ok(())
}

/// The motion blur of the frame the world was just updated to: the scene's
/// own, or else that of a window holding the frame, such as a whip pan's.
pub(crate) fn frame_motion_blur(world: &World) -> Option<gaanim_renderer::effects::MotionBlur> {
    world
        .get_resource::<gaanim_renderer::effects::MotionBlur>()
        .copied()
        .or_else(|| {
            let time = world.get_resource::<Timeline>()?.current_time;
            world
                .get_resource::<gaanim_renderer::effects::MotionBlurWindows>()?
                .at(time)
        })
}

/// Whether any frame of the scene can be motion blurred.
fn scene_has_motion_blur(world: &World) -> bool {
    world
        .get_resource::<gaanim_renderer::effects::MotionBlur>()
        .is_some()
        || world
            .get_resource::<gaanim_renderer::effects::MotionBlurWindows>()
            .is_some_and(|windows| !windows.0.is_empty())
}

/// Compile the frame the world was just updated to.
fn updated_world_job(
    app: &mut App,
    config: &ExportConfig,
    pins: &mut gaanim_renderer::pipeline::PinnedElements,
) -> crate::gpu::FrameJob {
    let resolved_camera = frame_camera(app.world());
    let composed = gaanim_renderer::pipeline::compile_frame_pinned(
        app.world_mut(),
        resolved_camera.as_ref().map(|resolved| &resolved.camera),
        pins,
    )
    .transformed(capture_camera_to_vello_transform(
        resolved_camera.as_ref(),
        config.width,
        config.height,
        config.fit,
    ));
    let post_process = export_post_process(
        app.world(),
        capture_camera_frame(
            resolved_camera.as_ref(),
            config.width,
            config.height,
            config.fit,
        ),
        composed.transition.as_ref(),
    );
    crate::gpu::FrameJob {
        scene: composed.scene,
        layers: composed.transition,
        effects: composed.effects,
        backgrounds: composed.backgrounds,
        base_color: clear_color(app.world()),
        post: post_process,
    }
}

/// Render the frame at `time`, which the world was just updated to, as the
/// average of its motion blur sub-frames in linear light.
///
/// Sub-frames stay inside the segment that shows the frame, so a cut never
/// bleeds into it, and motion-blur-exempt drawables keep their look at `time`.
/// Sub-frame times a motion-blurred frame at `time` averages, kept inside
/// its segment.
pub(crate) fn motion_blur_times(
    world: &World,
    time: f64,
    fps: f64,
    blur: gaanim_renderer::effects::MotionBlur,
) -> Vec<f64> {
    let (start, end) = {
        let timeline = world.resource::<Timeline>();
        let duration = timeline.cached_duration;
        timeline
            .segments
            .iter()
            .rev()
            .find(|segment| segment.start_time <= time + 1e-9 && time <= segment.end_time + 1e-9)
            // The end of a segment is the first instant of the next one.
            .map_or((0.0, duration), |segment| {
                (segment.start_time, (segment.end_time - 1e-6).min(duration))
            })
    };
    blur.sample_times(time, fps, start, end.max(start))
}

fn render_motion_blurred(
    app: &mut App,
    gpu: &mut GpuContext,
    config: &ExportConfig,
    time: f64,
    fps: f64,
    blur: gaanim_renderer::effects::MotionBlur,
) -> Result<Vec<u8>> {
    let mut pins = gaanim_renderer::pipeline::PinnedElements::default();
    {
        let resolved_camera = frame_camera(app.world());
        gaanim_renderer::pipeline::compile_scene_pinned(
            app.world_mut(),
            resolved_camera.as_ref().map(|resolved| &resolved.camera),
            &mut pins,
        );
    }
    let times = motion_blur_times(app.world(), time, fps, blur);
    let mut average = LinearAverage::new(config.width as usize, config.height as usize);
    // The sub-frame the GPU is drawing: the next one is updated and composed
    // meanwhile, and the one before it is added after that.
    let mut in_flight: Option<f64> = None;
    for &sample in &times {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(sample);
        app.update();
        check_custom_animation_errors(app.world())?;
        let job = updated_world_job(app, config, &mut pins);
        let drawn = in_flight
            .take()
            .map(|previous| {
                gpu.finish_frame()
                    .map_err(|error| frame_render_error(error, previous))
            })
            .transpose()?;
        gpu.submit_frame(job)
            .map_err(|error| frame_render_error(error, sample))?;
        in_flight = Some(sample);
        if let Some(pixels) = drawn {
            average.add_frame(&pixels);
        }
    }
    if let Some(previous) = in_flight {
        let pixels = gpu
            .finish_frame()
            .map_err(|error| frame_render_error(error, previous))?;
        average.add_frame(&pixels);
    }
    Ok(average.finish())
}

/// Running sum of RGBA8 frames as premultiplied linear light.
///
/// Bands of rows are summed and encoded on the compute task pool; each
/// pixel's arithmetic is the same as on one thread.
struct LinearAverage {
    sum: Vec<f32>,
    width: usize,
    frames: u32,
}

impl LinearAverage {
    fn new(width: usize, height: usize) -> Self {
        Self {
            sum: vec![0.0; width * height * 4],
            width,
            frames: 0,
        }
    }

    /// Add a frame read back from the GPU, reading a mapped frame in place.
    fn add_frame(&mut self, pixels: &crate::gpu::FramePixels) {
        pixels.with_rows(self.width * 4, |rgba, stride| self.add_rows(rgba, stride));
    }

    /// Add RGBA8 rows that start every `stride` bytes.
    fn add_rows(&mut self, rgba: &[u8], stride: usize) {
        let linear = srgb_to_linear_table();
        let row = self.width * 4;
        for_row_bands(&mut self.sum, row, |first_row, band| {
            for (y, sums) in band.chunks_exact_mut(row).enumerate() {
                let start = (first_row + y) * stride;
                for (sum, pixel) in sums
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(rgba[start..start + row].as_chunks::<4>().0)
                {
                    let alpha = f32::from(pixel[3]) / 255.0;
                    for channel in 0..3 {
                        sum[channel] += linear[usize::from(pixel[channel])] * alpha;
                    }
                    sum[3] += alpha;
                }
            }
        });
        self.frames += 1;
    }

    fn finish(self) -> Vec<u8> {
        let frames = self.frames.max(1) as f32;
        let row = self.width * 4;
        let mut rgba = vec![0_u8; self.sum.len()];
        let sums = &self.sum;
        for_row_bands(&mut rgba, row, |first_row, band| {
            let start = first_row * row;
            let sums = &sums[start..start + band.len()];
            for (out, sum) in band
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(sums.as_chunks::<4>().0)
            {
                let alpha = sum[3] / frames;
                for channel in 0..3 {
                    let straight = if sum[3] > 0.0 {
                        sum[channel] / sum[3]
                    } else {
                        0.0
                    };
                    out[channel] = linear_to_srgb_u8(straight);
                }
                out[3] = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        });
        rgba
    }
}

/// `f` of consecutive bands of whole rows of `values` (`row` values each),
/// given the index of each band's first row, on the compute task pool
/// (started here when no app did, as when a bundle is rendered).
fn for_row_bands<T: Send>(values: &mut [T], row: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    let rows = values.len() / row.max(1);
    let pool = bevy::tasks::ComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
    let tasks = pool.thread_num().min(rows);
    if tasks < 2 {
        f(0, values);
        return;
    }
    let band = rows.div_ceil(tasks);
    let f = &f;
    pool.scope(|scope| {
        for (index, part) in values.chunks_mut(band * row).enumerate() {
            scope.spawn(async move { f(index * band, part) });
        }
    });
}

fn srgb_to_linear_table() -> &'static [f32; 256] {
    static TABLE: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|value| {
            let c = value as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    })
}

fn linear_to_srgb_u8(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

/// Post-process of the frame just updated in `world`, with the camera frame
/// in output pixels, blending a shader transition's `layers` first.
fn export_post_process(
    world: &World,
    frame: kurbo::Rect,
    layers: Option<&gaanim_renderer::pipeline::TransitionScenes>,
) -> Option<gaanim_renderer::post_process::PostProcessRequest> {
    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let request = world
        .get_resource::<gaanim_renderer::post_process::CanvasPostProcess>()
        .and_then(|post| {
            post.request_with(time, frame, |entity| {
                world
                    .get::<gaanim_animation::FloatSignal>(entity)
                    .map(|signal| signal.value)
            })
        });
    gaanim_renderer::post_process::PostProcessRequest::with_transition(
        request,
        layers.map(|layers| &layers.shader),
        frame,
        time as f32,
    )
}

/// A `frame_width`x`frame_height` rectangle centered in the output and moved
/// down by `offset_y` pixels.
fn centered_frame(
    output_width: u32,
    output_height: u32,
    frame_width: f64,
    frame_height: f64,
    offset_y: f64,
) -> kurbo::Rect {
    let center = kurbo::Point::new(
        f64::from(output_width) / 2.0,
        f64::from(output_height) / 2.0 + offset_y,
    );
    kurbo::Rect::from_center_size(center, (frame_width, frame_height))
}

fn output_fit_scale(
    viewport_width: u32,
    viewport_height: u32,
    output_width: u32,
    output_height: u32,
    fit: crate::config::OutputFit,
) -> f64 {
    let fit_x = output_width as f64 / viewport_width as f64;
    let fit_y = output_height as f64 / viewport_height as f64;
    match fit {
        crate::config::OutputFit::Cover => fit_x.max(fit_y),
        crate::config::OutputFit::Error | crate::config::OutputFit::Contain => fit_x.min(fit_y),
    }
}

/// Camera frame of a capture in output pixels, matching
/// [`capture_camera_to_vello_transform`].
/// The camera a frame is rendered through: the resolved camera, which adds
/// bindings, follow, dynamic framing, and shake to the authored camera.
pub(crate) fn frame_camera(world: &World) -> Option<gaanim_math::ResolvedCamera> {
    world
        .get_resource::<gaanim_math::ResolvedCamera>()
        .copied()
        .or_else(|| {
            world
                .get_resource::<gaanim_math::Camera>()
                .copied()
                .map(|camera| {
                    gaanim_math::ResolvedCamera::new(camera, gaanim_math::CameraViewport::default())
                })
        })
}

fn capture_camera_frame(
    resolved: Option<&gaanim_math::ResolvedCamera>,
    output_width: u32,
    output_height: u32,
    fit: crate::config::OutputFit,
) -> kurbo::Rect {
    let Some(resolved) = resolved else {
        return centered_frame(
            output_width,
            output_height,
            f64::from(output_width),
            f64::from(output_height),
            0.0,
        );
    };
    let (width, height) = (
        resolved.camera.viewport_width.max(1),
        resolved.camera.viewport_height.max(1),
    );
    let fit_scale = output_fit_scale(width, height, output_width, output_height, fit);
    let scale = fit_scale * resolved.viewport.scale;
    centered_frame(
        output_width,
        output_height,
        f64::from(width) * scale,
        f64::from(height) * scale,
        resolved.viewport.offset_y * fit_scale,
    )
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
                // Under perspective the canvas stays at the origin, unrotated,
                // as in the preview: 3D content is projected onto it.
                let (zoom, cam_x, cam_y, angle) = match camera.projection {
                    gaanim_math::Projection::Orthographic { zoom } => {
                        (zoom, camera.position.x, camera.position.y, camera.z_angle())
                    }
                    _ => (1.0, 0.0, 0.0, 0.0),
                };
                (
                    zoom,
                    camera.pixels_per_unit(),
                    camera.viewport_width.max(1),
                    camera.viewport_height.max(1),
                    cam_x,
                    cam_y,
                    angle,
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
    let fit_scale = output_fit_scale(
        viewport_width,
        viewport_height,
        output_width,
        output_height,
        fit,
    );
    let scale = pixels_per_unit * zoom * fit_scale * viewport.scale;
    let offset_y = viewport.offset_y * fit_scale;

    kurbo::Affine::translate((
        output_width as f64 / 2.0,
        output_height as f64 / 2.0 + offset_y,
    )) * kurbo::Affine::scale_non_uniform(scale, -scale)
        * kurbo::Affine::rotate(-angle)
        * kurbo::Affine::translate((-cam_x, -cam_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_average_blends_in_linear_light() {
        let mut average = LinearAverage::new(2, 1);
        average.add_rows(&[0, 0, 0, 255, 255, 255, 255, 255], 8);
        average.add_rows(&[255, 255, 255, 255, 255, 255, 255, 0], 8);
        let out = average.finish();
        // Black and white average to linear 0.5, sRGB 188.
        assert_eq!(&out[..4], &[188, 188, 188, 255]);
        // A transparent sample adds coverage, not color.
        assert_eq!(&out[4..], &[255, 255, 255, 128]);
        for value in [0_u8, 1, 17, 128, 254, 255] {
            let linear = srgb_to_linear_table()[usize::from(value)];
            assert_eq!(linear_to_srgb_u8(linear), value);
        }
    }

    #[test]
    fn linear_average_reads_padded_rows_in_bands() {
        // Rows of 3 pixels padded to 4, as a mapped GPU frame aligns them.
        let (width, height) = (3, 40);
        let frame = |seed: u8| -> Vec<u8> {
            (0..height * 16)
                .map(|i| (i as u8).wrapping_mul(seed).wrapping_add(seed))
                .collect()
        };
        let mut banded = LinearAverage::new(width, height);
        let mut row_by_row = Vec::new();
        for seed in [3, 7, 11] {
            let rgba = frame(seed);
            banded.add_rows(&rgba, 16);
            row_by_row.push(rgba);
        }
        let out = banded.finish();
        for y in 0..height {
            let mut single = LinearAverage::new(width, 1);
            for rgba in &row_by_row {
                single.add_rows(&rgba[y * 16..y * 16 + 12], 12);
            }
            assert_eq!(
                &out[y * 12..y * 12 + 12],
                single.finish().as_slice(),
                "row {y}"
            );
        }
    }

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
    fn empty_render_ranges_are_rejected() {
        assert!(validate_render_range(2.0, 4.0, 10.0).is_ok());
        let error = validate_render_range(12.0, 10.0, 10.0)
            .unwrap_err()
            .to_string();
        assert!(error.contains("selects no frames"), "{error}");
        assert!(error.contains("10.000s"), "{error}");
    }

    #[test]
    fn capture_times_absorb_floating_point_drift_at_the_scene_end() {
        // 135.55 accumulated in a different order than the clip duration.
        let duration = 135.55;
        let drifted = duration + 2.0 * f64::EPSILON * duration;
        assert!(drifted > duration);

        let resolved = resolve_capture_times(&[0.0, 12.5, drifted], duration).unwrap();

        assert_eq!(resolved, vec![0.0, 12.5, duration]);
        let error = resolve_capture_times(&[duration + 0.01], duration)
            .unwrap_err()
            .to_string();
        assert!(error.contains("exceeds scene duration"));
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
    fn capture_post_process_frame_matches_the_camera_transform() {
        let camera = gaanim_math::Camera::ortho_2d_frame(16.0, 9.0, 1280, 720);
        let viewport = gaanim_math::CameraViewport {
            scale: 0.8,
            offset_y: -30.0,
        };
        let resolved = gaanim_math::ResolvedCamera::new(camera, viewport);
        for fit in [
            crate::config::OutputFit::Contain,
            crate::config::OutputFit::Cover,
        ] {
            let transform = capture_camera_to_vello_transform(Some(&resolved), 1000, 800, fit);
            let frame = capture_camera_frame(Some(&resolved), 1000, 800, fit);
            let top_left = transform * kurbo::Point::new(-8.0, 4.5);
            let bottom_right = transform * kurbo::Point::new(8.0, -4.5);
            assert!((frame.origin() - top_left).hypot() < 1e-9, "{fit:?}");
            assert!(
                (kurbo::Point::new(frame.x1, frame.y1) - bottom_right).hypot() < 1e-9,
                "{fit:?}"
            );
        }
    }

    #[test]
    fn frames_render_through_the_resolved_camera() {
        let authored = gaanim_math::Camera::ortho_2d(960, 540);
        let mut shaken = authored;
        shaken.position.x += 0.14;
        let mut world = World::new();
        world.insert_resource(authored);
        world.insert_resource(gaanim_math::ResolvedCamera::new(
            shaken,
            gaanim_math::CameraViewport::default(),
        ));
        assert_eq!(frame_camera(&world).unwrap().camera, shaken);

        world.remove_resource::<gaanim_math::ResolvedCamera>();
        assert_eq!(frame_camera(&world).unwrap().camera, authored);
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
}
