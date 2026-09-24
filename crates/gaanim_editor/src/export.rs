use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::SceneModel;
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

use crate::ui_kit::{
    ButtonTone, Icon, card_frame, chip, field_frame, icon_button, paint_icon, palette,
    primary_button, progress_track, secondary_button, section_label, segmented, status_badge,
};

const EXPORT_WORKER_POLL_INTERVAL: Duration = Duration::from_millis(100);
const EXPORT_WORKER_STALL_TIMEOUT: Duration = Duration::from_secs(120);
const EXPORT_SUCCESS_MESSAGE: &str = "Export completed successfully";

#[derive(Resource, Clone, Debug)]
pub struct ProjectPaths {
    pub project_dir: PathBuf,
    pub output_dir: PathBuf,
    pub script_path: PathBuf,
}

#[derive(Resource, Clone, Default)]
pub struct StashedReplay {
    pub canvas: Option<SceneModel>,
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
        let cancelled = state.message.starts_with("export cancelled");
        let (icon, color, title) = if state.completed_successfully {
            (Icon::Check, palette::LOOP, "Exportación completada")
        } else if cancelled {
            (Icon::Close, palette::TEXT_MUTED, "Exportación cancelada")
        } else {
            (Icon::Warning, palette::DANGER, "La exportación falló")
        };
        let summary = [
            state.elapsed_seconds.map(short_duration),
            state.encoder_label.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let displayed_path = state
            .completed_output_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| state.output_path.clone());
        let can_open = state.completed_successfully && state.completed_output_path.is_some();

