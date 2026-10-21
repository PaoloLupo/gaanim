use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::SceneModel;
use gaanim_api::export::export_canvas;
use gaanim_export::encoder::{ExportFormat, VideoEncoder};
use gaanim_export::prelude::*;
use gaanim_timeline::timeline::Timeline;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) mod output_file;
use output_file::{OutputKind, OutputParts};

use crate::ui_kit::{
    ButtonTone, Icon, card_frame, chip, field_frame, icon_button, paint_icon, palette,
    primary_button, secondary_button, section_label, segmented, small_button, status_badge,
};

const EXPORT_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(100);
const EXPORT_WORKER_STALL_TIMEOUT: Duration = Duration::from_secs(120);
const EXPORT_SUCCESS_MESSAGE: &str = "Export completed successfully";

#[derive(Resource, Clone, Debug)]
pub struct ProjectPaths {
    pub project_dir: PathBuf,
    pub output_dir: PathBuf,
    pub script_path: PathBuf,
    /// The project's `[polls] relay`, for audience polls.
    pub poll_relay: Option<String>,
}

#[derive(Resource, Clone, Default)]
pub struct StashedReplay {
    pub canvas: Option<SceneModel>,
    /// The playback bundle shown instead of a script's scene: Presenter View
    /// previews its recorded frames.
    pub bundle: Option<StashedBundle>,
    /// Changes on every replay, even when segment names and timings stay equal.
    pub revision: u64,
}

/// An open playback bundle and the pixel size of its frame.
#[derive(Clone, Debug)]
pub struct StashedBundle {
    pub path: PathBuf,
    pub size: (u32, u32),
}

#[derive(Resource)]
pub struct ExportState {
    pub dialog_open: bool,
    pub format: ExportFormat,
    /// Record a playback bundle (`.gaanim`) instead of a video.
    pub bundle: bool,
    /// Export a PDF deck, a page for each point where the presentation rests.
    pub deck: bool,
    /// The deck prints each slide's speaker notes.
    pub notes: bool,
    pub quality: ExportQuality,
    /// Frame rate chosen in the dialog; `None` takes the quality's.
    pub fps: Option<FrameRate>,
    /// Keep the alpha channel, for the formats that carry it.
    pub transparent: bool,
    pub video_encoder: VideoEncoder,
    pub output_path: String,
    /// The folder and name the dialog edits; `output_path` follows them and
    /// the format. `None` until the dialog names the first export.
    output_parts: Option<OutputParts>,
    pub width: u32,
    pub height: u32,
    pub fit: OutputFit,
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

/// Where a running export is, for the playback bar and the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportStatus {
    /// Share of the frames written, between zero and one. Frames follow the
    /// timeline, so it is also the share of the timeline exported.
    pub fraction: f32,
    pub current: u64,
    pub total: u64,
    pub elapsed: f64,
    /// Seconds left at the current speed, once a frame is written.
    pub remaining: Option<f64>,
    pub cancelling: bool,
}

impl ExportState {
    /// Where the running export is, or `None` without one.
    pub fn status(&self) -> Option<ExportStatus> {
        if !self.active {
            return None;
        }
        let lock = self.progress_shared.lock().ok()?;
        let progress = lock.as_ref()?;
        let (telemetry_frame, telemetry_total) = progress.telemetry.progress();
        let total = telemetry_total.max(progress.total_frames);
        let current = telemetry_frame.max(progress.current_frame).min(total);
        let elapsed = progress.started_at.elapsed().as_secs_f64();
        Some(ExportStatus {
            fraction: if total > 0 {
                current as f32 / total as f32
            } else {
                0.0
            },
            current,
            total,
            elapsed,
            remaining: (current > 0 && total >= current)
                .then(|| elapsed * (total - current) as f64 / current as f64),
            cancelling: self.cancel_requested.load(Ordering::Acquire),
        })
    }

    /// Ask the running export to stop; it reports itself as cancelled.
    pub fn cancel(&self) {
        if self.active {
            self.cancel_requested.store(true, Ordering::Release);
        }
    }
}

/// Mirrors a background export in the terminal the editor was started from,
/// with the progress bar of `gaanim export`, and reports how it ended.
struct TerminalProgress {
    done: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    started: Instant,
    file: String,
}

