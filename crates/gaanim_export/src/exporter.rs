use bevy::prelude::*;
use bevy::ecs::observer::On;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::app::AppExit;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Mutex;
use std::sync::mpsc::{channel, Sender, Receiver, TryRecvError};
use std::time::Instant;

use gaanim_timeline::timeline::Timeline;

use crate::config::ExportConfig;
use crate::encoder::{EncoderConfig, ParallelEncoder, Result};

/// Standardized CLI theme for gaanim tools.
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

#[derive(Resource)]
struct ExportPipeline {
    pub encoder: ParallelEncoder,
    pub progress_bar: ProgressBar,
    pub current_time: f64,
    pub frame_time_step: f64,
    pub total_frames: u64,
    pub rendered_frames: u64,
    pub tx: Sender<Vec<u8>>,
    pub rx: Mutex<Receiver<Vec<u8>>>,
    pub waiting_for_gpu: bool,
    pub start_time: Instant,
    pub last_frame_time: Instant,
    pub export_width: u32,
    pub export_height: u32,
}

#[derive(Resource)]
struct SetupCallback(Option<Box<dyn FnOnce(&mut World) + Send + Sync>>);

/// Ticks the virtual clock step-by-step, captures Vulkan texture screenshots asynchronously,
/// and feeds them to the background encoder, exiting once all frames are written.
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
                // We got the frame! Push to the background encoder thread
                if let Err(e) = pipeline.encoder.push_frame(frame_data) {
                    bevy::prelude::error!("Encoder error: {}", e);
                    exit.write(AppExit::Success);
                    return;
                }

                // Update CLI progress
                pipeline.rendered_frames += 1;
                let speed = 1.0 / pipeline.last_frame_time.elapsed().as_secs_f64();
                pipeline.progress_bar.set_message(format!("{:.1} fps", speed));
                pipeline.progress_bar.inc(1);

                pipeline.waiting_for_gpu = false;

                // Check if finished
                if pipeline.rendered_frames >= pipeline.total_frames {
                    pipeline.progress_bar.finish_with_message("Done!");
                    println!("  Finalizing video file...");
                    if let Err(e) = pipeline.encoder.finalize() {
                        bevy::prelude::error!("Encoder finalization error: {}", e);
                    }

                    let duration = pipeline.start_time.elapsed();
                    println!("------------------------------------------------------------");
                    println!("✓ Export successfully completed in {:.2}s!", duration.as_secs_f64());
                    println!("------------------------------------------------------------");

                    // Exit Bevy cleanly
                    exit.write(AppExit::Success);
                    return;
                }

                // Advance virtual time clock
                pipeline.current_time += pipeline.frame_time_step;
            }
            Err(TryRecvError::Empty) => {
                // GPU is still processing, wait for the next frame
                // Sleep for 1ms to prevent pegging the CPU in a tight loop and starving the OS event loop.
                std::thread::sleep(std::time::Duration::from_millis(1));
                return;
            }
            Err(TryRecvError::Disconnected) => {
                bevy::prelude::error!("GPU screenshot channel disconnected");
                exit.write(AppExit::Success);
                return;
            }
        }
    }

    // Trigger next render & screenshot
    pipeline.last_frame_time = Instant::now();

    // 1. Advance the virtual timeline clock
    timeline.seek_request = Some(pipeline.current_time);

    // 2. Request a GPU screenshot targeting the primary window
    let tx_clone = pipeline.tx.clone();
    let export_width = pipeline.export_width;
    let export_height = pipeline.export_height;

    commands.spawn(Screenshot::primary_window())
        .observe(move |trigger: On<ScreenshotCaptured>| {
            let format = trigger.event().image.texture_descriptor.format;
            let size = trigger.event().image.texture_descriptor.size;

            if let Some(mut data) = trigger.event().image.data.clone() {
                // A. Convert BGRA to RGBA if necessary (resolves reddish channel swap on X11/Vulkan)
                if format == bevy::render::render_resource::TextureFormat::Bgra8Unorm
                    || format == bevy::render::render_resource::TextureFormat::Bgra8UnormSrgb
                {
                    for chunk in data.chunks_exact_mut(4) {
                        chunk.swap(0, 2);
                    }
                }

                // B. Resize if the captured image size differs from the configured size
                // (e.g. due to wgpu padding or scale-factor changes)
                if size.width != export_width || size.height != export_height {
                    let rgba_image = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                        size.width,
                        size.height,
                        data,
                    ).expect("Bevy screenshot buffer size mismatch");
                    let resized = image::imageops::resize(
                        &rgba_image,
                        export_width,
                        export_height,
                        image::imageops::FilterType::Lanczos3,
                    );
                    data = resized.into_raw();
                }

                let _ = tx_clone.send(data);
            }
        });

    pipeline.waiting_for_gpu = true;
}

