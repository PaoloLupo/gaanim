use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::Canvas;
use gaanim_api::export::export_canvas;
use gaanim_export::encoder::{EncodingSpeed, ExportFormat};
use gaanim_export::prelude::*;
use gaanim_timeline::timeline::Timeline;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Stdio;
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
    /// Changes on every replay, even when segment names and timings stay equal.
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
    pub elapsed_seconds: Option<f64>,
    pub encoder_label: Option<String>,
    pub progress_shared: Arc<Mutex<Option<ExportProgress>>>,
}

#[derive(Clone)]
pub struct ExportProgress {
    pub current_frame: u64,
    pub total_frames: u64,
    pub started_at: Instant,
    pub result: Option<Result<(), String>>,
    pub telemetry: ExportTelemetry,
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
            elapsed_seconds: None,
            encoder_label: None,
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
        let mut complete_open = true;
        egui::Window::new("Export Complete")
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut complete_open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&state.message).size(16.0).strong());
                if let Some(elapsed) = state.elapsed_seconds {
                    ui.label(format!("Elapsed: {:.2}s", elapsed));
                }
                if let Some(encoder) = &state.encoder_label {
                    ui.label(format!("Encoder: {encoder}"));
                }
                ui.add(
                    egui::Label::new(format!("Output: {}", state.output_path))
                        .wrap_mode(egui::TextWrapMode::Truncate),
                );
                ui.add_space(8.0);
                if ui.button(egui::RichText::new("OK").size(14.0)).clicked() {
                    trigger_ok = true;
                }
            });
        if !complete_open {
            trigger_ok = true;
        }
    }

    if trigger_ok {
        state.show_complete = false;
    }

    if state.active {
        let (prog_frame, prog_total, prog_done, encoder_label, elapsed_seconds) = {
            let lock = state.progress_shared.lock().unwrap();
            if let Some(ref prog) = *lock {
                let (telemetry_frame, telemetry_total) = prog.telemetry.progress();
                let current_frame = telemetry_frame.max(prog.current_frame);
                let total_frames = telemetry_total.max(prog.total_frames);
                let result_ready = prog.result.is_some();
                (
                    current_frame,
                    total_frames,
                    current_frame >= total_frames && result_ready,
                    prog.telemetry.encoder(),
                    prog.started_at.elapsed().as_secs_f64(),
                )
            } else {
                (0, 1, false, None, 0.0)
            }
        };

        if prog_done {
            let (message, encoder_label, elapsed_seconds) = {
                let lock = state.progress_shared.lock().unwrap();
                if let Some(progress) = lock.as_ref() {
                    let message = match progress.result.as_ref() {
                        Some(Ok(())) => "Export completed successfully".to_string(),
                        Some(Err(error)) => format!("Export failed: {error}"),
                        None => "Export finished without a result".to_string(),
                    };
                    (
                        message,
                        progress.telemetry.encoder(),
                        progress.started_at.elapsed().as_secs_f64(),
                    )
                } else {
                    (
                        "Export finished without a result".to_string(),
                        encoder_label,
                        0.0,
                    )
                }
            };
            state.message = message;
            state.elapsed_seconds = Some(elapsed_seconds);
            state.encoder_label = encoder_label;
            state.show_complete = true;
            state.active = false;
            *state.progress_shared.lock().unwrap() = None;
        } else {
            let progress = if prog_total > 0 {
                prog_frame as f32 / prog_total as f32
            } else {
                0.0
            };
            let eta_seconds = if prog_frame > 0 && prog_total > prog_frame {
                elapsed_seconds * (prog_total - prog_frame) as f64 / prog_frame as f64
            } else {
                0.0
            };
            let mut active_open = true;
            egui::Window::new("Exporting...")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut active_open)
                .collapsible(false)
                .resizable(false)
                .default_width(480.0)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("Exporting video").size(16.0).strong());
                    ui.label(
                        egui::RichText::new(format!("Frame {}/{}", prog_frame, prog_total))
                            .size(16.0)
                            .strong(),
                    );
                    ui.label(format!(
                        "Encoder: {}",
                        encoder_label.as_deref().unwrap_or("detecting...")
                    ));
                    ui.add(
                        egui::Label::new(format!("Output: {}", state.output_path))
                            .wrap_mode(egui::TextWrapMode::Truncate),
                    );
                    ui.add_space(8.0);
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .desired_width(420.0)
                            .desired_height(24.0)
                            .text(format!("{:.1}%", progress * 100.0)),
                    );
                    ui.label(format!(
                        "Elapsed: {:.1}s · ETA: {:.1}s",
                        elapsed_seconds, eta_seconds
                    ));
                    ui.add_space(8.0);
                    if ui.button("Cancel").clicked() {
                        trigger_cancel = true;
                    }
                });
            if trigger_cancel || !active_open {
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
            let telemetry = ExportTelemetry::new();
            let canvas = replay_stash.canvas.clone().unwrap();
            let worker_paths = project_paths
                .as_ref()
                .map(|paths| (paths.script_path.clone(), paths.project_dir.clone()));
            let needs_worker = canvas.has_native_3d_content();

            state.active = true;
            state.dialog_open = false;
            state.elapsed_seconds = None;
            state.encoder_label = None;

            *progress.lock().unwrap() = Some(ExportProgress {
                current_frame: 0,
                total_frames: total,
                started_at: Instant::now(),
                result: None,
                telemetry: telemetry.clone(),
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
                            telemetry.clone(),
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
                    config.telemetry = Some(telemetry.clone());
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

fn forward_worker_line(line: &str, telemetry: &ExportTelemetry) {
    for fragment in line.split('\r') {
        let fragment = fragment.trim();
        if fragment.is_empty() {
            continue;
        }
        if let Some(progress) = fragment.strip_prefix("GAANIM_EXPORT_PROGRESS ") {
            let mut values = progress.split_whitespace();
            if let (Some(current), Some(total)) = (values.next(), values.next())
                && let (Ok(current), Ok(total)) = (current.parse::<u64>(), total.parse::<u64>())
            {
                telemetry.set_progress(current, total);
                continue;
            }
        }
        if let Some(encoder) = fragment.strip_prefix("Encoder:") {
            telemetry.set_encoder(encoder.trim());
            continue;
        }
        telemetry.push_log(fragment);
    }
}

fn forward_worker_stream<R>(reader: R, telemetry: ExportTelemetry) -> std::thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            forward_worker_line(&line, &telemetry);
        }
    })
}