impl TerminalProgress {
    fn start(telemetry: ExportTelemetry, file: String) -> Self {
        gaanim_core::console::info("export", format!("{file} · from the editor"));
        let done = Arc::new(AtomicBool::new(false));
        let finished = done.clone();
        let handle = std::thread::spawn(move || {
            // indicatif draws only on a terminal, like `gaanim export`.
            let mut bar = None;
            let mut last = (0, 0);
            let mut last_speed_at = Instant::now();
            let mut last_speed_frame = 0;
            while !finished.load(Ordering::Acquire) {
                let (current, total) = telemetry.progress();
                if total > 0 && (current, total) != last {
                    let bar = bar.get_or_insert_with(|| {
                        let bar = gaanim_export::exporter::create_progress_bar(total);
                        bar.set_message("…");
                        bar
                    });
                    bar.set_length(total);
                    bar.set_position(current);
                    let seconds = last_speed_at.elapsed().as_secs_f64();
                    if seconds >= 1.0 {
                        let fps = current.saturating_sub(last_speed_frame) as f64 / seconds;
                        bar.set_message(format!("{fps:.1} fps"));
                        last_speed_at = Instant::now();
                        last_speed_frame = current;
                    }
                    last = (current, total);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if let Some(bar) = bar {
                bar.finish_and_clear();
            }
        });
        Self {
            done,
            handle: Some(handle),
            started: Instant::now(),
            file,
        }
    }

    fn finish(mut self, result: &Result<(), String>) {
        self.done.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let elapsed = short_duration(self.started.elapsed().as_secs_f64());
        match result {
            Ok(()) => gaanim_core::console::success("export", format!("{} · {elapsed}", self.file)),
            Err(error) if error.starts_with("export cancelled") => {
                gaanim_core::console::warn("export", format!("{} · cancelled", self.file))
            }
            Err(error) => gaanim_core::console::error("export", error),
        }
    }
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
}

/// Rates the dialog offers besides the quality's, with what uses them.
const FRAME_RATES: [(u32, u32, &str); 8] = [
    (24, 1, "film"),
    (25, 1, "PAL"),
    (30, 1, ""),
    (50, 1, "PAL"),
    (60, 1, ""),
    (24_000, 1001, "NTSC, film"),
    (30_000, 1001, "NTSC"),
    (60_000, 1001, "NTSC"),
];

/// Formats whose files keep the alpha channel.
fn keeps_alpha(format: ExportFormat) -> bool {
    matches!(
        format,
        ExportFormat::Webm | ExportFormat::Mov | ExportFormat::Webp | ExportFormat::PngSequence
    )
}

/// What the export dialog writes: a video format, a playback bundle or a
/// PDF deck.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FormatChoice {
    Video(ExportFormat),
    Bundle,
    Deck,
}

impl FormatChoice {
    /// The choice `state` last made.
    fn of(state: &ExportState) -> Self {
        if state.deck {
            Self::Deck
        } else if state.bundle {
            Self::Bundle
        } else {
            Self::Video(state.format)
        }
    }
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            dialog_open: false,
            format: ExportFormat::Mp4,
            bundle: false,
            deck: false,
            notes: true,
            quality: ExportQuality::Standard,
            fps: None,
            transparent: false,
            video_encoder: VideoEncoder::Auto,
            output_path: "output.mp4".to_string(),
            output_parts: None,
            width: 1920,
            height: 1080,
            fit: OutputFit::Error,
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
    bundle: Option<Res<crate::bundle_player::BundlePlayback>>,
) {
    let Ok(ctx) = ctx.ctx_mut() else { return };

    // Name the first export after the project, script or bundle, in the
    // project's output folder.
    if state.output_parts.is_none() {
        let project = project_paths
            .as_ref()
            .map(|paths| (paths.project_dir.as_path(), paths.script_path.as_path()));
        let folder = project_paths
            .as_ref()
            .map(|paths| output_file::relative_folder(&paths.output_dir, Some(&paths.project_dir)))
            .unwrap_or_default();
        let parts = OutputParts {
            folder,
            name: output_file::default_name(project, bundle.as_ref().map(|bundle| bundle.path())),
        };
        state.output_path =
            output_file::join(&parts, output_kind(FormatChoice::of(&state), state.format));
        state.output_parts = Some(parts);
    }

    // --- Collect intent from egui into local variables first ---
    let mut trigger_export = false;
    let mut trigger_cancel = false;
    let mut trigger_ok = false;
    let mut trigger_open = false;
    let mut trigger_retry = false;

    if state.show_complete {
        let cancelled = state.message.starts_with("export cancelled");
        let (icon, color, title) = if state.completed_successfully {
            (Icon::Check, palette::LOOP, "Export complete")
        } else if cancelled {
            (Icon::Close, palette::TEXT_MUTED, "Export cancelled")
        } else {
            (Icon::Warning, palette::DANGER, "Export failed")
        };
        let summary = [
            state.elapsed_seconds.map(short_duration),
            state.encoder_label.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        // A PNG sequence is a folder of frames: show and open the folder.
        let sequence = FormatChoice::of(&state) == FormatChoice::Video(ExportFormat::PngSequence);
        let opened_path = state
            .completed_output_path
            .as_deref()
            .map(|path| exported_location(path, sequence));
        let displayed_path = opened_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| state.output_path.clone());
        let can_open = state.completed_successfully && state.completed_output_path.is_some();

        // A card in the corner, not a modal: playback goes on around it.
        egui::Area::new(egui::Id::new("export_complete"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -140.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                card_frame().show(ui, |ui| {
                    ui.set_width(420.0);
                    ui.spacing_mut().item_spacing.y = 10.0;
                    ui.horizontal(|ui| {
                        status_badge(ui, icon, color);
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(title)
                                    .size(16.0)
                                    .strong()
                                    .color(palette::TEXT),
                            );
                            if !summary.is_empty() {
                                ui.label(
                                    egui::RichText::new(&summary)
                                        .size(12.0)
                                        .color(palette::TEXT_MUTED),
                                );
                            }
                        });
                    });
                    if !state.completed_successfully && !cancelled {
                        egui::Frame::new()
                            .fill(palette::DANGER.gamma_multiply(0.08))
                            .inner_margin(egui::Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new(&state.message)
                                        .size(12.0)
                                        .color(palette::TEXT_MUTED),
                                );
                            });
                    } else if state.completed_successfully {
                        field_frame().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&displayed_path)
                                        .monospace()
                                        .size(11.5)
                                        .color(palette::TEXT_MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&displayed_path);
                        });
                        // e.g. the file was written but could not be opened.
                        if state.message != EXPORT_SUCCESS_MESSAGE {
                            notice(ui, palette::STOP, &state.message);
                        }
                    }
                    ui.add_space(4.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if can_open {
                            let open_label = if sequence { "Open folder" } else { "Open file" };
                            if primary_button(ui, open_label, None, true).clicked() {
                                trigger_open = true;
                            }
                            if secondary_button(ui, "Close", true).clicked() {
                                trigger_ok = true;
                            }
                        } else if !state.completed_successfully && !cancelled {
                            // A failure says what to do next: try again with the
                            // same settings, copy the error, or install FFmpeg.
                            if primary_button(ui, "Try again", None, true)
                                .on_hover_text("Opens the export with the same settings")
                                .clicked()
                            {
                                trigger_retry = true;
                            }
                            if secondary_button(ui, "Close", true).clicked() {
                                trigger_ok = true;
                            }
                            if secondary_button(ui, "Copy error", true).clicked() {
                                ui.ctx().copy_text(state.message.clone());
                            }
                            if mentions_ffmpeg(&state.message)
                                && secondary_button(ui, "How to install FFmpeg", true).clicked()
                            {
                                let _ = crate::platform::open_detached(FFMPEG_HELP_URL);
                            }
                        } else if primary_button(ui, "Close", None, true).clicked() {
                            trigger_ok = true;
                        }
                    });
                });
            });
    }

    if trigger_open && let Some(path) = state.completed_output_path.clone() {
        let sequence = FormatChoice::of(&state) == FormatChoice::Video(ExportFormat::PngSequence);
        match open_exported_file(&exported_location(&path, sequence)) {
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
    if trigger_retry {
        state.show_complete = false;
        state.dialog_open = true;
    }

    if state.active {
        let (prog_done, encoder_label) = {
            let lock = state.progress_shared.lock().unwrap();
            if let Some(ref prog) = *lock {
                let (telemetry_frame, telemetry_total) = prog.telemetry.progress();
                let current_frame = telemetry_frame.max(prog.current_frame);
                let total_frames = telemetry_total.max(prog.total_frames);
                let result_ready = prog.result.is_some();
                (
                    current_frame >= total_frames && result_ready,
                    prog.telemetry.encoder(),
                )
            } else {
                (false, None)
            }
        };

        if prog_done {
            let (message, encoder_label, elapsed_seconds, succeeded) = {
                let lock = state.progress_shared.lock().unwrap();
                if let Some(progress) = lock.as_ref() {
                    let succeeded = matches!(progress.result.as_ref(), Some(Ok(())));
                    let message = match progress.result.as_ref() {
                        Some(Ok(())) => EXPORT_SUCCESS_MESSAGE.to_string(),
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
            // The export runs in the background: the playback bar and the
            // timeline show its progress, and playback stays available.
            return;
        }
    }

    if !state.dialog_open {
        return;
    }

    // Collect all values BEFORE the button handlers (to avoid borrow issues)
    let mut current_format = state.format;
    let mut current_choice = FormatChoice::of(&state);
    let mut current_notes = state.notes;
    let mut current_quality = state.quality;
    let mut current_fps = state.fps;
    let mut current_transparent = state.transparent;
    let mut current_encoder = state.video_encoder;
    let mut current_parts = state
        .output_parts
        .clone()
        .unwrap_or_else(|| output_file::split(&state.output_path));
    let mut current_width = state.width;
    let mut current_height = state.height;
    let mut current_fit = state.fit;
    // A playback bundle exports its recorded frames, at its recorded rate.
    let bundle_source = bundle.as_ref().map(|bundle| bundle.path().to_path_buf());
    let has_replay = replay_stash.canvas.is_some() || bundle_source.is_some();
    let scene_resolution = match &bundle {
        Some(bundle) => {
            let (width, height) = bundle.output_size();
            Some((f64::from(width), f64::from(height)))
        }
        None => replay_stash
            .canvas
            .as_ref()
            .map(|canvas| (canvas.frame.width, canvas.frame.height)),
    };
    let dur = timeline.cached_duration;
    // An open bundle exports video; a script's scene can also be recorded
    // into a bundle.
    let offers_bundle = bundle_source.is_none();
    if !offers_bundle && current_choice == FormatChoice::Bundle {
        current_choice = FormatChoice::Video(current_format);
    }
    // A bundle records whole rates.
    if current_choice == FormatChoice::Bundle
        && current_fps.is_some_and(|fps| fps.as_whole().is_none())
    {
        current_fps = None;
    }
    let preset_fps = match current_choice {
        FormatChoice::Video(format) => current_quality.preset().fps_for(format),
        FormatChoice::Bundle | FormatChoice::Deck => current_quality.fps(),
    };
    // A bundle's video runs at its recorded rate.
    let fps = match &bundle {
        Some(bundle) => FrameRate::whole(bundle.fps()),
        None => current_fps.unwrap_or(FrameRate::whole(preset_fps)),
    };
    let total = fps.frames_in(dur);

    // Enter submits unless a field is using it (text or number entry).
    let enter_submits = ctx.input(|input| input.key_pressed(egui::Key::Enter))
        && ctx.memory(|memory| memory.focused().is_none());
    let modal = egui::Modal::new(egui::Id::new("export_config"))
        .frame(card_frame())
        .backdrop_color(egui::Color32::from_black_alpha(150))
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing.y = 8.0;

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        egui::RichText::new("Export")
                            .size(17.0)
                            .strong()
                            .color(palette::TEXT),
                    );
                    let detail = if current_choice == FormatChoice::Deck {
                        let slides = crate::cli::deck_slides(timeline.segments.iter().map(
                            |segment| {
                                (
                                    segment.name.as_str(),
                                    segment.notes.as_deref(),
                                    segment.stops.iter().map(|stop| stop.time).collect(),
                                )
                            },
                        ));
                        let pages =
                            gaanim_export::deck::deck_pages(&slides, dur, current_notes).len();
                        let noun = if pages == 1 { "page" } else { "pages" };
                        format!("{} · {pages} {noun}", crate::format_time(dur))
                    } else {
                        format!("{} · {} frames at {} fps", crate::format_time(dur), total, fps)
                    };
                    ui.label(
                        egui::RichText::new(detail)
                            .size(12.0)
                            .color(palette::TEXT_MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                        .on_hover_text("Close · Esc")
                        .clicked()
                    {
                        trigger_cancel = true;
                    }
                });
            });
            ui.add_space(8.0);

            section_label(ui, "Format");
            let videos = [
                (FormatChoice::Video(ExportFormat::Mp4), "MP4", "H.264"),
                (FormatChoice::Video(ExportFormat::Webm), "WebM", "VP9"),
                (FormatChoice::Video(ExportFormat::Mov), "MOV", "ProRes"),
                // Eight formats share the row: short details keep them apart.
                (FormatChoice::Video(ExportFormat::Webp), "WebP", "Animated"),
                (FormatChoice::Video(ExportFormat::Gif), "GIF", "Animated"),
                (FormatChoice::Video(ExportFormat::PngSequence), "PNG", "Frames"),
                (FormatChoice::Deck, "PDF", "Stops"),
                (FormatChoice::Bundle, "Gaanim", "Scene"),
            ];
            let choices = if offers_bundle {
                &videos[..]
            } else {
                &videos[..videos.len() - 1]
            };
            segmented(ui, "export_format", &mut current_choice, choices);
            if current_choice == FormatChoice::Video(ExportFormat::PngSequence) {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "One PNG per frame (frame_00000.png, frame_00001.png…) in the output folder, to edit or composite in another program.",
                        )
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                    )
                    .wrap(),
                );
            }
            if current_choice == FormatChoice::Video(ExportFormat::Mov) {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "Apple ProRes for video editors: 422 HQ, or 4444 with an alpha channel if you keep transparency. Large files, no visible loss.",
                        )
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                    )
                    .wrap(),
                );
            }
            if current_choice == FormatChoice::Bundle {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "A .gaanim file with the whole scene: it plays, presents and exports to video on any computer.",
                        )
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                    )
                    .wrap(),
                );
            }
            if current_choice == FormatChoice::Deck {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(
                            "A PDF with a page for each stop in the presentation (scene.stop()) and one for the end, to hand out or print.",
                        )
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                    )
                    .wrap(),
                );
                crate::ui_kit::checkbox(ui, &mut current_notes, "Include speaker notes")
                    .on_hover_text(
                        "Adds, after each slide's last page, a page with the notes from scene.segment(notes=...).",
                    );
            }
            let recording = current_choice == FormatChoice::Bundle;
            let deck = current_choice == FormatChoice::Deck;
            if let FormatChoice::Video(format) = current_choice
                && keeps_alpha(format)
            {
                crate::ui_kit::checkbox(ui, &mut current_transparent, "Keep transparency")
                    .on_hover_text(
                        "Saves the alpha channel. The scene needs a transparent background, for example Scene(background=\"#00000000\").",
                    );
            }
            ui.add_space(6.0);

            // A deck's pages are lossless frames at the chosen size.
            if !deck {
            section_label(ui, "Quality");
            // With a rate of its own, the quality only chooses the encoding.
            let own_rate = current_fps.is_some() && bundle_source.is_none();
            segmented(
                ui,
                "export_quality",
                &mut current_quality,
                &[
                    (
                        ExportQuality::Draft,
                        "Draft",
                        if own_rate { "fast" } else { "30 fps · fast" },
                    ),
                    (
                        ExportQuality::Standard,
                        "Standard",
                        if own_rate { "balanced" } else { "60 fps · balanced" },
                    ),
                    (
                        ExportQuality::Production,
                        "Production",
                        if own_rate { "best" } else { "60 fps · best" },
                    ),
                ],
            );

            // A bundle's video keeps its recorded rate.
            if bundle_source.is_none() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    section_label(ui, "Frames per second");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = |rate: Option<FrameRate>| match rate {
                            None => format!("From quality · {preset_fps} fps"),
                            Some(rate) => FRAME_RATES
                                .iter()
                                .find(|(numerator, denominator, _)| {
                                    FrameRate::new(*numerator, *denominator) == Some(rate)
                                })
                                .filter(|(_, _, usage)| !usage.is_empty())
                                .map(|(_, _, usage)| format!("{rate} fps · {usage}"))
                                .unwrap_or_else(|| format!("{rate} fps")),
                        };
                        egui::ComboBox::from_id_salt("export_fps")
                            .width(200.0)
                            .selected_text(label(current_fps))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut current_fps, None, label(None));
                                for (numerator, denominator, _) in FRAME_RATES {
                                    // A bundle records whole rates.
                                    if recording && denominator != 1 {
                                        continue;
                                    }
                                    let rate = FrameRate::new(numerator, denominator);
                                    ui.selectable_value(&mut current_fps, rate, label(rate));
                                }
                            });
                    });
                });
            }
            if current_choice == FormatChoice::Video(ExportFormat::Mp4) {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    section_label(ui, "Encoder");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("export_encoder")
                            .width(200.0)
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
                });
                if current_encoder == VideoEncoder::H264Vaapi {
                    notice(
                        ui,
                        palette::STOP,
                        "VAAPI is used only if you choose it: a driver fault can reset the GPU.",
                    );
                }
            }
            ui.add_space(6.0);
            }

            if !recording {
            section_label(ui, "Resolution");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (label, width, height) in RESOLUTION_PRESETS {
                    if chip(
                        ui,
                        label,
                        current_width == width && current_height == height,
                    )
                    .clicked()
                    {
                        current_width = width;
                        current_height = height;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    field_frame().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                        ui.add(
                            egui::DragValue::new(&mut current_height)
                                .range(1..=16384)
                                .speed(4.0),
                        )
                        .on_hover_text("Height (px)");
                    });
                    ui.label(egui::RichText::new("×").color(palette::TEXT_FAINT));
                    field_frame().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                        ui.add(
                            egui::DragValue::new(&mut current_width)
                                .range(1..=16384)
                                .speed(4.0),
                        )
                        .on_hover_text("Width (px)");
                    });
                });
            });
            ui.add_space(6.0);

            section_label(ui, "If the aspect ratio does not match the scene");
            segmented(
                ui,
                "export_fit",
                &mut current_fit,
                &[
                    (OutputFit::Error, "Fail with an error", ""),
                    (OutputFit::Contain, "Contain", ""),
                    (OutputFit::Cover, "Cover", ""),
                ],
            );
            if let Some((scene_w, scene_h)) = scene_resolution
                && aspect_differs((scene_w, scene_h), (current_width, current_height))
            {
                let scene_aspect = aspect_label(scene_w, scene_h);
                match current_fit {
                    OutputFit::Error => notice(
                        ui,
                        palette::DANGER,
                        &format!(
                            "The scene is {scene_aspect} and the output is not: the export will fail. Choose Contain or Cover."
                        ),
                    ),
                    OutputFit::Contain => notice(
                        ui,
                        palette::TEXT_MUTED,
                        &format!("The scene ({scene_aspect}) will show in full, with bars."),
                    ),
                    OutputFit::Cover => notice(
                        ui,
                        palette::TEXT_MUTED,
                        &format!("The scene ({scene_aspect}) will fill the frame and be cropped."),
                    ),
                }
            }
            ui.add_space(6.0);
            }

            let kind = output_kind(current_choice, current_format);
            let project_dir = project_paths
                .as_ref()
                .map(|paths| paths.project_dir.as_path());
            output_file_section(ui, &mut current_parts, kind, project_dir);
            ui.add_space(14.0);

            ui.horizontal(|ui| {
                let summary = if recording {
                    format!("bundle · {fps} fps")
                } else if deck {
                    format!("{current_width}×{current_height} · PDF")
                } else {
                    format!("{current_width}×{current_height} · {fps} fps")
                };
                ui.label(
                    egui::RichText::new(summary)
                    .size(12.0)
                    .color(palette::TEXT_FAINT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let named = !current_parts.name.trim().is_empty();
                    if primary_button(ui, "Export", Some(Icon::Export), named)
                        .on_hover_text(if named {
                            "Enter"
                        } else {
                            "Type a name for the file"
                        })
                        .clicked()
                    {
                        trigger_export = true;
                    }
                    if secondary_button(ui, "Cancel", true).clicked() {
                        trigger_cancel = true;
                    }
                });
            });
        });
    if modal.should_close() {
        trigger_cancel = true;
    }
    if enter_submits && !trigger_cancel {
        trigger_export = true;
    }
    if let FormatChoice::Video(format) = current_choice {
        current_format = format;
    }
    let current_output =
        output_file::join(&current_parts, output_kind(current_choice, current_format));
    // An export needs a name to write.
    if current_parts.name.trim().is_empty() {
        trigger_export = false;
    }

    // Apply state changes AFTER the egui closures (no borrow conflicts)
    state.format = current_format;
    state.bundle = current_choice == FormatChoice::Bundle && offers_bundle;
    state.deck = current_choice == FormatChoice::Deck;
    state.notes = current_notes;
    state.quality = current_quality;
    state.fps = current_fps;
    state.transparent = current_transparent;
    state.video_encoder = current_encoder;
    state.output_path = current_output;
    state.output_parts = Some(current_parts);
    state.width = current_width;
    state.height = current_height;
    state.fit = current_fit;

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
            let record_bundle = state.bundle;
            let deck = state.deck.then_some(state.notes);
            let qual = state.quality;
            let fps = state.fps;
            let transparent = state.transparent && keeps_alpha(fmt) && !record_bundle;
            let video_encoder = if fmt == ExportFormat::Mp4 {
                state.video_encoder
            } else {
                VideoEncoder::Auto
            };
            let output_size = (state.width, state.height);
            let output_fit = state.fit;
            let progress = state.progress_shared.clone();
            let cancel_requested = state.cancel_requested.clone();
            let telemetry = ExportTelemetry::new();
            let canvas = replay_stash.canvas.clone();
            let worker_paths = project_paths
                .as_ref()
                .filter(|_| bundle_source.is_none())
                .map(|paths| (paths.script_path.clone(), paths.project_dir.clone()));

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
            let file_name = Path::new(&out)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| out.clone());
            std::thread::spawn(move || {
                // The worker process reports to the editor; the in-process
                // exports draw their own terminal bar.
                let terminal = worker_paths
                    .is_some()
                    .then(|| TerminalProgress::start(telemetry.clone(), file_name));
                let result = match worker_paths {
                    Some((script_path, project_dir)) => run_export_worker(
                        &script_path,
                        &project_dir,
                        &out,
                        WorkerEncoding {
                            quality: qual,
                            format: if deck.is_some() {
                                crate::cli::PDF_EXTENSION
                            } else if record_bundle {
                                gaanim_bundle::EXTENSION
                            } else {
                                export_format_arg(fmt)
                            },
                            video_encoder,
                            fps,
                            transparent,
                            notes: deck.unwrap_or(false),
                        },
                        output_size,
                        output_fit,
                        telemetry.clone(),
                        cancel_requested,
                    ),
                    None if deck.is_some() => {
                        let command = crate::cli::ExportCommand {
                            input: bundle_source,
                            output: Some(out.clone()),
                            quality: qual.arg().to_string(),
                            width: Some(output_size.0),
                            height: Some(output_size.1),
                            fit: output_fit,
                            notes: deck.unwrap_or(false),
                            ..Default::default()
                        };
                        export_deck(&command, canvas, telemetry.clone())
                    }
                    None if record_bundle => match canvas {
                        Some(canvas) => {
                            let fps = fps
                                .and_then(FrameRate::as_whole)
                                .unwrap_or(crate::cli::bundle_fps(qual.arg()));
                            let mut config =
                                crate::cli::bundle_config(None, &canvas, &out, Some(fps));
                            config.telemetry = Some(telemetry.clone());
                            gaanim_api::export::record_canvas(canvas, config)
                                .map_err(|error| error.to_string())
                        }
                        None => Err("No replay data available".to_string()),
                    },
                    None => {
                        let mut config = ExportConfig::new(&out);
                        // The preset sets fps, CRF and speed for the format.
                        config.format = fmt;
                        let mut config = config.with_quality(qual.preset());
                        // An open bundle exports at its recorded rate.
                        if let Some(fps) = fps.filter(|_| bundle_source.is_none()) {
                            config.fps = fps;
                        }
                        config.transparent = transparent;
                        config.width = output_size.0;
                        config.height = output_size.1;
                        config.fit = output_fit;
                        config.aspect_ratio = AspectRatioPreset::Custom;
                        config.video_encoder = video_encoder;
                        config.headless = true;
                        config.telemetry = Some(telemetry.clone());
                        match (bundle_source, canvas) {
                            (Some(bundle), _) => {
                                gaanim_export::prelude::export_bundle(&bundle, config)
                                    .map_err(|error| error.to_string())
                            }
                            (None, Some(canvas)) => {
                                export_canvas(canvas, config).map_err(|error| error.to_string())
                            }
                            (None, None) => Err("No replay data available".to_string()),
                        }
                    }
                };
                if let Some(terminal) = terminal {
                    terminal.finish(&result);
                }
                if let Ok(mut lock) = progress_clone.lock()
                    && let Some(ref mut p) = *lock
                {
                    p.result = Some(result);
                    // A recording counts its own work, which can exceed
                    // the dialog's frame estimate.
                    p.total_frames = p.total_frames.max(p.telemetry.progress().1);
                    p.current_frame = p.total_frames;
                }
            });
        }
    }
}

