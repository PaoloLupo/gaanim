use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_export::encoder::{EncodingSpeed, ExportFormat, ParallelEncoder};
use gaanim_export::gpu::GpuContext;
use gaanim_timeline::timeline::Timeline;
use std::time::{Duration, Instant};

#[derive(Resource)]
pub struct ExportState {
    pub dialog_open: bool,
    pub format: ExportFormat,
    pub quality: ExportQuality,
    pub output_path: String,
    pub active: bool,
    pub current_frame: u64,
    pub total_frames: u64,
    pub progress: f32,
    pub started_at: Option<Instant>,
    pub message: String,
    pub show_complete: bool,
    pub skip_render: bool,
    // pub need_full_update: bool,
    pub frame_time_step: f64,
    pub current_time: f64,
    pub export_width: u32,
    pub export_height: u32,
    pub fps: u32,
    pub crf: u32,
    pub encoding_speed: EncodingSpeed,
    // pub video_encoder_name: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportQuality {
    Draft,
    Standard,
    Production,
}

impl ExportQuality {
    fn encoding_speed(self) -> EncodingSpeed {
        match self {
            Self::Draft => EncodingSpeed::Fast,
            Self::Standard => EncodingSpeed::Balanced,
            Self::Production => EncodingSpeed::Best,
        }
    }

    fn fps(self) -> u32 {
        match self {
            Self::Draft => 30,
            Self::Standard => 60,
            Self::Production => 60,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Standard => "Standard",
            Self::Production => "Production",
        }
    }

    fn crf(self) -> u32 {
        match self {
            Self::Draft => 24,
            Self::Standard => 18,
            Self::Production => 14,
        }
    }
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            dialog_open: false,
            format: ExportFormat::Mp4,
            quality: ExportQuality::Standard,
            output_path: "output.mp4".to_string(),
            active: false,
            current_frame: 0,
            total_frames: 0,
            progress: 0.0,
            started_at: None,
            message: String::new(),
            show_complete: false,
            skip_render: false,
            // need_full_update: false,
            frame_time_step: 0.0,
            current_time: 0.0,
            export_width: 1920,
            export_height: 1080,
            fps: 60,
            crf: 18,
            encoding_speed: EncodingSpeed::Balanced,
            // video_encoder_name: String::new(),
        }
    }
}

impl ExportState {
    fn format_label(&self) -> &'static str {
        match self.format {
            ExportFormat::Mp4 => "MP4",
            ExportFormat::Webm => "WebM",
            ExportFormat::Webp => "WebP",
            ExportFormat::Gif => "GIF",
            ExportFormat::PngSequence => "PNG",
        }
    }
}
/// Shows the export config dialog or progress window.
pub fn export_dialog_system(
    mut ctx: bevy_egui::EguiContexts,
    mut state: ResMut<ExportState>,
    timeline: ResMut<Timeline>,
) {
    let Ok(ctx) = ctx.ctx_mut() else { return };

    if state.show_complete {
        egui::Window::new("Export Complete")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(&state.message);
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    state.show_complete = false;
                    state.skip_render = false;
                }
            });
    }

    if state.active {
        if state.current_frame >= state.total_frames && state.total_frames > 0 {
            state.active = false;
            state.skip_render = false;
            let elapsed = state
                .started_at
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.0);
            state.message = format!(
                "Export completed in {:.1}s!\n{}",
                elapsed, state.output_path
            );
            state.show_complete = true;
            return;
        }

        egui::Window::new("Exporting...")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(&state.message);
                ui.add_space(8.0);
                ui.add(
                    egui::ProgressBar::new(state.progress)
                        .desired_width(300.0)
                        .text(format!(
                            "Frame {}/{} ({:.0}%)",
                            state.current_frame,
                            state.total_frames,
                            state.progress * 100.0,
                        )),
                );
                ui.add_space(8.0);
                if ui.button("Cancel").clicked() {
                    state.active = false;
                    state.skip_render = false;
                }
            });
        return;
    }

    if !state.dialog_open {
        return;
    }

    egui::Window::new("Export Scene")
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Format:");
                egui::ComboBox::from_id_salt("export_format")
                    .selected_text(state.format_label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.format, ExportFormat::Mp4, "MP4");
                        ui.selectable_value(&mut state.format, ExportFormat::Webm, "WebM");
                        ui.selectable_value(&mut state.format, ExportFormat::Webp, "WebP");
                        ui.selectable_value(&mut state.format, ExportFormat::Gif, "GIF");
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Quality:");
                egui::ComboBox::from_id_salt("export_quality")
                    .selected_text(state.quality.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.quality,
                            ExportQuality::Draft,
                            "Draft (480p30)",
                        );
                        ui.selectable_value(
                            &mut state.quality,
                            ExportQuality::Standard,
                            "Standard (1080p60)",
                        );
                        ui.selectable_value(
                            &mut state.quality,
                            ExportQuality::Production,
                            "Production (4K60)",
                        );
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Output:");
                ui.text_edit_singleline(&mut state.output_path);
            });

            let dur = timeline.cached_duration;
            let fps = state.quality.fps();
            let total = (dur * fps as f64).ceil() as u64;
            ui.label(format!(
                "Duration: {:.1}s → {} frames at {}fps",
                dur, total, fps
            ));

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Export").clicked() {
                    state.total_frames = total;
                    state.export_width = 1920;
                    state.export_height = 1080;
                    state.fps = fps;
                    state.crf = state.quality.crf();
                    state.encoding_speed = state.quality.encoding_speed();
                    state.active = true;
                    state.current_frame = 0;
                    state.current_time = 0.0;
                    state.frame_time_step = 1.0 / fps as f64;
                    state.progress = 0.0;
                    state.started_at = Some(Instant::now());
                    state.message = format!("Exporting {}...", state.format_label());
                    state.skip_render = true;
                    state.dialog_open = false;
                }
                if ui.button("Cancel").clicked() {
                    state.dialog_open = false;
                }
            });
        });
}

