use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::Canvas;
use gaanim_api::export::export_canvas;
use gaanim_export::encoder::{EncodingSpeed, ExportFormat};
use gaanim_export::prelude::*;
use gaanim_timeline::timeline::Timeline;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Resource, Clone, Debug)]
pub struct ProjectPaths {
    pub project_dir: PathBuf,
    pub output_dir: PathBuf,
    pub script_path: PathBuf,
}

#[derive(Resource, Clone, Default)]
pub struct StashedReplay {
    pub canvas: Option<Canvas>,
    /// Changes on every replay, even when slide names and timings stay equal.
    pub revision: u64,
}

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
    pub result: Option<Result<(), String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExportQuality {
    Draft,
    Standard,
    Production,
}

impl ExportQuality {
    fn preset(self) -> QualityPreset {
        match self {
            Self::Draft => QualityPreset::Draft,
            Self::Standard => QualityPreset::Standard,
            Self::Production => QualityPreset::Production,
        }
    }
    fn arg(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Standard => "standard",
            Self::Production => "production",
        }
    }
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
    project_paths: Option<Res<ProjectPaths>>,
) {
    let Ok(ctx) = ctx.ctx_mut() else { return };

    // Initialize default output path from gaanim.toml if still default
    if let Some(ref proj) = project_paths {
        if state.output_path == "output.mp4" {
            // Show relative to project for nicer UX: e.g. "exports/output.mp4"
            let rel = proj
                .output_dir
                .strip_prefix(&proj.project_dir)
                .unwrap_or(&proj.output_dir)
                .join("output.mp4");
            state.output_path = rel.to_string_lossy().to_string();
        }
    }

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
            state.message = {
                let lock = state.progress_shared.lock().unwrap();
                match lock.as_ref().and_then(|progress| progress.result.as_ref()) {
                    Some(Ok(())) => "Export completed successfully".to_string(),
                    Some(Err(error)) => format!("Export failed: {error}"),
                    None => "Export finished without a result".to_string(),
                }
            };
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
    let has_replay = replay_stash.canvas.is_some();
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
            let out_raw = state.output_path.clone();
            // Resolve relative output against project_dir so gaanim.toml's output_dir is respected
            let out = if let Some(ref proj) = project_paths {
                let p = PathBuf::from(&out_raw);
                if p.is_absolute() {
                    out_raw
                } else {
                    proj.project_dir.join(p).to_string_lossy().to_string()
                }
            } else {
                out_raw
            };
            let fmt = state.format;
            let qual = state.quality;
            let progress = state.progress_shared.clone();
            let canvas = replay_stash.canvas.clone().unwrap();
            let worker_paths = project_paths
                .as_ref()
                .map(|paths| (paths.script_path.clone(), paths.project_dir.clone()));
            let needs_worker = canvas.has_native_3d_content();

            state.active = true;
            state.dialog_open = false;

            *progress.lock().unwrap() = Some(ExportProgress {
                current_frame: 0,
                total_frames: total,
                started_at: Instant::now(),
                result: None,
            });

            let progress_clone = progress.clone();
            std::thread::spawn(move || {
                let result = if needs_worker {
                    match worker_paths {
                        Some((script_path, project_dir)) => run_export_worker(
                            &script_path,
                            &project_dir,
                            &out,
                            qual,
                            fmt,
                        ),
                        None => Err(
                            "3D export requires an open project script so it can run in an isolated process"
                                .to_string(),
                        ),
                    }
                } else {
                    let mut config = ExportConfig::new(&out).with_quality(qual.preset());
                    config.fps = fps;
                    config.crf = qual.crf();
                    config.encoding_speed = qual.encoding_speed();
                    config.format = fmt;
                    config.headless = true;
                    export_canvas(canvas, config).map_err(|error| error.to_string())
                };
                if let Ok(mut lock) = progress_clone.lock() {
                    if let Some(ref mut p) = *lock {
                        p.result = Some(result);
                        p.current_frame = p.total_frames;
                    }
                }
            });
        }
    }
}

fn run_export_worker(
    script_path: &std::path::Path,
    project_dir: &std::path::Path,
    output_path: &str,
    quality: ExportQuality,
    format: ExportFormat,
) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Gaanim executable: {error}"))?;
    let status = std::process::Command::new(executable)
        .arg("--export-worker")
        .arg(script_path)
        .arg(output_path)
        .arg(quality.arg())
        .arg(export_format_arg(format))
        .current_dir(project_dir)
        .status()
        .map_err(|error| format!("could not start the isolated export worker: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("export worker exited with {status}"))
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

fn export_format_arg(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Mp4 => "mp4",
        ExportFormat::Webm => "webm",
        ExportFormat::Webp => "webp",
        ExportFormat::Gif => "gif",
        ExportFormat::PngSequence => "png",
    }
}

pub fn export_per_frame_system() {}