/// Export the PDF deck `command` describes in this process: from its input
/// bundle, or from the script's `canvas`.
#[cfg(not(target_arch = "wasm32"))]
fn export_deck(
    command: &crate::cli::ExportCommand,
    canvas: Option<gaanim_api::canvas::SceneModel>,
    telemetry: ExportTelemetry,
) -> Result<(), String> {
    let telemetry = Some(telemetry);
    match (command.input.is_some(), canvas) {
        (true, _) => crate::cli::export_bundle_pdf(command, telemetry)
            .map(|_| ())
            .map_err(|error| match error {
                crate::cli::CommandError::Usage(message)
                | crate::cli::CommandError::Failed(message) => message,
            }),
        (false, Some(canvas)) => {
            let output = command.output.clone().unwrap_or_default();
            crate::cli::export_slides_pdf(canvas, command, &output, None, telemetry).map(|_| ())
        }
        (false, None) => Err("No replay data available".to_string()),
    }
}

/// PDF decks are laid out by the desktop app.
#[cfg(target_arch = "wasm32")]
fn export_deck(
    _command: &crate::cli::ExportCommand,
    _canvas: Option<gaanim_api::canvas::SceneModel>,
    _telemetry: ExportTelemetry,
) -> Result<(), String> {
    Err("PDF export needs the Gaanim desktop app".to_string())
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
    open_exported_file_with(path, |path| crate::platform::open(path))
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
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

/// What the export worker writes and how it encodes it.
struct WorkerEncoding {
    quality: ExportQuality,
    /// A video format's extension, or the bundle's.
    format: &'static str,
    video_encoder: VideoEncoder,
    fps: Option<FrameRate>,
    transparent: bool,
    /// A PDF deck prints each slide's speaker notes.
    notes: bool,
}

#[allow(clippy::too_many_arguments)]
fn run_export_worker(
    script_path: &std::path::Path,
    project_dir: &std::path::Path,
    output_path: &str,
    encoding: WorkerEncoding,
    output_size: (u32, u32),
    fit: OutputFit,
    telemetry: ExportTelemetry,
    cancel_requested: Arc<AtomicBool>,
) -> Result<(), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("could not locate the Gaanim executable: {error}"))?;
    let mut command = Command::new(executable);
    command
        .arg("--export-worker")
        .arg(script_path)
        .arg(output_path)
        .arg(encoding.quality.arg())
        .arg(encoding.format)
        .arg("--encoder")
        .arg(encoding.video_encoder.arg_name())
        .arg("--width")
        .arg(output_size.0.to_string())
        .arg("--height")
        .arg(output_size.1.to_string())
        .arg("--fit")
        .arg(match fit {
            OutputFit::Error => "error",
            OutputFit::Contain => "contain",
            OutputFit::Cover => "cover",
        });
    if let Some(fps) = encoding.fps {
        command.arg("--fps").arg(fps.ffmpeg_arg());
    }
    if encoding.transparent {
        command.arg("--transparent");
    }
    if encoding.notes {
        command.arg("--notes");
    }
    command
        .env("GAANIM_EXPORT_WORKER", "1")
        .current_dir(project_dir)
        // Never the editor's stdin: under `--listen stdio` a thread here
        // blocks reading it, and on Windows the worker's startup queries
        // of that same pipe wait behind the read, so it hung before its
        // first frame.
        .stdin(Stdio::null())
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
            // A worker stopped mid-recording cannot clean up after itself.
            remove_partial(output_path);
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
        remove_partial(output_path);
        Err(worker_error_context(
            &format!("export worker exited with {status}"),
            output_path,
            &telemetry,
        ))
    }
}