/// Wrapper for non-Send types (GpuContext contains vello::Renderer !Send).
/// Registered via `init_non_send_resource`.
#[derive(Default)]
pub struct ExportRuntime {
    pub gpu: Option<GpuContext>,
    pub encoder: Option<ParallelEncoder>,
}

/// Processes as many export frames as possible within ~33ms per Bevy Update,
/// keeping the UI responsive while matching real-time export speed.
/// Calls `timeline_seek_system` directly to advance the world per-frame
/// without relying on Bevy's schedule.
pub fn export_per_frame_system(world: &mut World) {
    let is_active = world.resource::<ExportState>().active;

    if !is_active {
        let mut runtime = world.non_send_resource_mut::<ExportRuntime>();
        if let Some(mut enc) = runtime.encoder.take() {
            let _ = enc.finalize();
        }
        runtime.gpu = None;
        return;
    }

    let batch_start = Instant::now();
    let batch_budget = Duration::from_millis(33);

    loop {
        let (total_frames, frame_time_step, export_width, export_height): (u64, f64, u32, u32);
        {
            let s = world.resource::<ExportState>();
            if s.current_frame >= s.total_frames || s.total_frames == 0 {
                return;
            }
            total_frames = s.total_frames;
            frame_time_step = s.frame_time_step;
            export_width = s.export_width;
            export_height = s.export_height;
        }

        // Read current frame/time atomically
        let (current_frame, current_time) = {
            let s = world.resource::<ExportState>();
            (s.current_frame, s.current_time)
        };

        // --- Init (frame 0 only) ---
        if current_frame == 0 {
            let has_encoder = world.non_send_resource::<ExportRuntime>().encoder.is_some();
            if !has_encoder {
                let (output_path, format, fps, crf, enc_speed) = {
                    let s = world.resource::<ExportState>();
                    (s.output_path.clone(), s.format, s.fps, s.crf, s.encoding_speed)
                };
                let enc_config = gaanim_export::encoder::EncoderConfig {
                    output_path, width: export_width, height: export_height,
                    fps, format, transparent: false, crf,
                    encoding_speed: enc_speed,
                    video_encoder: gaanim_export::encoder::detect_best_encoder(),
                };
                match ParallelEncoder::new(enc_config) {
                    Ok(enc) => world.non_send_resource_mut::<ExportRuntime>().encoder = Some(enc),
                    Err(e) => {
                        error!("Encoder init error: {e}");
                        world.resource_mut::<ExportState>().active = false;
                        return;
                    }
                }
            }

            let has_gpu = world.non_send_resource::<ExportRuntime>().gpu.is_some();
            if !has_gpu {
                match GpuContext::new(export_width, export_height) {
                    Ok(g) => world.non_send_resource_mut::<ExportRuntime>().gpu = Some(g),
                    Err(e) => {
                        error!("GPU init error: {e}");
                        world.resource_mut::<ExportState>().active = false;
                        return;
                    }
                }
            }
        }

        // --- Seek timeline ---
        {
            let mut timeline = world.resource_mut::<Timeline>();
            timeline.seek_request = Some(current_time);
        }
        gaanim_timeline::timeline_seek_system(world);

        // --- Compile scene ---
        let camera = world.get_resource::<gaanim_math::Camera>().cloned();
        let raw_scene = gaanim_renderer::pipeline::compile_scene_from_world(world, camera.as_ref());

        let (zoom, cam_x, cam_y) = camera.as_ref().map(|c| match c.projection {
            gaanim_math::Projection::Orthographic { zoom } => (zoom, c.position.x, c.position.y),
            _ => (1.0, 0.0, 0.0),
        }).unwrap_or((1.0, 0.0, 0.0));

        let mut vello_scene = bevy_vello::vello::Scene::new();
        let camera_to_vello = kurbo::Affine::translate((export_width as f64 / 2.0, export_height as f64 / 2.0))
            * kurbo::Affine::scale(zoom)
            * kurbo::Affine::translate((-cam_x, cam_y));
        vello_scene.append(&raw_scene, Some(camera_to_vello));

        let bg_color = world.get_resource::<ClearColor>()
            .map(|cc| { let rgba = cc.0.to_srgba(); peniko::Color::from_rgba8((rgba.red * 255.0) as u8, (rgba.green * 255.0) as u8, (rgba.blue * 255.0) as u8, (rgba.alpha * 255.0) as u8) })
            .unwrap_or(peniko::Color::BLACK);

        // --- Render & encode ---
        {
            let mut rt = world.non_send_resource_mut::<ExportRuntime>();
            if let Some(ref mut gpu) = rt.gpu {
                if let Ok(frame_data) = gpu.render_frame(&vello_scene, bg_color) {
                    if let Some(ref enc) = rt.encoder {
                        if let Err(e) = enc.push_frame(frame_data) {
                            error!("Encoder push error: {e}");
                            rt.gpu = None; rt.encoder = None;
                            drop(rt);
                            world.resource_mut::<ExportState>().active = false;
                            return;
                        }
                    }
                }
            }
        }

        // --- Advance ---
        let new_frame = current_frame + 1;
        let new_time = current_time + frame_time_step;
        {
            let mut state = world.resource_mut::<ExportState>();
            state.current_frame = new_frame;
            state.current_time = new_time;
            if total_frames > 0 {
                state.progress = new_frame as f32 / total_frames as f32;
            }
        }

        if batch_start.elapsed() >= batch_budget || new_frame >= total_frames {
            break;
        }
    }
}