fn run_export_worker(
    script_path: &std::path::Path,
    project_dir: &std::path::Path,
    output_path: &str,
    quality: ExportQuality,
    format: ExportFormat,
    telemetry: ExportTelemetry,
) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Gaanim executable: {error}"))?;
    let mut child = std::process::Command::new(executable)
        .arg("--export-worker")
        .arg(script_path)
        .arg(output_path)
        .arg(quality.arg())
        .arg(export_format_arg(format))
        .env("GAANIM_EXPORT_WORKER", "1")
        .current_dir(project_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start the isolated export worker: {error}"))?;
    let stdout_thread = child
        .stdout
        .take()
        .map(|stdout| forward_worker_stream(stdout, telemetry.clone()));
    let stderr_thread = child
        .stderr
        .take()
        .map(|stderr| forward_worker_stream(stderr, telemetry.clone()));
    let status = child
        .wait()
        .map_err(|error| format!("could not wait for the isolated export worker: {error}"))?;
    if let Some(thread) = stdout_thread {
        let _ = thread.join();
    }
    if let Some(thread) = stderr_thread {
        let _ = thread.join();
    }
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

#[cfg(test)]
mod tests {
    use super::{ExportTelemetry, forward_worker_line};

    #[test]
    fn worker_progress_markers_update_shared_telemetry_without_polluting_log() {
        let telemetry = ExportTelemetry::new();

        forward_worker_line("GAANIM_EXPORT_PROGRESS 936 1200", &telemetry);
        forward_worker_line("Encoder: NVIDIA (NVENC)", &telemetry);

        assert_eq!(telemetry.progress(), (936, 1200));
        assert_eq!(telemetry.encoder().as_deref(), Some("NVIDIA (NVENC)"));
        assert!(telemetry.logs().is_empty());
    }

    #[test]
    fn worker_output_is_preserved_for_the_editor_log() {
        let telemetry = ExportTelemetry::new();

        forward_worker_line("INFO export initialized\r  Frame 1/2", &telemetry);

        assert_eq!(
            telemetry.logs(),
            vec!["INFO export initialized", "Frame 1/2"]
        );
    }
}