/// Remove the `.partial` file a bundle export leaves while it records.
fn remove_partial(output_path: &str) {
    let partial = gaanim_export::bundle::partial_path(Path::new(output_path));
    let _ = std::fs::remove_file(partial);
}

/// Where the docs explain installing FFmpeg.
const FFMPEG_HELP_URL: &str = "https://paololupo.github.io/gaanim/empezar/instalacion/";

/// Whether an export error is about FFmpeg, missing or failing.
fn mentions_ffmpeg(message: &str) -> bool {
    message.to_lowercase().contains("ffmpeg")
}

const RESOLUTION_PRESETS: [(&str, u32, u32); 4] = [
    ("720p", 1280, 720),
    ("1080p", 1920, 1080),
    ("1440p", 2560, 1440),
    ("4K", 3840, 2160),
];

/// Same rule as `export_canvas`: a mismatch is more than one raster pixel
/// along both axes.
fn aspect_differs(scene: (f64, f64), output: (u32, u32)) -> bool {
    let (scene_w, scene_h) = scene;
    if scene_w <= 0.0 || scene_h <= 0.0 {
        return false;
    }
    let aspect = scene_w / scene_h;
    let (width, height) = (output.0 as f64, output.1 as f64);
    (width / aspect - height).abs() > 1.0 && (height * aspect - width).abs() > 1.0
}

