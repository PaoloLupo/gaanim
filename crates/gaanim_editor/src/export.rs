use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::Canvas;
use gaanim_api::export::export_canvas;
use gaanim_export::encoder::{EncodingSpeed, ExportFormat, VideoEncoder};
use gaanim_export::prelude::*;
use gaanim_timeline::timeline::Timeline;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const EXPORT_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(100);
const EXPORT_WORKER_STALL_TIMEOUT: Duration = Duration::from_secs(120);

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
    pub video_encoder: VideoEncoder,
    pub output_path: String,
    pub active: bool,
    pub show_complete: bool,
    pub message: String,
    pub elapsed_seconds: Option<f64>,
    pub encoder_label: Option<String>,
    /// Absolute path retained after an export completes successfully.
    pub completed_output_path: Option<PathBuf>,
    pub completed_successfully: bool,
    pub progress_shared: Arc<Mutex<Option<ExportProgress>>>,
    pub cancel_requested: Arc<AtomicBool>,
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
            video_encoder: VideoEncoder::Auto,
            output_path: "output.mp4".to_string(),
            active: false,
            show_complete: false,
            message: String::new(),
            elapsed_seconds: None,
            encoder_label: None,
            completed_output_path: None,
            completed_successfully: false,
            progress_shared: Arc::new(Mutex::new(None)),
            cancel_requested: Arc::new(AtomicBool::new(false)),
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
    let mut trigger_open = false;

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
                let displayed_path = state
                    .completed_output_path
                    .as_ref()
                    .map(|path| path.to_string_lossy())
                    .unwrap_or_else(|| state.output_path.as_str().into());
                ui.add(
                    egui::Label::new(format!("Output: {displayed_path}"))
                        .wrap_mode(egui::TextWrapMode::Truncate),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if state.completed_successfully
                        && state.completed_output_path.is_some()
                        && ui.button("Open exported file").clicked()
                    {
                        trigger_open = true;
                    }
                    if ui.button(egui::RichText::new("OK").size(14.0)).clicked() {
                        trigger_ok = true;
                    }
                });
            });
        if !complete_open {
            trigger_ok = true;
        }
    }

    if trigger_open && let Some(path) = state.completed_output_path.clone() {
        match open_exported_file(&path) {
            Ok(()) => trigger_ok = true,
            Err(error) => {
                state.message =
                    format!("Export completed, but the exported file could not be opened: {error}");
            }
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
            let (message, encoder_label, elapsed_seconds, succeeded) = {
                let lock = state.progress_shared.lock().unwrap();
                if let Some(progress) = lock.as_ref() {
                    let succeeded = matches!(progress.result.as_ref(), Some(Ok(())));
                    let message = match progress.result.as_ref() {
                        Some(Ok(())) => "Export completed successfully".to_string(),
                        Some(Err(error)) if error.starts_with("export cancelled") => error.clone(),
                        Some(Err(error)) => format!("Export failed: {error}"),
                        None => "Export finished without a result".to_string(),
                    };
                    (
                        message,
                        progress.telemetry.encoder(),
                        progress.started_at.elapsed().as_secs_f64(),
                        succeeded,
                    )
                } else {
                    (
                        "Export finished without a result".to_string(),
                        encoder_label,
                        0.0,
                        false,
                    )
                }
            };
            state.message = message;
            state.elapsed_seconds = Some(elapsed_seconds);
            state.encoder_label = encoder_label;
            state.completed_successfully = succeeded;
            if !succeeded {
                state.completed_output_path = None;
            }
            state.show_complete = true;
            state.active = false;
            state.cancel_requested.store(false, Ordering::Release);
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
                    let cancelling = state.cancel_requested.load(Ordering::Acquire);
                    if cancelling {
                        ui.label("Stopping export...");
                    }
                    if ui
                        .add_enabled(!cancelling, egui::Button::new("Cancel"))
                        .clicked()
                    {
                        trigger_cancel = true;
                    }
                });
            if trigger_cancel || !active_open {
                state.cancel_requested.store(true, Ordering::Release);
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
    let mut current_encoder = state.video_encoder;
    let mut current_output = state.output_path.clone();
    let has_replay = replay_stash.canvas.is_some();
    let scene_resolution = replay_stash
        .canvas
        .as_ref()
        .map(|canvas| (canvas.width, canvas.height));
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
                            "Draft (fast, 30 fps)",
                        );
                        ui.selectable_value(
                            &mut current_quality,
                            ExportQuality::Standard,
                            "Standard (balanced, 60 fps)",
                        );
                        ui.selectable_value(
                            &mut current_quality,
                            ExportQuality::Production,
                            "Production (best, 60 fps)",
                        );
                    });
            });
            if current_format == ExportFormat::Mp4 {
                ui.horizontal(|ui| {
                    ui.label("Encoder:");
                    egui::ComboBox::from_id_salt("export_encoder")
                        .selected_text(current_encoder.display_name())
                        .show_ui(ui, |ui| {
                            for encoder in [
                                VideoEncoder::Auto,
                                VideoEncoder::Libx264,
                                VideoEncoder::H264Nvenc,
                                VideoEncoder::H264Amf,
                                VideoEncoder::H264Qsv,
                                VideoEncoder::H264Vaapi,
                            ] {
                                ui.selectable_value(
                                    &mut current_encoder,
                                    encoder,
                                    encoder.display_name(),
                                );
                            }
                        });
                });
                if current_encoder == VideoEncoder::H264Vaapi {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "VAAPI is explicit-only: a driver failure can reset the GPU.",
                    );
                }
            }
            ui.horizontal(|ui| {
                ui.label("Output:");
                ui.text_edit_singleline(&mut current_output);
            });
            ui.label(format!(
                "Duration: {:.1}s → {} frames at {}fps",
                dur, total, fps
            ));
            if let Some((width, height)) = scene_resolution {
                ui.label(format!("Resolution: {width}×{height} (scene)"));
            }
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
    state.video_encoder = current_encoder;
    state.output_path = current_output;

    if trigger_cancel {
        state.dialog_open = false;
    }

    if trigger_export {
        if !has_replay {
            state.message = "No replay data available".to_string();
            state.completed_output_path = None;
            state.completed_successfully = false;
            state.show_complete = true;
            state.dialog_open = false;
        } else {
            let out_raw = state.output_path.clone();
            // Resolve relative output against project_dir so gaanim.toml's output_dir is respected.
            let out_path = match resolve_output_path(
                &out_raw,
                project_paths
                    .as_ref()
                    .map(|paths| paths.project_dir.as_path()),
            ) {
                Ok(path) => path,
                Err(error) => {
                    state.message = format!("Export failed: {error}");
                    state.completed_output_path = None;
                    state.completed_successfully = false;
                    state.show_complete = true;
                    state.dialog_open = false;
                    return;
                }
            };
            let out = out_path.to_string_lossy().into_owned();
            let fmt = state.format;
            let qual = state.quality;
            let video_encoder = if fmt == ExportFormat::Mp4 {
                state.video_encoder
            } else {
                VideoEncoder::Auto
            };
            let progress = state.progress_shared.clone();
            let cancel_requested = state.cancel_requested.clone();
            let telemetry = ExportTelemetry::new();
            let canvas = replay_stash.canvas.clone().unwrap();
            let worker_paths = project_paths
                .as_ref()
                .map(|paths| (paths.script_path.clone(), paths.project_dir.clone()));
            let needs_worker = canvas.has_native_3d_content();

            state.active = true;
            state.dialog_open = false;
            state.output_path = out.clone();
            state.completed_output_path = Some(out_path);
            state.completed_successfully = false;
            state.elapsed_seconds = None;
            state.encoder_label = None;
            state.cancel_requested.store(false, Ordering::Release);

            *progress.lock().unwrap() = Some(ExportProgress {
                current_frame: 0,
                total_frames: total,
                started_at: Instant::now(),
                result: None,
                telemetry: telemetry.clone(),
            });

            let progress_clone = progress.clone();
            std::thread::spawn(move || {
                let result = match worker_paths {
                    Some((script_path, project_dir)) => run_export_worker(
                        &script_path,
                        &project_dir,
                        &out,
                        qual,
                        (fmt, video_encoder),
                        telemetry.clone(),
                        cancel_requested,
                    ),
                    None if needs_worker => Err(
                        "3D export requires an open project script so it can run in an isolated process"
                            .to_string(),
                    ),
                    None => {
                        let mut config = ExportConfig::new(&out).with_quality(qual.preset());
                        config.width = canvas.width;
                        config.height = canvas.height;
                        config.aspect_ratio = AspectRatioPreset::Custom;
                        config.fps = fps;
                        config.crf = qual.crf();
                        config.encoding_speed = qual.encoding_speed();
                        config.format = fmt;
                        config.video_encoder = video_encoder;
                        config.headless = true;
                        config.telemetry = Some(telemetry.clone());
                        export_canvas(canvas, config).map_err(|error| error.to_string())
                    }
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

fn resolve_output_path(raw: &str, project_dir: Option<&Path>) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Ok(path);
    }
    let base = match project_dir {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir()
            .map_err(|error| format!("could not resolve the current directory: {error}"))?,
    };
    Ok(base.join(path))
}

fn open_exported_file(path: &Path) -> Result<(), String> {
    open_exported_file_with(path, |path| open::that(path))
}

fn open_exported_file_with<E: std::fmt::Display>(
    path: &Path,
    opener: impl FnOnce(&Path) -> Result<(), E>,
) -> Result<(), String> {
    opener(path).map_err(|error| format!("{} ({error})", path.display()))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerStopReason {
    Cancelled,
    Stalled,
}

fn worker_stop_reason(
    cancel_requested: bool,
    last_progress_at: Instant,
    now: Instant,
    stall_timeout: Duration,
) -> Option<WorkerStopReason> {
    if cancel_requested {
        Some(WorkerStopReason::Cancelled)
    } else if now.duration_since(last_progress_at) >= stall_timeout {
        Some(WorkerStopReason::Stalled)
    } else {
        None
    }
}

fn configure_export_worker(_command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        _command.process_group(0);
    }
}

fn terminate_export_worker(child: &mut Child) {
    #[cfg(unix)]
    {
        let process_group = format!("-{}", child.id());
        let _ = Command::new("kill")
            .args(["-KILL", "--", &process_group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn worker_error_context(summary: &str, output_path: &str, telemetry: &ExportTelemetry) -> String {
    let encoder = telemetry
        .encoder()
        .unwrap_or_else(|| "not reported".to_string());
    let logs = telemetry.logs();
    let last_output = logs
        .iter()
        .rev()
        .take(8)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{summary} (encoder: {encoder}, output: {output_path}). Last worker output:\n{}",
        if last_output.is_empty() {
            "(no output)"
        } else {
            &last_output
        }
    )
}

fn run_export_worker(
    script_path: &std::path::Path,
    project_dir: &std::path::Path,
    output_path: &str,
    quality: ExportQuality,
    encoding: (ExportFormat, VideoEncoder),
    telemetry: ExportTelemetry,
    cancel_requested: Arc<AtomicBool>,
) -> Result<(), String> {
    let (format, video_encoder) = encoding;
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Gaanim executable: {error}"))?;
    let mut command = Command::new(executable);
    command
        .arg("--export-worker")
        .arg(script_path)
        .arg(output_path)
        .arg(quality.arg())
        .arg(export_format_arg(format))
        .arg("--encoder")
        .arg(video_encoder.arg_name())
        .env("GAANIM_EXPORT_WORKER", "1")
        .current_dir(project_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_export_worker(&mut command);
    let mut child = command
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
    let mut last_frame = telemetry.progress().0;
    let mut last_progress_at = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not poll the isolated export worker: {error}"))?
        {
            break status;
        }

        let current_frame = telemetry.progress().0;
        if current_frame != last_frame {
            last_frame = current_frame;
            last_progress_at = Instant::now();
        }
        if let Some(reason) = worker_stop_reason(
            cancel_requested.load(Ordering::Acquire),
            last_progress_at,
            Instant::now(),
            EXPORT_WORKER_STALL_TIMEOUT,
        ) {
            terminate_export_worker(&mut child);
            let summary = match reason {
                WorkerStopReason::Cancelled => "export cancelled",
                WorkerStopReason::Stalled => {
                    "export worker made no frame progress for 120 seconds and was terminated"
                }
            };
            if let Some(thread) = stdout_thread {
                let _ = thread.join();
            }
            if let Some(thread) = stderr_thread {
                let _ = thread.join();
            }
            return Err(worker_error_context(summary, output_path, &telemetry));
        }
        std::thread::sleep(EXPORT_WORKER_POLL_INTERVAL);
    };
    if let Some(thread) = stdout_thread {
        let _ = thread.join();
    }
    if let Some(thread) = stderr_thread {
        let _ = thread.join();
    }
    if status.success() {
        Ok(())
    } else {
        Err(worker_error_context(
            &format!("export worker exited with {status}"),
            output_path,
            &telemetry,
        ))
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
    use super::{
        ExportTelemetry, WorkerStopReason, forward_worker_line, open_exported_file_with,
        resolve_output_path, worker_stop_reason,
    };
    use std::path::Path;
    use std::time::{Duration, Instant};

    #[test]
    fn worker_watchdog_stops_on_cancel_or_stalled_progress() {
        let last_progress_at = Instant::now();

        assert_eq!(
            worker_stop_reason(
                true,
                last_progress_at,
                last_progress_at,
                Duration::from_secs(120),
            ),
            Some(WorkerStopReason::Cancelled)
        );
        assert_eq!(
            worker_stop_reason(
                false,
                last_progress_at,
                last_progress_at + Duration::from_secs(120),
                Duration::from_secs(120),
            ),
            Some(WorkerStopReason::Stalled)
        );
    }

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

    #[test]
    fn relative_export_paths_resolve_against_the_project_as_absolute_paths() {
        let project = if cfg!(windows) {
            Path::new(r"C:\projects\demo")
        } else {
            Path::new("/projects/demo")
        };
        let resolved = resolve_output_path("exports/demo.mp4", Some(project)).unwrap();

        assert!(resolved.is_absolute());
        assert_eq!(resolved, project.join("exports/demo.mp4"));
    }

    #[test]
    fn absolute_export_paths_are_preserved() {
        let absolute = if cfg!(windows) {
            Path::new(r"C:\exports\demo.webp")
        } else {
            Path::new("/exports/demo.webp")
        };

        assert_eq!(
            resolve_output_path(absolute.to_str().unwrap(), None).unwrap(),
            absolute
        );
    }

    #[test]
    fn opener_errors_include_the_exported_path() {
        let path = Path::new("missing.mp4");
        let error = open_exported_file_with(path, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no default application",
            ))
        })
        .unwrap_err();

        assert!(error.contains("missing.mp4"));
        assert!(error.contains("no default application"));
    }
}