/// Runs the scene setup closure, queuing camera and Mobject creation commands.
fn setup_scene_system(world: &mut World) {
    if let Some(mut callback_res) = world.get_resource_mut::<SetupCallback>() {
        if let Some(callback) = callback_res.0.take() {
            callback(world);
        }
    }
}

/// The core offline rendering orchestrator.
///
/// Multi-Platform Headless-Friendly Design: uses winit to spawn an invisible window to satisfy
/// bevy_vello's internal window-dependency checks, while keeping the output fully hidden from the user.
/// Drives the timeline with 100% deterministic rates.
pub fn export_scene<F>(config: ExportConfig, setup_world_fn: F) -> Result<()>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
{
    let start_time = Instant::now();
    let config = config.apply_presets();

    println!("------------------------------------------------------------");
    println!("🦀 gaanim v2 — State-of-the-Art Headless Export Pipeline");
    println!("------------------------------------------------------------");
    println!("  Output file:   {}", config.output_path);
    println!("  Resolution:    {}x{}", config.width, config.height);
    println!("  Framerate:     {} FPS", config.fps);
    println!("  Format:        {:?}", config.format);
    println!("  Transparent:   {}", config.transparent);
    if let (Some(s), Some(e)) = (config.start_time, config.end_time) {
        println!("  Segment:       {:.2}s to {:.2}s", s, e);
    }
    println!("------------------------------------------------------------");

    // Initialize parallel encoder
    let encoder = ParallelEncoder::new(EncoderConfig {
        output_path: config.output_path.clone(),
        width: config.width,
        height: config.height,
        fps: config.fps,
        format: config.format,
        transparent: config.transparent,
        crf: config.crf,
    })?;

    // Create Bevy App
    let mut app = App::new();

    // Spawn a non-resizable window to force tiling window managers to keep it floating,
    // satisfying Vello window requirements while preventing stretching/scaling.
    app.add_plugins(
        DefaultPlugins.set(WindowPlugin {
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
        })
    )
    .add_plugins(gaanim_scene::GaanimScenePlugin)
    .add_plugins(gaanim_animation::GaanimAnimationPlugin)
    .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
    .add_plugins(gaanim_text::GaanimTextPlugin)
    .add_plugins(gaanim_renderer::GaanimRendererPlugin);

    // Register sequenced startup systems
    app.insert_resource(SetupCallback(Some(Box::new(setup_world_fn))));
    app.add_systems(Startup, setup_scene_system);

    // Initialize Bevy structures and run the Startup systems
    app.finish();
    app.cleanup();

    // Trigger the first Bevy update manually to run Startup systems
    // and populate the Timeline resource with all replay elements.
    app.update();

    // Determine total render bounds
    let timeline_duration = app.world().resource::<Timeline>().cached_duration;
    let render_start = config.start_time.unwrap_or(0.0).max(0.0);
    let render_end = config.end_time.unwrap_or(timeline_duration).min(timeline_duration);
    let render_length = render_end - render_start;

    let total_frames = (render_length * config.fps as f64).ceil() as u64;
    let pb = create_progress_bar(total_frames);

    let (tx, rx) = channel();

    // Insert our Pipeline state as a Resource
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
    });

    // Add our rendering loop system
    app.add_systems(Update, export_pipeline_system);

    // Start the winit event loop (this blocks until AppExit is received)
    app.run();

    Ok(())
}