        let modal = egui::Modal::new(egui::Id::new("export_complete"))
            .frame(card_frame())
            .backdrop_color(egui::Color32::from_black_alpha(150))
            .show(ctx, |ui| {
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
                        .corner_radius(8.0)
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
                        if primary_button(ui, "Abrir archivo", None, true).clicked() {
                            trigger_open = true;
                        }
                        if secondary_button(ui, "Cerrar", true).clicked() {
                            trigger_ok = true;
                        }
                    } else if primary_button(ui, "Cerrar", None, true).clicked() {
                        trigger_ok = true;
                    }
                });
            });
        if modal.should_close() {
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
            let cancelling = state.cancel_requested.load(Ordering::Acquire);
            let file_name = Path::new(&state.output_path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| state.output_path.clone());
            // Closing by the backdrop or Escape must not cancel a long export
            // by accident; only the explicit button does.
            egui::Modal::new(egui::Id::new("export_progress"))
                .frame(card_frame())
                .backdrop_color(egui::Color32::from_black_alpha(150))
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.spacing_mut().item_spacing.y = 12.0;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(
                                egui::RichText::new(if cancelling {
                                    "Deteniendo…"
                                } else {
                                    "Exportando"
                                })
                                .size(16.0)
                                .strong()
                                .color(palette::TEXT),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&file_name)
                                        .size(12.0)
                                        .color(palette::TEXT_MUTED),
                                )
                                .truncate(),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("{:.0}%", progress * 100.0))
                                    .monospace()
                                    .size(24.0)
                                    .color(palette::TEXT),
                            );
                        });
                    });
                    progress_track(
                        ui,
                        progress,
                        if cancelling {
                            palette::TEXT_FAINT
                        } else {
                            palette::ACCENT
                        },
                    );
                    ui.columns(4, |columns| {
                        stat(
                            &mut columns[0],
                            "Frames",
                            &format!("{prog_frame} / {prog_total}"),
                        );
                        stat(
                            &mut columns[1],
                            "Transcurrido",
                            &short_duration(elapsed_seconds),
                        );
                        stat(
                            &mut columns[2],
                            "Restante",
                            &if prog_frame > 0 {
                                format!("~{}", short_duration(eta_seconds))
                            } else {
                                "—".to_string()
                            },
                        );
                        stat(
                            &mut columns[3],
                            "Codificador",
                            encoder_label.as_deref().unwrap_or("detectando…"),
                        );
                    });
                    ui.add_space(2.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if secondary_button(ui, "Cancelar exportación", !cancelling).clicked() {
                            trigger_cancel = true;
                        }
                    });
                });
            if trigger_cancel {
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
    let mut current_width = state.width;
    let mut current_height = state.height;
    let mut current_fit = state.fit;
    let has_replay = replay_stash.canvas.is_some();
    let scene_resolution = replay_stash
        .canvas
        .as_ref()
        .map(|canvas| (canvas.frame.width, canvas.frame.height));
    let dur = timeline.cached_duration;
    let fps = current_quality.fps();
    let total = (dur * fps as f64).ceil() as u64;

    let previous_format = current_format;
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
                        egui::RichText::new("Exportar")
                            .size(17.0)
                            .strong()
                            .color(palette::TEXT),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · {} frames a {} fps",
                            crate::format_time(dur),
                            total,
                            current_quality.fps(),
                        ))
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                        .on_hover_text("Cerrar · Esc")
                        .clicked()
                    {
                        trigger_cancel = true;
                    }
                });
            });
            ui.add_space(8.0);

            section_label(ui, "Formato");
            segmented(
                ui,
                "export_format",
                &mut current_format,
                &[
                    (ExportFormat::Mp4, "MP4", "Video H.264"),
                    (ExportFormat::Webm, "WebM", "Video VP9"),
                    (ExportFormat::Webp, "WebP", "Imagen animada"),
                    (ExportFormat::Gif, "GIF", "Imagen animada"),
                ],
            );
            ui.add_space(6.0);

            section_label(ui, "Calidad");
            segmented(
                ui,
                "export_quality",
                &mut current_quality,
                &[
                    (ExportQuality::Draft, "Borrador", "30 fps · rápida"),
                    (ExportQuality::Standard, "Estándar", "60 fps · equilibrada"),
                    (ExportQuality::Production, "Producción", "60 fps · máxima"),
                ],
            );

            if current_format == ExportFormat::Mp4 {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    section_label(ui, "Codificador");
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
                        "VAAPI solo se usa si lo eliges: un fallo del driver puede reiniciar la GPU.",
                    );
                }
            }
            ui.add_space(6.0);

            section_label(ui, "Resolución");
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
                        .on_hover_text("Alto (px)");
                    });
                    ui.label(egui::RichText::new("×").color(palette::TEXT_FAINT));
                    field_frame().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                        ui.add(
                            egui::DragValue::new(&mut current_width)
                                .range(1..=16384)
                                .speed(4.0),
                        )
                        .on_hover_text("Ancho (px)");
                    });
                });
            });
            ui.add_space(6.0);

            section_label(ui, "Si la proporción no coincide con la escena");
            segmented(
                ui,
                "export_fit",
                &mut current_fit,
                &[
                    (OutputFit::Error, "Avisar con error", ""),
                    (OutputFit::Contain, "Contener", ""),
                    (OutputFit::Cover, "Cubrir", ""),
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
                            "La escena es {scene_aspect} y la salida no: la exportación fallará. Elige Contener o Cubrir."
                        ),
                    ),
                    OutputFit::Contain => notice(
                        ui,
                        palette::TEXT_MUTED,
                        &format!("La escena ({scene_aspect}) se verá completa, con bandas."),
                    ),
                    OutputFit::Cover => notice(
                        ui,
                        palette::TEXT_MUTED,
                        &format!("La escena ({scene_aspect}) llenará el cuadro y se recortará."),
                    ),
                }
            }
            ui.add_space(6.0);

            section_label(ui, "Archivo");
            ui.add(
                egui::TextEdit::singleline(&mut current_output)
                    .desired_width(f32::INFINITY)
                    .font(egui::FontId::monospace(12.0))
                    .text_color(palette::TEXT)
                    .frame(field_frame())
                    .margin(egui::Margin::ZERO),
            );
            ui.add_space(14.0);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{current_width}×{current_height} · {} fps",
                        current_quality.fps()
                    ))
                    .size(12.0)
                    .color(palette::TEXT_FAINT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if primary_button(ui, "Exportar", Some(Icon::Export), true)
                        .on_hover_text("Enter")
                        .clicked()
                    {
                        trigger_export = true;
                    }
                    if secondary_button(ui, "Cancelar", true).clicked() {
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
    if current_format != previous_format {
        current_output = with_format_extension(&current_output, current_format);
    }

    // Apply state changes AFTER the egui closures (no borrow conflicts)
    state.format = current_format;
    state.quality = current_quality;
    state.video_encoder = current_encoder;
    state.output_path = current_output;
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
            let qual = state.quality;
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
                        output_size,
                        output_fit,
                        telemetry.clone(),
                        cancel_requested,
                    ),
                    None if needs_worker => Err(
                        "3D export requires an open project script so it can run in an isolated process"
                            .to_string(),
                    ),
                    None => {
                        let mut config = ExportConfig::new(&out).with_quality(qual.preset());
                        config.width = output_size.0;
                        config.height = output_size.1;
                        config.fit = output_fit;
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
    output_size: (u32, u32),
    fit: OutputFit,
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
        .arg("--width")
        .arg(output_size.0.to_string())
        .arg("--height")
        .arg(output_size.1.to_string())
        .arg("--fit")
        .arg(match fit {
            OutputFit::Error => "error",
            OutputFit::Contain => "contain",
            OutputFit::Cover => "cover",
        })
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

/// Keep the output extension in step with the chosen format, leaving custom
/// extensions alone.
fn with_format_extension(path: &str, format: ExportFormat) -> String {
    const KNOWN: [&str; 4] = ["mp4", "webm", "webp", "gif"];
    let as_path = Path::new(path);
    match as_path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if KNOWN.contains(&ext.to_ascii_lowercase().as_str()) => as_path
            .with_extension(export_format_arg(format))
            .to_string_lossy()
            .into_owned(),
        _ => path.to_string(),
    }
}

fn short_duration(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let whole = seconds.round() as u64;
        format!("{}:{:02} min", whole / 60, whole % 60)
    }
}

fn stat(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.spacing_mut().item_spacing.y = 2.0;
    ui.label(
        egui::RichText::new(label)
            .size(11.0)
            .color(palette::TEXT_FAINT),
    );
    ui.add(
        egui::Label::new(
            egui::RichText::new(value)
                .monospace()
                .size(12.5)
                .color(palette::TEXT),
        )
        .truncate(),
    );
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
        ExportFormat::Webp => "webp",
        ExportFormat::Gif => "gif",
        ExportFormat::PngSequence => "png",
    }
}

pub fn export_per_frame_system() {}

#[cfg(test)]
mod tests {
    use super::{
        ExportFormat, ExportTelemetry, WorkerStopReason, aspect_differs, aspect_label,
        forward_worker_line, open_exported_file_with, resolve_output_path, short_duration,
        with_format_extension, worker_stop_reason,
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

    #[test]
    fn output_extension_follows_the_selected_format() {
        assert_eq!(
            with_format_extension("exports/output.mp4", ExportFormat::Gif),
            "exports/output.gif"
        );
        assert_eq!(
            with_format_extension("clip.WEBM", ExportFormat::Mp4),
            "clip.mp4"
        );
        // Custom or missing extensions are the user's choice.
        assert_eq!(
            with_format_extension("clip.mov", ExportFormat::Webm),
            "clip.mov"
        );
        assert_eq!(with_format_extension("clip", ExportFormat::Webm), "clip");
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
