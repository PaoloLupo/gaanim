use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_export::encoder::{EncodingSpeed, ExportFormat};
use gaanim_export::prelude::*;
use gaanim_timeline::timeline::Timeline;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub type ReplayFn = Arc<dyn Fn(&mut World) + Send + Sync>;

#[derive(Resource, Clone)]
pub struct StashedReplay(pub Option<ReplayFn>);

#[derive(Resource)]
pub struct ExportState {
    pub dialog_open: bool,
    pub format: ExportFormat,
    pub quality: ExportQuality,
    pub output_path: String,
    pub active: bool,
    pub show_complete: bool,
    pub message: String,
    pub progress_shared: Arc<Mutex<Option<ExportProgress>>>,
}

#[derive(Clone)]
pub struct ExportProgress {
    pub current_frame: u64,
    pub total_frames: u64,
    pub started_at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportQuality {
    Draft,
    Standard,
    Production,
}

impl ExportQuality {
    fn fps(self) -> u32 {
        match self {
            Self::Draft => 30,
            _ => 60,
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
    fn encoding_speed(self) -> EncodingSpeed {
        match self {
            Self::Draft => EncodingSpeed::Fast,
            Self::Standard => EncodingSpeed::Balanced,
            Self::Production => EncodingSpeed::Best,
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
            show_complete: false,
            message: String::new(),
            progress_shared: Arc::new(Mutex::new(None)),
        }
    }
}

/// Shows the export config dialog, progress window, or completion popup.
pub fn export_dialog_system(
    mut ctx: bevy_egui::EguiContexts,
    mut state: ResMut<ExportState>,
    timeline: ResMut<Timeline>,
    replay_stash: Res<StashedReplay>,
) {
    let Ok(ctx) = ctx.ctx_mut() else { return };

    // --- Collect intent from egui into local variables first ---
    let mut trigger_export = false;
    let mut trigger_cancel = false;
    let mut trigger_ok = false;

    if state.show_complete {
        egui::Window::new("Export Complete")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(&state.message);
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    trigger_ok = true;
                }
            });
    }

    if trigger_ok {
        state.show_complete = false;
    }

    if state.active {
        let (prog_frame, prog_total, prog_done) = {
            let lock = state.progress_shared.lock().unwrap();
            if let Some(ref prog) = *lock {
                (
                    prog.current_frame,
                    prog.total_frames,
                    prog.current_frame >= prog.total_frames,
                )
            } else {
                (0, 1, false)
            }
        };

        if prog_done {
            state.show_complete = true;
            state.active = false;
            *state.progress_shared.lock().unwrap() = None;
        } else {
            let progress = if prog_total > 0 {
                prog_frame as f32 / prog_total as f32
            } else {
                0.0
            };
            egui::Window::new("Exporting...")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("Frame {}/{}", prog_frame, prog_total));
                    ui.add_space(8.0);
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .desired_width(300.0)
                            .text(format!("{:.0}%", progress * 100.0)),
                    );
                    ui.add_space(8.0);
                    if ui.button("Cancel").clicked() {
                        trigger_cancel = true;
                    }
                });
            if trigger_cancel {
                state.active = false;
                *state.progress_shared.lock().unwrap() = None;
            }
            return;
        }
    }

    if !state.dialog_open {
        return;
    }

    // Collect all values BEFORE the button handlers (to avoid borrow issues)
    let mut current_format = state.format;
    let mut current_quality = state.quality;
    let mut current_output = state.output_path.clone();
    let has_replay = replay_stash.0.is_some();
    let dur = timeline.cached_duration;
    let fps = current_quality.fps();
    let total = (dur * fps as f64).ceil() as u64;

    egui::Window::new("Export Scene")
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Format:");
                egui::ComboBox::from_id_salt("export_fmt")
                    .selected_text(export_format_label(current_format))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut current_format, ExportFormat::Mp4, "MP4");
                        ui.selectable_value(&mut current_format, ExportFormat::Webm, "WebM");
                        ui.selectable_value(&mut current_format, ExportFormat::Webp, "WebP");
                        ui.selectable_value(&mut current_format, ExportFormat::Gif, "GIF");
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Quality:");
                egui::ComboBox::from_id_salt("export_qual")
                    .selected_text(current_quality.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut current_quality,
                            ExportQuality::Draft,
                            "Draft (480p30)",
                        );
                        ui.selectable_value(
                            &mut current_quality,
                            ExportQuality::Standard,
                            "Standard (1080p60)",
                        );
                        ui.selectable_value(
                            &mut current_quality,
                            ExportQuality::Production,
                            "Production (4K60)",
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Output:");
                ui.text_edit_singleline(&mut current_output);
            });
            ui.label(format!(
                "Duration: {:.1}s → {} frames at {}fps",
                dur, total, fps
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Export").clicked() {
                    trigger_export = true;
                }
                if ui.button("Cancel").clicked() {
                    trigger_cancel = true;
                }
            });
        });

    // Apply state changes AFTER the egui closures (no borrow conflicts)
    state.format = current_format;
    state.quality = current_quality;
    state.output_path = current_output;

    if trigger_cancel {
        state.dialog_open = false;
    }

    if trigger_export {
        if !has_replay {
            state.message = "No replay data available".to_string();
            state.show_complete = true;
            state.dialog_open = false;
        } else {
            let out = state.output_path.clone();
            let fmt = state.format;
            let qual = state.quality;
            let progress = state.progress_shared.clone();
            let replay = replay_stash.0.clone().unwrap();

            state.active = true;
            state.dialog_open = false;

            *progress.lock().unwrap() = Some(ExportProgress {
                current_frame: 0,
                total_frames: total,
                started_at: Instant::now(),
            });

            let progress_clone = progress.clone();
            let replay2 = replay.clone();
            std::thread::spawn(move || {
                let mut config = ExportConfig::new(&out).with_quality(match qual {
                    ExportQuality::Draft => QualityPreset::Draft,
                    ExportQuality::Standard => QualityPreset::Standard,
                    ExportQuality::Production => QualityPreset::Production,
                });
                config.fps = fps;
                config.crf = qual.crf();
                config.encoding_speed = qual.encoding_speed();
                config.format = fmt;
                config.headless = true;

                let _ = export_scene_direct(config, move |world| {
                    replay2(world);
                });
                if let Ok(mut lock) = progress_clone.lock() {
                    if let Some(ref mut p) = *lock {
                        p.current_frame = p.total_frames;
                    }
                }
            });
        }
    }
}

fn export_format_label(f: ExportFormat) -> &'static str {
    match f {
        ExportFormat::Mp4 => "MP4",
        ExportFormat::Webm => "WebM",
        ExportFormat::Webp => "WebP",
        ExportFormat::Gif => "GIF",
        ExportFormat::PngSequence => "PNG",
    }
}

pub fn export_per_frame_system() {}