/// `16:9` for integral frames, `1.78:1` otherwise.
fn aspect_label(width: f64, height: f64) -> String {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let integral = |value: f64| value.fract() == 0.0 && value > 0.0 && value < 1e9;
    if integral(width) && integral(height) {
        let (w, h) = (width as u64, height as u64);
        let divisor = gcd(w, h).max(1);
        format!("{}:{}", w / divisor, h / divisor)
    } else if height > 0.0 {
        format!("{:.2}:1", width / height)
    } else {
        "?".to_string()
    }
}

fn output_kind(choice: FormatChoice, format: ExportFormat) -> OutputKind {
    match choice {
        FormatChoice::Bundle => OutputKind::of(format, true),
        FormatChoice::Deck => OutputKind::File(crate::cli::PDF_EXTENSION),
        FormatChoice::Video(format) => OutputKind::of(format, false),
    }
}

/// The dialog's *File* section: the name with the format's extension,
/// the folder with a picker, the full path, and a notice when the export
/// would replace an earlier one.
fn output_file_section(
    ui: &mut egui::Ui,
    parts: &mut OutputParts,
    kind: OutputKind,
    project_dir: Option<&Path>,
) {
    section_label(ui, "File");
    let suffix = kind.suffix();
    let mono = egui::FontId::monospace(12.0);
    field_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let suffix_width = ui
                .painter()
                .layout_no_wrap(suffix.clone(), mono.clone(), palette::TEXT_FAINT)
                .size()
                .x;
            let response = ui.add(
                egui::TextEdit::singleline(&mut parts.name)
                    .hint_text("name")
                    .desired_width((ui.available_width() - suffix_width).max(80.0))
                    .font(mono.clone())
                    .text_color(palette::TEXT)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO),
            );
            if response.changed() {
                parts.name = output_file::sanitize_name(&parts.name);
            }
            ui.label(
                egui::RichText::new(&suffix)
                    .font(mono.clone())
                    .color(palette::TEXT_FAINT),
            );
        });
    });

    let folder = resolve_output_path(
        if parts.folder.is_empty() {
            "."
        } else {
            &parts.folder
        },
        project_dir,
    )
    .ok();
    // Rows as tall as their buttons, everything centred on one line.
    let row = |ui: &mut egui::Ui, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 26.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| add(ui),
        );
    };
    row(ui, &mut |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 16.0), egui::Sense::hover());
        paint_icon(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(13.0)),
            Icon::Folder,
            palette::TEXT_MUTED,
        );
        let shown = if parts.folder.is_empty() {
            "project folder".to_owned()
        } else {
            parts.folder.clone()
        };
        let label_width = (ui.available_width() - 90.0).max(60.0);
        let full_path = resolve_output_path(&output_file::join(parts, kind), project_dir)
            .map(|path| match kind {
                OutputKind::File(_) => path,
                OutputKind::Frames => exported_location(&path, true),
            })
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        ui.allocate_ui_with_layout(
            egui::vec2(label_width, 18.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(label_width);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(shown)
                            .size(12.5)
                            .color(palette::TEXT_MUTED),
                    )
                    .truncate(),
                )
                .on_hover_text(format!("Saves to {full_path}"));
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !crate::WEB
                && small_button(ui, "Change…", true)
                    .on_hover_text("Choose the folder to save in")
                    .clicked()
                && let Some(chosen) =
                    crate::platform::pick_folder("Export folder", folder.as_deref())
            {
                parts.folder = output_file::relative_folder(&chosen, project_dir);
            }
        });
    });

    if parts.name.trim().is_empty() {
        notice(ui, palette::STOP, "Type a name for the file.");
        return;
    }
    let Ok(resolved) = resolve_output_path(&output_file::join(parts, kind), project_dir) else {
        return;
    };
    if output_file::exists(&resolved, kind)
        && let Some(folder) = &folder
    {
        let free = output_file::free_name(folder, &parts.name, kind);
        row(ui, &mut |ui| {
            notice(
                ui,
                palette::STOP,
                &match kind {
                    OutputKind::File(_) => {
                        format!(
                            "{}{} already exists: it will be replaced.",
                            parts.name,
                            kind.suffix()
                        )
                    }
                    OutputKind::Frames => {
                        format!(
                            "The folder {} already has frames: they will be replaced.",
                            parts.name
                        )
                    }
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if small_button(ui, &format!("Use \"{free}\""), true)
                    .on_hover_text("Save with the next free name")
                    .clicked()
                {
                    parts.name = free.clone();
                }
            });
        });
    }
}

