use bevy::app::AppExit;
use bevy::ecs::observer::On;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::Instant;

use gaanim_timeline::timeline::Timeline;

use crate::config::ExportConfig;
use crate::encoder::{EncoderConfig, ExportError, ParallelEncoder, Result};
use crate::gpu::GpuContext;

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
}

#[derive(Resource)]
struct SetupCallback(Option<Box<dyn FnOnce(&mut World) + Send + Sync>>);

fn export_pipeline_system(
    mut commands: Commands,
    mut pipeline_res: ResMut<ExportPipeline>,
    mut timeline: ResMut<Timeline>,
    mut exit: MessageWriter<'_, AppExit>,
) {
    let pipeline = &mut *pipeline_res;
    if pipeline.waiting_for_gpu {
        let rx = pipeline.rx.lock().unwrap();
        match rx.try_recv() {
            Ok(frame_data) => {
                if let Err(e) = pipeline.encoder.push_frame(frame_data) {
                    bevy::prelude::error!("Encoder error: {}", e);
                    exit.write(AppExit::Success);
                    return;
                }

                pipeline.rendered_frames += 1;

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
                    println!("  Finalizing video file...");
                    if let Err(e) = pipeline.encoder.finalize() {
                        bevy::prelude::error!("Encoder finalization error: {}", e);
                    }

                    let duration = pipeline.start_time.elapsed();
                    println!("------------------------------------------------------------");
                    println!(
                        "✓ Export successfully completed in {:.2}s!",
                        duration.as_secs_f64()
                    );
                    println!("------------------------------------------------------------");

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
                bevy::prelude::error!("GPU frame channel disconnected");
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
    let config = config.apply_presets();

    println!("------------------------------------------------------------");
    println!("🦀 gaanim — Export");
    println!("------------------------------------------------------------");
    println!("  Output file:   {}", config.output_path);
    println!("  Resolution:    {}x{}", config.width, config.height);
    println!("  Framerate:     {} FPS", config.fps);
    println!("  Format:        {}", format_label(&config.format));
    println!("  Transparent:   {}", config.transparent);
    if let (Some(s), Some(e)) = (config.start_time, config.end_time) {
        println!("  Segment:       {:.2}s to {:.2}s", s, e);
    }
    println!("------------------------------------------------------------");

    let resize_filter = filter_for_quality(config.encoding_speed);

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
    })?;

    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            visible: true,
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
    }))
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin);

    app.insert_resource(SetupCallback(Some(Box::new(setup_world_fn))));
    app.add_systems(Startup, setup_scene_system);

    app.finish();
    app.cleanup();
    app.update();

    let timeline_duration = app.world().resource::<Timeline>().cached_duration;
    let render_start = config.start_time.unwrap_or(0.0).max(0.0);
    let render_end = config
        .end_time
        .unwrap_or(timeline_duration)
        .min(timeline_duration);
    let render_length = render_end - render_start;

    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    let pb = create_progress_bar(total_frames);

    // Bounded channel: at most 4 pending GPU frames to prevent memory blow-up
    let (tx, rx) = sync_channel::<Vec<u8>>(4);

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
    });

    app.add_systems(Update, export_pipeline_system);

    app.run();

    Ok(())
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
    let config = config.apply_presets();

    println!("------------------------------------------------------------");
    println!("🦀 gaanim v2 — Headless GPU-Direct Export");
    println!("------------------------------------------------------------");
    println!("  Output file:   {}", config.output_path);
    println!("  Resolution:    {}x{}", config.width, config.height);
    println!("  Framerate:     {} FPS", config.fps);
    println!("  Format:        {}", format_label(&config.format));
    if matches!(config.format, crate::encoder::ExportFormat::Mp4) {
        println!("  Encoder:       {}", config.video_encoder.display_name());
    }
    println!("  Transparent:   {}", config.transparent);
    if let (Some(s), Some(e)) = (config.start_time, config.end_time) {
        println!("  Segment:       {:.2}s to {:.2}s", s, e);
    }
    println!("------------------------------------------------------------");

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
    })?;

    let mut gpu = GpuContext::new(config.width, config.height)
        .map_err(crate::encoder::ExportError::General)?;

    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin);

    app.insert_resource(SetupCallback(Some(Box::new(setup_world_fn))));
    app.add_systems(Startup, setup_scene_system);

    app.finish();
    app.cleanup();
    app.update();

    let timeline_duration = app.world().resource::<Timeline>().cached_duration;
    let render_start = config.start_time.unwrap_or(0.0).max(0.0);
    let render_end = config
        .end_time
        .unwrap_or(timeline_duration)
        .min(timeline_duration);
    let render_length = render_end - render_start;

    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    let frame_time_step = 1.0 / config.fps as f64;
    let pb = create_progress_bar(total_frames);

    let mut current_time = render_start;
    let mut last_report = Instant::now();

    for frame_idx in 0..total_frames {
        {
            let world = app.world_mut();
            let mut timeline = world.resource_mut::<Timeline>();
            timeline.seek_request = Some(current_time);
        }

        app.update();

        let vello_scene = {
            let camera = app.world().get_resource::<gaanim_math::Camera>().cloned();
            let raw_scene = gaanim_renderer::pipeline::compile_scene_from_world(
                app.world_mut(),
                camera.as_ref(),
            );

            let (zoom, cam_x, cam_y) = camera
                .as_ref()
                .map(|c| {
                    let z = match c.projection {
                        gaanim_math::Projection::Orthographic { zoom } => zoom,
                        _ => 1.0,
                    };
                    (z, c.position.x, c.position.y)
                })
                .unwrap_or((1.0, 0.0, 0.0));

            let mut scene = bevy_vello::vello::Scene::new();
            let camera_to_vello =
                kurbo::Affine::translate((config.width as f64 / 2.0, config.height as f64 / 2.0))
                    * kurbo::Affine::scale(zoom)
                    * kurbo::Affine::translate((-cam_x, cam_y));
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

        let frame_data = gpu
            .render_frame(&vello_scene, bg_color)
            .map_err(ExportError::General)?;

        encoder
            .push_frame(frame_data)
            .map_err(|e| ExportError::Capture(format!("Encoder push error: {}", e)))?;

        if frame_idx.is_multiple_of(10) || frame_idx == total_frames - 1 {
            let speed = 10.0 / last_report.elapsed().as_secs_f64();
            pb.set_message(format!("{:.1} fps", speed));
            last_report = Instant::now();
        }
        pb.inc(1);

        current_time += frame_time_step;
    }

    pb.finish_with_message("Done!");
    println!("  Finalizing video file...");

    if let Err(e) = encoder.finalize() {
        bevy::prelude::error!("Encoder finalization error: {}", e);
    }

    let duration = start_time.elapsed();
    println!("------------------------------------------------------------");
    println!(
        "✓ Export successfully completed in {:.2}s!",
        duration.as_secs_f64()
    );
    println!("------------------------------------------------------------");

    Ok(())
}