/// What a finished export opens: the file, or for a PNG sequence the folder
/// its frames are in.
fn exported_location(path: &Path, sequence: bool) -> PathBuf {
    match path.parent() {
        Some(parent) if sequence && !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

pub(crate) fn short_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let whole = seconds.round() as u64;
        format!("{}:{:02} min", whole / 60, whole % 60)
    }
}

fn notice(ui: &mut egui::Ui, color: egui::Color32, text: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 16.0), egui::Sense::hover());
        paint_icon(
            ui.painter(),
            egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(12.0)),
            Icon::Warning,
            color,
        );
        ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(color)).wrap());
    });
}

fn export_format_arg(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Mp4 => "mp4",
        ExportFormat::Webm => "webm",
        ExportFormat::Mov => "mov",
        ExportFormat::Webp => "webp",
        ExportFormat::Gif => "gif",
        ExportFormat::PngSequence => "png",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExportTelemetry, WorkerStopReason, aspect_differs, aspect_label, forward_worker_line,
        open_exported_file_with, resolve_output_path, short_duration, worker_stop_reason,
    };
    use bevy::platform::time::Instant;
    use std::path::Path;
    use std::time::Duration;

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

    #[test]
    fn png_sequences_open_their_folder() {
        assert_eq!(
            super::exported_location(Path::new("exports/output/frame.png"), true),
            Path::new("exports/output")
        );
        assert_eq!(
            super::exported_location(Path::new("exports/output.mp4"), false),
            Path::new("exports/output.mp4")
        );
    }

    #[test]
    fn aspect_warning_matches_the_exporter_tolerance() {
        assert!(!aspect_differs((16.0, 9.0), (1920, 1080)));
        assert!(!aspect_differs((16.0, 9.0), (1921, 1080)));
        assert!(aspect_differs((16.0, 9.0), (1080, 1080)));
        assert!(aspect_differs((4.0, 3.0), (1920, 1080)));
        assert!(!aspect_differs((0.0, 9.0), (1920, 1080)));
        assert_eq!(aspect_label(16.0, 9.0), "16:9");
        assert_eq!(aspect_label(1920.0, 1080.0), "16:9");
        assert_eq!(aspect_label(14.2, 8.0), "1.77:1");
    }

    #[test]
    fn short_durations_switch_to_minutes() {
        assert_eq!(short_duration(12.34), "12.3 s");
        assert_eq!(short_duration(125.0), "2:05 min");
        assert_eq!(short_duration(-1.0), "0.0 s");
    }
}
